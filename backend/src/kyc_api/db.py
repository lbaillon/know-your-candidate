from typing import Protocol

import asyncpg
from fastapi import Request


class Queryable(Protocol):
    """Sous-ensemble de l'API asyncpg utilisé par le code métier.

    asyncpg.Pool et asyncpg.Connection l'implémentent tous les deux : les tests peuvent donc
    injecter une connexion unique, ouverte dans une transaction annulée à la fin, à la place du
    pool réel (voir docs/plans/phase-0-socle.md, section « Stratégie de test »).
    """

    async def fetchrow(
        self, query: str, *args: object, timeout: float | None = None
    ) -> asyncpg.Record | None: ...

    async def fetchval(self, query: str, *args: object, timeout: float | None = None) -> object: ...

    async def fetch(
        self, query: str, *args: object, timeout: float | None = None
    ) -> list[asyncpg.Record]: ...

    async def execute(self, query: str, *args: object, timeout: float | None = None) -> str: ...


async def get_pool(request: Request) -> Queryable:
    return request.app.state.pool
