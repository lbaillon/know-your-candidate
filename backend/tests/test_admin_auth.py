"""Tests de l'authentification admin — voir docs/plans/phase-3-categorisation.md, section
« Authentification admin (D3.1) ». GitHub est simulé par un `httpx.MockTransport` injecté via
`app.dependency_overrides` : jamais de réseau ici.
"""

import json
from collections.abc import AsyncIterator

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from kyc_api.admin import auth
from kyc_api.config import settings
from kyc_api.db import get_pool


def configure_admin(monkeypatch: pytest.MonkeyPatch, *, logins: str = "alice") -> None:
    monkeypatch.setattr(settings, "admin_github_client_id", "test-client-id")
    monkeypatch.setattr(settings, "admin_github_client_secret", "test-client-secret")
    monkeypatch.setattr(settings, "admin_github_logins", logins)
    monkeypatch.setattr(settings, "session_secret", "test-session-secret")


def github_transport(*, login: str = "alice", github_id: int = 1001) -> httpx.MockTransport:
    def handler(request: httpx.Request) -> httpx.Response:
        if request.url.path == "/login/oauth/access_token":
            return httpx.Response(200, json={"access_token": "test-token"})
        if request.url.path == "/user":
            return httpx.Response(200, json={"id": github_id, "login": login, "name": None})
        raise AssertionError(f"appel GitHub inattendu : {request.url}")

    return httpx.MockTransport(handler)


@pytest.fixture
async def admin_client(app: FastAPI, db_conn: asyncpg.Connection) -> AsyncIterator[AsyncClient]:
    app.dependency_overrides[get_pool] = lambda: db_conn
    transport = ASGITransport(app=app)
    async with AsyncClient(
        transport=transport, base_url="http://test", follow_redirects=False
    ) as ac:
        yield ac
    app.dependency_overrides.clear()


async def test_login_returns_503_when_oauth_is_not_configured(admin_client: AsyncClient):
    response = await admin_client.get("/admin/login")
    assert response.status_code == 503


async def test_login_redirects_to_github_with_a_state_and_no_scope(
    admin_client: AsyncClient, monkeypatch: pytest.MonkeyPatch
):
    configure_admin(monkeypatch)
    response = await admin_client.get("/admin/login")
    assert response.status_code in (302, 307)
    location = response.headers["location"]
    assert location.startswith("https://github.com/login/oauth/authorize?")
    assert "scope=" not in location
    assert "state=" in location


async def test_callback_rejects_a_mismatched_state(
    admin_client: AsyncClient, monkeypatch: pytest.MonkeyPatch, app: FastAPI
):
    configure_admin(monkeypatch)
    app.dependency_overrides[auth.get_github_transport] = github_transport
    await admin_client.get("/admin/login")  # pose oauth_state en session

    response = await admin_client.get(
        "/admin/auth/callback", params={"code": "abc", "state": "faux-state"}
    )
    assert response.status_code == 400


async def test_callback_refuses_a_login_outside_the_allowlist(
    admin_client: AsyncClient,
    monkeypatch: pytest.MonkeyPatch,
    app: FastAPI,
    db_conn: asyncpg.Connection,
):
    configure_admin(monkeypatch, logins="alice")
    app.dependency_overrides[auth.get_github_transport] = lambda: github_transport(login="mallory")

    login_response = await admin_client.get("/admin/login")
    state = login_response.headers["location"].split("state=")[1].split("&")[0]

    response = await admin_client.get(
        "/admin/auth/callback", params={"code": "abc", "state": state}
    )
    assert response.status_code == 403

    action = await db_conn.fetchrow(
        "SELECT action, admin_user_id, detail FROM admin_action WHERE action = 'login_refuse'"
    )
    assert action is not None
    assert action["admin_user_id"] is None
    assert json.loads(action["detail"])["login"] == "mallory"


