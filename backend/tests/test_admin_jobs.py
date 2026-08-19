"""Tests du suivi des jobs et du test de fumée (livrables 8 et 9) — voir
docs/plans/phase-3-categorisation.md, section « Suivi des jobs et test de fumée ».
"""

from collections.abc import AsyncIterator

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from kyc_api.admin import auth
from kyc_api.config import settings
from kyc_api.db import get_pool


def github_transport(*, login: str = "alice", github_id: int = 1001) -> httpx.MockTransport:
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/login/oauth/access_token":
            return httpx.Response(200, json={"access_token": "test-token"})
        if request.url.path == "/user":
            return httpx.Response(200, json={"id": github_id, "login": login, "name": None})
        raise AssertionError(f"appel GitHub inattendu : {request.url}")

    return httpx.MockTransport(handler)


@pytest.fixture
async def admin_client(
    app: FastAPI, db_conn: asyncpg.Connection, monkeypatch: pytest.MonkeyPatch
) -> AsyncIterator[AsyncClient]:
    app.dependency_overrides[get_pool] = lambda: db_conn
    monkeypatch.setattr(settings, "admin_github_client_id", "test-client-id")
    monkeypatch.setattr(settings, "admin_github_client_secret", "test-client-secret")
    monkeypatch.setattr(settings, "admin_github_logins", "alice")
    monkeypatch.setattr(settings, "session_secret", "test-session-secret")
    app.dependency_overrides[auth.get_github_transport] = github_transport

    transport = ASGITransport(app=app)
    async with AsyncClient(
        transport=transport,
        base_url="http://test",
        follow_redirects=True,
        headers={"Origin": "http://test"},
    ) as ac:
        login_response = await ac.get("/admin/login", follow_redirects=False)
        state = login_response.headers["location"].split("state=")[1].split("&")[0]
        await ac.get("/admin/auth/callback", params={"code": "abc", "state": state})
        yield ac
    app.dependency_overrides.clear()


async def test_jobs_list_shows_recent_jobs_and_worker_status(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await db_conn.execute("INSERT INTO job (type) VALUES ('noop')")
    response = await admin_client.get("/admin/jobs")
    assert response.status_code == 200
    assert b"noop" in response.content
    assert b"unknown" in response.content  # aucun battement de coeur dans ce test


async def test_triggering_an_allowed_job_type_creates_it_and_redirects(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    response = await admin_client.post("/admin/jobs", data={"type": "noop"}, follow_redirects=False)
    assert response.status_code == 303
    assert response.headers["location"].startswith("/admin/jobs/")

    job = await db_conn.fetchrow("SELECT type, status FROM job WHERE type = 'noop'")
    assert job is not None
    assert job["status"] == "pending"

    action = await db_conn.fetchrow("SELECT target FROM admin_action WHERE action = 'job_creation'")
    assert action is not None
    assert action["target"] == "noop"


async def test_triggering_a_disallowed_job_type_is_rejected(admin_client: AsyncClient):
    response = await admin_client.post(
        "/admin/jobs", data={"type": "ingest_scrutins"}, follow_redirects=False
    )
    assert response.status_code == 400


async def test_show_job_returns_404_for_unknown_job(admin_client: AsyncClient):
    response = await admin_client.get("/admin/jobs/999999")
    assert response.status_code == 404


async def test_show_job_renders_its_status(admin_client: AsyncClient, db_conn: asyncpg.Connection):
    job_id = await db_conn.fetchval("INSERT INTO job (type) VALUES ('noop') RETURNING id")
    response = await admin_client.get(f"/admin/jobs/{job_id}")
    assert response.status_code == 200
    assert b"pending" in response.content


async def test_job_fragment_returns_the_status_partial(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    job_id = await db_conn.fetchval("INSERT INTO job (type) VALUES ('noop') RETURNING id")
    response = await admin_client.get(f"/admin/fragments/jobs/{job_id}")
    assert response.status_code == 200
    assert b'id="job-status"' in response.content
    assert b"<html" not in response.content  # un fragment, pas une page complète


async def test_a_finished_job_fragment_does_not_carry_hx_trigger(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    job_id = await db_conn.fetchval(
        "INSERT INTO job (type, status) VALUES ('noop', 'done') RETURNING id"
    )
    response = await admin_client.get(f"/admin/fragments/jobs/{job_id}")
    assert response.status_code == 200
    assert b"hx-trigger" not in response.content
