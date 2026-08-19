"""Tests de GET /admin/export — voir docs/plans/phase-3-categorisation.md, section « Le cycle
export / import ». Les fonctions pures de sérialisation sont testées à part
(`tests/test_labels_io.py`) ; ici, ce qui ne se teste qu'avec une vraie base : filtre par statut,
filtre par thème, forme de la réponse HTTP.
"""

import json
from collections.abc import AsyncIterator
from datetime import date

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from kyc_api.admin import auth
from kyc_api.config import settings
from kyc_api.db import get_pool


async def insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int,
    legislature: int = 17,
    date_scrutin: date = date(2024, 3, 14),
    titre: str = "l'ensemble du projet de loi",
) -> int:
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """,
        an_uid,
    )
    return await conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, 'assemblee', 'SPO', $5, 'DecompteNominatif', 400, 400, 200, 150, 0,
                0, 577, $6)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        titre,
        source_document_id,
    )


async def insert_theme(conn: asyncpg.Connection, *, slug: str, rang: int) -> int:
    return await conn.fetchval(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description', 'négatif', 'positif', $2)
        RETURNING id
        """,
        slug,
        rang,
    )


async def insert_label(
    conn: asyncpg.Connection, *, scrutin_id: int, theme_id: int, author_id: int
) -> None:
    await conn.execute(
        """
        INSERT INTO scrutin_label
            (scrutin_id, theme_id, poids, position_pour, confiance, justification, method,
             author_id)
        VALUES ($1, $2, 1.000, 0.500, 0.700, 'justification suffisamment longue', 'manual', $3)
        """,
        scrutin_id,
        theme_id,
        author_id,
    )


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


async def _admin_user_id(conn: asyncpg.Connection) -> int:
    return await conn.fetchval("SELECT id FROM admin_user WHERE github_login = 'alice'")


async def test_export_json_lists_categorized_scrutins(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    author_id = await _admin_user_id(db_conn)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)

    response = await admin_client.get(
        "/admin/export", params={"statut": "categorises", "format": "json"}
    )
    assert response.status_code == 200
    assert response.headers["content-type"].startswith("application/json")

    data = json.loads(response.content)
    assert data["schema_version"] == 1
    assert len(data["scrutins"]) == 1
    assert data["scrutins"][0]["scrutin_uid"] == "SC1"
    assert data["scrutins"][0]["categorisation"][0]["theme"] == "social-fiscalite"
    assert data["scrutins"][0]["categorisation"][0]["poids"] == "1.000"


async def test_export_non_categorises_excludes_categorized_scrutins(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    categorized_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_scrutin(db_conn, an_uid="SC2", numero=2)
    author_id = await _admin_user_id(db_conn)
    await insert_label(db_conn, scrutin_id=categorized_id, theme_id=theme_id, author_id=author_id)

    response = await admin_client.get(
        "/admin/export", params={"statut": "non_categorises", "format": "json"}
    )
    data = json.loads(response.content)
    uids = {s["scrutin_uid"] for s in data["scrutins"]}
    assert uids == {"SC2"}


async def test_export_csv_has_the_expected_header(admin_client: AsyncClient):
    response = await admin_client.get("/admin/export", params={"format": "csv"})
    assert response.status_code == 200
    assert response.headers["content-type"].startswith("text/csv")
    first_line = response.text.splitlines()[0]
    assert first_line == (
        "schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,"
        "abstentions,participation,positions_groupes,url_an,theme,poids,position_pour,"
        "confiance,justification"
    )


async def test_export_filters_by_theme(admin_client: AsyncClient, db_conn: asyncpg.Connection):
    theme_a = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    theme_b = await insert_theme(db_conn, slug="environnement", rang=20)
    scrutin_a = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    scrutin_b = await insert_scrutin(db_conn, an_uid="SC2", numero=2)
    author_id = await _admin_user_id(db_conn)
    await insert_label(db_conn, scrutin_id=scrutin_a, theme_id=theme_a, author_id=author_id)
    await insert_label(db_conn, scrutin_id=scrutin_b, theme_id=theme_b, author_id=author_id)

    response = await admin_client.get(
        "/admin/export",
        params={"statut": "categorises", "format": "json", "theme": "environnement"},
    )
    data = json.loads(response.content)
    uids = {s["scrutin_uid"] for s in data["scrutins"]}
    assert uids == {"SC2"}


async def test_export_rejects_an_unknown_statut(admin_client: AsyncClient):
    response = await admin_client.get("/admin/export", params={"statut": "n_importe_quoi"})
    assert response.status_code == 400


async def test_export_rejects_an_unknown_format(admin_client: AsyncClient):
    response = await admin_client.get("/admin/export", params={"format": "xml"})
    assert response.status_code == 400
