import logging
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from pathlib import Path

import asyncpg
from fastapi import FastAPI
from fastapi.staticfiles import StaticFiles
from starlette.middleware.sessions import SessionMiddleware

from kyc_api import admin
from kyc_api.config import settings
from kyc_api.http_cache import HttpCacheMiddleware
from kyc_api.routers import api, fragments, health, pages

STATIC_DIR = Path(__file__).parent / "static"

logger = logging.getLogger(__name__)

_SESSION_MAX_AGE_SECONDS = 8 * 3600


@asynccontextmanager
async def lifespan(app: FastAPI) -> AsyncIterator[None]:
    if not settings.admin_oauth_configured:
        # Pas de compte de secours, pas de contournement de développement (CLAUDE.md) : /admin/login
        # rend un 503 dans ce cas (voir kyc_api.admin.auth.login), ce log en explique la cause à qui
        # démarre le serveur sans avoir lu jusque-là.
        logger.warning(
            "ADMIN_GITHUB_CLIENT_ID/ADMIN_GITHUB_CLIENT_SECRET non configurés : "
            "le back-office (/admin) restera inaccessible."
        )
    app.state.pool = await asyncpg.create_pool(settings.database_url, min_size=1, max_size=10)
    try:
        yield
    finally:
        await app.state.pool.close()


def create_app() -> FastAPI:
    app = FastAPI(title="Know Your Candidate", lifespan=lifespan)
    app.add_middleware(HttpCacheMiddleware)
    app.add_middleware(
        SessionMiddleware,
        secret_key=settings.session_secret,
        same_site="lax",
        https_only=settings.session_cookie_https_only,
        max_age=_SESSION_MAX_AGE_SECONDS,
    )
    app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")
    app.include_router(health.router)
    app.include_router(pages.router)
    app.include_router(fragments.router)
    app.include_router(api.router)
    admin.mount(app)
    return app


app = create_app()
