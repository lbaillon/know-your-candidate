import asyncpg
from httpx import AsyncClient


async def test_healthz_reports_worker_unknown_without_heartbeat(client: AsyncClient) -> None:
    response = await client.get("/healthz")

    assert response.status_code == 200
    assert response.json() == {"database": "ok", "worker": "unknown"}


async def test_healthz_reports_worker_ok_with_fresh_heartbeat(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await db_conn.execute(
        """
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at)
        VALUES ('test-worker', '0.1.0', now())
        """
    )

    response = await client.get("/healthz")

    assert response.json() == {"database": "ok", "worker": "ok"}


async def test_healthz_reports_worker_stale_with_old_heartbeat(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await db_conn.execute(
        """
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at)
        VALUES ('test-worker', '0.1.0', now() - interval '5 minutes')
        """
    )

    response = await client.get("/healthz")

    assert response.status_code == 200
    assert response.json() == {"database": "ok", "worker": "stale"}
