"""Insertion de jeux de données réalistes dans la transaction du test — voir
docs/plans/phase-2-api-ui.md, section « Stratégie de test ». Pas de dump : chaque test construit
exactement ce dont il a besoin. Complété au fil des commits qui en ont besoin.
"""

import json
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


async def insert_theme(
    conn: asyncpg.Connection,
    *,
    slug: str,
    rang: int = 1,
    libelle_pole_negatif: str | None = "pôle négatif",
    libelle_pole_positif: str | None = "pôle positif",
    axe_gauche_droite: bool = True,
) -> int:
    row = await conn.fetchrow(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang,
             axe_gauche_droite)
        VALUES ($1, $1, 'description du thème', $2, $3, $4, $5)
        RETURNING id
        """,
        slug,
        libelle_pole_negatif,
        libelle_pole_positif,
        rang,
        axe_gauche_droite,
    )
    assert row is not None
    return row["id"]


async def insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int = 1,
    legislature: int = 17,
    date_scrutin: date = date(2024, 3, 14),
    titre: str = "titre du scrutin",
) -> int:
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', $1, '{}'::jsonb)
        RETURNING id
        """,
        an_uid,
    )
    row = await conn.fetchrow(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, 'assemblee', 'SPO', $5, 'DecompteNominatif', 0, 0, 0, 0, 0, 0, 577,
                $6)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        titre,
        source_document_id,
    )
    assert row is not None
    return row["id"]


async def insert_score_run(
    conn: asyncpg.Connection,
    *,
    is_current: bool = True,
    formula_version: int = 1,
    contributions_min: int = 5,
    scrutins_min_par_theme: int = 10,
    eligible_theme_ids: list[int] | None = None,
) -> int:
    """Un `score_run`, sur le modèle de ce qu'écrit `recompute_scores` — voir
    docs/plans/phase-4-partis-scores.md. `eligible_theme_ids` peuple `counters ->
    'themes_eligibles_ids'`, la seule source que lit `queries/scores.py` pour savoir quels thèmes
    afficher (D4.7) : sans lui, `get_person_orientations` ne rend jamais rien.
    """
    counters = json.dumps({"themes_eligibles_ids": eligible_theme_ids or []})
    row = await conn.fetchrow(
        """
        INSERT INTO score_run
            (formula_version, contributions_min, scrutins_min_par_theme, is_current, finished_at,
             counters)
        VALUES ($1, $2, $3, $4, now(), $5::jsonb)
        RETURNING id
        """,
        formula_version,
        contributions_min,
        scrutins_min_par_theme,
        is_current,
        counters,
    )
    assert row is not None
    return row["id"]


async def insert_score_contribution(
    conn: asyncpg.Connection,
    *,
    run_id: int,
    person_id: int,
    theme_id: int,
    scrutin_id: int,
    position: str,
    apport: float | None,
    poids: float,
    exclusion: str | None = None,
) -> None:
    """`exclusion` vaut automatiquement `'abstention'` pour une abstention (F1,
    docs/plans/phase-4.1-partis-scores.md) : la contrainte
    `score_contribution_abstention_toujours_exclue` l'exige, et les appelants existants qui ne
    connaissent pas encore cette colonne n'ont rien à changer. Passer explicitement
    `'desaccord_mesure'` pour une contribution écartée pour désaccord entre les deux lectures.
    """
    if exclusion is None and position == "abstention":
        exclusion = "abstention"
    await conn.execute(
        """
        INSERT INTO score_contribution
            (run_id, person_id, theme_id, scrutin_id, position, apport, poids, exclusion)
        VALUES ($1, $2, $3, $4, $5::vote_position, $6, $7, $8::contribution_exclusion)
        """,
        run_id,
        person_id,
        theme_id,
        scrutin_id,
        position,
        apport,
        poids,
        exclusion,
    )


async def insert_person_theme_score(
    conn: asyncpg.Connection,
    *,
    run_id: int,
    person_id: int,
    theme_id: int,
    score: float,
    incertitude: float = 0.1,
    contributions: int = 5,
    abstentions: int = 0,
    relues: int = 0,
    ecartes_desaccord: int = 0,
) -> None:
    await conn.execute(
        """
        INSERT INTO person_theme_score
            (run_id, person_id, theme_id, score, incertitude, contributions, abstentions, relues,
             ecartes_desaccord)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        """,
        run_id,
        person_id,
        theme_id,
        score,
        incertitude,
        contributions,
        abstentions,
        relues,
        ecartes_desaccord,
    )


async def insert_mandat_theme_score(
    conn: asyncpg.Connection,
    *,
    run_id: int,
    mandat_id: int,
    theme_id: int,
    score: float,
    cohesion: float = 1.0,
    contributions: int = 10,
) -> None:
    await conn.execute(
        """
        INSERT INTO mandat_theme_score (run_id, mandat_id, theme_id, score, cohesion, contributions)
        VALUES ($1, $2, $3, $4, $5, $6)
        """,
        run_id,
        mandat_id,
        theme_id,
        score,
        cohesion,
        contributions,
    )


async def insert_groupe_theme_score(
    conn: asyncpg.Connection,
    *,
    run_id: int,
    organe_id: int,
    theme_id: int,
    score: float,
    cohesion: float = 1.0,
    contributions: int = 10,
    membres: int = 5,
) -> None:
    await conn.execute(
        """
        INSERT INTO groupe_theme_score
            (run_id, organe_id, theme_id, score, cohesion, contributions, membres)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        """,
        run_id,
        organe_id,
        theme_id,
        score,
        cohesion,
        contributions,
        membres,
    )


async def refresh_score_views(conn: asyncpg.Connection) -> None:
    """Sans `CONCURRENTLY` — même raison que `refresh_person_apercu` : chaque test tourne dans une
    transaction annulée.
    """
    await conn.execute("REFRESH MATERIALIZED VIEW person_theme_score_courant")
    await conn.execute("REFRESH MATERIALIZED VIEW mandat_theme_score_courant")
