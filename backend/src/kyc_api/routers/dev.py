from fastapi import APIRouter, Depends, HTTPException, Request
from fastapi.responses import RedirectResponse

from kyc_api.db import Queryable, get_pool
from kyc_api.jobs import create_job, get_job
from kyc_api.templating import templates

router = APIRouter()
fragments_router = APIRouter(prefix="/fragments")


@router.post("/dev/jobs")
async def create_demo_job(pool: Queryable = Depends(get_pool)) -> RedirectResponse:
    job_id = await create_job(pool, type="noop", payload={"seconds": 3})
    return RedirectResponse(url=f"/dev/jobs/{job_id}", status_code=303)


@router.get("/dev/jobs/{job_id}")
async def show_demo_job(request: Request, job_id: int, pool: Queryable = Depends(get_pool)):
    job = await get_job(pool, job_id)
    if job is None:
        raise HTTPException(status_code=404, detail="Job introuvable")
    return templates.TemplateResponse(request, "dev_job.html.jinja", {"job": job})


@fragments_router.get("/dev/jobs/{job_id}")
async def demo_job_fragment(request: Request, job_id: int, pool: Queryable = Depends(get_pool)):
    job = await get_job(pool, job_id)
    if job is None:
        raise HTTPException(status_code=404, detail="Job introuvable")
    return templates.TemplateResponse(request, "_job_status.html.jinja", {"job": job})
