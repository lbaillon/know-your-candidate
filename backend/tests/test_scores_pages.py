"""Tests des règles d'affichage non négociables de la fiche personne enrichie — voir
docs/plans/phase-4-partis-scores.md, section « Pages et affichage » : le pôle nommé avant le
chiffre, le niveau de preuve toujours présent, « données insuffisantes » sous le seuil, et les deux
états vides expliqués (jamais siégé / pas encore catégorisé — cas Retailleau).
"""

from datetime import date

import asyncpg
from httpx import AsyncClient

import factories


async def test_a_scored_orientation_names_the_pole_before_the_number(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(
        db_conn,
        slug="social-fiscalite",
        libelle_pole_negatif="redistribution et protection sociale étendue",
        libelle_pole_positif="maîtrise de la dépense et de la fiscalité",
    )
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    scrutin_id = await factories.insert_scrutin(db_conn, an_uid="SC1")
    await factories.insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutin_id,
        position="pour",
        apport=0.42,
        poids=1.0,
    )
    await factories.insert_person_theme_score(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        score=0.42,
        incertitude=0.1,
        contributions=5,
        relues=0,
    )
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert response.status_code == 200
    text = response.text
    pole_pos = text.find("maîtrise de la dépense et de la fiscalité")
    chiffre_pos = text.find("+0.42")
    assert pole_pos != -1
    assert chiffre_pos != -1
    assert pole_pos < chiffre_pos, "le pôle doit apparaître avant le chiffre, jamais l'inverse"


async def test_a_scored_orientation_always_shows_its_evidence_level(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(db_conn, slug="securite")
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-un")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    scrutin_id = await factories.insert_scrutin(db_conn, an_uid="SC1")
    await factories.insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutin_id,
        position="pour",
        apport=0.5,
        poids=1.0,
    )
    await factories.insert_person_theme_score(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        score=0.5,
        contributions=5,
        relues=0,
    )
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-un")

    assert response.status_code == 200
    assert "aucune de ces catégorisations n'a encore été relue par un humain" in response.text


async def test_below_threshold_shows_insufficient_data_with_the_real_count(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(db_conn, slug="agriculture")
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-deux")
    run_id = await factories.insert_score_run(
        db_conn, eligible_theme_ids=[theme_id], contributions_min=5
    )
    for i in range(3):
        scrutin_id = await factories.insert_scrutin(db_conn, an_uid=f"SC{i}", numero=i)
        await factories.insert_score_contribution(
            db_conn,
            run_id=run_id,
            person_id=person_id,
            theme_id=theme_id,
            scrutin_id=scrutin_id,
            position="pour",
            apport=0.5,
            poids=1.0,
        )
    # Pas de ligne person_theme_score : 3 < contributions_min (5).
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-deux")

    assert response.status_code == 200
    assert "données insuffisantes" in response.text
    assert "seulement 3 scrutins" in response.text
    # Pas de zéro affiché comme un score, pas de curseur pour ce thème.
    assert "score-cursor" not in response.text


async def test_a_person_with_no_mandate_or_vote_gets_our_coverage_not_a_claim_about_the_world(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    """Jamais « n'a jamais siégé » (F2, docs/plans/phase-2.1-fix.md) : l'absence de données ne
    prouve rien sur le monde, seulement sur notre référentiel.
    """
    person_id = await factories.insert_person(db_conn, wikidata_qid="Q1", an_uid=None)
    await factories.insert_slug(db_conn, person_id=person_id, slug="jamais-elue")

    response = await client.get("/personne/jamais-elue")

    assert response.status_code == 200
    assert "jamais siégé" not in response.text
    assert "Notre corpus ne couvre que les scrutins de l'Assemblée nationale" in response.text


async def test_a_person_with_an_out_of_corpus_vote_gets_the_uncategorized_message_not_never_sat(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    """Le cas Retailleau (plan phase 4) : un unique vote existe (le Congrès), hors corpus. La
    fiche ne doit jamais dire « jamais siégé », qui serait faux.
    """
    person_id = await factories.insert_person(db_conn, an_uid="PA-RETAILLEAU")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-congres")
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'VTCGR5L16V1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    scrutin_id = await db_conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTCGR5L16V1', 1, 16, $1, 'congres', 'SPO', 'Congrès', 'DecompteNominatif',
                800, 800, 400, 400, 0, 0, 925, $2)
        RETURNING id
        """,
        date(2024, 3, 4),
        source_document_id,
    )
    await db_conn.execute(
        "INSERT INTO vote (scrutin_id, person_id, position) VALUES ($1, $2, 'pour')",
        scrutin_id,
        person_id,
    )

    response = await client.get("/personne/candidat-congres")

    assert response.status_code == 200
    assert "Aucun des votes de cette personne ne porte" in response.text
    assert "jamais siégé" not in response.text
