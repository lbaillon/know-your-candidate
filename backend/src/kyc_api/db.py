from collections.abc import AsyncIterator
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


class AsyncTransaction(Protocol):
    async def __aenter__(self) -> object: ...

    async def __aexit__(self, *exc_info: object) -> bool | None: ...


class WritableQueryable(Queryable, Protocol):
    """Étend `Queryable` de `.transaction()` — nécessaire à partir de la phase 3 pour les
    premières écritures multi-instructions du backend (catégorisation, import). `asyncpg.Pool`
    n'a pas cette méthode (chaque appel `fetch*`/`execute` y acquiert sa propre connexion) : d'où
    `get_connection`, qui en retient une seule pour toute la durée de la requête plutôt que de
    l'emprunter au pool à chaque instruction.
    """

    def transaction(self) -> AsyncTransaction: ...


async def get_pool(request: Request) -> Queryable:
    return request.app.state.pool


async def get_connection(request: Request) -> AsyncIterator[WritableQueryable]:
    pool = request.app.state.pool
    async with pool.acquire() as conn:
        yield conn
