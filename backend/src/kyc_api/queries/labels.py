"""SQL de la catégorisation (`scrutin_label`, `label_revision`, `scrutin_axis_estimate`) — voir
docs/plans/phase-3-categorisation.md. Un seul module, aucune requête ailleurs (CLAUDE.md).
"""

import json
from collections import defaultdict
from dataclasses import dataclass
from decimal import Decimal

from kyc_api.db import Queryable, WritableQueryable
from kyc_api.schemas.common import Pagination
from kyc_api.schemas.label import (
    AxisEstimate,
    CategorizedScrutinSummary,
    LabelEntry,
    LabelRevisionEntry,
)

PUBLIC_LISTING_PAGE_SIZE = 30


async def get_next_to_categorize(
    pool: Queryable, *, excluded_scrutin_ids: list[int]
) -> tuple[int, int] | None:
    """`(legislature, numero)` du prochain scrutin de la file, ou `None` si elle est vide. La
    priorisation (`rang_priorite`, `part_minoritaire DESC`, `date_scrutin DESC`) vit dans la vue
    `scrutin_a_categoriser` (migration 0007), pas ici.
    """
    row = await pool.fetchrow(
        """
        SELECT legislature, numero
        FROM scrutin_a_categoriser
        WHERE NOT est_categorise AND scrutin_id <> ALL($1::bigint[])
        ORDER BY rang_priorite, part_minoritaire DESC NULLS LAST, date_scrutin DESC
        LIMIT 1
        """,
        excluded_scrutin_ids,
    )
    return (row["legislature"], row["numero"]) if row is not None else None


async def get_labels_for_scrutin(pool: Queryable, scrutin_id: int) -> list[LabelEntry]:
    rows = await pool.fetch(
        """
        SELECT t.slug AS theme_slug, t.libelle AS theme_libelle,
               t.libelle_pole_negatif, t.libelle_pole_positif,
               sl.poids::float8 AS poids, sl.position_pour::float8 AS position_pour,
               sl.confiance::float8 AS confiance, sl.justification, sl.method::text AS method,
               au.display_name AS author_display_name, li.filename AS import_filename,
               sl.created_at, sl.updated_at,
               ru.display_name AS reviewed_by_display_name, sl.reviewed_at
        FROM scrutin_label sl
        JOIN theme t ON t.id = sl.theme_id
        JOIN admin_user au ON au.id = sl.author_id
        LEFT JOIN admin_user ru ON ru.id = sl.reviewed_by
        LEFT JOIN label_import li ON li.id = sl.import_id
        WHERE sl.scrutin_id = $1
        ORDER BY t.slug
        """,
        scrutin_id,
    )
    return [LabelEntry(**dict(row)) for row in rows]


async def get_current_estimate(pool: Queryable, scrutin_id: int) -> AxisEstimate | None:
    """`None` la plupart du temps tant que `label_scrutins_heuristic` n'a pas tourné avec un
    ancrage complet (voir phase-3.0-feedback.md, F2) — le formulaire s'en passe très bien."""
    row = await pool.fetchrow(
        """
        SELECT sae.position_pour::float8 AS position_pour, sae.separation::float8 AS separation,
               sae.couverture::float8 AS couverture, sae.axis_version
        FROM scrutin_axis_estimate sae
        JOIN group_axis ga ON ga.version = sae.axis_version
        WHERE sae.scrutin_id = $1 AND sae.strategy = 'group_alignment' AND ga.is_current
        """,
        scrutin_id,
    )
    return AxisEstimate(**dict(row)) if row is not None else None


async def get_revisions_for_scrutin(pool: Queryable, scrutin_id: int) -> list[LabelRevisionEntry]:
    rows = await pool.fetch(
        """
        SELECT lr.avant, lr.apres, lr.method::text AS method,
               au.display_name AS author_display_name, lr.motif, lr.created_at
        FROM label_revision lr
        JOIN admin_user au ON au.id = lr.author_id
        WHERE lr.scrutin_id = $1
        ORDER BY lr.created_at DESC, lr.id DESC
        """,
        scrutin_id,
    )
    return [
        LabelRevisionEntry(
            avant=json.loads(row["avant"]),
            apres=json.loads(row["apres"]),
            method=row["method"],
            author_display_name=row["author_display_name"],
            motif=row["motif"],
            created_at=row["created_at"],
        )
        for row in rows
    ]


@dataclass
class LabelWrite:
    theme_id: int
    theme_slug: str
    poids: Decimal
    position_pour: Decimal | None
    confiance: Decimal
    justification: str


def to_numeric_4_3(value: str) -> Decimal:
    """Quantifie à trois décimales exactement : c'est ce qui rend `avant`/`apres` comparables
    caractère près (D3.13) et ce que `numeric(4,3)` porte réellement en base. Lève
    `decimal.InvalidOperation` sur une entrée non numérique — laissé remonter, l'appelant décide
    du message d'erreur adressé à l'admin.
    """
    return Decimal(value).quantize(Decimal("0.001"))


