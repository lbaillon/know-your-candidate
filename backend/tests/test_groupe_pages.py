"""Tests des scores de groupe — voir docs/plans/phase-4-partis-scores.md, D4.4, D4.12 : la page
`/groupe/{an_uid}` (score sur toute l'existence du groupe) et les positions restreintes à un mandat
sur la fiche personne (jamais fusionnées avec le score personnel).
"""

from datetime import date

import asyncpg
from httpx import AsyncClient

import factories


async def test_groupe_page_shows_score_and_cohesion(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(
        db_conn,
        slug="social-fiscalite",
        libelle_pole_negatif="redistribution",
        libelle_pole_positif="maîtrise",
    )
    organe_id = await factories.insert_organe(db_conn, an_uid="PO1", libelle="Groupe Exemple")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await factories.insert_groupe_theme_score(
        db_conn,
        run_id=run_id,
        organe_id=organe_id,
        theme_id=theme_id,
        score=0.35,
        cohesion=0.82,
        contributions=400,
        membres=60,
    )

    response = await client.get("/groupe/PO1")

    assert response.status_code == 200
    assert "Groupe Exemple" in response.text
    assert "+0.35" in response.text
    assert "82%" in response.text
    assert "60 membres" in response.text


async def test_a_non_inscrit_organe_is_a_404(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    await factories.insert_organe(
        db_conn, an_uid="PO-NI", libelle="Non-inscrits", is_non_inscrit=True
    )

    response = await client.get("/groupe/PO-NI")

    assert response.status_code == 404


async def test_an_unknown_organe_is_a_404(client: AsyncClient) -> None:
    response = await client.get("/groupe/inconnu")

    assert response.status_code == 404


async def test_person_page_shows_mandat_restricted_group_positions(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(db_conn, slug="travail")
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-un")
    organe_id = await factories.insert_organe(db_conn, an_uid="PO-LFI", libelle="LFI")
    await factories.insert_mandat(
        db_conn, an_uid="MDT1", person_id=person_id, organe_id=organe_id, debut=date(2022, 6, 22)
    )
    mandat_id = await db_conn.fetchval("SELECT id FROM mandat WHERE an_uid = $1", "MDT1")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await factories.insert_mandat_theme_score(
        db_conn, run_id=run_id, mandat_id=mandat_id, theme_id=theme_id, score=-0.4, cohesion=0.9
    )
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-un")

    assert response.status_code == 200
    assert "Positions des groupes où" in response.text
    assert "LFI" in response.text
    assert 'href="/groupe/PO-LFI"' in response.text
    assert "-0.40" in response.text


async def test_person_without_a_mandate_has_no_groupe_section(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-deux")

    response = await client.get("/personne/candidat-deux")

    assert response.status_code == 200
    assert "Positions des groupes où" not in response.text


async def test_groupe_page_shows_the_hidden_themes_footer(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    left_right = await factories.insert_theme(db_conn, slug="securite", axe_gauche_droite=True)
    hidden = await factories.insert_theme(
        db_conn, slug="institutions-democratie", axe_gauche_droite=False, rang=2
    )
    organe_id = await factories.insert_organe(db_conn, an_uid="PO3", libelle="Groupe Trois")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[left_right, hidden])
    await factories.insert_groupe_theme_score(
        db_conn, run_id=run_id, organe_id=organe_id, theme_id=left_right, score=0.2, cohesion=0.8
    )
    # Le thème caché est bien calculé (F2 : « le calcul continue de les produire en base »).
    await factories.insert_groupe_theme_score(
        db_conn, run_id=run_id, organe_id=organe_id, theme_id=hidden, score=-0.5, cohesion=0.7
    )

    response = await client.get("/groupe/PO3")

    assert response.status_code == 200
    assert "orientation-institutions-democratie" not in response.text
    assert "1 thème est calculé mais non publié" in response.text
