"""Insertion de jeux de données réalistes dans la transaction du test — voir
docs/plans/phase-2-api-ui.md, section « Stratégie de test ». Pas de dump : chaque test construit
exactement ce dont il a besoin. Complété au fil des commits qui en ont besoin.
"""

from datetime import date, timedelta

import asyncpg


async def insert_person(
    conn: asyncpg.Connection,
    *,
    an_uid: str | None = None,
    wikidata_qid: str | None = None,
    prenom: str | None = "Jean",
    nom: str | None = "Dupont",
    civilite: str | None = "M.",
) -> int:
    """`an_uid` par défaut si ni l'un ni l'autre n'est fourni : la contrainte
    `person_a_au_moins_un_identifiant` (migration 0005) l'exige.
    """
    if an_uid is None and wikidata_qid is None:
        an_uid = f"PA-test-{id(object())}"
    row = await conn.fetchrow(
        """
        INSERT INTO person (an_uid, wikidata_qid, civilite, prenom, nom)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        """,
        an_uid,
        wikidata_qid,
        civilite,
        prenom,
        nom,
    )
    assert row is not None
    return row["id"]


async def insert_slug(
    conn: asyncpg.Connection, *, person_id: int, slug: str, is_current: bool = True
) -> None:
    await conn.execute(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ($1, $2, $3)",
        slug,
        person_id,
        is_current,
    )


async def insert_candidate(
    conn: asyncpg.Connection,
    *,
    person_id: int,
    statut: str = "declare",
    source_url: str = "https://example.org/source",
    source_date: date = date(2026, 1, 1),
    note: str | None = None,
) -> None:
    await conn.execute(
        """
        INSERT INTO candidate (person_id, statut, source_url, source_date, note)
        VALUES ($1, $2, $3, $4, $5)
        """,
        person_id,
        statut,
        source_url,
        source_date,
        note,
    )


async def insert_organe(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    code_type: str = "GP",
    libelle: str = "Groupe de test",
    libelle_abrege: str | None = None,
    is_non_inscrit: bool = False,
    legislature: int | None = None,
) -> int:
    row = await conn.fetchrow(
        """
        INSERT INTO organe (an_uid, code_type, libelle, libelle_abrege, is_non_inscrit, legislature)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        """,
        an_uid,
        code_type,
        libelle,
        libelle_abrege,
        is_non_inscrit,
        legislature,
    )
    assert row is not None
    return row["id"]


async def insert_mandat(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    person_id: int,
    organe_id: int,
    type_organe: str = "GP",
    debut: date,
    fin: date | None = None,
    legislature: int | None = None,
) -> None:
    await conn.execute(
        """
        INSERT INTO mandat (an_uid, person_id, organe_id, type_organe, period, legislature)
        VALUES ($1, $2, $3, $4, daterange($5, $6, '[)'), $7)
        """,
        an_uid,
        person_id,
        organe_id,
        type_organe,
        debut,
        (fin + timedelta(days=1)) if fin else None,
        legislature,
    )


async def refresh_person_apercu(conn: asyncpg.Connection) -> None:
    """Sans `CONCURRENTLY` : `REFRESH MATERIALIZED VIEW CONCURRENTLY` ne peut pas s'exécuter dans
    une transaction, et chaque test tourne dans une transaction annulée (voir
    docs/plans/phase-2-api-ui.md, « Vues matérialisées et tests »). Le job du worker, lui, garde
    `CONCURRENTLY`.
    """
    await conn.execute("REFRESH MATERIALIZED VIEW person_apercu")
