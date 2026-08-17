"""Fragments HTMX — convention posée en phase 0 : routes explicites sous `/fragments/`, gabarits
préfixés `_` qui n'étendent jamais `base.html.jinja`. Chaque fragment a une page équivalente qui
fonctionne sans JS (voir docs/plans/phase-2-api-ui.md) : le `hx-get` d'un champ ou d'un formulaire
pointe ici, son `action`/`href` pointe sur la page.
"""

from fastapi import APIRouter, Depends, Request

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
        "_directory_list.html.jinja",
        {"persons": persons, "pagination": pagination, "q": q, "legislature": legislature},
    )
