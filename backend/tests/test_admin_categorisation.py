"""Tests de la file de travail et du formulaire de catégorisation — voir
docs/plans/phase-3-categorisation.md, section « Back-office : la file de travail ».
"""

from collections.abc import AsyncIterator
from datetime import date

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import AsyncClient

from kyc_api.admin import auth
from kyc_api.config import settings


async def _insert_source_document(conn: asyncpg.Connection, uid: str) -> int:
    return await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """,
        uid,
    )


async def insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int,
    legislature: int = 17,
    date_scrutin: date = date(2024, 3, 14),
    titre: str = "l'ensemble du projet de loi",
    nombre_votants: int = 400,
    effectif: int = 577,
) -> int:
    source_document_id = await _insert_source_document(conn, an_uid)
    return await conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, 'assemblee', 'SPO', $5, 'DecompteNominatif', $6, $6, 200, 150, 0,
                0, $7, $8)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        titre,
        nombre_votants,
        effectif,
        source_document_id,
    )


async def insert_theme(
    conn: asyncpg.Connection,
    *,
    slug: str,
    rang: int,
    pole_negatif: str | None = "pôle négatif",
    pole_positif: str | None = "pôle positif",
) -> int:
    return await conn.fetchval(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description', $2, $3, $4)
        RETURNING id
        """,
        slug,
        pole_negatif,
        pole_positif,
        rang,
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
    from httpx import ASGITransport

    from kyc_api.db import get_connection, get_pool

    async def _connection_override():
        yield db_conn

    app.dependency_overrides[get_pool] = lambda: db_conn
    app.dependency_overrides[get_connection] = _connection_override
    monkeypatch.setattr(settings, "admin_github_client_id", "test-client-id")
    monkeypatch.setattr(settings, "admin_github_client_secret", "test-client-secret")
    monkeypatch.setattr(settings, "admin_github_logins", "alice")
    monkeypatch.setattr(settings, "session_secret", "test-session-secret")
    app.dependency_overrides[auth.get_github_transport] = github_transport

    transport = ASGITransport(app=app)
    # Origin par défaut : la garde CSRF (D3.19) refuse toute méthode non sûre sans elle, et ce
    # fichier teste la file de travail, pas la garde CSRF (déjà couverte par test_admin_csrf.py).
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


async def test_next_categorisation_shows_the_first_scrutin_in_the_queue(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await insert_scrutin(db_conn, an_uid="SC1", numero=1, titre="l'ensemble de la loi Zucman")
    response = await admin_client.get("/admin/categorisation")
    assert response.status_code == 200
    assert b"Zucman" in response.content


async def test_next_categorisation_shows_empty_state_when_queue_is_empty(
    admin_client: AsyncClient,
):
    response = await admin_client.get("/admin/categorisation")
    assert response.status_code == 200
    assert b"Plus aucun scrutin" in response.content


async def test_submitting_a_single_theme_creates_a_categorisation(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)

    response = await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "vote sur la fiscalité du patrimoine",
        },
        follow_redirects=False,
    )
    assert response.status_code == 303
    assert response.headers["location"] == "/admin/categorisation"

    row = await db_conn.fetchrow(
        "SELECT poids, position_pour, method FROM scrutin_label WHERE scrutin_id = $1",
        scrutin_id,
    )
    assert row is not None
    assert row["poids"] == pytest.approx(1.0)
    assert row["position_pour"] == pytest.approx(0.5)
    assert row["method"] == "manual"

    revision = await db_conn.fetchrow(
        "SELECT method FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision is not None

    action = await db_conn.fetchrow(
        "SELECT action FROM admin_action WHERE action = 'categorisation_creation'"
    )
    assert action is not None


async def test_submitting_a_justification_too_short_is_rejected(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)

    response = await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "trop",
        },
        follow_redirects=False,
    )
    assert response.status_code == 200
    assert b"justification" in response.content.lower()

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 0


async def test_submitting_two_themes_summing_to_one_is_accepted(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    await insert_theme(db_conn, slug="environnement", rang=20)

    response = await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "justification suffisamment longue",
            "poids_1": "0.6",
            "theme_2": "environnement",
            "position_2": "-0.2",
            "confiance_2": "0.4",
            "justification_2": "justification suffisamment longue aussi",
            "poids_2": "0.4",
        },
        follow_redirects=False,
    )
    assert response.status_code == 303

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 2


