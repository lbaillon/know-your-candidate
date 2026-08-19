"""SQL de l'import (dépôt, aperçu, application) — voir docs/plans/phase-3-categorisation.md,
section « Import : validation, aperçu, application ». Un seul module, aucune requête ailleurs
(CLAUDE.md). Recherches groupées (`= ANY($1)`) plutôt qu'une par ligne : jusqu'à 25 000 lignes
possibles (D3.15).
"""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from decimal import Decimal
from typing import TYPE_CHECKING

from kyc_api.db import Queryable, WritableQueryable

if TYPE_CHECKING:
    # Import différé : kyc_api.import_validation importe déjà ce module pour les recherches
    # groupées (get_active_theme_slugs, etc.) — un import direct ici créerait un cycle. Ces deux
    # noms ne servent qu'à l'annotation de type, jamais à l'exécution (asdict() se passe très bien
    # de connaître la classe concrète).
    from kyc_api.import_validation import ImportPlan, ScrutinLabelData


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


async def update_apercu(pool: Queryable, import_id: int, apercu: dict) -> None:
    """La base a changé depuis le dépôt (plan, étape 2 de l'application) : on remplace l'aperçu
    périmé par le nouveau plutôt que d'appliquer une décision prise sur un état qui n'existe
    plus."""
    await pool.execute(
        "UPDATE label_import SET apercu = $2::jsonb WHERE id = $1", import_id, json.dumps(apercu)
    )


async def mark_rejected(pool: Queryable, import_id: int, *, decided_by: int) -> None:
    await pool.execute(
        """
        UPDATE label_import SET status = 'rejected', decided_by = $2, decided_at = now()
        WHERE id = $1
        """,
        import_id,
        decided_by,
    )


async def mark_applied(pool: Queryable, import_id: int, *, rapport: dict, decided_by: int) -> None:
    await pool.execute(
        """
        UPDATE label_import
        SET status = 'applied', rapport = $2::jsonb, decided_by = $3, decided_at = now()
        WHERE id = $1
        """,
        import_id,
        json.dumps(rapport),
        decided_by,
    )


async def apply_plan(
    conn: WritableQueryable,
    *,
    plan: ImportPlan,
    import_id: int,
    author_id: int,
    apply_conflicts: bool,
) -> dict:
    """Écrit le plan en SQL ensembliste (D3.14) : un DELETE, un INSERT, un INSERT de révisions —
    jamais une requête par scrutin, quel que soit le nombre de scrutins touchés. `plan` doit avoir
    été recalculé juste avant l'appel (jamais celui de l'aperçu stocké, voir la vérification de
    péremption dans l'appelant).
    """
    theme_ids = await get_theme_ids_by_slug(conn)

    conflits_ignores = [
        s.scrutin_uid for s in plan.scrutins if s.classement == "conflit" and not apply_conflicts
    ]
    touched = [
        s
        for s in plan.scrutins
        if s.classement != "inchange" and (s.classement != "conflit" or apply_conflicts)
    ]

    if not touched:
        return {
            "crees": 0,
            "modifies": 0,
            "ecrases": 0,
            "conflits_ignores": conflits_ignores,
            "scrutins_touches": [],
        }

    scrutin_ids = [s.scrutin_id for s in touched]
    ins_scrutin_ids: list[int] = []
    ins_theme_ids: list[int] = []
    ins_poids: list[Decimal] = []
    ins_position: list[Decimal | None] = []
    ins_confiance: list[Decimal] = []
    ins_justification: list[str] = []
    for s in touched:
        for entry in s.apres:
            ins_scrutin_ids.append(s.scrutin_id)
            ins_theme_ids.append(theme_ids[entry.theme])
            ins_poids.append(Decimal(entry.poids))
            ins_position.append(Decimal(entry.position_pour) if entry.position_pour else None)
            ins_confiance.append(Decimal(entry.confiance))
            ins_justification.append(entry.justification)

    # Clé "slug" (pas "theme") : même forme que kyc_api.queries.labels.replace_labels, pour que
    # label_revision.avant/apres reste comparable structurellement quelle que soit la méthode
    # (manual ou import) qui a écrit la ligne.
    def _to_slug_dict(entry: ScrutinLabelData) -> dict:
        d = asdict(entry)
        d["slug"] = d.pop("theme")
        return d

    rev_scrutin_ids = [s.scrutin_id for s in touched]
    rev_avant = [json.dumps([_to_slug_dict(d) for d in s.avant]) for s in touched]
    rev_apres = [json.dumps([_to_slug_dict(d) for d in s.apres]) for s in touched]
    rev_motif = [
        f"import #{import_id}"
        + (" — écrase une catégorisation relue" if s.classement == "conflit" else "")
        for s in touched
    ]

    async with conn.transaction():
        await conn.execute(
            "DELETE FROM scrutin_label WHERE scrutin_id = ANY($1::bigint[])", scrutin_ids
        )
        await conn.execute(
            """
            INSERT INTO scrutin_label
                (scrutin_id, theme_id, poids, position_pour, confiance, justification, method,
                 author_id, import_id)
            SELECT scrutin_id, theme_id, poids, position_pour, confiance, justification,
                   'import', $7, $8
            FROM UNNEST($1::bigint[], $2::smallint[], $3::numeric[], $4::numeric[], $5::numeric[],
                        $6::text[])
                AS t(scrutin_id, theme_id, poids, position_pour, confiance, justification)
            """,
            ins_scrutin_ids,
            ins_theme_ids,
            ins_poids,
            ins_position,
            ins_confiance,
            ins_justification,
            author_id,
            import_id,
        )
        await conn.execute(
            """
            INSERT INTO label_revision
                (scrutin_id, avant, apres, method, author_id, import_id, motif)
            SELECT scrutin_id, avant::jsonb, apres::jsonb, 'import', $5, $6, motif
            FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::text[])
                AS t(scrutin_id, avant, apres, motif)
            """,
            rev_scrutin_ids,
            rev_avant,
            rev_apres,
            rev_motif,
            author_id,
            import_id,
        )

    return {
        "crees": sum(1 for s in touched if s.classement == "creation"),
        "modifies": sum(1 for s in touched if s.classement == "modification"),
        "ecrases": sum(1 for s in touched if s.classement == "conflit"),
        "conflits_ignores": conflits_ignores,
        "scrutins_touches": [s.scrutin_uid for s in touched],
    }
