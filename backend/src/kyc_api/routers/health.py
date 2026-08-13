from datetime import UTC, datetime

from fastapi import APIRouter, Depends
from fastapi.responses import JSONResponse

from kyc_api.config import settings
from kyc_api.db import Queryable, get_pool

router = APIRouter()


@router.get("/healthz")
async def healthz(pool: Queryable = Depends(get_pool)) -> JSONResponse:
    try:
        last_seen_at = await pool.fetchval("SELECT max(last_seen_at) FROM worker_heartbeat")
    except Exception as exc:
        # Capture large et volontaire : ce endpoint existe précisément pour signaler que la base
        # est injoignable, et Postgres arrêté lève ConnectionRefusedError / asyncpg.InterfaceError,
        # pas seulement asyncpg.PostgresError. Un point de santé qui plante n'a aucune valeur —
        # c'est l'un des rares endroits où une capture large est le comportement correct (voir F7,
        # docs/plans/phase-0.1-fix.md).
        return JSONResponse(
            status_code=503,
            content={"database": "error", "worker": "unknown", "detail": type(exc).__name__},
        )

    if last_seen_at is None:
        worker_status = "unknown"
    else:
        assert isinstance(last_seen_at, datetime)
        age = (datetime.now(UTC) - last_seen_at).total_seconds()
        worker_status = "ok" if age <= settings.worker_stale_after_seconds else "stale"

    return JSONResponse(status_code=200, content={"database": "ok", "worker": worker_status})
