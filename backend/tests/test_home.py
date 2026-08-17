import asyncpg
from httpx import AsyncClient

import factories


async def test_home_lists_a_seeded_candidate(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.insert_candidate(
        db_conn,
        person_id=person_id,
        statut="declare",
        source_url="https://example.org/annonce",
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/")

    assert response.status_code == 200
    assert "Jean Dupont" in response.text
    assert "candidature déclarée" in response.text


async def test_home_shows_an_empty_state_when_there_is_no_candidate(client: AsyncClient) -> None:
    response = await client.get("/")

    assert response.status_code == 200
    assert "Aucun" in response.text


async def test_home_search_filters_out_non_matching_candidates(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    matching = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=matching, slug="jean-dupont")
    await factories.insert_candidate(db_conn, person_id=matching)

    other = await factories.insert_person(db_conn, an_uid="PA2", prenom="Alice", nom="Martin")
    await factories.insert_slug(db_conn, person_id=other, slug="alice-martin")
    await factories.insert_candidate(db_conn, person_id=other)
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/", params={"q": "dupont"})

    assert "Jean Dupont" in response.text
    assert "Alice Martin" not in response.text


async def test_a_candidate_without_a_photo_gets_a_placeholder_not_a_broken_image(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.insert_candidate(db_conn, person_id=person_id)
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/")

    assert "person-card__photo--placeholder" in response.text
    assert "<img" not in response.text


async def test_the_search_fragment_returns_partial_html_without_a_full_document(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.insert_candidate(db_conn, person_id=person_id)
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/fragments/recherche", params={"q": "dupont"})

    assert response.status_code == 200
    assert "<html" not in response.text
    assert "Jean Dupont" in response.text


async def test_a_person_who_never_sat_and_is_not_a_candidate_does_not_appear_on_home(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/")

    assert "Jean Dupont" not in response.text
