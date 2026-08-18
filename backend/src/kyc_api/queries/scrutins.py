"""SQL sur `scrutin` / `scrutin_groupe` — page d'un scrutin. Tout le SQL vit ici, aucune requête
ailleurs (CLAUDE.md).
"""

from asyncpg import Record

from kyc_api.db import Queryable
from kyc_api.schemas.common import Source
from kyc_api.schemas.scrutin import ScrutinDetail, ScrutinGroupe

# Licence AN, rappelée à chaque bloc de source (CLAUDE.md, « chaque bloc affiche sa source »).
AN_LICENCE = "Licence Ouverte (Etalab)"

_SELECT = """
    SELECT sc.id AS scrutin_id, sc.an_uid, sc.numero, sc.legislature, sc.date_scrutin,
           sc.chambre::text AS chambre, sc.titre, sc.type_libelle, sc.sort_code, sc.demandeur,
           sc.nombre_votants, sc.suffrages_exprimes, sc.pour, sc.contre, sc.abstentions,
           sc.non_votants, sc.non_votants_volontaires, sc.effectif,
           sc.participation::float8 AS participation,
           sd.url AS source_url, sd.fetched_at AS source_fetched_at
    FROM scrutin sc
    JOIN source_document sd ON sd.id = sc.source_document_id
"""


async def get_by_legislature_numero(
    pool: Queryable, legislature: int, numero: int
) -> ScrutinDetail | None:
    row = await pool.fetchrow(
        _SELECT + " WHERE sc.legislature = $1 AND sc.numero = $2", legislature, numero
    )
    return await _assemble(pool, row)


async def get_by_an_uid(pool: Queryable, an_uid: str) -> ScrutinDetail | None:
    row = await pool.fetchrow(_SELECT + " WHERE sc.an_uid = $1", an_uid)
    return await _assemble(pool, row)


async def _assemble(pool: Queryable, row: Record | None) -> ScrutinDetail | None:
    if row is None:
        return None

    groupe_rows = await pool.fetch(
        """
        SELECT sg.rang, o.an_uid AS organe_an_uid, o.libelle, sg.nombre_membres,
               sg.position_majoritaire, sg.pour, sg.contre, sg.abstentions, sg.non_votants
        FROM scrutin_groupe sg
        LEFT JOIN organe o ON o.id = sg.organe_id
        WHERE sg.scrutin_id = $1
        ORDER BY sg.rang
        """,
        row["scrutin_id"],
    )
    groupes = [
        ScrutinGroupe(
            rang=g["rang"],
            organe_an_uid=g["organe_an_uid"],
            libelle=g["libelle"],
            nombre_membres=g["nombre_membres"],
            position_majoritaire=g["position_majoritaire"],
            pour=g["pour"],
            contre=g["contre"],
            abstentions=g["abstentions"],
            non_votants=g["non_votants"],
        )
        for g in groupe_rows
    ]

    return ScrutinDetail(
        scrutin_id=row["scrutin_id"],
        an_uid=row["an_uid"],
        numero=row["numero"],
        legislature=row["legislature"],
        date_scrutin=row["date_scrutin"],
        chambre=row["chambre"],
        titre=row["titre"],
        type_libelle=row["type_libelle"],
        sort_code=row["sort_code"],
        demandeur=row["demandeur"],
        nombre_votants=row["nombre_votants"],
        suffrages_exprimes=row["suffrages_exprimes"],
        pour=row["pour"],
        contre=row["contre"],
        abstentions=row["abstentions"],
        non_votants=row["non_votants"],
        non_votants_volontaires=row["non_votants_volontaires"],
        effectif=row["effectif"],
        participation=row["participation"],
        source=Source(
            url=row["source_url"], fetched_at=row["source_fetched_at"], licence=AN_LICENCE
        ),
        groupes=groupes,
    )
