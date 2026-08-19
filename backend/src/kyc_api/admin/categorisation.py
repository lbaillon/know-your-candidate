"""File de travail et formulaire de catégorisation — voir docs/plans/phase-3-categorisation.md,
section « Back-office : la file de travail ». Routes montées sous le routeur protégé de
`kyc_api.admin` (garde d'authentification déjà posée là, pas ici).
"""

from decimal import Decimal, InvalidOperation

from fastapi import APIRouter, Depends, HTTPException, Query, Request
from fastapi.responses import RedirectResponse
from starlette.datastructures import FormData

from kyc_api.admin.auth import require_admin
from kyc_api.db import Queryable, WritableQueryable, get_connection, get_pool
from kyc_api.queries import labels as labels_queries
from kyc_api.queries import scrutins as scrutins_queries
from kyc_api.queries import themes as themes_queries
from kyc_api.queries.labels import LabelWrite, to_numeric_4_3
from kyc_api.schemas.admin import AdminUser
from kyc_api.schemas.theme import Theme
from kyc_api.templating import templates

router = APIRouter()

_SESSION_SKIPPED_KEY = "categorisation_passees"
_JUSTIFICATION_MIN_LENGTH = 10


@router.get("/categorisation")
async def next_categorisation(
    request: Request,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    skipped = request.session.get(_SESSION_SKIPPED_KEY, [])
    location = await labels_queries.get_next_to_categorize(pool, excluded_scrutin_ids=skipped)
    if location is None:
        return templates.TemplateResponse(
            request, "admin/categorisation_vide.html.jinja", {"admin_user": admin_user}
        )
    legislature, numero = location
    return await _render_form(request, pool, legislature, numero, admin_user)


@router.get("/categorisation/{legislature}/{numero}")
async def show_categorisation(
    request: Request,
    legislature: int,
    numero: int,
    deuxieme_theme: bool = Query(False),
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    return await _render_form(
        request, pool, legislature, numero, admin_user, force_second_theme=deuxieme_theme
    )


@router.post("/categorisation/{legislature}/{numero}")
async def submit_categorisation(
    request: Request,
    legislature: int,
    numero: int,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
    conn: WritableQueryable = Depends(get_connection),
):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)
    themes = await themes_queries.list_active(pool)
    themes_by_slug = {theme.slug: theme for theme in themes}

    form = await request.form()
    slot_count = 2 if str(form.get("theme_2") or "").strip() else 1

    entries, errors = _parse_entries(form, slot_count, themes_by_slug)

    if entries and len({e.theme_slug for e in entries}) != len(entries):
        errors.append("le même thème ne peut pas être sélectionné deux fois")

    total_poids = sum((e.poids for e in entries), start=Decimal("0"))
    if entries and total_poids != Decimal("1.000"):
        errors.append(f"la somme des poids doit valoir exactement 1 (obtenu {total_poids})")

    if not entries and not errors:
        errors.append("sélectionner au moins un thème")

    if errors:
        return await _render_form(
            request,
            pool,
            legislature,
            numero,
            admin_user,
            force_second_theme=slot_count == 2,
            errors=errors,
            form=form,
        )

    await labels_queries.replace_labels(
        conn,
        scrutin_id=scrutin.scrutin_id,
        entries=entries,
        method="manual",
        author_id=admin_user.id,
    )
    return RedirectResponse(url="/admin/categorisation", status_code=303)


@router.post("/categorisation/{legislature}/{numero}/supprimer")
async def delete_categorisation(
    request: Request,
    legislature: int,
    numero: int,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
    conn: WritableQueryable = Depends(get_connection),
):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)

    form = await request.form()
    motif = str(form.get("motif") or "").strip()
    if not motif:
        return await _render_form(
            request,
            pool,
            legislature,
            numero,
            admin_user,
            errors=["un motif est obligatoire pour retirer une catégorisation"],
        )

    await labels_queries.replace_labels(
        conn,
        scrutin_id=scrutin.scrutin_id,
        entries=[],
        method="manual",
        author_id=admin_user.id,
        motif=motif,
    )
    return RedirectResponse(url=f"/admin/categorisation/{legislature}/{numero}", status_code=303)


@router.post("/categorisation/passer")
async def skip_categorisation(
    request: Request, admin_user: AdminUser = Depends(require_admin)
) -> RedirectResponse:
    form = await request.form()
    scrutin_id_raw = form.get("scrutin_id")
    if scrutin_id_raw:
        skipped = list(request.session.get(_SESSION_SKIPPED_KEY, []))
        skipped.append(int(str(scrutin_id_raw)))
        request.session[_SESSION_SKIPPED_KEY] = skipped
    return RedirectResponse(url="/admin/categorisation", status_code=303)


