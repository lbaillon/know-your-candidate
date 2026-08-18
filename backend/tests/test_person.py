from datetime import date

import asyncpg
from httpx import AsyncClient

import factories


async def test_person_page_shows_identity_and_a_precise_vote_with_its_source_link(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="ASSEMBLEE", libelle="Assemblée nationale"
    )
    await factories.insert_mandat(
        db_conn,
        an_uid="M1",
        person_id=person_id,
        organe_id=organe_id,
        type_organe="ASSEMBLEE",
        debut=date(2022, 6, 22),
        legislature=16,
    )
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'VTANR5L16V42', 'https://data.assemblee-nationale.fr/scrutin/42',
                'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    scrutin_id = await db_conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTANR5L16V42', 42, 16, '2023-03-14', 'SPO', 'Sur la réforme des retraites',
                'DecompteNominatif', 400, 400, 200, 200, 0, 0, 577, $1)
        RETURNING id
        """,
        source_document_id,
    )
    await db_conn.execute(
        "INSERT INTO vote (scrutin_id, person_id, position) VALUES ($1, $2, 'pour')",
        scrutin_id,
        person_id,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert response.status_code == 200
    assert "Jean Dupont" in response.text
    assert "Sur la réforme des retraites" in response.text
    assert "14/03/2023" in response.text
    assert "a voté pour" in response.text
    assert 'href="/scrutin/16/42"' in response.text


async def test_a_person_who_never_sat_shows_the_explicit_empty_state(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(
        db_conn, wikidata_qid="Q1", prenom="Ada", nom="Exemple"
    )
    await factories.insert_slug(db_conn, person_id=person_id, slug="ada-exemple")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/ada-exemple")

    assert response.status_code == 200
    assert "n'a jamais siégé à l'Assemblée nationale" in response.text
    # L'état vide apparaît à la fois pour la frise et pour les votes récents.
    assert response.text.count("n'a jamais siégé à l'Assemblée nationale") == 2


async def test_the_orientations_zone_always_shows_the_phase_4_placeholder(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "phase 4" in response.text


async def test_a_person_who_sat_but_has_no_vote_gets_a_different_empty_state(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="ASSEMBLEE", libelle="Assemblée nationale"
    )
    await factories.insert_mandat(
        db_conn,
        an_uid="M1",
        person_id=person_id,
        organe_id=organe_id,
        type_organe="ASSEMBLEE",
        debut=date(2022, 6, 22),
        legislature=16,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "Aucun vote enregistré" in response.text
    assert "n'a jamais siégé à l'Assemblée nationale" not in response.text


async def test_an_old_slug_redirects_301_to_the_current_one(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-ancien", is_current=False)
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont", is_current=True)
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-ancien", follow_redirects=False)

    assert response.status_code == 301
    assert response.headers["location"] == "/personne/jean-dupont"


async def test_an_unknown_slug_is_a_404(client: AsyncClient) -> None:
    response = await client.get("/personne/personne-inconnue")

    assert response.status_code == 404


async def test_a_non_inscrit_mandate_never_says_a_siege_au_groupe(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="GP", libelle="Non inscrits", is_non_inscrit=True
    )
    await factories.insert_mandat(
        db_conn,
        an_uid="M1",
        person_id=person_id,
        organe_id=organe_id,
        type_organe="GP",
        debut=date(2022, 6, 22),
        legislature=16,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "a siégé au groupe" not in response.text
    assert "non-inscrit" in response.text.lower()


async def test_a_par_delegation_vote_is_signalled(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'VTANR5L16V42', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    scrutin_id = await db_conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTANR5L16V42', 42, 16, '2023-03-14', 'SPO', 'titre',
                'DecompteNominatif', 1, 1, 1, 0, 0, 0, 577, $1)
        RETURNING id
        """,
        source_document_id,
    )
    await db_conn.execute(
        """
        INSERT INTO vote (scrutin_id, person_id, position, par_delegation)
        VALUES ($1, $2, 'pour', true)
        """,
        scrutin_id,
        person_id,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "vote émis par délégation" in response.text


async def test_a_congres_vote_is_signalled_and_the_source_document_is_linked(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
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
        VALUES ('VTCGR5L16V1', 1, 16, '2024-03-04', 'congres', 'SPO', 'Révision constitutionnelle',
                'DecompteNominatif', 1, 1, 1, 0, 0, 0, 902, $1)
        RETURNING id
        """,
        source_document_id,
    )
    await db_conn.execute(
        "INSERT INTO vote (scrutin_id, person_id, position) VALUES ($1, $2, 'pour')",
        scrutin_id,
        person_id,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "Congrès du Parlement" in response.text


async def test_a_mise_au_point_is_shown_without_changing_the_recorded_vote(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'VTANR5L16V42', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    scrutin_id = await db_conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTANR5L16V42', 42, 16, '2023-03-14', 'SPO', 'titre',
                'DecompteNominatif', 1, 1, 0, 1, 0, 0, 577, $1)
        RETURNING id
        """,
        source_document_id,
    )
    await db_conn.execute(
        "INSERT INTO vote (scrutin_id, person_id, position) VALUES ($1, $2, 'contre')",
        scrutin_id,
        person_id,
    )
    await db_conn.execute(
        """
        INSERT INTO vote_mise_au_point (scrutin_id, person_id, position_declaree,
                                         source_document_id)
        VALUES ($1, $2, 'pour', $3)
        """,
        scrutin_id,
        person_id,
        source_document_id,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/personne/jean-dupont")

    assert "a voté contre" in response.text, "le vote enregistré reste celui de la source"
    assert "a déclaré après le scrutin avoir voulu voter pour" in response.text
    assert "le résultat du scrutin n'a pas été modifié" in response.text
