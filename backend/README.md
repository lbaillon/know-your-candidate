# backend/

API et interface web : FastAPI + Jinja2 + HTMX, gérée par `uv`.

Rôle : **lire** des données déjà calculées et rendre des pages. Aucun traitement lourd, aucun appel vers
l'open data. Les seules écritures autorisées sont les catégorisations saisies par un admin et les demandes
de job.

Contenu à venir (phase 0) : `pyproject.toml`, `src/kyc_api/`, `templates/`, `static/`, `tests/`.
