# backend/

API et interface web : FastAPI + Jinja2 + HTMX, gérée par `uv`.

Rôle : **lire** des données déjà calculées et rendre des pages. Aucun traitement lourd, aucun appel vers
l'open data. Les seules écritures autorisées sont les catégorisations saisies par un admin et les demandes
de job.

## Développement

```
uv sync                 # installe les dépendances
uv run uvicorn kyc_api.main:app --reload --port 8000
uv run ruff check .
uv run ruff format .
uv run ty check
uv run pytest
```

Nécessite `DATABASE_URL` (voir [.env.example](../.env.example) à la racine) et une base migrée — voir la
section « Démarrage rapide » du [README](../README.md) racine.

## Structure

```
pyproject.toml           projet uv, dépendances, config Ruff + ty
src/kyc_api/
  main.py                 création de l'app FastAPI, montage des routeurs
  config.py                réglages via variables d'environnement (Pydantic Settings)
  db.py                    dépendance FastAPI donnant accès à la base (pool ou connexion de test)
  jobs.py                  création d'un job et lecture de son état (SQL écrit à la main)
  templating.py            instance Jinja2Templates partagée
  routers/
    health.py               /healthz
    pages.py                 page d'accueil
  templates/
  static/
tests/
```
