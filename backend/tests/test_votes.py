import re
from datetime import date

import asyncpg
from httpx import AsyncClient

import factories
from kyc_api.queries.persons import VOTES_PAGE_SIZE


async def insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int,
    legislature: int,
    date_scrutin: date,
    titre: str = "titre",
    chambre: str = "assemblee",
    effectif: int = 577,
) -> int:
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """,
        an_uid,
    )
    scrutin_id = await conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, $5, 'SPO', $6, 'DecompteNominatif', 1, 1, 1, 0, 0, 0, $7, $8)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        chambre,
        titre,
        effectif,
        source_document_id,
    )
    assert scrutin_id is not None
    return scrutin_id


async def insert_vote(
    conn: asyncpg.Connection,
    *,
    scrutin_id: int,
    person_id: int,
    position: str = "pour",
    groupe_organe_id: int | None = None,
) -> None:
    await conn.execute(
        """
        INSERT INTO vote (scrutin_id, person_id, position, groupe_organe_id)
        VALUES ($1, $2, $3, $4)
        """,
        scrutin_id,
        person_id,
        position,
        groupe_organe_id,
    )


async def _seed_person(db_conn: asyncpg.Connection) -> int:
    person_id = await factories.insert_person(db_conn, an_uid="PA1", prenom="Jean", nom="Dupont")
    await factories.insert_slug(db_conn, person_id=person_id, slug="jean-dupont")
    return person_id


async def test_votes_page_lists_a_vote(client: AsyncClient, db_conn: asyncpg.Connection) -> None:
    person_id = await _seed_person(db_conn)
    scrutin_id = await insert_scrutin(
        db_conn, an_uid="V1", numero=1, legislature=17, date_scrutin=date(2024, 9, 1)
    )
    await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes")

    assert response.status_code == 200
    assert "a voté pour" in response.text


async def test_votes_are_ordered_most_recent_first(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    old = await insert_scrutin(
        db_conn,
        an_uid="V-OLD",
        numero=1,
        legislature=17,
        date_scrutin=date(2024, 1, 1),
        titre="ancien",
    )
    recent = await insert_scrutin(
        db_conn,
        an_uid="V-NEW",
        numero=2,
        legislature=17,
        date_scrutin=date(2024, 6, 1),
        titre="recent",
    )
    await insert_vote(db_conn, scrutin_id=old, person_id=person_id)
    await insert_vote(db_conn, scrutin_id=recent, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes")

    assert response.text.index("recent") < response.text.index("ancien")


async def test_legislature_filter_narrows_the_list(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    v15 = await insert_scrutin(
        db_conn,
        an_uid="V15",
        numero=1,
        legislature=15,
        date_scrutin=date(2018, 1, 1),
        titre="quinze",
    )
    v17 = await insert_scrutin(
        db_conn,
        an_uid="V17",
        numero=1,
        legislature=17,
        date_scrutin=date(2024, 1, 1),
        titre="dixsept",
    )
    await insert_vote(db_conn, scrutin_id=v15, person_id=person_id)
    await insert_vote(db_conn, scrutin_id=v17, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes", params={"legislature": 15})

    assert "quinze" in response.text
    assert "dixsept" not in response.text


async def test_position_filter_narrows_the_list(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    pour = await insert_scrutin(
        db_conn,
        an_uid="V1",
        numero=1,
        legislature=17,
        date_scrutin=date(2024, 1, 1),
        titre="pour-titre",
    )
    contre = await insert_scrutin(
        db_conn,
        an_uid="V2",
        numero=2,
        legislature=17,
        date_scrutin=date(2024, 2, 1),
        titre="contre-titre",
    )
    await insert_vote(db_conn, scrutin_id=pour, person_id=person_id, position="pour")
    await insert_vote(db_conn, scrutin_id=contre, person_id=person_id, position="contre")

    response = await client.get("/personne/jean-dupont/votes", params={"position": "contre"})

    assert "contre-titre" in response.text
    assert "pour-titre" not in response.text


async def test_date_range_filter_narrows_the_list(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    early = await insert_scrutin(
        db_conn,
        an_uid="V1",
        numero=1,
        legislature=17,
        date_scrutin=date(2024, 1, 1),
        titre="janvier",
    )
    late = await insert_scrutin(
        db_conn, an_uid="V2", numero=2, legislature=17, date_scrutin=date(2024, 6, 1), titre="juin"
    )
    await insert_vote(db_conn, scrutin_id=early, person_id=person_id)
    await insert_vote(db_conn, scrutin_id=late, person_id=person_id)

    response = await client.get(
        "/personne/jean-dupont/votes", params={"du": "2024-03-01", "au": "2024-12-31"}
    )

    assert "juin" in response.text
    assert "janvier" not in response.text


async def test_cursor_pagination_shows_a_charger_plus_link_when_there_is_more(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    for i in range(VOTES_PAGE_SIZE + 5):
        scrutin_id = await insert_scrutin(
            db_conn,
            an_uid=f"V{i}",
            numero=i,
            legislature=17,
            date_scrutin=date(2024, 1, 1),
        )
        await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes")

    assert "Charger plus" in response.text


async def test_cursor_pagination_has_no_charger_plus_link_on_the_last_page(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    scrutin_id = await insert_scrutin(
        db_conn, an_uid="V1", numero=1, legislature=17, date_scrutin=date(2024, 1, 1)
    )
    await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes")

    assert "Charger plus" not in response.text


async def test_following_the_cursor_reaches_the_next_batch(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    for i in range(VOTES_PAGE_SIZE + 1):
        scrutin_id = await insert_scrutin(
            db_conn,
            an_uid=f"V{i}",
            numero=i,
            legislature=17,
            date_scrutin=date(2024, 1, 1) if i > 0 else date(2020, 1, 1),
            titre=f"scrutin numero {i}" if i > 0 else "le plus ancien",
        )
        await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    first_page = await client.get("/personne/jean-dupont/votes")
    assert "le plus ancien" not in first_page.text

    # Extraire le curseur du lien « Charger plus ».
    match = re.search(r"avant=([^&\"]+)", first_page.text)
    assert match is not None
    cursor = match.group(1)

    second_page = await client.get(
        "/personne/jean-dupont/votes", params={"avant": cursor.replace("%2C", ",")}
    )
    assert "le plus ancien" in second_page.text


async def test_the_fragment_returns_partial_html(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    scrutin_id = await insert_scrutin(
        db_conn, an_uid="V1", numero=1, legislature=17, date_scrutin=date(2024, 1, 1)
    )
    await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    response = await client.get("/fragments/personne/jean-dupont/votes")

    assert response.status_code == 200
    assert "<html" not in response.text


async def test_a_malformed_cursor_degrades_to_the_first_page_instead_of_erroring(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await _seed_person(db_conn)
    scrutin_id = await insert_scrutin(
        db_conn, an_uid="V1", numero=1, legislature=17, date_scrutin=date(2024, 1, 1)
    )
    await insert_vote(db_conn, scrutin_id=scrutin_id, person_id=person_id)

    response = await client.get("/personne/jean-dupont/votes", params={"avant": "n-importe-quoi"})

    assert response.status_code == 200
