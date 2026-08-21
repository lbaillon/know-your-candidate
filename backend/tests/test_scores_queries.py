"""Tests de `queries/scores.py` — voir docs/plans/phase-4-partis-scores.md, commit « Vues
matérialisées et lectures backend ». Ces tests écrivent directement dans les tables de score
(sans passer par le job worker) : ils vérifient la lecture, pas le calcul, qui est couvert côté
worker (worker/tests/scoring.rs, worker/tests/recompute_scores.rs).
"""

from datetime import date

import asyncpg

from factories import (
    insert_groupe_theme_score,
    insert_mandat,
    insert_mandat_theme_score,
    insert_organe,
    insert_person,
    insert_person_theme_score,
    insert_score_contribution,
    insert_score_run,
    insert_scrutin,
    insert_theme,
    refresh_score_views,
)
from kyc_api.queries import scores as scores_queries


async def test_orientation_with_score_returns_score_and_real_counts(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="social-fiscalite")
    person_id = await insert_person(db_conn)
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])

    scrutins = [await insert_scrutin(db_conn, an_uid=f"SC{i}", numero=i) for i in range(3)]
    await insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutins[0],
        position="pour",
        apport=0.6,
        poids=1.0,
    )
    await insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutins[1],
        position="contre",
        apport=0.6,
        poids=1.0,
    )
    await insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        scrutin_id=scrutins[2],
        position="abstention",
        apport=None,
        poids=0.0,
    )
    await insert_person_theme_score(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=theme_id,
        score=0.42,
        incertitude=0.05,
        contributions=2,
        abstentions=1,
        relues=0,
    )
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_orientations(db_conn, person_id)

    assert len(orientations) == 1
    orientation = orientations[0]
    assert orientation.theme.slug == "social-fiscalite"
    assert orientation.score == 0.42
    assert orientation.incertitude == 0.05
    assert orientation.relues == 0
    assert orientation.contributions == 2, "seules les deux contributions de poids > 0 comptent"
    assert orientation.abstentions == 1
    assert orientation.seuil_atteint is True


async def test_orientation_below_threshold_has_no_score_but_a_real_contribution_count(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="agriculture")
    person_id = await insert_person(db_conn)
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id], contributions_min=5)

    scrutins = [await insert_scrutin(db_conn, an_uid=f"SC{i}", numero=i) for i in range(3)]
    for scrutin_id in scrutins:
        await insert_score_contribution(
            db_conn,
            run_id=run_id,
            person_id=person_id,
            theme_id=theme_id,
            scrutin_id=scrutin_id,
            position="pour",
            apport=0.5,
            poids=1.0,
        )
    # Pas de ligne person_theme_score : 3 contributions < contributions_min = 5.
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_orientations(db_conn, person_id)

    assert len(orientations) == 1
    orientation = orientations[0]
    assert orientation.score is None
    assert orientation.incertitude is None
    assert orientation.relues is None
    assert orientation.contributions == 3, "le compte réel s'affiche même sous le seuil (D4.2)"
    assert orientation.seuil_atteint is False


async def test_an_ineligible_theme_never_appears(db_conn: asyncpg.Connection) -> None:
    eligible_theme_id = await insert_theme(db_conn, slug="securite")
    ineligible_theme_id = await insert_theme(db_conn, slug="europe", rang=2)
    person_id = await insert_person(db_conn)
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[eligible_theme_id])

    scrutin_id = await insert_scrutin(db_conn, an_uid="SC1")
    # Une contribution existe même pour le thème inéligible (données réelles, D4.7 : le thème
    # n'existe simplement pas dans ce run, quoi qu'il arrive par ailleurs).
    await insert_score_contribution(
        db_conn,
        run_id=run_id,
        person_id=person_id,
        theme_id=ineligible_theme_id,
        scrutin_id=scrutin_id,
        position="pour",
        apport=0.5,
        poids=1.0,
    )
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_orientations(db_conn, person_id)

    assert [o.theme.slug for o in orientations] == ["securite"]


async def test_get_person_theme_contributions_lists_all_ordered_by_date_desc(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="sante")
    person_id = await insert_person(db_conn)
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])

    older = await insert_scrutin(db_conn, an_uid="SC-OLD", numero=1, date_scrutin=date(2024, 1, 1))
    newer = await insert_scrutin(db_conn, an_uid="SC-NEW", numero=2, date_scrutin=date(2024, 6, 1))
    for scrutin_id in (older, newer):
        await insert_score_contribution(
            db_conn,
            run_id=run_id,
            person_id=person_id,
            theme_id=theme_id,
            scrutin_id=scrutin_id,
            position="pour",
            apport=0.5,
            poids=1.0,
        )

    contributions = await scores_queries.get_person_theme_contributions(
        db_conn, person_id, theme_id
    )

    assert [c.scrutin_an_uid for c in contributions] == ["SC-NEW", "SC-OLD"]


