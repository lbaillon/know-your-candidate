"""SQL de l'import (dépôt, aperçu, application) — voir docs/plans/phase-3-categorisation.md,
section « Import : validation, aperçu, application ». Un seul module, aucune requête ailleurs
(CLAUDE.md). Recherches groupées (`= ANY($1)`) plutôt qu'une par ligne : jusqu'à 25 000 lignes
possibles (D3.15).
"""

import json
from dataclasses import dataclass

from kyc_api.db import Queryable, WritableQueryable


@dataclass
class ExistingLabel:
    scrutin_id: int
    theme_slug: str
    poids: str
    position_pour: str | None
    confiance: str
    justification: str
    method: str
    reviewed_at: object | None


async def resolve_scrutin_ids(pool: Queryable, scrutin_uids: list[str]) -> dict[str, int]:
    """`{an_uid: id}` pour les uids qui existent réellement — un uid absent du résultat est un uid
    inconnu en base (job de l'appelant de le signaler)."""
    if not scrutin_uids:
        return {}
    rows = await pool.fetch(
        "SELECT an_uid, id FROM scrutin WHERE an_uid = ANY($1::text[])", scrutin_uids
    )
    return {row["an_uid"]: row["id"] for row in rows}


async def get_active_theme_slugs(pool: Queryable) -> set[str]:
    rows = await pool.fetch("SELECT slug FROM theme WHERE actif")
    return {row["slug"] for row in rows}


async def theme_has_axis_by_slug(pool: Queryable) -> dict[str, bool]:
    rows = await pool.fetch("SELECT slug, libelle_pole_negatif IS NOT NULL AS has_axis FROM theme")
    return {row["slug"]: row["has_axis"] for row in rows}


async def get_theme_ids_by_slug(pool: Queryable) -> dict[str, int]:
    rows = await pool.fetch("SELECT slug, id FROM theme")
    return {row["slug"]: row["id"] for row in rows}


async def get_existing_labels(pool: Queryable, scrutin_ids: list[int]) -> list[ExistingLabel]:
    if not scrutin_ids:
        return []
    rows = await pool.fetch(
        """
        SELECT sl.scrutin_id, t.slug AS theme_slug, sl.poids::text AS poids,
               sl.position_pour::text AS position_pour, sl.confiance::text AS confiance,
               sl.justification, sl.method::text AS method, sl.reviewed_at
        FROM scrutin_label sl
        JOIN theme t ON t.id = sl.theme_id
        WHERE sl.scrutin_id = ANY($1::bigint[])
        ORDER BY sl.scrutin_id, t.slug
        """,
        scrutin_ids,
    )
    return [ExistingLabel(**dict(row)) for row in rows]


async def create_label_import(
    conn: WritableQueryable | Queryable,
    *,
    filename: str,
    format: str,
    schema_version: int,
    generateur: str | None,
    contenu: str,
    content_hash: str,
    apercu: dict,
    uploaded_by: int,
) -> int:
    row = await conn.fetchrow(
        """
        INSERT INTO label_import
            (filename, format, schema_version, generateur, contenu, content_hash, apercu,
             uploaded_by)
        VALUES ($1, $2, $3, $4, $5, $6, $7::jsonb, $8)
        RETURNING id
        """,
        filename,
        format,
        schema_version,
        generateur,
        contenu,
        content_hash,
        json.dumps(apercu),
        uploaded_by,
    )
    assert row is not None
    return row["id"]


async def get_label_import(pool: Queryable, import_id: int) -> dict | None:
    row = await pool.fetchrow(
        """
        SELECT id, filename, format, schema_version, generateur, contenu, content_hash, status,
               apercu, rapport, uploaded_by, uploaded_at, decided_by, decided_at
        FROM label_import
        WHERE id = $1
        """,
        import_id,
    )
    if row is None:
        return None
    data = dict(row)
    data["apercu"] = json.loads(data["apercu"])
    data["rapport"] = json.loads(data["rapport"]) if data["rapport"] is not None else None
    return data
