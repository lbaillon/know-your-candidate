"""Pages publiques, toujours montées."""

from fastapi import APIRouter, Request

from kyc_api.templating import templates

router = APIRouter()


@router.get("/")
async def index(request: Request):
    return templates.TemplateResponse(request, "index.html.jinja", {})
