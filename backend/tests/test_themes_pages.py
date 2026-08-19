"""Tests des pages publiques de la catégorisation — voir docs/plans/phase-3-categorisation.md,
section « Pages publiques ». Les règles d'affichage non négociables (position jamais en nombre nu,
méthode visible sans cliquer, justification jamais tronquée, aucune estimation automatique) sont
chacune couvertes par un test qui porte leur nom.
"""

from datetime import date

import asyncpg
import lxml.html
import pytest
from httpx import AsyncClient


async def insert_scrutin(
    conn: asyncpg.Connection,
    *,
    an_uid: str,
    numero: int,
    legislature: int = 17,
    date_scrutin: date = date(2024, 3, 14),
    titre: str = "l'ensemble du projet de loi",
) -> int:
    source_document_id = await conn.fetchval(
        """
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        """,
        an_uid,
    )
    return await conn.fetchval(
        """
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, $3, $4, 'assemblee', 'SPO', $5, 'DecompteNominatif', 400, 400, 200, 200,
                0, 0, 577, $6)
        RETURNING id
        """,
        an_uid,
        numero,
        legislature,
        date_scrutin,
        titre,
        source_document_id,
    )


async def insert_theme(
    conn: asyncpg.Connection,
    *,
    slug: str,
    rang: int = 1,
    libelle_pole_negatif: str | None = "négatif",
    libelle_pole_positif: str | None = "positif",
) -> int:
    return await conn.fetchval(
        """
        INSERT INTO theme
            (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description du thème', $2, $3, $4)
        RETURNING id
        """,
        slug,
        libelle_pole_negatif,
        libelle_pole_positif,
        rang,
    )


async def insert_admin(conn: asyncpg.Connection, *, login: str = "bob") -> int:
    return await conn.fetchval(
        """
        INSERT INTO admin_user (github_id, github_login, display_name) VALUES ($1, $2, $2)
        RETURNING id
        """,
        hash(login) % 1_000_000,
        login,
    )


async def insert_label(
    conn: asyncpg.Connection,
    *,
    scrutin_id: int,
    theme_id: int,
    author_id: int,
    poids: str = "1.000",
    position_pour: str | None = "0.500",
    justification: str = "justification suffisamment longue",
) -> None:
    await conn.execute(
        """
        INSERT INTO scrutin_label
            (scrutin_id, theme_id, poids, position_pour, confiance, justification, method,
             author_id)
        VALUES ($1, $2, $3::numeric, $4::numeric, 0.700, $5, 'manual', $6)
        """,
        scrutin_id,
        theme_id,
        poids,
        position_pour,
        justification,
        author_id,
    )


