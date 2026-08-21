//! Tests de contrainte de la migration 0011 (accord des deux lectures) — voir
//! docs/plans/phase-4.1-partis-scores.md, section « Migration `0010_score_accord.sql` » (numérotée
//! 0011 en pratique, 0010 étant déjà pris par les vues matérialisées de la phase 4).

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

struct Fixture {
    person_id: i64,
    theme_id: i16,
    scrutin_id: i64,
    run_id: i64,
}

async fn fixture(pool: &PgPool) -> Fixture {
    Fixture {
        person_id: insert_person(pool, "PA1").await,
        theme_id: insert_theme(pool, "theme-a").await,
        scrutin_id: insert_scrutin(pool, "SC1").await,
        run_id: insert_run(pool).await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn insert_contribution(
    pool: &PgPool,
    f: &Fixture,
    position: &str,
    apport: Option<f64>,
    poids: f64,
    exclusion: Option<&str>,
) -> sqlx::Result<sqlx::postgres::PgQueryResult> {
    sqlx::query!(
        r#"
        INSERT INTO score_contribution (run_id, person_id, theme_id, scrutin_id, position, apport, poids, exclusion)
        VALUES ($1, $2, $3, $4, $5::text::vote_position, $6::float8::numeric, $7::float8::numeric, $8::text::contribution_exclusion)
        "#,
        f.run_id,
        f.person_id,
        f.theme_id,
        f.scrutin_id,
        position,
        apport,
        poids,
        exclusion,
    )
    .execute(pool)
    .await
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_theme_defaults_to_left_right_axis(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let axe: bool = sqlx::query_scalar!(
        "SELECT axe_gauche_droite FROM theme WHERE id = $1",
        theme_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        axe,
        "un thème ajouté sans y penser est traité comme gauche-droite, donc filtrable par F1"
    );
}

/// PIÈGE À NE PAS REPRODUIRE (voir le commentaire de la migration) : une contrainte écrite avec
/// `exclusion = 'abstention'` plutôt que `IS NOT DISTINCT FROM` laisserait passer une abstention
/// sans exclusion, parce que la comparaison à NULL vaut NULL et qu'un CHECK à NULL passe. Ce test
/// vérifie le comportement voulu, pas l'implémentation naïve.
#[sqlx::test(migrations = "../db/migrations")]
async fn an_abstention_without_an_exclusion_is_rejected(pool: PgPool) {
    let f = fixture(&pool).await;

    let result = insert_contribution(&pool, &f, "abstention", None, 0.0, None).await;

    assert!(
        result.is_err(),
        "une abstention doit toujours porter exclusion = 'abstention', jamais NULL"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_abstention_with_the_abstention_exclusion_is_accepted(pool: PgPool) {
    let f = fixture(&pool).await;

    insert_contribution(&pool, &f, "abstention", None, 0.0, Some("abstention"))
        .await
        .expect("une abstention correctement exclue doit passer");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_abstention_with_the_abstention_exclusion_is_rejected(pool: PgPool) {
    let f = fixture(&pool).await;

    let result = insert_contribution(&pool, &f, "pour", Some(0.5), 0.0, Some("abstention")).await;

    assert!(
        result.is_err(),
        "exclusion = 'abstention' n'a de sens que sur une ligne dont la position est 'abstention'"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_pour_contribution_excluded_for_disagreement_is_accepted(pool: PgPool) {
    let f = fixture(&pool).await;

    insert_contribution(&pool, &f, "pour", Some(0.5), 0.0, Some("desaccord_mesure"))
        .await
        .expect("un vote pour/contre écarté pour désaccord de mesure (F1) doit rester écrit");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_excluded_contribution_with_a_nonzero_weight_is_rejected(pool: PgPool) {
    let f = fixture(&pool).await;

    let result = insert_contribution(
        &pool,
        &f,
        "pour",
        Some(0.5),
        0.10000,
        Some("desaccord_mesure"),
    )
    .await;

    assert!(
        result.is_err(),
        "toute exclusion doit annuler le poids, F1 comme l'abstention (D4.10)"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_excluded_contribution_can_have_a_nonzero_weight(pool: PgPool) {
    let f = fixture(&pool).await;

    insert_contribution(&pool, &f, "pour", Some(0.5), 0.50000, None)
        .await
        .expect("une contribution non écartée pèse normalement");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn ecartes_desaccord_cannot_be_negative(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let run_id = insert_run(&pool).await;

    let result = sqlx::query!(
        r#"
        INSERT INTO person_theme_score
            (run_id, person_id, theme_id, score, incertitude, contributions, ecartes_desaccord)
        VALUES ($1, $2, $3, 0.200, 0.100, 5, -1)
        "#,
        run_id,
        person_id,
        theme_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un compte de contributions écartées ne peut pas être négatif"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn ecartes_desaccord_defaults_to_zero(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let theme_id = insert_theme(&pool, "theme-a").await;
    let run_id = insert_run(&pool).await;

    sqlx::query!(
        r#"
        INSERT INTO person_theme_score (run_id, person_id, theme_id, score, incertitude, contributions)
        VALUES ($1, $2, $3, 0.200, 0.100, 5)
        "#,
        run_id,
        person_id,
        theme_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let ecartes: i32 = sqlx::query_scalar!(
        "SELECT ecartes_desaccord FROM person_theme_score WHERE run_id = $1 AND person_id = $2 AND theme_id = $3",
        run_id,
        person_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ecartes, 0);
}
