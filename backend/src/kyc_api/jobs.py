import json
from datetime import datetime
from typing import Any

from pydantic import BaseModel

from kyc_api.db import Queryable


class Job(BaseModel):
    id: int
    type: str
    status: str
    progress: float | None
    progress_message: str | None
    error: str | None

    @property
    def is_finished(self) -> bool:
        return self.status in ("done", "failed", "cancelled")


class JobSummary(BaseModel):
    """Une ligne de la liste `/admin/jobs` (D3, livrable 8) : moins de champs que `Job`, pas de
    suivi de progression en direct sur cette vue-là."""

    id: int
    type: str
    status: str
    created_at: datetime
    finished_at: datetime | None


async def create_job(pool: Queryable, *, type: str, payload: dict[str, Any] | None = None) -> int:
    row = await pool.fetchrow(
        """
        INSERT INTO job (type, payload)
        VALUES ($1, $2)
        RETURNING id
        """,
        type,
        json.dumps(payload or {}),
    )
    assert row is not None
    return row["id"]


async def get_job(pool: Queryable, job_id: int) -> Job | None:
    row = await pool.fetchrow(
        """
        SELECT id, type, status, progress, progress_message, error
        FROM job
        WHERE id = $1
        """,
        job_id,
    )
    if row is None:
        return None
    return Job(**dict(row))


async def list_recent(pool: Queryable, *, limit: int = 50) -> list[JobSummary]:
    rows = await pool.fetch(
        """
        SELECT id, type, status, created_at, finished_at
        FROM job
        ORDER BY created_at DESC, id DESC
        LIMIT $1
        """,
        limit,
    )
    return [JobSummary(**dict(row)) for row in rows]
