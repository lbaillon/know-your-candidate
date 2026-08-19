"""Tests de l'application d'un import — voir docs/plans/phase-3-categorisation.md, section
« ⚠️ À concevoir ici, pas ailleurs ». Chaque test porte le nom de la règle qu'il défend (D3.16,
D3.14, D3.15) plutôt que celui d'un scénario, comme demandé par le plan.
"""

import json
from collections.abc import AsyncIterator
from datetime import UTC, date, datetime

import asyncpg
import httpx
import pytest
from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from kyc_api import labels_io
from kyc_api.admin import auth
from kyc_api.config import settings
from kyc_api.db import get_connection, get_pool
from kyc_api.import_validation import build_plan
from kyc_api.queries import imports as imports_queries


async def insert_scrutin(conn: asyncpg.Connection, *, an_uid: str, numero: int) -> int:
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
        VALUES ($1, $2, 17, $3, 'assemblee', 'SPO', 'titre', 'DecompteNominatif', 350, 350, 200,
                150, 0, 0, 577, $4)
        RETURNING id
        """,
        an_uid,
        numero,
        date(2024, 3, 14),
        source_document_id,
    )


async def insert_theme(conn: asyncpg.Connection, *, slug: str, rang: int) -> int:
    return await conn.fetchval(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'd', 'négatif', 'positif', $2)
        RETURNING id
        """,
        slug,
        rang,
    )


async def insert_admin(conn: asyncpg.Connection, *, login: str = "bob") -> int:
    # "alice" est le login que crée le flux OAuth simulé par la fixture admin_client : un autre
    # nom ici évite une collision sur la contrainte unique github_login.
    return await conn.fetchval(
        """
        INSERT INTO admin_user (github_id, github_login, display_name) VALUES ($1, $2, $2)
        RETURNING id
        """,
        hash(login) % 1_000_000,
        login,
    )


async def insert_label(
    conn: asyncpg.Connection,
    *,
    scrutin_id: int,
    theme_id: int,
    author_id: int,
    method: str = "manual",
    reviewed_at: object | None = None,
    poids: str = "1.000",
    position_pour: str = "0.500",
    justification: str = "justification suffisamment longue",
) -> None:
    # scrutin_label_import_renseigne (migration 0007) exige un import_id dès que method='import' :
    # une ligne d'archive minimale suffit, son contenu n'est pas relu par ces tests.
    import_id = None
    if method == "import":
        import_id = await conn.fetchval(
            """
            INSERT INTO label_import
                (filename, format, schema_version, contenu, content_hash, apercu, uploaded_by)
            VALUES ('fixture.csv', 'csv', 1, '', 'hash', '{}'::jsonb, $1)
            RETURNING id
            """,
            author_id,
        )

    await conn.execute(
        """
        INSERT INTO scrutin_label
            (scrutin_id, theme_id, poids, position_pour, confiance, justification, method,
             author_id, import_id, reviewed_by, reviewed_at)
        VALUES ($1, $2, $3::numeric, $4::numeric, 0.700, $5, $6::label_method, $7, $8, $9, $10)
        """,
        scrutin_id,
        theme_id,
        poids,
        position_pour,
        justification,
        method,
        author_id,
        import_id,
        author_id if reviewed_at is not None else None,
        reviewed_at,
    )


