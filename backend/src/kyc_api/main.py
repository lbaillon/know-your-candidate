from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path

import asyncpg
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from kyc_api.config import settings
from kyc_api.http_cache import HttpCacheMiddleware
from kyc_api.routers import fragments, health, pages

STATIC_DIR = Path(__file__).parent / "static"


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    app.state.pool = await asyncpg.create_pool(settings.database_url, min_size=1, max_size=10)
    try:
        yield
    finally:
        await app.state.pool.close()


def create_app() -> FastAPI:
    app = FastAPI(title="Know Your Candidate", lifespan=lifespan)
    app.add_middleware(HttpCacheMiddleware)
    app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")
    app.include_router(health.router)
    app.include_router(pages.router)
    app.include_router(fragments.router)
    return app


app = create_app()
