"""Pages publiques, toujours montées."""

from datetime import date

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse

from kyc_api.cursor import InvalidCursor, parse_cursor
from kyc_api.db import Queryable, get_pool
from kyc_api.documents import render_document
from kyc_api.queries import candidates as candidates_queries
from kyc_api.queries import labels as labels_queries
from kyc_api.queries import persons as persons_queries
from kyc_api.queries import scores as scores_queries
from kyc_api.queries import scrutins as scrutins_queries
from kyc_api.queries import themes as themes_queries
from kyc_api.templating import templates
from kyc_api.timeline import build_timeline

router = APIRouter()


@router.get("/")
async def index(request: Request, q: str | None = None, pool: Queryable = Depends(get_pool)):
    candidates = await candidates_queries.list_candidates(pool, q=q)
    return templates.TemplateResponse(
        request, "home.html.jinja", {"candidates": candidates, "q": q}
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
        "directory.html.jinja",
        {"persons": persons, "pagination": pagination, "q": q, "legislature": legislature},
    )


@router.get("/personne/{slug}")
async def person_detail(request: Request, slug: str, pool: Queryable = Depends(get_pool)):
    resolution = await persons_queries.resolve_slug(pool, slug)
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        # 301 et non 302 : le slug ancien est réputé définitif (voir plan d'exécution, « Routes
        # exactes »), pas un simple aléa temporaire.
        return RedirectResponse(url=f"/personne/{resolution.current_slug}", status_code=301)

    person = await persons_queries.get_person_detail(pool, resolution.person_id)
    if person is None:
        raise HTTPException(status_code=404)

    segments = await persons_queries.get_timeline_segments(pool, resolution.person_id)
    timeline = build_timeline(segments, today=date.today())
    recent_votes = await persons_queries.get_recent_votes(pool, resolution.person_id)
    orientations = await scores_queries.get_person_orientations(pool, resolution.person_id)

    return templates.TemplateResponse(
        request,
        "person.html.jinja",
        {
            "person": person,
            "timeline": timeline,
            "recent_votes": recent_votes,
            "orientations": orientations,
            # La personne a-t-elle jamais siégé (fiche vide "jamais vu de vote") ou a-t-elle des
            # votes qui, simplement, ne portent pas encore sur un scrutin catégorisé ? Les deux
            # cas exigent une phrase différente (plan phase 4, cas Retailleau) : `recent_votes`
            # n'est pas tronqué à zéro, seulement à dix, donc sa non-vacuité suffit à trancher.
            "a_des_votes": len(recent_votes) > 0,
        },
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
    if resolution is None:
        raise HTTPException(status_code=404)
    if not resolution.is_current:
        return RedirectResponse(url=f"/personne/{resolution.current_slug}/votes", status_code=301)

    person = await persons_queries.get_person_detail(pool, resolution.person_id)
    if person is None:
        raise HTTPException(status_code=404)

    try:
        cursor = parse_cursor(avant)
    except InvalidCursor:
        # Un curseur corrompu n'a aucune raison de faire échouer une page publique : on dégrade
        # silencieusement vers la première page plutôt que de rendre une erreur.
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
        "person_votes.html.jinja",
        {
            "person": person,
            "slug": person.slug,
            "votes": votes_page.votes,
            "next_cursor": votes_page.next_cursor,
            "filter_qs": filter_qs,
            "legislature": legislature,
            "position": position,
            "du": du,
            "au": au,
        },
    )


@router.get("/scrutin/{legislature}/{numero}")
async def scrutin_detail(
    request: Request, legislature: int, numero: int, pool: Queryable = Depends(get_pool)
):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)
    labels = await labels_queries.get_labels_for_scrutin(pool, scrutin.scrutin_id)
    return templates.TemplateResponse(
        request, "scrutin.html.jinja", {"scrutin": scrutin, "labels": labels}
    )


@router.get("/scrutin/{legislature}/{numero}/categorisation")
async def scrutin_categorisation(
    request: Request, legislature: int, numero: int, pool: Queryable = Depends(get_pool)
):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)
    labels = await labels_queries.get_labels_for_scrutin(pool, scrutin.scrutin_id)
    revisions = await labels_queries.get_revisions_for_scrutin(pool, scrutin.scrutin_id)
    if not labels and not revisions:
        # Rien à détailler : cette page n'est atteignable que depuis un lien affiché sur une
        # catégorisation existante (plan, « Pages publiques »), jamais devinable pour un scrutin
        # qui n'en a jamais eu.
        raise HTTPException(status_code=404)
    return templates.TemplateResponse(
        request,
        "categorisation.html.jinja",
        {"scrutin": scrutin, "labels": labels, "revisions": revisions},
    )


@router.get("/scrutin/{an_uid}")
async def scrutin_alias(request: Request, an_uid: str, pool: Queryable = Depends(get_pool)):
    scrutin = await scrutins_queries.get_by_an_uid(pool, an_uid)
    if scrutin is None:
        raise HTTPException(status_code=404)
    # 301 : alias -> URL canonique, sur le même modèle qu'un ancien slug de personne.
    return RedirectResponse(url=f"/scrutin/{scrutin.legislature}/{scrutin.numero}", status_code=301)


@router.get("/themes")
async def themes(request: Request, pool: Queryable = Depends(get_pool)):
    theme_list = await themes_queries.list_active(pool)
    return templates.TemplateResponse(request, "themes.html.jinja", {"themes": theme_list})


@router.get("/theme/{slug}")
async def theme_detail(
    request: Request,
    slug: str,
    legislature: int | None = None,
    page: int = 1,
    pool: Queryable = Depends(get_pool),
):
    theme = await themes_queries.get_by_slug(pool, slug)
    if theme is None:
        raise HTTPException(status_code=404)
    scrutins, pagination = await labels_queries.list_categorized_scrutins(
        pool, theme_slug=slug, legislature=legislature, page=page
    )
    return templates.TemplateResponse(
        request,
        "theme.html.jinja",
        {
            "theme": theme,
            "scrutins": scrutins,
            "pagination": pagination,
            "legislature": legislature,
        },
    )


@router.get("/scrutins")
async def scrutins_list(
    request: Request,
    theme: str | None = None,
    legislature: int | None = None,
    page: int = 1,
    pool: Queryable = Depends(get_pool),
):
    theme_list = await themes_queries.list_active(pool)
    scrutins, pagination = await labels_queries.list_categorized_scrutins(
        pool, theme_slug=theme, legislature=legislature, page=page
    )
    return templates.TemplateResponse(
        request,
        "scrutins.html.jinja",
        {
            "scrutins": scrutins,
            "pagination": pagination,
            "themes": theme_list,
            "theme": theme,
            "legislature": legislature,
        },
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
