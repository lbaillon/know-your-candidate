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
# contre sa raison d'être. /static/ a déjà son propre cache conditionnel via StaticFiles. /admin
# porte son propre Cache-Control (private, no-store — voir kyc_api.admin.headers, D3.1 point 7) :
# le cache public ne doit jamais s'en mêler.
_EXCLUDED_PATHS = frozenset({"/healthz"})
_EXCLUDED_PREFIXES = ("/static/", "/admin")


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
        # --- HEAD : pourquoi ce détour par le scope ASGI, et pourquoi la restauration -----------
        #
        # Nos routes sont déclarées `@router.get(...)`, donc avec `GET` pour seule méthode.
        # Contrairement à Starlette nu — dont `Route.__init__` ajoute `HEAD` dès que `GET` est
        # présent — FastAPI ne le fait pas pour les routes utilisateur : seules ses propres routes
        # internes (`/docs`, `/openapi.json`), qui sont de simples `Route` Starlette, l'obtiennent.
        # Une requête HEAD reçoit donc un 405 avant d'atteindre le moindre gestionnaire, alors que
        # HTTP exige qu'un HEAD porte les mêmes en-têtes que le GET correspondant.
        #
        # On réécrit donc la méthode dans le scope avant de router, et **on la restaure ensuite,
        # sans condition**. La restauration n'est pas de la propreté : c'est elle qui rend tout le
        # reste correct. uvicorn relit `scope["method"]` non pas au routage mais **au moment
        # d'émettre** (`uvicorn/protocols/http/httptools_impl.py`, lignes 514 et 531), pour décider
        # s'il supprime le corps d'un HEAD. Le laisser à « GET » a deux effets, tous deux mesurés à
        # la revue de la phase 2.1 (F11, docs/plans/phase-2.1-fix.md) :
        #
        #   1. sur les chemins qui sortent par anticipation plus bas (`/healthz`, `/static/`, tout
        #      statut différent de 200), uvicorn **écrit le corps sur le fil** — 36, 12 810 et 22
        #      octets mesurés en socket brut. La RFC 9110 § 9.3.2 l'interdit, et sur une connexion
        #      keep-alive ces octets atterrissent là où le client attend la réponse suivante ;
        #   2. si on vide le corps soi-même pour compenser, uvicorn attend toujours les octets
        #      annoncés par `Content-Length` et lève `RuntimeError: Response content shorter than
        #      Content-Length` — une exception ASGI par requête HEAD.
        #
        # D'où l'absence, plus bas, de tout vidage manuel du corps : **c'est uvicorn qui le
        # supprime**, correctement, dès que le scope dit la vérité. Le réintroduire ramènerait (2).
        #
        # Contrainte à connaître avant d'ajouter quoi que ce soit en aval : entre la réécriture et
        # la restauration, gestionnaires et middlewares voient `request.method == "GET"`. Sans
        # conséquence aujourd'hui — aucun gestionnaire ne lit la méthode — mais à revérifier avant
        # d'introduire un middleware qui la journalise ou un flux qui s'y fie.
        #
        # Avertissement de couverture : **aucun test de cette suite ne peut attraper cette classe
        # de bug.** `ASGITransport` (httpx), sur lequel ils tournent tous, n'implémente pas la
        # comptabilité `Content-Length` d'uvicorn ; il supprime bien le corps d'un HEAD, donc les
        # assertions restent justes, mais une suite verte ne prouve rien ici. Toute modification de
        # ce bloc se vérifie contre un vrai serveur, en socket brut.
        original_method = request.method
        if original_method == "HEAD":
            request.scope["method"] = "GET"

        try:
            response = await call_next(request)
        finally:
            request.scope["method"] = original_method

        if original_method not in ("GET", "HEAD") or response.status_code != 200:
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
