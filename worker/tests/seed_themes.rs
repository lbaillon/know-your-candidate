//! Tests d'intégration du job `seed_themes` — voir docs/plans/phase-3-categorisation.md, section
//! « Le seed des thèmes ». Même modèle que `seed_candidates` (docs/plans/phase-2-api-ui.md) :
//! chaque test écrit son propre fichier TOML éphémère.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::seed_themes;
use sqlx::PgPool;
use tempfile::NamedTempFile;

fn write_toml(contents: &str) -> NamedTempFile {
    use std::io::Write as _;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file
}

async fn theme_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM theme"#)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_theme_with_an_axis(pool: PgPool) {
    let file = write_toml(
        r#"
        [[theme]]
        slug = "social-fiscalite"
        libelle = "Social / fiscalité"
        description = "Prélèvements, prestations, retraites."
        pole_negatif = "redistribution"
        pole_positif = "maîtrise de la fiscalité"
        rang = 10
        "#,
    );

    let counters = seed_themes::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(counters["themes_crees"], 1);
    assert_eq!(counters["themes_mis_a_jour"], 0);

    let row = sqlx::query!(
        r#"SELECT libelle, description, libelle_pole_negatif, libelle_pole_positif, rang, actif
           FROM theme WHERE slug = 'social-fiscalite'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.libelle, "Social / fiscalité");
    assert_eq!(row.libelle_pole_negatif.as_deref(), Some("redistribution"));
    assert_eq!(
        row.libelle_pole_positif.as_deref(),
        Some("maîtrise de la fiscalité")
    );
    assert_eq!(row.rang, 10);
    assert!(row.actif);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_theme_without_an_axis(pool: PgPool) {
    let file = write_toml(
        r#"
        [[theme]]
        slug = "autre"
        libelle = "Autre"
        description = "Ne relève d'aucun thème."
        rang = 99
        "#,
    );

    seed_themes::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();

    let row = sqlx::query!(
        "SELECT libelle_pole_negatif, libelle_pole_positif FROM theme WHERE slug = 'autre'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.libelle_pole_negatif.is_none());
    assert!(row.libelle_pole_positif.is_none());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_theme_with_only_one_pole_is_rejected_without_writing_anything(pool: PgPool) {
    let file = write_toml(
        r#"
        [[theme]]
        slug = "boiteux"
        libelle = "Boiteux"
        description = "Un seul pôle renseigné."
        pole_negatif = "seulement celui-ci"
        rang = 1
        "#,
    );

    let result = seed_themes::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
    assert_eq!(theme_count(&pool).await, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_duplicate_slug_in_the_file_is_rejected(pool: PgPool) {
    let file = write_toml(
        r#"
        [[theme]]
        slug = "double"
        libelle = "Premier"
        description = "d1"
        rang = 1

        [[theme]]
        slug = "double"
        libelle = "Second"
        description = "d2"
        rang = 2
        "#,
    );

    let result = seed_themes::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
    assert_eq!(theme_count(&pool).await, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_duplicate_rang_is_rejected(pool: PgPool) {
    let file = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1

        [[theme]]
        slug = "b"
        libelle = "B"
        description = "db"
        rang = 1
        "#,
    );

    let result = seed_themes::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
    assert_eq!(theme_count(&pool).await, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_misspelled_table_key_is_rejected_without_writing_anything(pool: PgPool) {
    let file = write_toml(
        r#"
        [[themes]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1
        "#,
    );

    let result = seed_themes::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
    assert_eq!(theme_count(&pool).await, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_file_is_a_no_op(pool: PgPool) {
    let contents = r#"
        [[theme]]
        slug = "social-fiscalite"
        libelle = "Social / fiscalité"
        description = "Prélèvements, prestations, retraites."
        pole_negatif = "redistribution"
        pole_positif = "maîtrise de la fiscalité"
        rang = 10
    "#;

    let first = write_toml(contents);
    let counters_first = seed_themes::seed(&pool, first.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(counters_first["themes_crees"], 1);

    let second = write_toml(contents);
    let counters_second = seed_themes::seed(&pool, second.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(counters_second["themes_crees"], 0);
    assert_eq!(counters_second["themes_mis_a_jour"], 0);
    assert_eq!(counters_second["themes_desactives"], 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn removing_a_theme_from_the_file_deactivates_it_instead_of_deleting_it(pool: PgPool) {
    let first = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1

        [[theme]]
        slug = "b"
        libelle = "B"
        description = "db"
        rang = 2
        "#,
    );
    seed_themes::seed(&pool, first.path().to_str().unwrap())
        .await
        .unwrap();

    let second = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1
        "#,
    );
    let counters = seed_themes::seed(&pool, second.path().to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(counters["themes_desactives"], 1);

    // Toujours en base, jamais supprimé — seulement désactivé.
    assert_eq!(theme_count(&pool).await, 2);
    let actif: bool = sqlx::query_scalar!("SELECT actif FROM theme WHERE slug = 'b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!actif);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_theme_reappearing_in_the_file_is_reactivated(pool: PgPool) {
    let with_b = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1

        [[theme]]
        slug = "b"
        libelle = "B"
        description = "db"
        rang = 2
        "#,
    );
    seed_themes::seed(&pool, with_b.path().to_str().unwrap())
        .await
        .unwrap();

    let without_b = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1
        "#,
    );
    seed_themes::seed(&pool, without_b.path().to_str().unwrap())
        .await
        .unwrap();

    let with_b_again = write_toml(
        r#"
        [[theme]]
        slug = "a"
        libelle = "A"
        description = "da"
        rang = 1

        [[theme]]
        slug = "b"
        libelle = "B"
        description = "db"
        rang = 2
        "#,
    );
    seed_themes::seed(&pool, with_b_again.path().to_str().unwrap())
        .await
        .unwrap();

    let actif: bool = sqlx::query_scalar!("SELECT actif FROM theme WHERE slug = 'b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(actif);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn missing_file_fails_cleanly(pool: PgPool) {
    let result = seed_themes::seed(&pool, "/does/not/exist.toml").await;
    assert!(result.is_err());
}
