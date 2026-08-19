"""Vérification d'origine (D3.19) — voir docs/plans/phase-3-categorisation.md, section
« Authentification admin ». Un jeton à propager dans chaque formulaire ferait double emploi avec
le cookie de session en `SameSite=Lax` posé par `SessionMiddleware` : deux mécanismes pour le même
travail, c'est un de trop à maintenir juste.
"""

from urllib.parse import urlsplit

from fastapi import HTTPException, Request

_UNSAFE_METHODS = frozenset({"POST", "PUT", "PATCH", "DELETE"})


async def verify_csrf(request: Request) -> None:
    if request.method not in _UNSAFE_METHODS:
        return

    origin = request.headers.get("origin") or request.headers.get("referer")
    if not origin:
        raise HTTPException(status_code=403, detail="origine manquante")

    origin_host = urlsplit(origin).netloc
    if origin_host != request.headers.get("host", ""):
        raise HTTPException(status_code=403, detail="origine invalide")
