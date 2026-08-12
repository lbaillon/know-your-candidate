from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", env_file_encoding="utf-8")

    database_url: str
    log_level: str = "info"

    # Au-delà de ce délai sans battement de cœur, le worker est considéré comme éteint.
    # Cohérent avec la marge prise côté worker pour la reprise des jobs zombies (voir
    # docs/plans/phase-0-socle.md), qui expire un worker après 1 minute sans battement.
    worker_stale_after_seconds: int = 60


settings = Settings()
