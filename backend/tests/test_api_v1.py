import asyncpg
from httpx import AsyncClient

import factories


async def test_candidats_lists_a_seeded_candidate_with_its_source(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.insert_candidate(
        db_conn, person_id=person_id, source_url="https://example.org/annonce"
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/api/v1/candidats")

    assert response.status_code == 200
    body = response.json()
    assert len(body["data"]) == 1
    assert body["data"][0]["nom"] == "Dupont"
    assert body["data"][0]["candidature"]["source_url"] == "https://example.org/annonce"


async def test_personnes_returns_a_pagination_block(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/api/v1/personnes")

    assert response.status_code == 200
    body = response.json()
    assert body["pagination"]["page"] == 1
    assert body["pagination"]["total"] == 1


async def test_personne_detail_returns_the_identity_and_its_source(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await db_conn.execute(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_acteur', 'PA1', 'https://data.assemblee-nationale.fr/acteur/PA1', 'hash',
                '{}'::jsonb)
        """
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/api/v1/personnes/jean-dupont")

    assert response.status_code == 200
    body = response.json()
    assert body["nom"] == "Dupont"
    assert body["source"]["url"] == "https://data.assemblee-nationale.fr/acteur/PA1"
    assert body["source"]["licence"] == "Licence Ouverte (Etalab)"


async def test_personne_detail_redirects_301_for_an_old_slug(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-ancien", is_current=False)
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont", is_current=True)
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/api/v1/personnes/jean-ancien", follow_redirects=False)

    assert response.status_code == 301
    assert response.headers["location"] == "/api/v1/personnes/jean-dupont"


async def test_personne_detail_404_for_an_unknown_slug(client: AsyncClient) -> None:
    response = await client.get("/api/v1/personnes/personne-inconnue")

    assert response.status_code == 404


async def test_personne_votes_returns_data_and_a_cursor_pagination_block(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'V1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    scrutin_id = await db_conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('V1', 1, 17, '2024-01-01', 'SPO', 'titre', 'DecompteNominatif', 1, 1, 1, 0, 0, 0,
                577, $1)
        RETURNING id
        """,
        source_document_id,
    )
    await db_conn.execute(
        "INSERT INTO vote (scrutin_id, person_id, position) VALUES ($1, $2, 'pour')",
        scrutin_id,
        person_id,
    )

    response = await client.get("/api/v1/personnes/jean-dupont/votes")

    assert response.status_code == 200
    body = response.json()
    assert len(body["data"]) == 1
    assert body["data"][0]["position"] == "pour"
    assert body["pagination"]["next_cursor"] is None


async def test_personne_votes_rejects_a_malformed_cursor_with_400(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")

    response = await client.get(
        "/api/v1/personnes/jean-dupont/votes", params={"avant": "n-importe-quoi"}
    )

    assert response.status_code == 400


async def test_scrutin_detail_returns_counters_and_its_source(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'V1', 'https://data.assemblee-nationale.fr/scrutins.zip', 'hash',
                '{}'::jsonb)
        RETURNING id
        """
    )
    await db_conn.execute(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTANR5L17V42', 42, 17, '2024-03-14', 'SPO', 'Sur la réforme des retraites',
                'DecompteNominatif', 400, 400, 200, 200, 0, 0, 577, $1)
        """,
        source_document_id,
    )

    response = await client.get("/api/v1/scrutins/17/42")

    assert response.status_code == 200
    body = response.json()
    assert body["titre"] == "Sur la réforme des retraites"
    assert body["pour"] == 200
    assert body["source"]["url"] == "https://data.assemblee-nationale.fr/scrutins.zip"


async def test_scrutin_detail_404_for_an_unknown_scrutin(client: AsyncClient) -> None:
    response = await client.get("/api/v1/scrutins/17/999999")

    assert response.status_code == 404


# --- La page et l'API ne divergent jamais (D2.8) --------------------------------------------


async def test_the_person_page_and_the_api_agree_on_the_same_values(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.insert_candidate(
        db_conn, statut="pressenti", person_id=person_id, source_url="https://example.org/x"
    )
    await factories.refresh_person_apercu(db_conn)

    page = await client.get("/personne/jean-dupont")
    api = await client.get("/api/v1/personnes/jean-dupont")
    api_body = api.json()

    assert "Jean Dupont" in page.text
    assert api_body["prenom"] == "Jean"
    assert api_body["nom"] == "Dupont"
    assert "candidature pressentie" in page.text
    assert api_body["candidature"]["statut"] == "pressenti"
    assert api_body["candidature"]["source_url"] == "https://example.org/x"


async def test_the_scrutin_page_and_the_api_agree_on_the_same_values(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'V1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    await db_conn.execute(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTANR5L17V42', 42, 17, '2024-03-14', 'SPO', 'Un titre bien précis',
                'DecompteNominatif', 400, 400, 321, 79, 0, 0, 577, $1)
        """,
        source_document_id,
    )

    page = await client.get("/scrutin/17/42")
    api = await client.get("/api/v1/scrutins/17/42")
    api_body = api.json()

    assert "Un titre bien précis" in page.text
    assert api_body["titre"] == "Un titre bien précis"
    assert api_body["pour"] == 321
    assert str(api_body["pour"]) in page.text


async def test_a_congres_vote_is_signalled_the_same_way_via_the_api(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    source_document_id = await db_conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', 'VTCGR5L16V1', 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """
    )
    await db_conn.execute(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ('VTCGR5L16V1', 1, 16, '2024-03-04', 'congres', 'SPO', 'titre',
                'DecompteNominatif', 1, 1, 1, 0, 0, 0, 902, $1)
        """,
        source_document_id,
    )

    response = await client.get("/api/v1/scrutins/16/1")

    assert response.json()["chambre"] == "congres"


async def test_du_and_au_are_accepted_as_iso_dates(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")

    response = await client.get(
        "/api/v1/personnes/jean-dupont/votes", params={"du": "2020-01-01", "au": "2026-01-01"}
    )

    assert response.status_code == 200
    assert response.json()["data"] == []
