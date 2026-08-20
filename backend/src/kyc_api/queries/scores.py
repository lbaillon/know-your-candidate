"""SQL sur les scores et contributions — voir docs/plans/phase-4-partis-scores.md. Tout le SQL vit
ici, aucune requête ailleurs (CLAUDE.md) ; les gabarits et l'API consomment les mêmes modèles
Pydantic (D2.10).
"""

from datetime import date, timedelta
from itertools import groupby

from asyncpg import Record

from kyc_api.db import Queryable
from kyc_api.schemas.score import (
    GroupeOrientation,
    MandatOrientation,
    MandatOrientationGroup,
    PersonOrientation,
    ScoreContributionDetail,
)
from kyc_api.schemas.theme import Theme

_THEME_FIELDS = (
    "t.id, t.slug, t.libelle, t.description, t.libelle_pole_negatif, t.libelle_pole_positif, t.rang"
)


def _theme_from_row(row: Record) -> Theme:
    return Theme(
        id=row["id"],
        slug=row["slug"],
        libelle=row["libelle"],
        description=row["description"],
        libelle_pole_negatif=row["libelle_pole_negatif"],
        libelle_pole_positif=row["libelle_pole_positif"],
        rang=row["rang"],
    )


async def get_person_orientations(pool: Queryable, person_id: int) -> list[PersonOrientation]:
    """Une ligne par thème éligible du run courant (D4.7) — figé au moment de ce run, pas
    recalculé contre la valeur *actuelle* de `score_parametre` qui a pu changer depuis. Même sans
    score personnel, le thème apparaît avec son compte réel de contributions : `score_contribution`
    n'applique jamais le seuil `contributions_min` à l'écriture (D4.2).
    """
    rows = await pool.fetch(
        f"""
        WITH courant AS (
            SELECT id, counters FROM score_run WHERE is_current
        ),
        eligibles AS (
            SELECT (jsonb_array_elements_text(
                        coalesce(counters -> 'themes_eligibles_ids', '[]'::jsonb)
                    ))::smallint AS theme_id
            FROM courant
        )
        SELECT {_THEME_FIELDS},
               ptc.score::float8 AS score, ptc.incertitude::float8 AS incertitude, ptc.relues,
               coalesce(sc.contributions, 0) AS contributions,
               coalesce(sc.abstentions, 0) AS abstentions
        FROM eligibles e
        JOIN theme t ON t.id = e.theme_id
        JOIN courant c ON true
        LEFT JOIN person_theme_score_courant ptc ON ptc.person_id = $1 AND ptc.theme_id = t.id
        LEFT JOIN LATERAL (
            SELECT count(*) FILTER (WHERE scc.poids > 0)         AS contributions,
                   count(*) FILTER (WHERE scc.position = 'abstention') AS abstentions
            FROM score_contribution scc
            WHERE scc.run_id = c.id AND scc.person_id = $1 AND scc.theme_id = t.id
        ) sc ON true
        ORDER BY t.rang
        """,
        person_id,
    )
    return [
        PersonOrientation(
            theme=_theme_from_row(row),
            score=row["score"],
            incertitude=row["incertitude"],
            relues=row["relues"],
            contributions=row["contributions"],
            abstentions=row["abstentions"],
        )
        for row in rows
    ]


async def get_person_theme_contributions(
    pool: Queryable, person_id: int, theme_id: int
) -> list[ScoreContributionDetail]:
    """Toutes les contributions du run courant pour ce couple (personne, thème) — jamais tronqué
    (D4, « la page d'explication liste toutes les contributions, pas les cinq premières »).
    """
    rows = await pool.fetch(
        """
        SELECT s.id AS scrutin_id, s.an_uid AS scrutin_an_uid, s.legislature, s.numero, s.titre,
               s.date_scrutin, sc.position::text AS position,
               sc.apport::float8 AS apport, sc.poids::float8 AS poids
        FROM score_contribution sc
        JOIN score_run sr ON sr.id = sc.run_id AND sr.is_current
        JOIN scrutin s ON s.id = sc.scrutin_id
        WHERE sc.person_id = $1 AND sc.theme_id = $2
        ORDER BY s.date_scrutin DESC, s.id DESC
        """,
        person_id,
        theme_id,
    )
    return [ScoreContributionDetail(**dict(row)) for row in rows]