async def test_mandat_orientation_reflects_the_mandat_period(db_conn: asyncpg.Connection) -> None:
    theme_id = await insert_theme(db_conn, slug="travail")
    person_id = await insert_person(db_conn)
    organe_id = await insert_organe(db_conn, an_uid="PO1", libelle="Groupe de test")
    await insert_mandat(
        db_conn,
        an_uid="MDT1",
        person_id=person_id,
        organe_id=organe_id,
        debut=date(2022, 6, 1),
    )
    mandat_id = await db_conn.fetchval("SELECT id FROM mandat WHERE an_uid = $1", "MDT1")
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await insert_mandat_theme_score(
        db_conn, run_id=run_id, mandat_id=mandat_id, theme_id=theme_id, score=0.3, cohesion=0.8
    )
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_mandat_orientations(db_conn, person_id)

    assert len(orientations) == 1
    orientation = orientations[0]
    assert orientation.organe_an_uid == "PO1"
    assert orientation.debut == date(2022, 6, 1)
    assert orientation.fin is None, "mandat toujours en cours : pas de borne haute"
    assert orientation.score == 0.3
    assert orientation.cohesion == 0.8


async def test_groupe_orientation_lists_the_group_score(db_conn: asyncpg.Connection) -> None:
    theme_id = await insert_theme(db_conn, slug="institutions")
    organe_id = await insert_organe(db_conn, an_uid="PO2", libelle="Autre groupe")
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await insert_groupe_theme_score(
        db_conn,
        run_id=run_id,
        organe_id=organe_id,
        theme_id=theme_id,
        score=-0.2,
        cohesion=0.95,
        contributions=120,
        membres=45,
    )

    orientations = await scores_queries.get_groupe_orientations(db_conn, organe_id)

    assert len(orientations) == 1
    orientation = orientations[0]
    assert orientation.score == -0.2
    assert orientation.cohesion == 0.95
    assert orientation.membres == 45


# --- F2, docs/plans/phase-4.1-partis-scores.md : un thème dont l'axe ne se lit pas gauche-droite
# reste calculé mais n'apparaît jamais dans une lecture publique. -------------------------------


async def test_a_non_left_right_theme_never_appears_in_person_orientations(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="institutions-democratie", axe_gauche_droite=False)
    person_id = await insert_person(db_conn)
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await insert_person_theme_score(
        db_conn, run_id=run_id, person_id=person_id, theme_id=theme_id, score=-0.4, contributions=10
    )
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_orientations(db_conn, person_id)

    assert orientations == []


async def test_a_non_left_right_theme_never_appears_in_mandat_orientations(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="agriculture", axe_gauche_droite=False)
    person_id = await insert_person(db_conn)
    organe_id = await insert_organe(db_conn, an_uid="PO1")
    await insert_mandat(
        db_conn, an_uid="MDT1", person_id=person_id, organe_id=organe_id, debut=date(2022, 6, 1)
    )
    mandat_id = await db_conn.fetchval("SELECT id FROM mandat WHERE an_uid = $1", "MDT1")
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await insert_mandat_theme_score(
        db_conn, run_id=run_id, mandat_id=mandat_id, theme_id=theme_id, score=0.3, cohesion=0.9
    )
    await refresh_score_views(db_conn)

    orientations = await scores_queries.get_person_mandat_orientations(db_conn, person_id)

    assert orientations == []


async def test_a_non_left_right_theme_never_appears_in_groupe_orientations(
    db_conn: asyncpg.Connection,
) -> None:
    theme_id = await insert_theme(db_conn, slug="europe", axe_gauche_droite=False)
    organe_id = await insert_organe(db_conn, an_uid="PO1")
    run_id = await insert_score_run(db_conn, eligible_theme_ids=[theme_id])
    await insert_groupe_theme_score(
        db_conn, run_id=run_id, organe_id=organe_id, theme_id=theme_id, score=0.1, cohesion=0.6
    )

    orientations = await scores_queries.get_groupe_orientations(db_conn, organe_id)

    assert orientations == []


async def test_count_hidden_eligible_themes_counts_only_non_left_right_eligible_ones(
    db_conn: asyncpg.Connection,
) -> None:
    left_right = await insert_theme(db_conn, slug="securite", axe_gauche_droite=True)
    hidden_a = await insert_theme(db_conn, slug="institutions-democratie", axe_gauche_droite=False)
    hidden_b = await insert_theme(db_conn, slug="agriculture", axe_gauche_droite=False, rang=2)
    await insert_score_run(db_conn, eligible_theme_ids=[left_right, hidden_a, hidden_b])

    count = await scores_queries.count_hidden_eligible_themes(db_conn)

    assert count == 2


async def test_count_hidden_eligible_themes_is_zero_without_a_current_run(
    db_conn: asyncpg.Connection,
) -> None:
    count = await scores_queries.count_hidden_eligible_themes(db_conn)

    assert count == 0
