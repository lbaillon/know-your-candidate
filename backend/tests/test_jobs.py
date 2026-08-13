import asyncpg
from httpx import ASGITransport, AsyncClient

from kyc_api.config import settings
from kyc_api.jobs import create_job, get_job
from kyc_api.main import create_app


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


async def test_dev_routes_are_404_when_disabled() -> None:
    # F9 — voir docs/plans/phase-0.1-fix.md : POST /dev/jobs n'est pas authentifiée, donc son
    # montage doit rester conditionné à un drapeau explicite, faux par défaut.
    original = settings.enable_dev_routes
    settings.enable_dev_routes = False
    try:
        app = create_app()
        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as ac:
            response = await ac.post("/dev/jobs")
    finally:
        settings.enable_dev_routes = original

    assert response.status_code == 404


async def test_homepage_survives_dev_routes_being_disabled() -> None:
    # La page d'accueil vivait sur le même routeur que les routes de démonstration : l'éteindre
    # la faisait disparaître avec elles, donc 404 sur `/` en production. Une page publique ne
    # vit jamais sur un routeur qu'on conditionne.
    original = settings.enable_dev_routes
    settings.enable_dev_routes = False
    try:
        app = create_app()
        transport = ASGITransport(app=app)
        async with AsyncClient(transport=transport, base_url="http://test") as ac:
            response = await ac.get("/")
    finally:
        settings.enable_dev_routes = original

    assert response.status_code == 200
    # Le bouton de démonstration ne doit pas être proposé quand la route qu'il vise n'existe pas.
    assert "/dev/jobs" not in response.text