def csv_for(scrutin_uid: str, theme: str, *, position: str = "0.500") -> str:
    return (
        "schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,"
        "abstentions,participation,positions_groupes,url_an,theme,poids,position_pour,confiance,"
        "justification\n"
        f"1,{scrutin_uid},17,1,2024-03-14,titre,type,sort,200,150,0,0.700,,https://example.org,"
        f"{theme},1.000,{position},0.700,justification suffisamment longue\n"
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

    async def _connection_override():
        yield db_conn

    app.dependency_overrides[get_connection] = _connection_override
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


async def deposit(admin_client: AsyncClient, content: str) -> int:
    response = await admin_client.post(
        "/admin/import",
        files={"fichier": ("export.csv", content, "text/csv")},
        follow_redirects=False,
    )
    assert response.status_code == 303, response.content
    return int(response.headers["location"].rsplit("/", 1)[-1])


async def test_a_conflicting_manual_categorisation_is_not_overwritten_without_confirmation(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    admin_id = await insert_admin(db_conn)
    await insert_label(
        db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=admin_id, method="manual"
    )

    import_id = await deposit(admin_client, csv_for("SC1", "social-fiscalite", position="-0.900"))
    response = await admin_client.post(
        f"/admin/import/{import_id}/appliquer", data={}, follow_redirects=False
    )
    assert response.status_code == 303

    row = await db_conn.fetchrow(
        "SELECT position_pour::text AS position_pour, method FROM scrutin_label "
        "WHERE scrutin_id = $1",
        scrutin_id,
    )
    assert row is not None
    assert row["position_pour"] == "0.500"  # inchangée
    assert row["method"] == "manual"

    revision_count = await db_conn.fetchval(
        "SELECT count(*) FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision_count == 0


async def test_a_conflict_is_overwritten_when_confirmed(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    admin_id = await insert_admin(db_conn)
    await insert_label(
        db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=admin_id, method="manual"
    )

    import_id = await deposit(admin_client, csv_for("SC1", "social-fiscalite", position="-0.900"))
    response = await admin_client.post(
        f"/admin/import/{import_id}/appliquer",
        data={"ecraser_conflits": "1"},
        follow_redirects=False,
    )
    assert response.status_code == 303

    row = await db_conn.fetchrow(
        "SELECT position_pour::text AS position_pour, method FROM scrutin_label "
        "WHERE scrutin_id = $1",
        scrutin_id,
    )
    assert row is not None
    assert row["position_pour"] == "-0.900"
    assert row["method"] == "import"

    revision = await db_conn.fetchrow(
        "SELECT motif FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision is not None
    assert "écrase" in revision["motif"]


async def test_a_scrutin_absent_from_the_file_is_never_touched(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    untouched_id = await insert_scrutin(db_conn, an_uid="SC-UNTOUCHED", numero=1)
    included_id = await insert_scrutin(db_conn, an_uid="SC-INCLUDED", numero=2)
    admin_id = await insert_admin(db_conn)
    await insert_label(
        db_conn, scrutin_id=untouched_id, theme_id=theme_id, author_id=admin_id, method="import"
    )

    import_id = await deposit(admin_client, csv_for("SC-INCLUDED", "social-fiscalite"))
    await admin_client.post(f"/admin/import/{import_id}/appliquer", data={})

    untouched_row = await db_conn.fetchrow(
        "SELECT position_pour::text AS position_pour FROM scrutin_label WHERE scrutin_id = $1",
        untouched_id,
    )
    assert untouched_row is not None
    assert untouched_row["position_pour"] == "0.500"  # jamais touché

    included_row = await db_conn.fetchrow(
        "SELECT scrutin_id FROM scrutin_label WHERE scrutin_id = $1", included_id
    )
    assert included_row is not None


async def test_a_scrutin_present_in_the_file_has_its_themes_fully_replaced(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_a = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    theme_b = await insert_theme(db_conn, slug="environnement", rang=20)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    admin_id = await insert_admin(db_conn)
    # Catégorisé initialement sur theme_a seul, importé pour être recatégorisé sur theme_b seul.
    await insert_label(
        db_conn, scrutin_id=scrutin_id, theme_id=theme_a, author_id=admin_id, method="import"
    )

    import_id = await deposit(admin_client, csv_for("SC1", "environnement"))
    await admin_client.post(f"/admin/import/{import_id}/appliquer", data={})

    rows = await db_conn.fetch(
        "SELECT theme_id FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert {r["theme_id"] for r in rows} == {theme_b}


async def test_the_history_reflects_exactly_the_change(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)

    import_id = await deposit(admin_client, csv_for("SC1", "social-fiscalite", position="0.500"))
    await admin_client.post(f"/admin/import/{import_id}/appliquer", data={})

    revision = await db_conn.fetchrow(
        "SELECT avant, apres FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision is not None
    avant = json.loads(revision["avant"])
    apres = json.loads(revision["apres"])
    assert avant == []
    assert apres == [
        {
            "slug": "social-fiscalite",
            "poids": "1.000",
            "position_pour": "0.500",
            "confiance": "0.700",
            "justification": "justification suffisamment longue",
        }
    ]


async def test_a_stale_preview_is_refused_and_recomputed(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    admin_id = await insert_admin(db_conn)

    import_id = await deposit(admin_client, csv_for("SC1", "social-fiscalite"))

    # La base change après le dépôt : un autre admin catégorise le même scrutin entre-temps.
    await insert_label(
        db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=admin_id, method="manual"
    )

    response = await admin_client.post(
        f"/admin/import/{import_id}/appliquer", data={}, follow_redirects=False
    )
    assert response.status_code == 409
    assert b"chang\xc3\xa9" in response.content

    # Rien n'a été appliqué : le import est toujours pending, la ligne manuelle est intacte.
    record = await imports_queries.get_label_import(db_conn, import_id)
    assert record is not None
    assert record["status"] == "pending"
    row = await db_conn.fetchrow(
        "SELECT method FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert row is not None
    assert row["method"] == "manual"


async def test_an_interrupted_application_leaves_nothing_behind(db_conn: asyncpg.Connection):
    await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1", numero=1)
    admin_id = await insert_admin(db_conn)

    csv_content = csv_for("SC1", "social-fiscalite")
    import_id = await db_conn.fetchval(
        """
        INSERT INTO label_import
            (filename, format, schema_version, contenu, content_hash, apercu, uploaded_by)
        VALUES ('export.csv', 'csv', 1, $1, 'hash', '{}'::jsonb, $2)
        RETURNING id
        """,
        csv_content,
        admin_id,
    )

    parsed = labels_io.read_csv(csv_content)
    plan = await build_plan(db_conn, parsed)
    assert plan.is_valid, plan.problems

    class FailingConn:
        """Panne injectée juste avant la dernière instruction de la transaction — voir
        `queries.imports.apply_plan` : DELETE, INSERT scrutin_label, INSERT label_revision.
        """

        def __init__(self, real: asyncpg.Connection):
            self._real = real

        def transaction(self):
            return self._real.transaction()

        async def fetch(self, query, *args, **kwargs):
            return await self._real.fetch(query, *args, **kwargs)

        async def fetchrow(self, query, *args, **kwargs):
            return await self._real.fetchrow(query, *args, **kwargs)

        async def fetchval(self, query, *args, **kwargs):
            return await self._real.fetchval(query, *args, **kwargs)

        async def execute(self, query, *args, **kwargs):
            if "INSERT INTO label_revision" in query:
                raise RuntimeError("panne simulée")
            return await self._real.execute(query, *args, **kwargs)

    with pytest.raises(RuntimeError, match="panne simulée"):
        await imports_queries.apply_plan(
            FailingConn(db_conn),
            plan=plan,
            import_id=import_id,
            author_id=admin_id,
            apply_conflicts=False,
        )

    count = await db_conn.fetchval(
        "SELECT count(*) FROM scrutin_label WHERE scrutin_id = $1", scrutin_id
    )
    assert count == 0
    revision_count = await db_conn.fetchval(
        "SELECT count(*) FROM label_revision WHERE scrutin_id = $1", scrutin_id
    )
    assert revision_count == 0


async def test_export_then_reimport_of_the_full_dataset_changes_nothing(
    admin_client: AsyncClient, db_conn: asyncpg.Connection
):
    """La propriété d'aller-retour (plan, « stratégie de test de l'import ») : exporter
    l'intégralité des scrutins catégorisés, réimporter le résultat, vérifier qu'aucune ligne ne
    change et qu'aucune révision n'est créée. Un seul jeu de données couvrant les cas cités par le
    plan : deux thèmes de poids 0.6/0.4, une position négative, une position nulle, une
    justification avec virgule/guillemet/accent.
    """
    theme_a = await insert_theme(db_conn, slug="social-fiscalite", rang=10)
    theme_b = await insert_theme(db_conn, slug="environnement", rang=20)
    admin_id = await insert_admin(db_conn)

    scrutin_multi = await insert_scrutin(db_conn, an_uid="SC-MULTI", numero=1)
    await insert_label(
        db_conn,
        scrutin_id=scrutin_multi,
        theme_id=theme_a,
        author_id=admin_id,
        poids="0.600",
        position_pour="0.000",
        justification='justification, avec "guillemet" et été accentué',
    )
    await insert_label(
        db_conn,
        scrutin_id=scrutin_multi,
        theme_id=theme_b,
        author_id=admin_id,
        poids="0.400",
        position_pour="-0.750",
        justification="deuxième justification suffisamment longue",
    )

    scrutin_reviewed = await insert_scrutin(db_conn, an_uid="SC-RELU", numero=2)
    await insert_label(
        db_conn,
        scrutin_id=scrutin_reviewed,
        theme_id=theme_a,
        author_id=admin_id,
        method="import",
        reviewed_at=datetime.now(UTC),
    )

    export_response = await admin_client.get(
        "/admin/export", params={"statut": "categorises", "format": "csv"}
    )
    assert export_response.status_code == 200

    import_id = await deposit(admin_client, export_response.text)
    apply_response = await admin_client.post(
        f"/admin/import/{import_id}/appliquer", data={"ecraser_conflits": "1"}
    )
    assert apply_response.status_code == 200

    revision_count = await db_conn.fetchval("SELECT count(*) FROM label_revision")
    assert revision_count == 0, "un aller-retour identique ne doit créer aucune révision"

    record = await imports_queries.get_label_import(db_conn, import_id)
    assert record is not None
    assert all(s["classement"] == "inchange" for s in record["apercu"]["scrutins"])
