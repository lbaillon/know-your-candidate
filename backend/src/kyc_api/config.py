from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

# .env est partagé par le backend et le worker et vit à la racine du dépôt (voir .env.example) —
# pas dans backend/, d'où le chemin absolu : env_file résolu depuis le cwd casserait dès que le
# process est lancé avec `cd backend` (cas de `make dev`).
_REPO_ROOT_ENV = Path(__file__).resolve().parents[3] / ".env"


class Settings(BaseSettings):
    # extra="ignore" : .env contient aussi WORKER_ID (lu par le worker Rust, pas par le backend).
    model_config = SettingsConfigDict(
        env_file=_REPO_ROOT_ENV, env_file_encoding="utf-8", extra="ignore"
    )

    database_url: str
    log_level: str = "info"

    # Au-delà de ce délai sans battement de cœur, le worker est considéré comme éteint.
    # Cohérent avec la marge prise côté worker pour la reprise des jobs zombies (voir
    # docs/plans/phase-0-socle.md), qui expire un worker après 1 minute sans battement.
    worker_stale_after_seconds: int = 60

    # `POST /dev/jobs` n'est pas authentifiée : à partir de la phase 5, où `main` se déploie
    # automatiquement, la laisser montée par défaut serait un déni de service trivial. Défaut sûr,
    # activé explicitement en local et dans les tests (voir F9, docs/plans/phase-0.1-fix.md).
    enable_dev_routes: bool = False


settings = Settings()
