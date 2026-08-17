//! Ouverture et clôture d'un `ingestion_run` — voir docs/plans/phase-1-ingestion.md.

use sqlx::PgPool;

pub async fn start(
    pool: &PgPool,
    source: &str,
    job_id: i64,
    params: serde_json::Value,
) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"
        INSERT INTO ingestion_run (source, status, started_at, job_id, params)
        VALUES ($1, 'running', now(), $2, $3)
        RETURNING id
        "#,
        source,
        job_id,
        params,
    )
    .fetch_one(pool)
    .await
}

pub async fn finish_ok(
    pool: &PgPool,
    run_id: i64,
    counters: serde_json::Value,
    url: &str,
    content_hash: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE ingestion_run
        SET status = 'done', counters = $2, finished_at = now(), url = $3, content_hash = $4
        WHERE id = $1
        "#,
        run_id,
        counters,
        url,
        content_hash,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

pub async fn finish_err(pool: &PgPool, run_id: i64, error: &str) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE ingestion_run
        SET status = 'failed', error = $2, finished_at = now()
        WHERE id = $1
        "#,
        run_id,
        error,
    )
    .execute(pool)
    .await
    .map(|_| ())
}
