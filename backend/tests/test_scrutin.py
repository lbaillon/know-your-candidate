from datetime import date

import asyncpg
from httpx import AsyncClient


async def _insert_source_document(conn: asyncpg.Connection, uid: str) -> int:
    return await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://data.assemblee-nationale.fr/scrutins.zip', 'hash',
                '{}'::jsonb)
        RETURNING id
        """,
        uid,
    )


async def _insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int,
    legislature: int,
    date_scrutin: date = date(2024, 3, 14),
    titre: str = "Sur la réforme des retraites",
    chambre: str = "assemblee",
    nombre_votants: int = 400,
    pour: int = 200,
    contre: int = 200,
    effectif: int = 577,
) -> int:
    source_document_id = await _insert_source_document(conn, an_uid)
    return await conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, $5, 'SPO', $6, 'DecompteNominatif', $7, $7, $8, $9, 0, 0, $10, $11)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        chambre,
        titre,
        nombre_votants,
        pour,
        contre,
        effectif,
        source_document_id,
    )


async def test_scrutin_page_shows_title_date_and_counters(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await _insert_scrutin(db_conn, an_uid="VTANR5L17V42", numero=42, legislature=17)

    response = await client.get("/scrutin/17/42")

    assert response.status_code == 200
    assert "Sur la réforme des retraites" in response.text
    assert "14/03/2024" in response.text
    assert 'href="https://www.assemblee-nationale.fr/dyn/17/scrutins/42"' in response.text


async def test_unknown_scrutin_is_a_404(client: AsyncClient) -> None:
    response = await client.get("/scrutin/17/999999")

    assert response.status_code == 404


async def test_an_uid_alias_redirects_301_to_the_canonical_url(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await _insert_scrutin(db_conn, an_uid="VTANR5L17V42", numero=42, legislature=17)

    response = await client.get("/scrutin/VTANR5L17V42", follow_redirects=False)

    assert response.status_code == 301
    assert response.headers["location"] == "/scrutin/17/42"


async def test_unknown_an_uid_alias_is_a_404(client: AsyncClient) -> None:
    response = await client.get("/scrutin/VTANR5L17V999999")

    assert response.status_code == 404


async def test_congres_scrutin_is_signalled(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await _insert_scrutin(
        db_conn,
        an_uid="VTCGR5L16V1",
        numero=1,
        legislature=16,
        chambre="congres",
        effectif=902,
    )

    response = await client.get("/scrutin/16/1")

    assert "Congrès du Parlement" in response.text


async def test_ghost_group_is_shown_as_unidentified_not_hidden(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    scrutin_id = await _insert_scrutin(db_conn, an_uid="VTANR5L17V42", numero=42, legislature=17)
    await db_conn.execute(
        """
        INSERT INTO scrutin_groupe (scrutin_id, rang, organe_id, nombre_membres, pour, contre,
                                     abstentions, non_votants)
        VALUES ($1, 0, NULL, 10, 5, 5, 0, 0)
        """,
        scrutin_id,
    )

    response = await client.get("/scrutin/17/42")

    assert "Groupe non identifié par la source" in response.text


async def test_a_real_group_breakdown_row_shows_its_label(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    scrutin_id = await _insert_scrutin(db_conn, an_uid="VTANR5L17V42", numero=42, legislature=17)
    organe_id = await db_conn.fetchval(
        """
        INSERT INTO organe (an_uid, code_type, libelle) VALUES ('PO1', 'GP', 'Groupe Test')
        RETURNING id
        """
    )
    await db_conn.execute(
        """
        INSERT INTO scrutin_groupe (scrutin_id, rang, organe_id, nombre_membres, pour, contre,
                                     abstentions, non_votants)
        VALUES ($1, 1, $2, 88, 40, 30, 10, 8)
        """,
        scrutin_id,
        organe_id,
    )

    response = await client.get("/scrutin/17/42")

    assert "Groupe Test" in response.text
