"""Voir docs/plans/phase-0-socle.md, section « Stratégie de test ».

Une base jetable est migrée une fois par session ; chaque test reçoit ensuite sa propre
connexion, ouverte dans une transaction annulée à la fin (jamais de COMMIT), et cette connexion
remplace le pool réel via `app.dependency_overrides`. C'est pour cela que `Queryable` (kyc_api.db)
ne demande que le sous-ensemble de méthodes que Pool et Connection partagent.
"""

import os
import re
import sys
from urllib.parse import urlsplit, urlunsplit

TEST_DATABASE_URL = os.environ.get(
    "TEST_DATABASE_URL", "postgresql://kyc:kyc@localhost:5432/kyc_test"
)
# La config est lue au chargement du module kyc_api.config (import plus bas) : les variables
# doivent donc être posées avant tout import de kyc_api.
os.environ.setdefault("DATABASE_URL", TEST_DATABASE_URL)
# Les routes de démonstration sont désactivées par défaut (voir F9, docs/plans/phase-0.1-fix.md) ;
# les tests en ont besoin, donc on les réactive explicitement ici plutôt que de dépendre du .env
# du développeur.
os.environ.setdefault("ENABLE_DEV_ROUTES", "true")

from collections.abc import AsyncIterator  # noqa: E402
from pathlib import Path  # noqa: E402

import asyncpg  # noqa: E402
import pytest  # noqa: E402
from httpx import ASGITransport, AsyncClient  # noqa: E402

from kyc_api.db import get_pool  # noqa: E402
from kyc_api.main import create_app  # noqa: E402

MIGRATIONS_DIR = Path(__file__).parents[2] / "db" / "migrations"

# Nom de base attendu dans TEST_DATABASE_URL : c'est une valeur de configuration (env var ou
# défaut ci-dessus), pas une entrée utilisateur, mais elle finit interpolée dans un `CREATE
# DATABASE` (les noms de base ne sont pas paramétrables en SQL) — on la valide donc à cette
# frontière plutôt que de faire confiance à un format libre.
_VALID_DB_NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")


def _maintenance_url_and_db_name(database_url: str) -> tuple[str, str]:
    """Dérive une URL de connexion à `postgres` (toujours présente) à partir de l'URL de test, et
    le nom de la base de test elle-même — pour pouvoir la créer si elle n'existe pas encore.
    """
    parts = urlsplit(database_url)
    db_name = parts.path.lstrip("/")
    if not _VALID_DB_NAME.match(db_name):
        raise ValueError(
            f"nom de base invalide dans TEST_DATABASE_URL : {db_name!r} "
            "(lettres, chiffres, underscore, ne commence pas par un chiffre)"
        )
    maintenance_parts = parts._replace(path="/postgres")
    return urlunsplit(maintenance_parts), db_name


async def _ensure_test_database_exists(database_url: str) -> None:
    maintenance_url, db_name = _maintenance_url_and_db_name(database_url)
    try:
        conn = await asyncpg.connect(maintenance_url)
    except (OSError, asyncpg.PostgresError) as exc:
        print(
            f"Impossible de joindre PostgreSQL ({exc}). "
            "Démarrer la base avec `make db-up` avant de lancer les tests.",
            file=sys.stderr,
        )
        raise

    try:
        exists = await conn.fetchval("SELECT 1 FROM pg_database WHERE datname = $1", db_name)
        if not exists:
            # Les scripts d'init de l'image Postgres ne rejouent qu'à la création du volume : un
            # volume déjà existant ne les rejoue pas, d'où la création explicite ici plutôt que de
            # dépendre d'un script de conteneur.
            await conn.execute(f'CREATE DATABASE "{db_name}"')
    finally:
        await conn.close()


@pytest.fixture(scope="session")
async def _migrated_db() -> AsyncIterator[None]:
    await _ensure_test_database_exists(TEST_DATABASE_URL)

    conn = await asyncpg.connect(TEST_DATABASE_URL)
    try:
        # La base de test est réutilisée d'une invocation de pytest à l'autre (ce n'est pas une
        # base recréée par le système à chaque run) : on la vide avant de rejouer les migrations,
        # sinon "CREATE TYPE" échoue dès le deuxième `pytest` sur le même Postgres.
        await conn.execute("DROP SCHEMA public CASCADE; CREATE SCHEMA public;")
        for migration in sorted(MIGRATIONS_DIR.glob("*.sql")):
            await conn.execute(migration.read_text(encoding="utf-8"))
        yield
    finally:
        await conn.close()


@pytest.fixture
async def db_conn(_migrated_db: None) -> AsyncIterator[asyncpg.Connection]:
    conn = await asyncpg.connect(TEST_DATABASE_URL)
    transaction = conn.transaction()
    await transaction.start()
    try:
        yield conn
    finally:
        await transaction.rollback()
        await conn.close()


@pytest.fixture
async def client(db_conn: asyncpg.Connection) -> AsyncIterator[AsyncClient]:
    app = create_app()
    app.dependency_overrides[get_pool] = lambda: db_conn
    transport = ASGITransport(app=app)
    # follow_redirects=True : httpx ne suit pas les redirections par défaut, contrairement à un
    # navigateur ou à une requête HTMX — on veut tester le parcours réel (POST -> 303 -> page).
    async with AsyncClient(
        transport=transport, base_url="http://test", follow_redirects=True
    ) as ac:
        yield ac
    app.dependency_overrides.clear()
