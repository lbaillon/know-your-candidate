from datetime import date

import asyncpg
from httpx import AsyncClient

import factories


async def test_directory_lists_a_person_without_being_a_candidate(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/deputes")

    assert response.status_code == 200
    assert "Jean Dupont" in response.text


async def test_a_person_without_a_current_slug_is_not_listed(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/deputes")

    assert "Jean Dupont" not in response.text


async def test_search_filters_the_directory_by_name(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    matching = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=matching, slug="jean-dupont")
    other = await factories.insert_person(db_conn, an_uid="PA2", prenom="Alice", nom="Martin")
    await factories.insert_slug(db_conn, person_id=other, slug="alice-martin")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/deputes", params={"q": "martin"})

    assert "Alice Martin" in response.text
    assert "Jean Dupont" not in response.text


async def test_legislature_filter_only_keeps_people_who_sat_in_it(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    in_15 = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=in_15, slug="jean-dupont")
    organe_15 = await factories.insert_organe(db_conn, an_uid="PO1", code_type="ASSEMBLEE")
    await factories.insert_mandat(
        db_conn,
        an_uid="M1",
        person_id=in_15,
        organe_id=organe_15,
        type_organe="ASSEMBLEE",
        debut=date(2017, 6, 20),
        fin=date(2022, 6, 20),
        legislature=15,
    )

    in_17 = await factories.insert_person(db_conn, an_uid="PA2", prenom="Alice", nom="Martin")
    await factories.insert_slug(db_conn, person_id=in_17, slug="alice-martin")
    organe_17 = await factories.insert_organe(db_conn, an_uid="PO2", code_type="ASSEMBLEE")
    await factories.insert_mandat(
        db_conn,
        an_uid="M2",
        person_id=in_17,
        organe_id=organe_17,
        type_organe="ASSEMBLEE",
        debut=date(2024, 7, 18),
        legislature=17,
    )
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/deputes", params={"legislature": 15})

    assert "Jean Dupont" in response.text
    assert "Alice Martin" not in response.text


async def test_the_directory_fragment_returns_partial_html(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    await factories.refresh_person_apercu(db_conn)

    response = await client.get("/fragments/deputes")

    assert response.status_code == 200
    assert "<html" not in response.text
    assert "Jean Dupont" in response.text


async def test_a_last_known_group_is_shown_with_its_period(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="GP", libelle="Groupe Test", libelle_abrege="GT"
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

    response = await client.get("/deputes")

    assert "GT" in response.text


async def test_a_non_inscrit_mandate_is_never_shown_as_belonging_to_a_group(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    organe_id = await factories.insert_organe(
        db_conn, an_uid="PO1", code_type="GP", libelle="Non inscrit", is_non_inscrit=True
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

    response = await client.get("/deputes")

    assert "non-inscrit" in response.text.lower()
    assert "Non inscrit" not in response.text
