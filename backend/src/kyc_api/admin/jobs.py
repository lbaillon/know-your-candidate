"""Suivi des jobs et test de fumée `noop` (livrables 8 et 9) — voir
docs/plans/phase-3-categorisation.md, section « Suivi des jobs et test de fumée », étendue par
docs/plans/phase-4-partis-scores.md (D4.5). Aucune route publique ne crée de job (CLAUDE.md) : ce
routeur vit entièrement derrière `require_admin`.
"""

from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse

from kyc_api.admin.auth import require_admin
from kyc_api.db import Queryable, get_pool
from kyc_api.jobs import create_job, get_job, list_recent
from kyc_api.queries import admin as admin_queries
from kyc_api.schemas.admin import AdminUser
from kyc_api.templating import templates

router = APIRouter()

# Jobs d'ingestion (ingest_acteurs, ingest_scrutins, enrich_wikidata, seed_candidates, assign_slugs)
# toujours en ligne de commande (`cargo run -- enqueue ...`) : rien de plus depuis la phase 3, la
# phase 5 étendra cette liste si elle en a besoin. `recompute_scores` rejoint la liste blanche en
# phase 4 (D4.5) : rafraîchissement des scores déclenché à la main depuis le back-office, jamais
# planifié — éviter les recalculs surprises.
ALLOWED_JOB_TYPES = frozenset(
    {"noop", "label_scrutins_heuristic", "seed_themes", "refresh_views", "recompute_scores"}
)


@router.get("/jobs")
async def list_jobs(
    request: Request,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    jobs = await list_recent(pool, limit=50)
    worker_status = await admin_queries.get_worker_status(pool)
    return templates.TemplateResponse(
        request,
        "admin/jobs.html.jinja",
        {
            "admin_user": admin_user,
            "jobs": jobs,
            "worker_status": worker_status,
            "allowed_job_types": sorted(ALLOWED_JOB_TYPES),
        },
    )


@router.post("/jobs")
async def trigger_job(
    request: Request,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
) -> RedirectResponse:
    form = await request.form()
    job_type = str(form.get("type") or "").strip()
    if job_type not in ALLOWED_JOB_TYPES:
        raise HTTPException(status_code=400, detail=f"type de job non autorisé : {job_type!r}")

    job_id = await create_job(pool, type=job_type)
    await admin_queries.log_admin_action(
        pool, admin_user_id=admin_user.id, action="job_creation", target=job_type
    )
    return RedirectResponse(url=f"/admin/jobs/{job_id}", status_code=303)


@router.get("/jobs/{job_id}")
async def show_job(
    request: Request,
    job_id: int,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    job = await get_job(pool, job_id)
    if job is None:
        raise HTTPException(status_code=404)
    return templates.TemplateResponse(
        request, "admin/job.html.jinja", {"admin_user": admin_user, "job": job}
    )


@router.get("/fragments/jobs/{job_id}")
async def job_fragment(
    request: Request,
    job_id: int,
    admin_user: AdminUser = Depends(require_admin),
    pool: Queryable = Depends(get_pool),
):
    job = await get_job(pool, job_id)
    if job is None:
        raise HTTPException(status_code=404)
    return templates.TemplateResponse(
        request, "admin/_job_status.html.jinja", {"admin_user": admin_user, "job": job}
    )
