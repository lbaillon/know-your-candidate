import asyncpg

from kyc_api.jobs import create_job, get_job


async def test_create_job_inserts_a_pending_row(db_conn: asyncpg.Connection) -> None:
    job_id = await create_job(db_conn, type="noop", payload={"seconds": 1})

    row = await db_conn.fetchrow("SELECT type, status, payload FROM job WHERE id = $1", job_id)

    assert row is not None
    assert row["type"] == "noop"
    assert row["status"] == "pending"


async def test_get_job_returns_none_for_unknown_id(db_conn: asyncpg.Connection) -> None:
    job = await get_job(db_conn, 999_999)

    assert job is None


async def test_get_job_reflects_row_state(db_conn: asyncpg.Connection) -> None:
    job_id = await create_job(db_conn, type="noop")

    job = await get_job(db_conn, job_id)

    assert job is not None
    assert job.id == job_id
    assert job.status == "pending"
    assert not job.is_finished


# F11 (docs/plans/phase-1.1-fix.md) : la route qui créait un job depuis une requête HTTP non
# authentifiée a été supprimée (CLAUDE.md, « aucune route publique ne crée de job »). Ce que ce
# test vérifiait — un job créé se relit avec le bon état — se vérifie désormais en insérant
# directement en base plutôt qu'en passant par HTTP.
async def test_a_job_created_directly_can_be_read_back_once_done(
    db_conn: asyncpg.Connection,
) -> None:
    job_id = await create_job(db_conn, type="noop", payload={"seconds": 1})
    await db_conn.execute("UPDATE job SET status = 'done' WHERE id = $1", job_id)

    job = await get_job(db_conn, job_id)

    assert job is not None
    assert job.status == "done"
    assert job.is_finished
