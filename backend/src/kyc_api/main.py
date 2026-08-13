from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path

import asyncpg
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles

from kyc_api.config import settings
from kyc_api.routers import dev, health, pages

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
    app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")
    app.include_router(health.router)
    app.include_router(pages.router)
    if settings.enable_dev_routes:
        app.include_router(dev.router)
        app.include_router(dev.fragments_router)
    return app


app = create_app()
