"""Pages publiques, toujours montées."""

from fastapi import APIRouter, Request

from kyc_api.documents import render_document
from kyc_api.templating import templates

router = APIRouter()


@router.get("/")
async def index(request: Request):
    return templates.TemplateResponse(request, "index.html.jinja", {})


@router.get("/methodologie")
async def methodologie(request: Request):
    html_content = render_document("methodology.md")
    return templates.TemplateResponse(
        request,
        "document.html.jinja",
        {"title": "Méthodologie", "html_content": html_content},
    )


@router.get("/sources")
async def sources(request: Request):
    html_content = render_document("data-sources.md")
    return templates.TemplateResponse(
        request,
        "document.html.jinja",
        {"title": "Sources de données", "html_content": html_content},
    )
