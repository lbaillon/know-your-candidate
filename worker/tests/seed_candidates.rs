//! Tests d'intégration du job `seed_candidates` — voir docs/plans/phase-2-api-ui.md, section
//! « Le seed des candidat·es ». Chaque test écrit son propre fichier TOML éphémère
//! (`tempfile::NamedTempFile`) : ce sont de petits fichiers texte inventés pour l'occasion, pas des
//! archives binaires versionnées comme les fixtures AN.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::seed_candidates;
use sqlx::PgPool;
use tempfile::NamedTempFile;

fn write_toml(contents: &str) -> NamedTempFile {
    use std::io::Write as _;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file
}

async fn insert_person_an(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_candidate_identified_by_an_uid(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        note = "test"
        "#,
    );

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(counters["entrees_lues"], 1);
    assert_eq!(counters["candidats_crees"], 1);
    assert_eq!(counters["personnes_creees"], 0);

    let row = sqlx::query!(
        "SELECT statut::text AS \"statut!\", source_url, note FROM candidate WHERE person_id = $1",
        person_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.statut, "declare");
    assert_eq!(row.source_url, "https://example.org/annonce");
    assert_eq!(row.note.as_deref(), Some("test"));
}

/// Le cas qui casse (D2.1) : une personne jamais élue à l'Assemblée, identifiée par son seul
/// `wikidata_qid`, créée par le job plutôt que refusée.
#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_candidate_identified_by_wikidata_qid_and_creates_the_person(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        wikidata_qid = "Q1"
        prenom = "Ada"
        nom = "Exemple"
        statut = "pressenti"
        source_url = "https://example.org/annonce"
        source_date = 2026-01-15
        "#,
    );

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(counters["personnes_creees"], 1);
    assert_eq!(counters["candidats_crees"], 1);

    let person = sqlx::query!("SELECT prenom, nom, an_uid FROM person WHERE wikidata_qid = 'Q1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(person.prenom.as_deref(), Some("Ada"));
    assert_eq!(person.nom.as_deref(), Some("Exemple"));
    assert_eq!(person.an_uid, None, "jamais élue à l'Assemblée");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_an_uid_fails_without_writing_anything(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA_INCONNU"
        statut = "declare"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_statut_is_rejected_before_any_write(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "ne_sait_pas"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn duplicate_identifier_in_the_file_is_rejected(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01

        [[candidate]]
        an_uid = "PA1"
        statut = "retire"
        source_url = "https://example.org/2"
        source_date = 2026-02-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn wikidata_entry_without_a_name_is_rejected(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        wikidata_qid = "Q1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM person")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "aucune personne ne doit être créée par une entrée invalide"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn blank_source_url_is_rejected(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "   "
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap()).await;
    assert!(result.is_err());
}

/// Rejouer le même fichier ne doit ni recréer ni faire bouger `updated_at` — idempotence, comme le
/// reste de l'ingestion (CLAUDE.md).
#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_file_is_a_no_op(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );

    seed_candidates::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();
    let updated_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar!("SELECT updated_at FROM candidate")
            .fetch_one(&pool)
            .await
            .unwrap();

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap())
        .await
        .unwrap();
    let updated_at_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar!("SELECT updated_at FROM candidate")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(counters["candidats_crees"], 0);
    assert_eq!(counters["candidats_mis_a_jour"], 0);
    assert_eq!(updated_at_before, updated_at_after);
}

/// Le seed est la source de vérité de `candidate` (voir plan) : une personne retirée du fichier
/// n'est plus candidate après un nouveau passage.
#[sqlx::test(migrations = "../db/migrations")]
async fn removing_an_entry_retires_the_candidate(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let with_entry = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    seed_candidates::seed(&pool, with_entry.path().to_str().unwrap())
        .await
        .unwrap();

    let empty = write_toml("");
    let counters = seed_candidates::seed(&pool, empty.path().to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(counters["candidats_retires"], 1);
    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_changed_field_updates_the_row_and_is_counted_as_a_change(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let declare = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    seed_candidates::seed(&pool, declare.path().to_str().unwrap())
        .await
        .unwrap();

    let retire = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "retire"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    let counters = seed_candidates::seed(&pool, retire.path().to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(counters["candidats_crees"], 0);
    assert_eq!(counters["candidats_mis_a_jour"], 1);
    let statut = sqlx::query_scalar!("SELECT statut::text AS \"statut!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "retire");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn missing_file_fails_cleanly(pool: PgPool) {
    let result = seed_candidates::seed(&pool, "/does/not/exist.toml").await;
    assert!(result.is_err());
}
