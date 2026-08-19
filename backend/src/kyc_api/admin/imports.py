"""Dépôt, validation et aperçu d'un import — voir docs/plans/phase-3-categorisation.md, section
« Import : validation, aperçu, application ». Aucune écriture de catégorisation ici : seul
`label_import` reçoit une ligne. C'est ce qui permet de relire la validation sans se demander ce
qu'elle applique (plan, « Ordre des commits », commit 8).
"""

import hashlib

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse
from starlette.datastructures import UploadFile

from kyc_api import labels_io
from kyc_api.admin.auth import require_admin
from kyc_api.db import Queryable, get_pool
from kyc_api.import_validation import build_plan, plan_to_dict
from kyc_api.queries import admin as admin_queries
from kyc_api.queries import imports as imports_queries
from kyc_api.schemas.admin import AdminUser
from kyc_api.templating import templates

router = APIRouter()


@router.get("/import")
async def import_form(request: Request, admin_user: AdminUser = Depends(require_admin)):
    return templates.TemplateResponse(
        request, "admin/import_depot.html.jinja", {"admin_user": admin_user, "errors": []}
    )


@router.post("/import")
async def deposit_import(
    request: Request,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    form = await request.form()
    upload = form.get("fichier")
    if not isinstance(upload, UploadFile) or not upload.filename:
        return _render_depot(request, admin_user, ["aucun fichier déposé"])

    raw_bytes = await upload.read()
    if len(raw_bytes) > labels_io.MAX_FILE_BYTES:
        return _render_depot(
            request,
            admin_user,
            [
                f"fichier trop volumineux : {len(raw_bytes)} octets "
                f"(maximum {labels_io.MAX_FILE_BYTES})"
            ],
        )

    try:
        # utf-8-sig : un BOM de tête (export tableur courant) est absorbé au décodage plutôt que
        # de finir dans le premier champ — voir aussi la tolérance équivalente dans
        # kyc_api.labels_io.read_csv pour un contenu déjà décodé.
        content = raw_bytes.decode("utf-8-sig")
    except UnicodeDecodeError:
        return _render_depot(request, admin_user, ["fichier illisible : encodage attendu UTF-8"])

    filename = upload.filename
    format_ = "json" if filename.lower().endswith(".json") else "csv"

    try:
        parsed = labels_io.read_json(content) if format_ == "json" else labels_io.read_csv(content)
    except labels_io.MalformedImport as err:
        return _render_depot(request, admin_user, [str(err)])

    plan = await build_plan(pool, parsed)
    if not plan.is_valid:
        return _render_depot(
            request,
            admin_user,
            [f"{problem.source} : {problem.message}" for problem in plan.problems],
            problems_total=plan.problems_total,
        )

    content_hash = hashlib.sha256(content.encode()).hexdigest()
    import_id = await imports_queries.create_label_import(
        pool,
        filename=filename,
        format=format_,
        schema_version=parsed.schema_version,
        generateur=parsed.generateur.modele if parsed.generateur else None,
        contenu=content,
        content_hash=content_hash,
        apercu=plan_to_dict(plan),
        uploaded_by=admin_user.id,
    )
    await admin_queries.log_admin_action(
        pool, admin_user_id=admin_user.id, action="import_depot", target=filename
    )
    return RedirectResponse(url=f"/admin/import/{import_id}", status_code=303)


@router.get("/import/{import_id}")
async def show_import(
    request: Request,
    import_id: int,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    record = await imports_queries.get_label_import(pool, import_id)
    if record is None:
        raise HTTPException(status_code=404)
    return templates.TemplateResponse(
        request, "admin/import_apercu.html.jinja", {"admin_user": admin_user, "record": record}
    )


def _render_depot(
    request: Request,
    admin_user: AdminUser,
    errors: list[str],
    *,
    problems_total: int | None = None,
):
    if problems_total is not None and problems_total > len(errors):
        errors = [*errors, f"… et {problems_total - len(errors)} de plus"]
    return templates.TemplateResponse(
        request,
        "admin/import_depot.html.jinja",
        {"admin_user": admin_user, "errors": errors},
        status_code=422,
    )
