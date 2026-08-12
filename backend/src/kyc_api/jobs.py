import json
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
