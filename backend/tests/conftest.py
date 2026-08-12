"""Voir docs/plans/phase-0-socle.md, section « Stratégie de test ».

Une base jetable est migrée une fois par session ; chaque test reçoit ensuite sa propre
connexion, ouverte dans une transaction annulée à la fin (jamais de COMMIT), et cette connexion
remplace le pool réel via `app.dependency_overrides`. C'est pour cela que `Queryable` (kyc_api.db)
ne demande que le sous-ensemble de méthodes que Pool et Connection partagent.
"""

import os

TEST_DATABASE_URL = os.environ.get(
    "TEST_DATABASE_URL", "postgresql://kyc:kyc@localhost:5432/kyc_test"
)
# La config est lue au chargement du module kyc_api.config (import plus bas) : la variable doit
# donc être posée avant tout import de kyc_api.
os.environ.setdefault("DATABASE_URL", TEST_DATABASE_URL)

from collections.abc import AsyncIterator  # noqa: E402
from pathlib import Path  # noqa: E402

import asyncpg  # noqa: E402
import pytest  # noqa: E402
from httpx import ASGITransport, AsyncClient  # noqa: E402

from kyc_api.db import get_pool  # noqa: E402
from kyc_api.main import create_app  # noqa: E402

MIGRATIONS_DIR = Path(__file__).parents[2] / "db" / "migrations"


@pytest.fixture(scope="session")
async def _migrated_db() -> AsyncIterator[None]:
    conn = await asyncpg.connect(TEST_DATABASE_URL)
    try:
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
    async with AsyncClient(transport=transport, base_url="http://test") as ac:
        yield ac
    app.dependency_overrides.clear()
