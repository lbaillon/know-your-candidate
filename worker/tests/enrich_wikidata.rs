//! Tests d'intégration de `update_wikidata_qids` et `upsert_photos`, avec des lignes construites à
//! la main — pas de réseau (voir F2, docs/plans/phase-1.1-fix.md). Le chemin HTTP (SPARQL, Commons)
//! n'est vérifié que par l'exécution réelle du job (F2c) : introduire une abstraction de client
//! pour deux appels ne vaut pas son coût.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::enrich_wikidata::{PhotoRow, update_wikidata_qids, upsert_photos};
use sqlx::PgPool;

async fn insert_person(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn checksum(pool: &PgPool, table: &str) -> String {
    let query = format!("SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM {table} t");
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn update_wikidata_qids_writes_the_qid(pool: PgPool) {
    insert_person(&pool, "PA1").await;
    insert_person(&pool, "PA2").await;

    let updated = update_wikidata_qids(&pool, &[("PA1", "Q1"), ("PA2", "Q2")])
        .await
        .unwrap();
    assert_eq!(updated, 2);

    let qid: Option<String> =
        sqlx::query_scalar!("SELECT wikidata_qid FROM person WHERE an_uid = 'PA1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(qid.as_deref(), Some("Q1"));
}

/// Le test le plus important : rejouer la même écriture ne doit toucher aucune ligne (condensé
/// inchangé), et surtout jamais violer l'unicité de `wikidata_qid` — la garantie que F2a vise à
/// protéger.
#[sqlx::test(migrations = "../db/migrations")]
async fn update_wikidata_qids_rerun_is_a_no_op(pool: PgPool) {
    insert_person(&pool, "PA1").await;
    insert_person(&pool, "PA2").await;

    update_wikidata_qids(&pool, &[("PA1", "Q1"), ("PA2", "Q2")])
        .await
        .unwrap();
    let before = checksum(&pool, "person").await;

    let updated_again = update_wikidata_qids(&pool, &[("PA1", "Q1"), ("PA2", "Q2")])
        .await
        .unwrap();
    let after = checksum(&pool, "person").await;

    assert_eq!(updated_again, 0, "aucune ligne n'est réellement modifiée");
    assert_eq!(
        before, after,
        "person doit être inchangé, updated_at compris"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn upsert_photos_inserts_a_row_with_its_licence(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;

    upsert_photos(
        &pool,
        &[PhotoRow {
            person_id,
            url: "https://upload.wikimedia.org/A.jpg".to_string(),
            commons_file: "File:A.jpg".to_string(),
            licence: "CC BY-SA 4.0".to_string(),
            licence_url: Some("https://creativecommons.org/licenses/by-sa/4.0/".to_string()),
            auteur: Some("Jean Dupont".to_string()),
        }],
    )
    .await
    .unwrap();

    let row = sqlx::query!(
        "SELECT url, licence, auteur FROM person_photo WHERE person_id = $1",
        person_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.url, "https://upload.wikimedia.org/A.jpg");
    assert_eq!(row.licence, "CC BY-SA 4.0");
    assert_eq!(row.auteur.as_deref(), Some("Jean Dupont"));
}

/// Idempotence sur `person_photo` (jamais vérifiée avant F2, voir le plan) : rejouer la même
/// photo ne doit toucher aucune ligne, `fetched_at` compris.
#[sqlx::test(migrations = "../db/migrations")]
async fn upsert_photos_rerun_is_a_no_op(pool: PgPool) {
    let person_id = insert_person(&pool, "PA1").await;
    let row = PhotoRow {
        person_id,
        url: "https://upload.wikimedia.org/A.jpg".to_string(),
        commons_file: "File:A.jpg".to_string(),
        licence: "CC BY-SA 4.0".to_string(),
        licence_url: None,
        auteur: None,
    };

    upsert_photos(&pool, &[row]).await.unwrap();
    let before = checksum(&pool, "person_photo").await;

    let row_again = PhotoRow {
        person_id,
        url: "https://upload.wikimedia.org/A.jpg".to_string(),
        commons_file: "File:A.jpg".to_string(),
        licence: "CC BY-SA 4.0".to_string(),
        licence_url: None,
        auteur: None,
    };
    upsert_photos(&pool, &[row_again]).await.unwrap();
    let after = checksum(&pool, "person_photo").await;

    assert_eq!(
        before, after,
        "person_photo doit être inchangé, fetched_at compris"
    );
}
