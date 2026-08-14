//! Ouverture et clôture d'un `ingestion_run` — voir docs/plans/phase-1-ingestion.md.
//!
//! Le schéma de `ingestion_run` n'est complété (job_id, params, url, content_hash, error) qu'à la
//! migration 0004 ; avant elle, un échec est encodé dans `counters` pour ne pas perdre
//! l'information, et corrigé dès que la colonne `error` existe (voir Étapes, plan de phase 1).

use sqlx::PgPool;

pub async fn start(pool: &PgPool, source: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO ingestion_run (source, status, started_at)
        VALUES ($1, 'running', now())
        RETURNING id
        "#,
        source,
    )
    .fetch_one(pool)
    .await
}

pub async fn finish_ok(
    pool: &PgPool,
    run_id: i64,
    counters: serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE ingestion_run
        SET status = 'done', counters = $2, finished_at = now()
        WHERE id = $1
        "#,
        run_id,
        counters,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn finish_err(pool: &PgPool, run_id: i64, error: &str) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE ingestion_run
        SET status = 'failed',
            counters = counters || jsonb_build_object('error', $2::text),
            finished_at = now()
        WHERE id = $1
        "#,
        run_id,
        error,
    )
    .execute(pool)
    .await
    .map(|_| ())
}
