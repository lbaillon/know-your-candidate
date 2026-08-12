from datetime import UTC, datetime

import asyncpg
from fastapi import APIRouter, Depends
from fastapi.responses import JSONResponse

from kyc_api.config import settings
from kyc_api.db import Queryable, get_pool

router = APIRouter()


@router.get("/healthz")
async def healthz(pool: Queryable = Depends(get_pool)) -> JSONResponse:
    try:
        last_seen_at = await pool.fetchval("SELECT max(last_seen_at) FROM worker_heartbeat")
    except asyncpg.PostgresError:
        return JSONResponse(status_code=503, content={"database": "error", "worker": "unknown"})

    if last_seen_at is None:
        worker_status = "unknown"
    else:
        assert isinstance(last_seen_at, datetime)
        age = (datetime.now(UTC) - last_seen_at).total_seconds()
        worker_status = "ok" if age <= settings.worker_stale_after_seconds else "stale"

    return JSONResponse(status_code=200, content={"database": "ok", "worker": worker_status})