async def replace_labels(
    conn: WritableQueryable,
    *,
    scrutin_id: int,
    entries: list[LabelWrite],
    method: str,
    author_id: int,
    motif: str | None = None,
) -> str:
    """Remplace intégralement les catégorisations d'un scrutin dans une seule transaction (plan,
    section « Back-office : la file de travail ») : suppression, insertion, une ligne
    `label_revision` (avant/après, thèmes triés par slug) **si et seulement si** l'état a changé,
    une ligne `admin_action`. `entries` vide retire toute catégorisation (suppression, motif
    obligatoire côté appelant). Renvoie l'action journalisée.
    """
    async with conn.transaction():
        before_rows = await conn.fetch(
            """
            SELECT t.slug, sl.poids::text AS poids, sl.position_pour::text AS position_pour,
                   sl.confiance::text AS confiance, sl.justification
            FROM scrutin_label sl
            JOIN theme t ON t.id = sl.theme_id
            WHERE sl.scrutin_id = $1
            ORDER BY t.slug
            """,
            scrutin_id,
        )
        avant = [dict(row) for row in before_rows]
        had_existing = len(avant) > 0

        await conn.execute("DELETE FROM scrutin_label WHERE scrutin_id = $1", scrutin_id)

        for entry in entries:
            await conn.execute(
                """
                INSERT INTO scrutin_label
                    (scrutin_id, theme_id, poids, position_pour, confiance, justification,
                     method, author_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                """,
                scrutin_id,
                entry.theme_id,
                entry.poids,
                entry.position_pour,
                entry.confiance,
                entry.justification,
                method,
                author_id,
            )

        apres = sorted(
            (
                {
                    "slug": e.theme_slug,
                    "poids": str(e.poids),
                    "position_pour": (
                        str(e.position_pour) if e.position_pour is not None else None
                    ),
                    "confiance": str(e.confiance),
                    "justification": e.justification,
                }
                for e in entries
            ),
            key=lambda d: d["slug"] or "",
        )

        if not entries:
            action = "categorisation_suppression"
        elif had_existing:
            action = "categorisation_modification"
        else:
            action = "categorisation_creation"

        if avant != apres:
            await conn.execute(
                """
                INSERT INTO label_revision (scrutin_id, avant, apres, method, author_id, motif)
                VALUES ($1, $2::jsonb, $3::jsonb, $4, $5, $6)
                """,
                scrutin_id,
                json.dumps(avant),
                json.dumps(apres),
                method,
                author_id,
                motif,
            )

        await conn.execute(
            """
            INSERT INTO admin_action (admin_user_id, action, target, detail)
            VALUES ($1, $2, $3, '{}'::jsonb)
            """,
            author_id,
            action,
            str(scrutin_id),
        )

    return action


async def list_categorized_scrutins(
    pool: Queryable, *, theme_slug: str | None, legislature: int | None, page: int
) -> tuple[list[CategorizedScrutinSummary], Pagination]:
    """Les scrutins catégorisés, publics (`/scrutins`, `/theme/{slug}`) — jamais leur mesure
    automatique (D3.8), qui n'apparaît nulle part ici."""
    page = max(page, 1)
    offset = (page - 1) * PUBLIC_LISTING_PAGE_SIZE

    where = """
        EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id)
        AND (
            $1::text IS NULL
            OR EXISTS (
                SELECT 1 FROM scrutin_label sl
                JOIN theme t ON t.id = sl.theme_id
                WHERE sl.scrutin_id = s.id AND t.slug = $1
            )
        )
        AND ($2::smallint IS NULL OR s.legislature = $2)
    """

    rows = await pool.fetch(
        f"""
        SELECT s.id AS scrutin_id, s.legislature, s.numero, s.titre, s.date_scrutin
        FROM scrutin s
        WHERE {where}
        ORDER BY s.date_scrutin DESC, s.id
        LIMIT $3 OFFSET $4
        """,
        theme_slug,
        legislature,
        PUBLIC_LISTING_PAGE_SIZE,
        offset,
    )
    total = await pool.fetchval(
        f"SELECT count(*) FROM scrutin s WHERE {where}", theme_slug, legislature
    )
    assert isinstance(total, int)

    scrutin_ids = [row["scrutin_id"] for row in rows]
    themes_by_scrutin: dict[int, list[str]] = defaultdict(list)
    if scrutin_ids:
        theme_rows = await pool.fetch(
            """
            SELECT sl.scrutin_id, t.libelle
            FROM scrutin_label sl
            JOIN theme t ON t.id = sl.theme_id
            WHERE sl.scrutin_id = ANY($1::bigint[])
            ORDER BY sl.scrutin_id, t.libelle
            """,
            scrutin_ids,
        )
        for row in theme_rows:
            themes_by_scrutin[row["scrutin_id"]].append(row["libelle"])

    summaries = [
        CategorizedScrutinSummary(
            legislature=row["legislature"],
            numero=row["numero"],
            titre=row["titre"],
            date_scrutin=row["date_scrutin"],
            themes=themes_by_scrutin.get(row["scrutin_id"], []),
        )
        for row in rows
    ]
    return summaries, Pagination(page=page, page_size=PUBLIC_LISTING_PAGE_SIZE, total=total)
