//! Tests d'intégration de `drain_once` — voir F3, docs/plans/phase-1.1-fix.md : un job qui échoue
//! doit faire échouer `drain_once` (donc `make ingest`) après avoir vidé la file jusqu'au bout,
//! jamais sortir en succès silencieux.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::drain_once;
use sqlx::PgPool;

/// `max_attempts = 1` pour que le job échoué passe directement en `failed`, sans le délai de
/// report exponentiel qui le laisserait `pending` après une seule tentative (voir `fail_job`,
/// `worker/src/jobs/queue.rs`).
async fn insert_failing_job(pool: &PgPool, job_type: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO job (type, payload, max_attempts) VALUES ($1, '{}'::jsonb, 1) RETURNING id",
        job_type,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `seconds: 1` sur les jobs `noop` pour garder les tests rapides (le défaut est 3 s).
async fn insert_noop_job(pool: &PgPool) -> i64 {
    insert_job_with_payload(pool, "noop", serde_json::json!({"seconds": 1})).await
}

async fn insert_job_with_payload(pool: &PgPool, job_type: &str, payload: serde_json::Value) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO job (type, payload) VALUES ($1, $2) RETURNING id",
        job_type,
        payload,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn job_status(pool: &PgPool, job_id: i64) -> String {
    sqlx::query_scalar!(
        r#"SELECT status::text AS "status!" FROM job WHERE id = $1"#,
        job_id
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn drain_once_fails_when_a_job_of_an_unknown_type_is_queued(pool: PgPool) {
    let job_id = insert_failing_job(&pool, "type_inconnu_de_test").await;

    let result = drain_once(&pool, "test-worker").await;

    assert!(
        result.is_err(),
        "un job en échec doit faire échouer drain_once"
    );
    assert_eq!(job_status(&pool, job_id).await, "failed");
}

/// La file est vidée jusqu'au bout même si un job échoue : un job sain mis en file après un job
/// en échec doit quand même s'exécuter, pas rester bloqué derrière le premier échec.
#[sqlx::test(migrations = "../db/migrations")]
async fn drain_once_keeps_draining_past_a_failure(pool: PgPool) {
    let failing_id = insert_failing_job(&pool, "type_inconnu_de_test").await;
    let noop_id = insert_noop_job(&pool).await;

    let result = drain_once(&pool, "test-worker").await;

    assert!(result.is_err());
    assert_eq!(job_status(&pool, failing_id).await, "failed");
    assert_eq!(
        job_status(&pool, noop_id).await,
        "done",
        "le job sain qui suit un échec doit tout de même être traité"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn drain_once_succeeds_when_only_a_noop_job_is_queued(pool: PgPool) {
    let job_id = insert_noop_job(&pool).await;

    let result = drain_once(&pool, "test-worker").await;

    assert!(result.is_ok());
    assert_eq!(job_status(&pool, job_id).await, "done");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn drain_once_succeeds_when_the_queue_is_empty(pool: PgPool) {
    let result = drain_once(&pool, "test-worker").await;
    assert!(result.is_ok());
}
