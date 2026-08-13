"""Pages publiques, toujours montées.

Séparées de `dev.py` volontairement : ce routeur-là est conditionné par `enable_dev_routes` et
disparaîtra en phase 2. Une page publique ne doit jamais vivre sur un routeur qu'on éteint.
"""

from fastapi import APIRouter, Request

from kyc_api.config import settings
from kyc_api.templating import templates

router = APIRouter()


@router.get("/")
async def index(request: Request):
    return templates.TemplateResponse(
        request,
        "index.html.jinja",
        {"show_demo": settings.enable_dev_routes},
    )