async def test_callback_accepts_a_login_in_the_allowlist_case_insensitively(
    admin_client: AsyncClient,
    monkeypatch: pytest.MonkeyPatch,
    app: FastAPI,
    db_conn: asyncpg.Connection,
):
    configure_admin(monkeypatch, logins="Alice, bob")
    app.dependency_overrides[auth.get_github_transport] = lambda: github_transport(login="alice")

    login_response = await admin_client.get("/admin/login")
    state = login_response.headers["location"].split("state=")[1].split("&")[0]

    response = await admin_client.get(
        "/admin/auth/callback", params={"code": "abc", "state": state}
    )
    assert response.status_code == 303
    assert response.headers["location"] == "/admin"

    admin_user = await db_conn.fetchrow("SELECT github_login, actif FROM admin_user")
    assert admin_user is not None
    assert admin_user["github_login"] == "alice"
    assert admin_user["actif"] is True

    action = await db_conn.fetchrow("SELECT action FROM admin_action WHERE action = 'login'")
    assert action is not None


async def test_dashboard_redirects_to_login_when_not_authenticated(admin_client: AsyncClient):
    response = await admin_client.get("/admin")
    assert response.status_code == 303
    assert response.headers["location"] == "/admin/login"


async def test_dashboard_returns_401_with_hx_redirect_for_an_htmx_request(
    admin_client: AsyncClient,
):
    response = await admin_client.get("/admin", headers={"HX-Request": "true"})
    assert response.status_code == 401
    assert response.headers["HX-Redirect"] == "/admin/login"


async def test_full_login_flow_then_dashboard_access(
    admin_client: AsyncClient, monkeypatch: pytest.MonkeyPatch, app: FastAPI
):
    configure_admin(monkeypatch)
    app.dependency_overrides[auth.get_github_transport] = github_transport

    login_response = await admin_client.get("/admin/login")
    state = login_response.headers["location"].split("state=")[1].split("&")[0]
    await admin_client.get("/admin/auth/callback", params={"code": "abc", "state": state})

    response = await admin_client.get("/admin")
    assert response.status_code == 200
    assert b"alice" in response.content


async def test_a_deactivated_admin_loses_access_immediately(
    admin_client: AsyncClient,
    monkeypatch: pytest.MonkeyPatch,
    app: FastAPI,
    db_conn: asyncpg.Connection,
):
    configure_admin(monkeypatch)
    app.dependency_overrides[auth.get_github_transport] = github_transport

    login_response = await admin_client.get("/admin/login")
    state = login_response.headers["location"].split("state=")[1].split("&")[0]
    await admin_client.get("/admin/auth/callback", params={"code": "abc", "state": state})

    await db_conn.execute("UPDATE admin_user SET actif = false WHERE github_login = 'alice'")

    response = await admin_client.get("/admin")
    assert response.status_code == 303
    assert response.headers["location"] == "/admin/login"


async def test_logout_clears_the_session_and_is_logged(
    admin_client: AsyncClient,
    monkeypatch: pytest.MonkeyPatch,
    app: FastAPI,
    db_conn: asyncpg.Connection,
):
    configure_admin(monkeypatch)
    app.dependency_overrides[auth.get_github_transport] = github_transport

    login_response = await admin_client.get("/admin/login")
    state = login_response.headers["location"].split("state=")[1].split("&")[0]
    await admin_client.get("/admin/auth/callback", params={"code": "abc", "state": state})

    response = await admin_client.post("/admin/logout", headers={"Origin": "http://test"})
    assert response.status_code == 303
    assert response.headers["location"] == "/admin/login"

    dashboard_response = await admin_client.get("/admin")
    assert dashboard_response.status_code == 303

    action = await db_conn.fetchrow("SELECT action FROM admin_action WHERE action = 'logout'")
    assert action is not None


async def test_admin_responses_carry_noindex_and_no_store(admin_client: AsyncClient):
    response = await admin_client.get("/admin/login")
    assert response.headers["X-Robots-Tag"] == "noindex, nofollow"
    assert response.headers["Cache-Control"] == "private, no-store"
