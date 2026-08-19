"""Le back-office — voir docs/plans/phase-3-categorisation.md. `mount(app)` regroupe tout ce que
`main.py` doit faire pour l'activer : le routeur, la garde d'authentification posée une seule fois
sur le routeur protégé plutôt que route par route, et les en-têtes communs.
"""

from fastapi import APIRouter, Depends, FastAPI, Request
from fastapi.responses import RedirectResponse
from fastapi.responses import Response as FastAPIResponse

from kyc_api.admin import auth, categorisation, exports, jobs
from kyc_api.admin.csrf import verify_csrf
from kyc_api.admin.headers import AdminHeadersMiddleware
from kyc_api.schemas.admin import AdminUser
from kyc_api.templating import templates

router = APIRouter(prefix="/admin")
router.include_router(auth.router, dependencies=[Depends(verify_csrf)])

# Le garde d'authentification est posée ici, sur ce routeur entier, pas route par route : une
# route admin ajoutée plus tard (catégorisation, import, jobs) est protégée par construction, sans
# que personne ait à y penser (plan, étape 5). Un routeur séparé plutôt qu'imbriqué dans `router` :
# FastAPI refuse un routeur au préfixe vide combiné à une route au chemin vide (le cas de l'index
# `/admin` lui-même), les deux doivent donc porter le préfixe directement.
protected_router = APIRouter(
    prefix="/admin", dependencies=[Depends(verify_csrf), Depends(auth.require_admin)]
)


@protected_router.get("")
async def dashboard(request: Request, admin_user: AdminUser = Depends(auth.require_admin)):
    return templates.TemplateResponse(
        request, "admin/dashboard.html.jinja", {"admin_user": admin_user}
    )


protected_router.include_router(categorisation.router)
protected_router.include_router(jobs.router)
protected_router.include_router(exports.router)


async def _handle_auth_required(request: Request, exc: Exception) -> FastAPIResponse:
    # Un fragment HTMX ne sait pas suivre une redirection utilement : `HX-Redirect` le fait suivre
    # côté client au lieu d'échanger le fragment contre une page de connexion (plan, étape 5).
    if request.headers.get("HX-Request") == "true":
        return FastAPIResponse(status_code=401, headers={"HX-Redirect": "/admin/login"})
    return RedirectResponse(url="/admin/login", status_code=303)


def mount(app: FastAPI) -> None:
    app.add_middleware(AdminHeadersMiddleware)
    app.include_router(router)
    app.include_router(protected_router)
    app.add_exception_handler(auth.AdminAuthRequired, _handle_auth_required)