async def test_themes_page_lists_active_themes_with_their_poles(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await insert_theme(db_conn, slug="social-fiscalite")

    response = await client.get("/themes")
    assert response.status_code == 200
    doc = lxml.html.fromstring(response.text)
    assert doc.xpath("//a[@href='/theme/social-fiscalite']")
    assert "négatif" in response.text
    assert "positif" in response.text


async def test_themes_page_carries_the_epistemic_caveat(client: AsyncClient) -> None:
    response = await client.get("/themes")
    assert response.status_code == 200
    assert "construction du projet" in response.text
    doc = lxml.html.fromstring(response.text)
    assert doc.xpath("//a[@href='/methodologie']")


async def test_theme_page_404s_on_unknown_slug(client: AsyncClient) -> None:
    response = await client.get("/theme/inconnu")
    assert response.status_code == 404


async def test_theme_page_lists_only_scrutins_categorized_on_this_theme(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    autre_theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=2)
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    autre_scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V2", numero=2)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)
    await insert_label(
        db_conn, scrutin_id=autre_scrutin_id, theme_id=autre_theme_id, author_id=author_id
    )

    response = await client.get("/theme/environnement")
    assert response.status_code == 200
    assert "/scrutin/17/1" in response.text
    assert "/scrutin/17/2" not in response.text


async def test_scrutins_page_lists_categorized_scrutins_only(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn)
    categorized_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_scrutin(db_conn, an_uid="VTANR5L17V2", numero=2)
    await insert_label(db_conn, scrutin_id=categorized_id, theme_id=theme_id, author_id=author_id)

    response = await client.get("/scrutins")
    assert response.status_code == 200
    assert "/scrutin/17/1" in response.text
    assert "/scrutin/17/2" not in response.text


async def test_scrutins_page_filters_by_theme_and_legislature(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    other_theme_id = await insert_theme(db_conn, slug="social-fiscalite", rang=2)
    author_id = await insert_admin(db_conn)
    matching_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1, legislature=17)
    wrong_theme_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V2", numero=2, legislature=17)
    wrong_legislature_id = await insert_scrutin(
        db_conn, an_uid="VTANR5L16V3", numero=3, legislature=16
    )
    await insert_label(db_conn, scrutin_id=matching_id, theme_id=theme_id, author_id=author_id)
    await insert_label(
        db_conn, scrutin_id=wrong_theme_id, theme_id=other_theme_id, author_id=author_id
    )
    await insert_label(
        db_conn, scrutin_id=wrong_legislature_id, theme_id=theme_id, author_id=author_id
    )

    response = await client.get("/scrutins?theme=environnement&legislature=17")
    assert response.status_code == 200
    assert "/scrutin/17/1" in response.text
    assert "/scrutin/17/2" not in response.text
    assert "/scrutin/16/3" not in response.text


async def test_position_is_never_shown_as_a_bare_number(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(
        db_conn,
        slug="environnement",
        libelle_pole_negatif="protection de l'environnement",
        libelle_pole_positif="croissance industrielle",
    )
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(
        db_conn,
        scrutin_id=scrutin_id,
        theme_id=theme_id,
        author_id=author_id,
        position_pour="0.600",
    )

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    assert "plutôt : croissance industrielle" in response.text
    # Le libellé du pôle précède le chiffre, jamais l'inverse.
    assert response.text.index("croissance industrielle") < response.text.index("+0.6")


async def test_theme_without_axis_shows_no_position(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(
        db_conn, slug="autre", libelle_pole_negatif=None, libelle_pole_positif=None
    )
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(
        db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id, position_pour=None
    )

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    assert "plutôt :" not in response.text


async def test_method_is_visible_without_clicking(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn, login="alice")
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    assert "catégorisé par alice" in response.text


async def test_justification_is_never_truncated(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    long_justification = "x" * 500 + " fin de la justification"
    await insert_label(
        db_conn,
        scrutin_id=scrutin_id,
        theme_id=theme_id,
        author_id=author_id,
        justification=long_justification,
    )

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    assert long_justification in response.text


async def test_no_automatic_estimate_ever_appears_on_a_public_page(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)
    group_axis_version = "test-1"
    await db_conn.execute(
        """
        INSERT INTO group_axis
            (version, description, grille_version, grille_date, source_url, content_hash,
             is_current)
        VALUES ($1, 'd', 'g1', '2024-01-01', 'https://example.org', 'hash', true)
        """,
        group_axis_version,
    )
    await db_conn.execute(
        """
        INSERT INTO scrutin_axis_estimate
            (scrutin_id, strategy, axis_version, position_pour, separation, couverture,
             votants_couverts)
        VALUES ($1, 'group_alignment', $2, 0.321, 0.654, 0.111, 500)
        """,
        scrutin_id,
        group_axis_version,
    )

    for path in ("/scrutin/17/1", "/scrutin/17/1/categorisation"):
        response = await client.get(path)
        assert response.status_code == 200
        assert "0.321" not in response.text
        assert "0,321" not in response.text
        assert "0.654" not in response.text


async def test_categorisation_detail_page_shows_full_history_with_authors(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn, login="alice")
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)
    await db_conn.execute(
        """
        INSERT INTO label_revision (scrutin_id, avant, apres, method, author_id, motif)
        VALUES ($1, '[]'::jsonb, '[{"slug": "environnement"}]'::jsonb, 'manual', $2, 'création')
        """,
        scrutin_id,
        author_id,
    )

    response = await client.get("/scrutin/17/1/categorisation")
    assert response.status_code == 200
    assert "alice" in response.text
    assert "création" in response.text


async def test_categorisation_detail_page_404s_when_never_categorized(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)

    response = await client.get("/scrutin/17/1/categorisation")
    assert response.status_code == 404


async def test_scrutin_page_links_to_categorisation_detail_when_categorized(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await insert_theme(db_conn, slug="environnement")
    author_id = await insert_admin(db_conn)
    scrutin_id = await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)
    await insert_label(db_conn, scrutin_id=scrutin_id, theme_id=theme_id, author_id=author_id)

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    doc = lxml.html.fromstring(response.text)
    assert doc.xpath("//a[@href='/scrutin/17/1/categorisation']")


async def test_scrutin_page_shows_no_categorisation_block_when_uncategorized(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await insert_scrutin(db_conn, an_uid="VTANR5L17V1", numero=1)

    response = await client.get("/scrutin/17/1")
    assert response.status_code == 200
    doc = lxml.html.fromstring(response.text)
    assert not doc.xpath("//a[@href='/scrutin/17/1/categorisation']")


@pytest.mark.parametrize("path", ["/themes", "/scrutins"])
async def test_pages_render_with_no_data_in_the_database(client: AsyncClient, path: str) -> None:
    response = await client.get(path)
    assert response.status_code == 200
