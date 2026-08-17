"""ETag fort + Cache-Control courts sur les pages publiques (D2.5) — voir
docs/plans/phase-2-api-ui.md, section « Cache HTTP ». Une redirection ou une erreur n'affirme rien
de stable : seules les réponses 200 en HTML ou en JSON en portent un.

Implémenté en middleware plutôt qu'en code répété dans chaque routeur : une route publique
oubliée ici serait un oubli silencieux, contrairement à un routeur qui omettrait un appel exprès.
"""

import hashlib
from collections.abc import Awaitable, Callable
from typing import cast

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import Response, StreamingResponse

CACHE_CONTROL = "public, max-age=60, stale-while-revalidate=300"

_CACHEABLE_CONTENT_TYPES = ("text/html", "application/json")
# /healthz reflète un état en temps réel (voir routers/health.py) : le mettre en cache irait
# contre sa raison d'être. /static/ a déjà son propre cache conditionnel via StaticFiles.
_EXCLUDED_PATHS = frozenset({"/healthz"})
_EXCLUDED_PREFIXES = ("/static/",)


def compute_etag(body: bytes) -> str:
    """ETag fort : un corps identique produit toujours le même ETag. sha256 par cohérence avec le
    hachage déjà utilisé ailleurs dans le dépôt (voir worker/src/an/http.rs), sans que la
    robustesse cryptographique importe ici.
    """
    return f'"{hashlib.sha256(body).hexdigest()}"'


class HttpCacheMiddleware(BaseHTTPMiddleware):
    async def dispatch(
        self, request: Request, call_next: Callable[[Request], Awaitable[Response]]
    ) -> Response:
        response = await call_next(request)

        if request.method != "GET" or response.status_code != 200:
            return response
        path = request.url.path
        if path in _EXCLUDED_PATHS or path.startswith(_EXCLUDED_PREFIXES):
            return response
        content_type = response.headers.get("content-type", "")
        if not content_type.startswith(_CACHEABLE_CONTENT_TYPES):
            return response

        # `call_next` déclare rendre un `Response`, mais `BaseHTTPMiddleware` construit toujours
        # en réalité un `StreamingResponse` en interne (comportement documenté de Starlette, pas
        # exposé dans la signature publique) : c'est ce type précis, avec son `body_iterator`,
        # qu'on doit lire pour consommer le corps avant de le remplacer.
        streaming_response = cast(StreamingResponse, response)
        chunks = [
            chunk.encode() if isinstance(chunk, str) else bytes(chunk)
            async for chunk in streaming_response.body_iterator
        ]
        body = b"".join(chunks)
        etag = compute_etag(body)

        if request.headers.get("if-none-match") == etag:
            return Response(status_code=304, headers={"ETag": etag, "Cache-Control": CACHE_CONTROL})

        new_response = Response(content=body, status_code=response.status_code)
        for key, value in response.headers.items():
            if key.lower() != "content-length":
                new_response.headers[key] = value
        new_response.headers["ETag"] = etag
        new_response.headers["Cache-Control"] = CACHE_CONTROL
        return new_response
