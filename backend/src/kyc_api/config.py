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

    # Authentification du back-office (D3.1, docs/plans/phase-3-categorisation.md). Vide par
    # défaut : /admin/login rend alors un 503 plutôt qu'un contournement de développement.
    admin_github_client_id: str = ""
    admin_github_client_secret: str = ""
    # Logins GitHub autorisés, séparés par des virgules. Vide = personne n'entre.
    admin_github_logins: str = ""
    session_secret: str = "changez-moi"
    public_base_url: str = "http://localhost:8000"

    @property
    def admin_github_logins_normalized(self) -> frozenset[str]:
        return frozenset(
            login.strip().lower() for login in self.admin_github_logins.split(",") if login.strip()
        )

    @property
    def admin_oauth_configured(self) -> bool:
        return bool(self.admin_github_client_id and self.admin_github_client_secret)

    @property
    def session_cookie_https_only(self) -> bool:
        return self.public_base_url.startswith("https://")


settings = Settings()
