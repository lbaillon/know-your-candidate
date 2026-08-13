import asyncpg
from httpx import ASGITransport, AsyncClient

from kyc_api.db import get_pool
from kyc_api.main import create_app


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


class _UnreachablePool:
    """Simule une base injoignable : Postgres arrêté lève `ConnectionRefusedError`, pas
    `asyncpg.PostgresError` — voir F7, docs/plans/phase-0.1-fix.md."""

    async def fetchval(self, query: str, *args: object, timeout: float | None = None) -> object:
        raise ConnectionRefusedError("connexion refusée (simulée)")


async def test_healthz_reports_503_when_database_is_unreachable() -> None:
    app = create_app()
    app.dependency_overrides[get_pool] = lambda: _UnreachablePool()
    transport = ASGITransport(app=app)

    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        response = await ac.get("/healthz")

    assert response.status_code == 503
    body = response.json()
    assert body["database"] == "error"
    assert body["worker"] == "unknown"
    assert body["detail"] == "ConnectionRefusedError"
