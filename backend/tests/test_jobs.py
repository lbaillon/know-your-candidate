import asyncpg
from httpx import AsyncClient

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


async def test_demo_job_route_creates_job_and_redirects_to_its_page(
    client: AsyncClient,
) -> None:
    response = await client.post("/dev/jobs")

    assert response.status_code == 200  # httpx suit la redirection 303 par défaut
    assert "Job de démonstration" in response.text


async def test_demo_job_fragment_polls_while_pending(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    job_id = await create_job(db_conn, type="noop")

    response = await client.get(f"/fragments/dev/jobs/{job_id}")

    assert response.status_code == 200
    assert 'hx-trigger="every 2s"' in response.text


async def test_demo_job_fragment_stops_polling_once_done(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    job_id = await create_job(db_conn, type="noop")
    await db_conn.execute("UPDATE job SET status = 'done' WHERE id = $1", job_id)

    response = await client.get(f"/fragments/dev/jobs/{job_id}")

    assert response.status_code == 200
    assert "hx-trigger" not in response.text


async def test_demo_job_page_404s_for_unknown_id(client: AsyncClient) -> None:
    response = await client.get("/dev/jobs/999999")

    assert response.status_code == 404