async def test_submitting_two_themes_not_summing_to_one_is_rejected(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    await insert_theme(db_conn, slug="environnement", rang=20)

    response = await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "justification suffisamment longue",
            "poids_1": "0.6",
            "theme_2": "environnement",
            "position_2": "-0.2",
            "confiance_2": "0.4",
            "justification_2": "justification suffisamment longue aussi",
            "poids_2": "0.6",
        },
        follow_redirects=False,
    )
    assert response.status_code == 200
    assert b"somme des poids" in response.content

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 0


async def test_a_position_on_a_theme_without_an_axis_is_ignored_not_written(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="autre", rang=99, pole_negatif=None, pole_positif=None)

    response = await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "autre",
            "confiance_1": "0.7",
            "justification_1": "ne relève d'aucun thème connu",
        },
        follow_redirects=False,
    )
    assert response.status_code == 303

    row = await db_conn.fetchrow(
        "SELECT position_pour FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert row is not None
    assert row["position_pour"] is None


async def test_editing_an_existing_categorisation_writes_a_second_revision(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)

    data = {
        "theme_1": "social-fiscalite",
        "position_1": "0.5",
        "confiance_1": "0.7",
        "justification_1": "première justification suffisamment longue",
    }
    await admin_client.post("/admin/categorisation/17/1", data=data, follow_redirects=False)

    data["position_1"] = "-0.5"
    data["justification_1"] = "position corrigée après relecture attentive"
    response = await admin_client.post(
        "/admin/categorisation/17/1", data=data, follow_redirects=False
    )
    assert response.status_code == 303

    revision_count = await db_conn.fetchval(
        "SELECT count(*) FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision_count == 2

    action = await db_conn.fetchrow(
        "SELECT action FROM admin_action WHERE action = 'categorisation_modification'"
    )
    assert action is not None


async def test_resubmitting_the_identical_form_writes_no_new_revision(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)

    data = {
        "theme_1": "social-fiscalite",
        "position_1": "0.500",
        "confiance_1": "0.7",
        "justification_1": "justification suffisamment longue",
    }
    await admin_client.post("/admin/categorisation/17/1", data=data, follow_redirects=False)
    await admin_client.post("/admin/categorisation/17/1", data=data, follow_redirects=False)

    revision_count = await db_conn.fetchval(
        "SELECT count(*) FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision_count == 1


async def test_deleting_a_categorisation_requires_a_motif(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "justification suffisamment longue",
        },
        follow_redirects=False,
    )

    response = await admin_client.post(
        "/admin/categorisation/17/1/supprimer", data={}, follow_redirects=False
    )
    assert response.status_code == 200
    assert b"motif" in response.content.lower()

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 1


async def test_deleting_a_categorisation_removes_it_and_logs_history(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    await admin_client.post(
        "/admin/categorisation/17/1",
        data={
            "theme_1": "social-fiscalite",
            "position_1": "0.5",
            "confiance_1": "0.7",
            "justification_1": "justification suffisamment longue",
        },
        follow_redirects=False,
    )

    response = await admin_client.post(
        "/admin/categorisation/17/1/supprimer",
        data={"motif": "erreur de lecture du scrutin"},
        follow_redirects=False,
    )
    assert response.status_code == 303
    assert response.headers["location"] == "/admin/categorisation/17/1"

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 0

    revision = await db_conn.fetchrow(
        # `id DESC`, pas `created_at DESC` : dans un test, les deux requêtes simulées partagent la
        # même transaction externe (voir conftest.py), et `now()` y est figée pour toute sa durée
        # — deux révisions créées dans le même test portent donc le même `created_at`.
        "SELECT motif FROM label_revision WHERE scrutin_id = $1 ORDER BY id DESC LIMIT 1",
        scrutin_id,
    )
    assert revision is not None
    assert revision["motif"] == "erreur de lecture du scrutin"


async def test_passer_excludes_the_scrutin_from_the_queue_for_this_session(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    scrutin_id = await insert_scrutin(
        db_conn, an_uid="SC1", numero=1, titre="l'ensemble du premier scrutin"
    )
    await insert_scrutin(db_conn, an_uid="SC2", numero=2, titre="l'ensemble du second scrutin")

    response = await admin_client.post(
        "/admin/categorisation/passer",
        data={"scrutin_id": str(scrutin_id)},
        follow_redirects=False,
    )
    assert response.status_code == 303

    next_response = await admin_client.get("/admin/categorisation")
    assert b"premier scrutin" not in next_response.content
    assert b"second scrutin" in next_response.content
