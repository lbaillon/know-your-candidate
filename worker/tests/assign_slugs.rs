//! Tests d'intégration du job `assign_slugs` — voir docs/plans/phase-2-api-ui.md, section
//! « assign_slugs ».

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::assign_slugs;
use kyc_worker::run;
use sqlx::PgPool;

async fn insert_job(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO job (type, payload) VALUES ('assign_slugs', '{}'::jsonb) RETURNING id"
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn run_once(pool: &PgPool) -> (i64, serde_json::Value) {
    let job_id = insert_job(pool).await;
    let run_id = run::start(pool, "assign_slugs", job_id, serde_json::json!({}))
        .await
        .unwrap();
    let counters = assign_slugs::assign(pool, run_id).await.unwrap();
    (run_id, counters)
}

async fn insert_person_an(pool: &PgPool, an_uid: &str, prenom: &str, nom: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid, prenom, nom) VALUES ($1, $2, $3) RETURNING id",
        an_uid,
        prenom,
        nom,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn current_slug(pool: &PgPool, person_id: i64) -> Option<String> {
    sqlx::query_scalar!(
        "SELECT slug FROM person_slug WHERE person_id = $1 AND is_current",
        person_id,
    )
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn assigns_a_slugified_name(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1", "Jean-Luc", "Mélenchon").await;

    let (_, counters) = run_once(&pool).await;

    assert_eq!(counters["personnes_traitees"], 1);
    assert_eq!(counters["slugs_derives_d_identifiant"], 0);
    assert_eq!(
        current_slug(&pool, person_id).await,
        Some("jean-luc-melenchon".to_string())
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn suffixes_a_slug_already_taken_by_another_person(pool: PgPool) {
    let first = insert_person_an(&pool, "PA1", "Jean", "Dupont").await;
    let second = insert_person_an(&pool, "PA2", "Jean", "Dupont").await;

    run_once(&pool).await;

    let first_slug = current_slug(&pool, first).await.unwrap();
    let second_slug = current_slug(&pool, second).await.unwrap();
    assert_ne!(first_slug, second_slug);
    assert!(first_slug == "jean-dupont" || second_slug == "jean-dupont");
    assert!(first_slug == "jean-dupont-2" || second_slug == "jean-dupont-2");
}

/// Un ancien slug (`is_current = false`) reste réservé : une nouvelle personne dont le nom
/// produirait le même slug doit être suffixée, pas entrer en collision.
#[sqlx::test(migrations = "../db/migrations")]
async fn does_not_reuse_a_slug_reserved_by_a_former_holder(pool: PgPool) {
    let former_holder = insert_person_an(&pool, "PA1", "Jean", "Dupont").await;
    sqlx::query!(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ('jean-dupont', $1, false)",
        former_holder,
    )
    .execute(&pool)
    .await
    .unwrap();
    let new_person = insert_person_an(&pool, "PA2", "Jean", "Dupont").await;

    run_once(&pool).await;

    let new_slug = current_slug(&pool, new_person).await.unwrap();
    assert_ne!(new_slug, "jean-dupont");
}

/// Cas D2.1 qui casse dès le premier jour : une personne créée sans jamais avoir siégé (par le
/// futur job `seed_candidates`) n'a ni prénom ni nom exploitable pour un slug.
#[sqlx::test(migrations = "../db/migrations")]
async fn falls_back_to_the_lowercased_identifier_when_the_name_does_not_slugify(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA900042", "", "").await;

    let (run_id, counters) = run_once(&pool).await;

    assert_eq!(counters["slugs_derives_d_identifiant"], 1);
    assert_eq!(
        current_slug(&pool, person_id).await,
        Some("pa900042".to_string())
    );

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'slug_derive_d_identifiant'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 1);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn falls_back_to_wikidata_qid_when_there_is_no_an_uid(pool: PgPool) {
    let person_id = sqlx::query_scalar!(
        "INSERT INTO person (wikidata_qid) VALUES ($1) RETURNING id",
        "Q123",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    run_once(&pool).await;

    assert_eq!(
        current_slug(&pool, person_id).await,
        Some("q123".to_string())
    );
}

/// Propriété centrale de D2.3 : un slug déjà attribué n'est jamais réécrit, même si le nom change
/// ensuite (un mariage, une correction orthographique...) — sinon les liens partagés casseraient.
#[sqlx::test(migrations = "../db/migrations")]
async fn never_overwrites_an_existing_current_slug(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1", "Jean", "Dupont").await;
    run_once(&pool).await;
    let slug_before = current_slug(&pool, person_id).await.unwrap();

    sqlx::query!("UPDATE person SET nom = 'Durand' WHERE id = $1", person_id)
        .execute(&pool)
        .await
        .unwrap();
    let (_, counters) = run_once(&pool).await;

    assert_eq!(
        counters["personnes_traitees"], 0,
        "plus aucune personne sans slug courant"
    );
    assert_eq!(current_slug(&pool, person_id).await, Some(slug_before));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_is_a_no_op_once_everyone_has_a_slug(pool: PgPool) {
    insert_person_an(&pool, "PA1", "Jean", "Dupont").await;
    run_once(&pool).await;

    let (_, counters) = run_once(&pool).await;

    assert_eq!(counters["personnes_traitees"], 0);
}