def _parse_entries(
    form: FormData, slot_count: int, themes_by_slug: dict[str, Theme]
) -> tuple[list[LabelWrite], list[str]]:
    entries: list[LabelWrite] = []
    errors: list[str] = []

    for i in range(1, slot_count + 1):
        theme_slug = str(form.get(f"theme_{i}") or "").strip()
        theme = themes_by_slug.get(theme_slug)
        if theme is None:
            errors.append(f"thème {i} : sélection invalide")
            continue

        try:
            confiance = to_numeric_4_3(str(form.get(f"confiance_{i}") or ""))
        except InvalidOperation:
            errors.append(f"thème {i} : confiance invalide")
            continue

        justification = str(form.get(f"justification_{i}") or "").strip()
        if len(justification) < _JUSTIFICATION_MIN_LENGTH:
            errors.append(f"thème {i} : justification trop courte (dix caractères minimum)")
            continue

        position_pour = None
        if theme.has_axis:
            try:
                position_pour = to_numeric_4_3(str(form.get(f"position_{i}") or ""))
            except InvalidOperation:
                errors.append(f"thème {i} : position invalide")
                continue
            if not (Decimal("-1") <= position_pour <= Decimal("1")):
                errors.append(f"thème {i} : position hors bornes")
                continue

        if slot_count == 1:
            poids = Decimal("1.000")
        else:
            try:
                poids = to_numeric_4_3(str(form.get(f"poids_{i}") or ""))
            except InvalidOperation:
                errors.append(f"thème {i} : poids invalide")
                continue
            if not (Decimal("0") < poids <= Decimal("1")):
                errors.append(f"thème {i} : poids hors bornes")
                continue

        entries.append(
            LabelWrite(
                theme_id=theme.id,
                theme_slug=theme.slug,
                poids=poids,
                position_pour=position_pour,
                confiance=confiance,
                justification=justification,
            )
        )

    return entries, errors


_EMPTY_SLOT = {"theme": "", "position": "", "confiance": "", "justification": "", "poids": ""}


def _slot_values(
    labels: list, form: FormData | None, estimate, *, want_two: bool
) -> list[dict[str, str]]:
    """Une entrée par emplacement de thème du formulaire (1 ou 2, D3.17). Priorité aux valeurs
    postées (réaffichage après erreur) sur les valeurs déjà en base (édition d'une catégorisation
    existante) sur un emplacement vide (nouveau scrutin) — dans cet ordre.
    """
    slots: list[dict[str, str]] = []
    if form is not None:
        slot_count = 2 if str(form.get("theme_2") or "").strip() else 1
        for i in range(1, slot_count + 1):
            slots.append(
                {
                    "theme": str(form.get(f"theme_{i}") or ""),
                    "position": str(form.get(f"position_{i}") or ""),
                    "confiance": str(form.get(f"confiance_{i}") or ""),
                    "justification": str(form.get(f"justification_{i}") or ""),
                    "poids": str(form.get(f"poids_{i}") or ""),
                }
            )
    else:
        for label in labels[:2]:
            slots.append(
                {
                    "theme": label.theme_slug,
                    "position": f"{label.position_pour:.3f}"
                    if label.position_pour is not None
                    else "",
                    "confiance": f"{label.confiance:.3f}",
                    "justification": label.justification,
                    "poids": f"{label.poids:.3f}",
                }
            )

    while len(slots) < (2 if want_two else 1):
        slots.append(dict(_EMPTY_SLOT))

    # Pré-remplissage par la mesure automatique (D3.7, D3.8) : seulement une case encore vide,
    # jamais une position déjà saisie ou déjà en base — et jamais le thème, la machine n'en
    # connaît aucun.
    if estimate is not None:
        for slot in slots:
            if not slot["theme"] and not slot["position"]:
                slot["position"] = f"{estimate.position_pour:.3f}"

    return slots


async def _render_form(
    request: Request,
    pool: Queryable,
    legislature: int,
    numero: int,
    admin_user: AdminUser,
    *,
    force_second_theme: bool = False,
    errors: list[str] | None = None,
    form: FormData | None = None,
):
    scrutin = await scrutins_queries.get_by_legislature_numero(pool, legislature, numero)
    if scrutin is None:
        raise HTTPException(status_code=404)

    themes = await themes_queries.list_active(pool)
    labels = await labels_queries.get_labels_for_scrutin(pool, scrutin.scrutin_id)
    estimate = await labels_queries.get_current_estimate(pool, scrutin.scrutin_id)
    revisions = await labels_queries.get_revisions_for_scrutin(pool, scrutin.scrutin_id)
    show_second_theme = force_second_theme or len(labels) >= 2

    return templates.TemplateResponse(
        request,
        "admin/categorisation.html.jinja",
        {
            "admin_user": admin_user,
            "scrutin": scrutin,
            "themes": themes,
            "labels": labels,
            "estimate": estimate,
            "revisions": revisions,
            "show_second_theme": show_second_theme,
            "slots": _slot_values(labels, form, estimate, want_two=show_second_theme),
            "errors": errors or [],
        },
    )
