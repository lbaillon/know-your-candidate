"""Pages publiques, toujours montées."""

from fastapi import APIRouter, Depends, Request

from kyc_api.db import Queryable, get_pool
from kyc_api.documents import render_document
from kyc_api.queries import candidates as candidates_queries
from kyc_api.queries import persons as persons_queries
from kyc_api.templating import templates

router = APIRouter()


@router.get("/")
async def index(request: Request, q: str | None = None, pool: Queryable = Depends(get_pool)):
    candidates = await candidates_queries.list_candidates(pool, q=q)
    return templates.TemplateResponse(
        request, "home.html.jinja", {"candidates": candidates, "q": q}
    )


@router.get("/deputes")
async def deputes(
    request: Request,
    q: str | None = None,
    legislature: int | None = None,
    page: int = 1,
    pool: Queryable = Depends(get_pool),
):
    persons, pagination = await persons_queries.list_directory(
        pool, q=q, legislature=legislature, page=page
    )
    return templates.TemplateResponse(
        request,
        "directory.html.jinja",
        {"persons": persons, "pagination": pagination, "q": q, "legislature": legislature},
    )


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
