"""En-têtes communs à tout `/admin` — voir docs/plans/phase-3-categorisation.md, section
« Authentification admin », point 7. Un middleware plutôt qu'un rappel dans chaque routeur : une
route admin ajoutée plus tard les porte par construction (même raisonnement que `require_admin`,
posé sur le routeur entier).
"""

from collections.abc import Awaitable, Callable

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response


class AdminHeadersMiddleware(BaseHTTPMiddleware):
    async def dispatch(
        self, request: Request, call_next: Callable[[Request], Awaitable[Response]]
    ) -> Response:
        response = await call_next(request)
        if request.url.path.startswith("/admin"):
            response.headers["X-Robots-Tag"] = "noindex, nofollow"
            response.headers["Cache-Control"] = "private, no-store"
        return response
