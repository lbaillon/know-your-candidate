"""Tests de la page d'explication `/personne/{slug}/theme/{slug}` — voir
docs/plans/phase-4-partis-scores.md, D4 « L'explication est le produit » : toutes les
contributions sont listées, jamais tronquées ; un thème inéligible ou inconnu rend 404.
"""

import asyncpg
from httpx import AsyncClient

import factories


async def test_the_page_lists_every_contribution_not_just_the_first_few(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(
        db_conn,
        slug="social-fiscalite",
        libelle_pole_negatif="redistribution",
        libelle_pole_positif="maîtrise de la fiscalité",
    )
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-un")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])

    scrutins = []
    for i in range(12):
        scrutin_id = await factories.insert_scrutin(
            db_conn, an_uid=f"SC{i}", numero=i, titre=f"scrutin numero {i}"
        )
        scrutins.append(scrutin_id)
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
        contributions=12,
        relues=0,
    )
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-un/theme/social-fiscalite")

    assert response.status_code == 200
    for i in range(12):
        assert f"scrutin numero {i}" in response.text


async def test_an_abstention_is_listed_and_marked_as_not_entering_the_calculation(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    theme_id = await factories.insert_theme(db_conn, slug="securite")
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-deux")
    run_id = await factories.insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    scrutin_id = await factories.insert_scrutin(db_conn, an_uid="SC1", titre="scrutin abstenu")
    await factories.insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutin_id,
        position="abstention",
        apport=None,
        poids=0.0,
    )
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-deux/theme/securite")

    assert response.status_code == 200
    assert "scrutin abstenu" in response.text
    assert "n'entre pas dans le calcul" in response.text


async def test_an_ineligible_or_unknown_theme_is_a_404(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-trois")

    response = await client.get("/personne/candidat-trois/theme/inconnu")

    assert response.status_code == 404


async def test_below_threshold_theme_still_shows_its_contributions(
    client: AsyncClient, db_conn: asyncpg.Connection
) -> None:
    """Même sous le seuil (pas de score public sur la fiche), la page d'explication reste la
    preuve honnête de ce qui existe : elle liste les votes réels plutôt que de rendre un 404.
    """
    theme_id = await factories.insert_theme(db_conn, slug="agriculture")
    person_id = await factories.insert_person(db_conn, an_uid="PA1")
    await factories.insert_slug(db_conn, person_id=person_id, slug="candidat-quatre")
    run_id = await factories.insert_score_run(
        db_conn, eligible_theme_ids=[theme_id], contributions_min=5
    )
    scrutin_id = await factories.insert_scrutin(db_conn, an_uid="SC1", titre="scrutin isolé")
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
    await factories.refresh_score_views(db_conn)

    response = await client.get("/personne/candidat-quatre/theme/agriculture")

    assert response.status_code == 200
    assert "données insuffisantes" in response.text
    assert "scrutin isolé" in response.text
