"""SQL de l'export (D3.6) — voir docs/plans/phase-3-categorisation.md, section « Le cycle
export / import ». Un seul module, aucune requête ailleurs (CLAUDE.md). Trois requêtes bulk
plutôt qu'une par scrutin (jusqu'à 25 000 lignes, D3.15) : un `WHERE ... = ANY($ids)` pour les
groupes et pour les catégorisations, assemblés en mémoire ensuite.
"""

from collections import defaultdict

from kyc_api.db import Queryable
from kyc_api.schemas.import_ import ExportCategorisation, ExportGroupe, ExportScrutin

_STATUTS = ("non_categorises", "categorises", "tous")


def an_url(legislature: int, numero: int) -> str:
    return f"https://www.assemblee-nationale.fr/dyn/{legislature}/scrutins/{numero}"


async def list_scrutins_for_export(
    pool: Queryable,
    *,
    statut: str,
    theme_slug: str | None = None,
    limite: int | None = None,
) -> list[ExportScrutin]:
    if statut not in _STATUTS:
        raise ValueError(f"statut inconnu : {statut!r} (attendu : {', '.join(_STATUTS)})")

    statut_clause = {
        "non_categorises": (
            "AND NOT EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id)"
        ),
        "categorises": "AND EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id)",
        "tous": "",
    }[statut]

    rows = await pool.fetch(
        f"""
        SELECT s.id AS scrutin_id, s.an_uid, s.legislature, s.numero, s.date_scrutin, s.titre,
               s.type_libelle, s.sort_code, s.pour, s.contre, s.abstentions, s.non_votants,
               s.participation::float8 AS participation
        FROM scrutin s
        CROSS JOIN corpus_parametre c
        WHERE s.chambre = 'assemblee' AND s.participation >= c.participation_min
        {statut_clause}
        AND (
            $1::text IS NULL
            OR EXISTS (
                SELECT 1 FROM scrutin_label sl
                JOIN theme t ON t.id = sl.theme_id
                WHERE sl.scrutin_id = s.id AND t.slug = $1
            )
        )
        ORDER BY s.date_scrutin DESC, s.id
        LIMIT $2
        """,
        theme_slug,
        limite,
    )
    scrutin_ids = [row["scrutin_id"] for row in rows]
    groupes_by_scrutin = await _fetch_groupes(pool, scrutin_ids)
    categorisations_by_scrutin = await _fetch_categorisations(pool, scrutin_ids)

    return [
        ExportScrutin(
            scrutin_uid=row["an_uid"],
            legislature=row["legislature"],
            numero=row["numero"],
            date=row["date_scrutin"],
            titre=row["titre"],
            type=row["type_libelle"] or "",
            sort=row["sort_code"],
            pour=row["pour"],
            contre=row["contre"],
            abstentions=row["abstentions"],
            non_votants=row["non_votants"],
            participation=(
                f"{row['participation']:.3f}" if row["participation"] is not None else None
            ),
            groupes=groupes_by_scrutin.get(row["scrutin_id"], []),
            url_an=an_url(row["legislature"], row["numero"]),
            categorisation=categorisations_by_scrutin.get(row["scrutin_id"], []),
        )
        for row in rows
    ]


async def _fetch_groupes(pool: Queryable, scrutin_ids: list[int]) -> dict[int, list[ExportGroupe]]:
    if not scrutin_ids:
        return {}
    rows = await pool.fetch(
        """
        SELECT sg.scrutin_id, coalesce(o.libelle_abrege, o.libelle, 'non identifié') AS abrege,
               sg.nombre_membres, sg.pour, sg.contre, sg.abstentions, sg.position_majoritaire
        FROM scrutin_groupe sg
        LEFT JOIN organe o ON o.id = sg.organe_id
        WHERE sg.scrutin_id = ANY($1::bigint[])
        ORDER BY sg.scrutin_id, sg.rang
        """,
        scrutin_ids,
    )
    out: dict[int, list[ExportGroupe]] = defaultdict(list)
    for row in rows:
        out[row["scrutin_id"]].append(
            ExportGroupe(
                abrege=row["abrege"],
                membres=row["nombre_membres"],
                pour=row["pour"],
                contre=row["contre"],
                abstentions=row["abstentions"],
                position_majoritaire=row["position_majoritaire"],
            )
        )
    return out


async def _fetch_categorisations(
    pool: Queryable, scrutin_ids: list[int]
) -> dict[int, list[ExportCategorisation]]:
    if not scrutin_ids:
        return {}
    rows = await pool.fetch(
        """
        SELECT sl.scrutin_id, t.slug AS theme, sl.poids::float8 AS poids,
               sl.position_pour::float8 AS position_pour, sl.confiance::float8 AS confiance,
               sl.justification
        FROM scrutin_label sl
        JOIN theme t ON t.id = sl.theme_id
        WHERE sl.scrutin_id = ANY($1::bigint[])
        ORDER BY sl.scrutin_id, t.slug
        """,
        scrutin_ids,
    )
    out: dict[int, list[ExportCategorisation]] = defaultdict(list)
    for row in rows:
        out[row["scrutin_id"]].append(
            ExportCategorisation(
                theme=row["theme"],
                poids=f"{row['poids']:.3f}",
                position_pour=(
                    f"{row['position_pour']:.3f}" if row["position_pour"] is not None else None
                ),
                confiance=f"{row['confiance']:.3f}",
                justification=row["justification"],
            )
        )
    return out
