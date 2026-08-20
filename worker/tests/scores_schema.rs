//! Tests de contrainte de la migration 0009 (scores) — voir
//! docs/plans/phase-4-partis-scores.md, section « Migration `0009_scores.sql` ».

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sqlx::PgPool;

async fn insert_person(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid, nom, prenom) VALUES ($1, 'Dupont', 'Jean') RETURNING id",
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_theme(pool: &PgPool, slug: &str) -> i16 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO theme (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description', 'pôle négatif', 'pôle positif', 1)
        RETURNING id
        "#,
        slug,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_organe(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO organe (an_uid, code_type, libelle) VALUES ($1, 'GP', 'Groupe de test') RETURNING id",
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_scrutin(pool: &PgPool, an_uid: &str) -> i64 {
    let source_document_id = sqlx::query_scalar!(
        r#"
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        "#,
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query_scalar!(
        r#"
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, 1, 17, '2024-01-01', 'SPO', 'titre', 'DecompteNominatif', 0, 0, 0, 0, 0, 0, 577, $2)
        RETURNING id
        "#,
        an_uid,
        source_document_id,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_run(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO score_run (formula_version, contributions_min, scrutins_min_par_theme)
        VALUES (1, 5, 10)
        RETURNING id
        "#
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn only_one_current_score_run_is_allowed(pool: PgPool) {
    sqlx::query!(
        r#"INSERT INTO score_run (formula_version, contributions_min, scrutins_min_par_theme, is_current)
           VALUES (1, 5, 10, true)"#
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query!(
        r#"INSERT INTO score_run (formula_version, contributions_min, scrutins_min_par_theme, is_current)
           VALUES (1, 5, 10, true)"#
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "au plus un run courant à la fois");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_abstention_with_an_apport_is_rejected(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO score_contribution (run_id, person_id, theme_id, scrutin_id, position, apport, poids)
        VALUES ($1, $2, $3, $4, 'abstention', 0.200, 0.00000)
        "#,
        run_id,
        person_id,
        theme_id,
        scrutin_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "une abstention n'a pas de direction (D4.10), elle ne peut pas porter d'apport"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_abstention_without_an_apport_is_rejected(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO score_contribution (run_id, person_id, theme_id, scrutin_id, position, apport, poids)
        VALUES ($1, $2, $3, $4, 'pour', NULL, 0.50000)
        "#,
        run_id,
        person_id,
        theme_id,
        scrutin_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un vote pour/contre doit toujours porter un apport"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_abstention_with_a_nonzero_weight_is_rejected(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO score_contribution (run_id, person_id, theme_id, scrutin_id, position, apport, poids)
        VALUES ($1, $2, $3, $4, 'abstention', NULL, 0.10000)
        "#,
        run_id,
        person_id,
        theme_id,
        scrutin_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "une abstention est listée mais ne pèse jamais (D4.10)"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_score_outside_the_axis_range_is_rejected(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO person_theme_score (run_id, person_id, theme_id, score, incertitude, contributions)
        VALUES ($1, $2, $3, 1.500, 0.100, 5)
        "#,
        run_id,
        person_id,
        theme_id,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "un score doit rester dans [-1, 1]");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_cohesion_outside_zero_one_is_rejected(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let organe_id = insert_organe(&pool, "PO1").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO groupe_theme_score (run_id, organe_id, theme_id, score, cohesion, contributions, membres)
        VALUES ($1, $2, $3, 0.200, 1.500, 10, 20)
        "#,
        run_id,
        organe_id,
        theme_id,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "la cohésion doit rester dans [0, 1]");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_bipolarite_outside_zero_one_is_rejected(pool: PgPool) {
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    sqlx::query!(
        r#"INSERT INTO group_axis (version, description, grille_version, grille_date, source_url, content_hash, is_current)
           VALUES ('v1', 'd', 'g1', '2026-01-01', 'https://example.org', 'hash1', true)"#
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query!(
        r#"
        INSERT INTO scrutin_axis_estimate
            (scrutin_id, strategy, axis_version, position_pour, separation, couverture, votants_couverts, bipolarite)
        VALUES ($1, 'group_alignment', 'v1', 0.200, 0.800, 0.950, 400, 1.500)
        "#,
        scrutin_id,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "la bipolarité doit rester dans [0, 1]");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_null_bipolarite_is_accepted(pool: PgPool) {
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    sqlx::query!(
        r#"INSERT INTO group_axis (version, description, grille_version, grille_date, source_url, content_hash, is_current)
           VALUES ('v1', 'd', 'g1', '2026-01-01', 'https://example.org', 'hash1', true)"#
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query!(
        r#"
        INSERT INTO scrutin_axis_estimate
            (scrutin_id, strategy, axis_version, position_pour, separation, couverture, votants_couverts)
        VALUES ($1, 'group_alignment', 'v1', 0.200, 0.800, 0.950, 400)
        "#,
        scrutin_id,
    )
    .execute(&pool)
    .await
    .expect("une estimation existante avant cette migration n'a pas de bipolarité connue");
}