async def get_person_mandat_orientations(
    pool: Queryable, person_id: int
) -> list[MandatOrientation]:
    """Les positions des groupes où la personne a effectivement siégé, restreintes à la période de
    chaque mandat (D4.12) — jamais le repli « positions du parti » (D4.8, refusé explicitement).
    """
    rows = await pool.fetch(
        f"""
        SELECT o.an_uid AS organe_an_uid, o.libelle AS organe_libelle,
               o.libelle_abrege AS organe_libelle_abrege,
               lower(m.period) AS debut, upper(m.period) AS fin_exclusive,
               {_THEME_FIELDS},
               mts.score::float8 AS score, mts.cohesion::float8 AS cohesion, mts.contributions
        FROM mandat m
        JOIN organe o ON o.id = m.organe_id
        JOIN mandat_theme_score_courant mts ON mts.mandat_id = m.id
        JOIN theme t ON t.id = mts.theme_id
        WHERE m.person_id = $1 AND m.type_organe = 'GP'
        ORDER BY m.period, t.rang
        """,
        person_id,
    )
    orientations = []
    for row in rows:
        fin_exclusive = row["fin_exclusive"]
        fin = fin_exclusive - timedelta(days=1) if fin_exclusive is not None else None
        orientations.append(
            MandatOrientation(
                organe_an_uid=row["organe_an_uid"],
                organe_libelle=row["organe_libelle"],
                organe_libelle_abrege=row["organe_libelle_abrege"],
                debut=row["debut"],
                fin=fin,
                theme=_theme_from_row(row),
                score=row["score"],
                cohesion=row["cohesion"],
                contributions=row["contributions"],
            )
        )
    return orientations


def _mandat_group_key(o: MandatOrientation) -> tuple[str, str, str | None, date, date | None]:
    return (o.organe_an_uid, o.organe_libelle, o.organe_libelle_abrege, o.debut, o.fin)


def group_mandat_orientations(
    orientations: list[MandatOrientation],
) -> list[MandatOrientationGroup]:
    """Un bloc par mandat (D4.12) : `get_person_mandat_orientations` trie déjà par période, ce
    regroupement replie donc des lignes déjà consécutives, il ne trie jamais rien lui-même.
    """
    groups = []
    for key, members in groupby(orientations, key=_mandat_group_key):
        organe_an_uid, organe_libelle, organe_libelle_abrege, debut, fin = key
        groups.append(
            MandatOrientationGroup(
                organe_an_uid=organe_an_uid,
                organe_libelle=organe_libelle,
                organe_libelle_abrege=organe_libelle_abrege,
                debut=debut,
                fin=fin,
                orientations=list(members),
            )
        )
    return groups


async def get_groupe_orientations(pool: Queryable, organe_id: int) -> list[GroupeOrientation]:
    """Le score d'un groupe sur toute son existence (D4.4), pour `/groupe/{an_uid}`."""
    rows = await pool.fetch(
        f"""
        SELECT {_THEME_FIELDS},
               gts.score::float8 AS score, gts.cohesion::float8 AS cohesion,
               gts.contributions, gts.membres
        FROM groupe_theme_score gts
        JOIN score_run sr ON sr.id = gts.run_id AND sr.is_current
        JOIN theme t ON t.id = gts.theme_id
        WHERE gts.organe_id = $1
        ORDER BY t.rang
        """,
        organe_id,
    )
    return [
        GroupeOrientation(
            theme=_theme_from_row(row),
            score=row["score"],
            cohesion=row["cohesion"],
            contributions=row["contributions"],
            membres=row["membres"],
        )
        for row in rows
    ]
