//! Tests d'intégration du job `ingest_acteurs` sur les fixtures JSON versionnées (voir
//! `fixtures/README.md`). `#[sqlx::test]` crée une base fraîche par test et joue les migrations.
//! Le zip lu par le job est reconstruit en mémoire par `support::zip_fixture_dir` : le contenu des
//! fixtures est inventé (D1.17, docs/plans/phase-1.1-fix.md), donc les compteurs ci-dessous sont
//! propres à cette fixture, pas ceux du corpus réel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use kyc_worker::jobs::ingest_acteurs;
use kyc_worker::run;
use sqlx::PgPool;

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
    let bytes = support::zip_fixture_dir("amo30");
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

/// 17 acteurs dans la fixture (voir fixtures/README.md), tous présents dans le référentiel : les
/// acteurs absents du référentiel (`PA900023`, `PA900024`, `PA900099`) ne sont cités que par les
/// scrutins, pas par `amo30/`, et ce job ne les voit donc jamais.
#[sqlx::test(migrations = "../db/migrations")]
async fn ingests_organes_and_persons(pool: PgPool) {
    let (_, counters) = run_fixture(&pool).await;

    assert_eq!(counters["personnes"], 17);
    // 8 organes dans la fixture, 1 seul (PO900007, GROUPESENAT) n'est pas d'un type retenu (D1.12).
    assert_eq!(counters["organes"], 7);

    let person_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM person")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(person_count, 17);

    let organe_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM organe")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(organe_count, 7);

    let groupesenat_present: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM organe WHERE an_uid = 'PO900007') AS \"exists!\""
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        !groupesenat_present,
        "un organe d'un type non retenu ne doit pas entrer dans le référentiel"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn normalizes_inclusion_and_charniere_and_journalizes_them(pool: PgPool) {
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["normalisation_inclusions"], 1);
    assert_eq!(counters["normalisation_charnieres"], 1);

    let kinds: Vec<String> = sqlx::query_scalar!(
        r#"SELECT DISTINCT kind FROM ingestion_anomaly WHERE ingestion_run_id = $1 ORDER BY kind"#,
        run_id,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(kinds.contains(&"mandat_inclus".to_string()));
    assert!(kinds.contains(&"mandat_charniere".to_string()));

    let inclus_retenu: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM mandat WHERE an_uid = 'PM900010A') AS \"exists!\""
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(inclus_retenu, "le mandat englobant doit être conservé");
    let inclus_ecarte: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM mandat WHERE an_uid = 'PM900010B') AS \"exists!\""
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!inclus_ecarte, "le mandat inclus doit être écarté");
}

/// Chevauchement réel entre deux organes différents (`PA900012`, voir fixtures/README.md) :
/// toujours journalisé. **Attendu à écarter/mettre à jour par F5** (docs/plans/phase-1.1-fix.md,
/// D1.16) : aujourd'hui la plage la plus courte est encore retirée de `mandat` ; après F5 les
/// deux mandats doivent coexister. Ne pas retoucher cette note en dehors du commit F5.
#[sqlx::test(migrations = "../db/migrations")]
async fn real_cross_organe_overlap_is_journalized(pool: PgPool) {
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["normalisation_chevauchements"], 1);

    let longer_retained: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM mandat WHERE an_uid = 'PM900012A') AS \"exists!\""
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(longer_retained, "le mandat le plus long doit toujours être conservé");

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'mandat_chevauchement'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 1);
}

/// Un chevauchement avec le pseudo-groupe non-inscrit n'est jamais une vraie contradiction
/// (methodology.md § 2) : les deux mandats coexistent, sans aucune anomalie.
#[sqlx::test(migrations = "../db/migrations")]
async fn overlap_with_non_inscrit_is_kept_without_anomaly(pool: PgPool) {
    let (_, counters) = run_fixture(&pool).await;

    let both_retained: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM mandat WHERE an_uid IN ('PM900013A', 'PM900013B')"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(both_retained, 2);

    // Seul PA900012 doit produire un chevauchement journalisé sur cette fixture : PA900013 (NI)
    // n'y contribue pas.
    assert_eq!(counters["normalisation_chevauchements"], 1);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn mandat_en_cours_is_kept_as_is(pool: PgPool) {
    run_fixture(&pool).await;

    let fin: Option<String> = sqlx::query_scalar(
        "SELECT upper(period)::text FROM mandat WHERE an_uid = 'PM900014A'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fin, None, "un mandat en cours n'a pas de borne supérieure");
}

/// `PA900015` porte deux mandats à écarter, pour deux raisons distinctes : ce sont des compteurs,
/// pas des `ingestion_anomaly` (voir docs/plans/phase-1-ingestion.md, étape `ingest_acteurs`).
#[sqlx::test(migrations = "../db/migrations")]
async fn mandats_manquant_leur_date_debut_ou_incoherents_sont_ecartes(pool: PgPool) {
    let (_, counters) = run_fixture(&pool).await;

    assert_eq!(counters["mandats_ignores_date_debut_manquante"], 1);
    assert_eq!(counters["mandats_ignores_date_fin_avant_debut"], 1);

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM mandat WHERE an_uid IN ('PM900015A', 'PM900015B')"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
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
