"""Tests des routes de dépôt et d'aperçu d'un import — voir
docs/plans/phase-3-categorisation.md, section « Import : validation, aperçu, application ». La
validation elle-même est testée à part (`tests/test_import_validation.py`,
`tests/test_labels_io.py`) ; ici, ce qui ne se teste qu'avec une vraie requête HTTP : dépôt
multipart, redirection, page d'aperçu, journalisation.
"""

from collections.abc import AsyncIterator

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from kyc_api import labels_io
from kyc_api.admin import auth
from kyc_api.config import settings
from kyc_api.db import get_pool

VALID_CSV = (
    "schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,"
    "abstentions,participation,positions_groupes,url_an,theme,poids,position_pour,confiance,"
    "justification\n"
    "1,SC1,17,1,2024-03-14,titre,type,sort,200,150,0,0.700,,https://example.org,"
    "social-fiscalite,1.000,0.500,0.700,justification suffisamment longue\n"
)


async def _seed(conn: asyncpg.Connection) -> None:
    await conn.execute(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ('social-fiscalite', 'Social', 'd', 'négatif', 'positif', 10)
        """
    )
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'SC1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    await conn.execute(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('SC1', 1, 17, '2024-03-14', 'assemblee', 'SPO', 'titre', 'DecompteNominatif',
                350, 350, 200, 150, 0, 0, 577, $1)
        """,
        source_document_id,
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


async def test_import_form_is_reachable(admin_client: AsyncClient):
    response = await admin_client.get("/admin/import")
    assert response.status_code == 200


async def test_depositing_without_a_file_is_rejected(admin_client: AsyncClient):
    response = await admin_client.post("/admin/import", files={}, follow_redirects=False)
    assert response.status_code == 422


async def test_depositing_a_valid_csv_creates_a_pending_import_and_redirects(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await _seed(db_conn)
    response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", VALID_CSV, "text/csv")},
        follow_redirects=False,
    )
    assert response.status_code == 303
    assert response.headers["location"].startswith("/admin/import/")

    record = await db_conn.fetchrow("SELECT status, filename, format FROM label_import")
    assert record is not None
    assert record["status"] == "pending"
    assert record["filename"] == "export.csv"
    assert record["format"] == "csv"

    action = await db_conn.fetchrow("SELECT target FROM admin_action WHERE action = 'import_depot'")
    assert action is not None
    assert action["target"] == "export.csv"


async def test_depositing_an_invalid_file_re_renders_the_form_with_errors(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", "not,a,valid,header\n1,2,3,4\n", "text/csv")},
        follow_redirects=False,
    )
    assert response.status_code == 422
    assert b"obligatoire" in response.content

    count = await db_conn.fetchval("SELECT count(*) FROM label_import")
    assert count == 0


async def test_depositing_a_file_with_semantic_errors_re_renders_the_form_with_the_reason(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await _seed(db_conn)
    bad_csv = VALID_CSV.replace("social-fiscalite", "theme-inconnu")
    response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", bad_csv, "text/csv")},
        follow_redirects=False,
    )
    assert response.status_code == 422
    assert b"theme-inconnu" in response.content


async def test_oversized_file_is_rejected(
    admin_client: AsyncClient, monkeypatch: pytest.MonkeyPatch
):
    monkeypatch.setattr(labels_io, "MAX_FILE_BYTES", 10)
    response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", VALID_CSV, "text/csv")},
        follow_redirects=False,
    )
    assert response.status_code == 422
    assert b"volumineux" in response.content


async def test_show_import_returns_404_for_unknown_id(admin_client: AsyncClient):
    response = await admin_client.get("/admin/import/999999")
    assert response.status_code == 404


async def test_show_import_renders_the_preview_with_counts(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await _seed(db_conn)
    deposit_response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", VALID_CSV, "text/csv")},
        follow_redirects=True,
    )
    assert deposit_response.status_code == 200
    assert b"SC1" in deposit_response.content
    assert b"creation" in deposit_response.content
