"""Fragments HTMX — convention posée en phase 0 : routes explicites sous `/fragments/`, gabarits
préfixés `_` qui n'étendent jamais `base.html.jinja`. Chaque fragment a une page équivalente qui
fonctionne sans JS (voir docs/plans/phase-2-api-ui.md) : le `hx-get` d'un champ ou d'un formulaire
pointe ici, son `action`/`href` pointe sur la page.
"""

from datetime import date

from fastapi import APIRouter, Depends, HTTPException, Request

from kyc_api.cursor import InvalidCursor, parse_cursor
from kyc_api.db import Queryable, get_pool
from kyc_api.queries import candidates as candidates_queries
from kyc_api.queries import persons as persons_queries
from kyc_api.templating import templates

router = APIRouter(prefix="/fragments")


@router.get("/recherche")
async def recherche(request: Request, q: str | None = None, pool: Queryable = Depends(get_pool)):
    candidates = await candidates_queries.list_candidates(pool, q=q)
    return templates.TemplateResponse(
        request, "_search_results.html.jinja", {"candidates": candidates}
    )


@router.get("/personnes")
async def personnes(
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
        "_directory_list.html.jinja",
        {"persons": persons, "pagination": pagination, "q": q, "legislature": legislature},
    )


@router.get("/personne/{slug}/votes")
async def person_votes(
    request: Request,
    slug: str,
    legislature: int | None = None,
    position: str | None = None,
    du: date | None = None,
    au: date | None = None,
    groupe: str | None = None,
    avant: str | None = None,
    pool: Queryable = Depends(get_pool),
):
    resolution = await persons_queries.resolve_slug(pool, slug)
    if resolution is None or not resolution.is_current:
        raise HTTPException(status_code=404)

    try:
        cursor = parse_cursor(avant)
    except InvalidCursor:
        cursor = None

    votes_page = await persons_queries.list_votes(
        pool,
        resolution.person_id,
        legislature=legislature,
        position=position,
        du=du,
        au=au,
        groupe=groupe,
        avant=cursor,
    )
    filter_qs = persons_queries.votes_filter_query_string(
        legislature=legislature, position=position, du=du, au=au, groupe=groupe
    )

    return templates.TemplateResponse(
        request,
        "_vote_list.html.jinja",
        {
            "slug": resolution.current_slug,
            "votes": votes_page.votes,
            "next_cursor": votes_page.next_cursor,
            "filter_qs": filter_qs,
        },
    )
