"""Flux OAuth GitHub, session, garde d'authentification — voir
docs/plans/phase-3-categorisation.md, section « Authentification admin (D3.1) ». Écrit à la main
plutôt qu'avec une bibliothèque OAuth généraliste : une seule autorité, un seul flux, aucune
extensibilité recherchée. C'est le seul module du backend qui fait un appel HTTP sortant
(CLAUDE.md, « Ce que la phase 3 ne fait pas »).
"""

import secrets
from urllib.parse import urlencode

import httpx
from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse

from kyc_api.config import settings
from kyc_api.db import Queryable, get_pool
from kyc_api.queries import admin as admin_queries
from kyc_api.schemas.admin import AdminUser

router = APIRouter()

GITHUB_AUTHORIZE_URL = "https://github.com/login/oauth/authorize"
GITHUB_TOKEN_URL = "https://github.com/login/oauth/access_token"
GITHUB_USER_URL = "https://api.github.com/user"
_TIMEOUT_SECONDS = 10.0


class AdminAuthRequired(Exception):
    """Levée par `require_admin`, convertie en redirection (navigation normale) ou en 401
    `HX-Redirect` (requête HTMX) par le gestionnaire installé dans `kyc_api.admin.mount` — un
    fragment ne sait pas suivre une redirection utilement."""


def get_github_transport() -> httpx.AsyncBaseTransport | None:
    """Point d'injection pour les tests : `app.dependency_overrides` la remplace par un
    `httpx.MockTransport`, jamais de réseau en test (plan, commit 4). `None` laisse httpx utiliser
    le transport réseau réel par défaut.
    """
    return None


async def require_admin(request: Request, pool: Queryable = Depends(get_pool)) -> AdminUser:
    """Recharge `admin_user` à chaque requête (plan, étape 5) : un compte passé à `actif = false`
    perd la main immédiatement, sans attendre l'expiration de sa session."""
    admin_user_id = request.session.get("admin_user_id")
    if admin_user_id is None:
        raise AdminAuthRequired()
    user = await admin_queries.get_active_admin_user(pool, admin_user_id)
    if user is None:
        request.session.clear()
        raise AdminAuthRequired()
    return user


def _redirect_uri() -> str:
    return f"{settings.public_base_url}/admin/auth/callback"


@router.get("/login")
async def login(request: Request) -> RedirectResponse:
    if not settings.admin_oauth_configured:
        raise HTTPException(
            status_code=503,
            detail="authentification admin non configurée (ADMIN_GITHUB_CLIENT_ID manquant)",
        )
    state = secrets.token_urlsafe(32)
    request.session["oauth_state"] = state
    # Sans aucun `scope` : le profil public suffit à connaître le login, et ne rien demander est
    # la meilleure façon de ne rien obtenir de trop.
    params = {
        "client_id": settings.admin_github_client_id,
        "redirect_uri": _redirect_uri(),
        "state": state,
    }
    return RedirectResponse(url=f"{GITHUB_AUTHORIZE_URL}?{urlencode(params)}")


@router.get("/auth/callback")
async def callback(
    request: Request,
    code: str,
    state: str,
    pool: Queryable = Depends(get_pool),
    transport: httpx.AsyncBaseTransport | None = Depends(get_github_transport),
) -> RedirectResponse:
    expected_state = request.session.pop("oauth_state", None)
    if expected_state is None or state != expected_state:
        raise HTTPException(status_code=400, detail="state invalide ou expiré")

    async with httpx.AsyncClient(transport=transport, timeout=_TIMEOUT_SECONDS) as client:
        token_response = await client.post(
            GITHUB_TOKEN_URL,
            data={
                "client_id": settings.admin_github_client_id,
                "client_secret": settings.admin_github_client_secret,
                "code": code,
                "redirect_uri": _redirect_uri(),
            },
            headers={"Accept": "application/json"},
        )
        token_response.raise_for_status()
        access_token = token_response.json().get("access_token")
        if not access_token:
            raise HTTPException(status_code=502, detail="échange de jeton GitHub échoué")

        user_response = await client.get(
            GITHUB_USER_URL,
            headers={
                "Authorization": f"Bearer {access_token}",
                "Accept": "application/vnd.github+json",
            },
        )
        user_response.raise_for_status()
        github_user = user_response.json()

    login_name = str(github_user["login"]).strip().lower()
    if login_name not in settings.admin_github_logins_normalized:
        await admin_queries.log_admin_action(
            pool, admin_user_id=None, action="login_refuse", detail={"login": login_name}
        )
        raise HTTPException(status_code=403, detail="accès refusé")

    admin_user = await admin_queries.upsert_admin_user(
        pool,
        github_id=github_user["id"],
        github_login=github_user["login"],
        display_name=github_user.get("name") or github_user["login"],
    )
    await admin_queries.log_admin_action(
        pool, admin_user_id=admin_user.id, action="login", target=admin_user.github_login
    )
    request.session["admin_user_id"] = admin_user.id
    return RedirectResponse(url="/admin", status_code=303)


@router.post("/logout")
async def logout(request: Request, pool: Queryable = Depends(get_pool)) -> RedirectResponse:
    admin_user_id = request.session.pop("admin_user_id", None)
    if admin_user_id is not None:
        await admin_queries.log_admin_action(pool, admin_user_id=admin_user_id, action="logout")
    return RedirectResponse(url="/admin/login", status_code=303)
