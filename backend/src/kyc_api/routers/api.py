"""API JSON `/api/v1/` — miroir des pages, servi par les **mêmes fonctions de `queries/`** (D2.10) :
la page et l'API ne peuvent pas diverger puisqu'elles lisent la donnée par le même chemin. Chaque
objet renvoyé porte son bloc `source` (déjà un champ de `PersonDetail`/`ScrutinDetail`) ; chaque
liste porte un bloc `pagination`.
"""

from datetime import date

from fastapi import APIRouter, Depends, HTTPException
from fastapi.responses import RedirectResponse

from kyc_api.cursor import InvalidCursor, parse_cursor
from kyc_api.db import Queryable, get_pool
from kyc_api.queries import candidates as candidates_queries
from kyc_api.queries import persons as persons_queries
from kyc_api.queries import scores as scores_queries
from kyc_api.queries import scrutins as scrutins_queries

router = APIRouter(prefix="/api/v1")


@router.get("/candidats")
async def candidats(q: str | None = None, pool: Queryable = Depends(get_pool)):
    candidates = await candidates_queries.list_candidates(pool, q=q)
    return {"data": candidates}


@router.get("/personnes")
async def personnes(
    q: str | None = None,
    legislature: int | None = None,
    page: int = 1,
    pool: Queryable = Depends(get_pool),
):
    persons, pagination = await persons_queries.list_directory(
        pool, q=q, legislature=legislature, page=page
    )
    return {"data": persons, "pagination": pagination}


@router.get("/personnes/{slug}")
async def personne_detail(slug: str, pool: Queryable = Depends(get_pool)):
    resolution = await persons_queries.resolve_slug(pool, slug)
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        return RedirectResponse(url=f"/api/v1/personnes/{resolution.current_slug}", status_code=301)

    person = await persons_queries.get_person_detail(pool, resolution.person_id)
    if person is None:
        raise HTTPException(status_code=404)
    return person


@router.get("/personnes/{slug}/votes")
async def personne_votes(
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
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        return RedirectResponse(
            url=f"/api/v1/personnes/{resolution.current_slug}/votes", status_code=301
        )

    try:
        cursor = parse_cursor(avant)
    except InvalidCursor as exc:
        # Différent de la page (D2.5/D2.8) : un client HTML tape rarement une URL à la main, un
        # consommateur d'API si, et il a besoin de savoir que sa requête est fautive plutôt que de
        # recevoir silencieusement la première page.
        raise HTTPException(status_code=400, detail="curseur invalide") from exc

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
    return {"data": votes_page.votes, "pagination": {"next_cursor": votes_page.next_cursor}}


@router.get("/personnes/{slug}/scores")
async def personne_scores(slug: str, pool: Queryable = Depends(get_pool)):
    """Mêmes valeurs que le bloc « Orientations » de la fiche (D2.10, D4) : une ligne par thème
    éligible du run courant, score `null` sous le seuil de contributions (D4.2).
    """
    resolution = await persons_queries.resolve_slug(pool, slug)
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        return RedirectResponse(
            url=f"/api/v1/personnes/{resolution.current_slug}/scores", status_code=301
        )

    orientations = await scores_queries.get_person_orientations(pool, resolution.person_id)
    return {"data": orientations}


@router.get("/personnes/{slug}/themes/{theme_slug}/contributions")
async def personne_theme_contributions(
    slug: str, theme_slug: str, pool: Queryable = Depends(get_pool)
):
    """Mêmes valeurs que la page d'explication (D4, « L'explication est le produit ») : toutes
    les contributions, jamais tronquées. 404 si le thème n'est pas éligible dans le run courant.
    """
    resolution = await persons_queries.resolve_slug(pool, slug)
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        return RedirectResponse(
            url=f"/api/v1/personnes/{resolution.current_slug}/themes/{theme_slug}/contributions",
            status_code=301,
        )

    orientations = await scores_queries.get_person_orientations(pool, resolution.person_id)
    orientation = next((o for o in orientations if o.theme.slug == theme_slug), None)
    if orientation is None:
        raise HTTPException(status_code=404)

    contributions = await scores_queries.get_person_theme_contributions(
        pool, resolution.person_id, orientation.theme.id
    )
    return {"data": contributions}


@router.get("/scrutins/{legislature}/{numero}")
async def scrutin_detail(legislature: int, numero: int, pool: Queryable = Depends(get_pool)):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)
    return scrutin
