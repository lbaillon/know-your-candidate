//! Tests d'intégration du job `ingest_acteurs` sur l'extrait AMO30 versionné (voir
//! `fixtures/README.md`). `#[sqlx::test]` crée une base fraîche par test et joue les migrations.

#![allow(clippy::unwrap_used)]

use kyc_worker::jobs::ingest_acteurs;
use kyc_worker::run;
use sqlx::PgPool;

const FIXTURE: &[u8] = include_bytes!("fixtures/amo30-extrait.zip");

async fn insert_job(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO job (type, payload) VALUES ('ingest_acteurs', '{}'::jsonb) RETURNING id"
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn run_fixture(pool: &PgPool) -> (i64, serde_json::Value) {
    let job_id = insert_job(pool).await;
    let run_id = run::start(pool, "an_amo30", job_id, serde_json::json!({}))
        .await
        .unwrap();
    let bytes = bytes::Bytes::from_static(FIXTURE);
    let counters = ingest_acteurs::ingest_bytes(pool, run_id, bytes)
        .await
        .unwrap();
    (run_id, counters)
}

async fn checksum(pool: &PgPool, table: &str) -> String {
    let query = format!("SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM {table} t");
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn ingests_organes_persons_and_mandats(pool: PgPool) {
    let (_, counters) = run_fixture(&pool).await;

    // 53 acteurs cités par la fixture, dont 2 absents du référentiel (PA720634, PA429842,
    // voir fixtures/README.md) : ingest_acteurs n'en crée donc que 51.
    assert_eq!(counters["personnes"], 51);
    assert!(counters["organes"].as_u64().unwrap() > 0);

    let person_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM person")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(person_count, 51);
}

/// `PA332747` et `PA2511` sont choisis dans la fixture précisément pour ces deux cas (voir
/// `fixtures/README.md`), mais rien n'empêche d'autres acteurs réels du lot d'en présenter aussi
/// (85 inclusions et 10 charnières existent sur l'ensemble d'AMO30) : on vérifie donc leur
/// présence, pas un total global.
#[sqlx::test(migrations = "../db/migrations")]
async fn normalizes_inclusion_and_charniere_and_journalizes_them(pool: PgPool) {
    let (run_id, counters) = run_fixture(&pool).await;

    assert!(counters["normalisation_inclusions"].as_u64().unwrap() >= 1);
    assert!(counters["normalisation_charnieres"].as_u64().unwrap() >= 1);

    let kinds: Vec<String> = sqlx::query_scalar!(
        r#"SELECT DISTINCT kind FROM ingestion_anomaly WHERE ingestion_run_id = $1 ORDER BY kind"#,
        run_id,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(kinds.contains(&"mandat_inclus".to_string()));
    assert!(kinds.contains(&"mandat_charniere".to_string()));
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_fixture_is_a_no_op(pool: PgPool) {
    run_fixture(&pool).await;
    let organe_before = checksum(&pool, "organe").await;
    let person_before = checksum(&pool, "person").await;
    let mandat_before = checksum(&pool, "mandat").await;

    run_fixture(&pool).await;
    let organe_after = checksum(&pool, "organe").await;
    let person_after = checksum(&pool, "person").await;
    let mandat_after = checksum(&pool, "mandat").await;

    assert_eq!(organe_before, organe_after, "organe doit être inchangé");
    assert_eq!(person_before, person_after, "person doit être inchangé");
    assert_eq!(mandat_before, mandat_after, "mandat doit être inchangé");
}
