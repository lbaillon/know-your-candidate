"""SQL de l'authentification et du journal admin — voir docs/plans/phase-3-categorisation.md,
section « Authentification admin (D3.1) ». Un seul module, aucune requête ailleurs (CLAUDE.md).
"""

import json
from datetime import UTC, datetime

from kyc_api.config import settings
from kyc_api.db import Queryable
from kyc_api.schemas.admin import AdminUser


async def get_active_admin_user(pool: Queryable, admin_user_id: int) -> AdminUser | None:
    """`actif = false` rend `None` comme un compte inconnu : un admin désactivé perd la main
    immédiatement, sans attendre l'expiration de sa session (plan, étape 5)."""
    row = await pool.fetchrow(
        """
        SELECT id, github_id, github_login, display_name, actif
        FROM admin_user
        WHERE id = $1 AND actif
        """,
        admin_user_id,
    )
    return AdminUser(**dict(row)) if row is not None else None


async def upsert_admin_user(
    pool: Queryable, *, github_id: int, github_login: str, display_name: str
) -> AdminUser:
    row = await pool.fetchrow(
        """
        INSERT INTO admin_user (github_id, github_login, display_name)
        VALUES ($1, $2, $3)
        ON CONFLICT (github_id) DO UPDATE SET
            github_login = EXCLUDED.github_login,
            display_name = EXCLUDED.display_name,
            last_seen_at = now()
        RETURNING id, github_id, github_login, display_name, actif
        """,
        github_id,
        github_login,
        display_name,
    )
    assert row is not None
    return AdminUser(**dict(row))


async def get_worker_status(pool: Queryable) -> str:
    """Même règle que `/healthz` (routers/health.py) : un worker éteint ne doit jamais faire
    tomber le back-office, seulement le signaler sur le tableau de bord."""
    last_seen_at = await pool.fetchval("SELECT max(last_seen_at) FROM worker_heartbeat")
    if last_seen_at is None:
        return "unknown"
    assert isinstance(last_seen_at, datetime)
    age = (datetime.now(UTC) - last_seen_at).total_seconds()
    return "ok" if age <= settings.worker_stale_after_seconds else "stale"


async def log_admin_action(
    pool: Queryable,
    *,
    admin_user_id: int | None,
    action: str,
    target: str | None = None,
    detail: dict[str, object] | None = None,
) -> None:
    await pool.execute(
        """
        INSERT INTO admin_action (admin_user_id, action, target, detail)
        VALUES ($1, $2, $3, $4)
        """,
        admin_user_id,
        action,
        target,
        json.dumps(detail or {}),
    )
