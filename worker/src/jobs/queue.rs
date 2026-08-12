use sqlx::PgPool;

/// Job pris par ce worker : la ligne est déjà passée en `running` en base au moment où cette
/// structure existe.
#[derive(Debug)]
pub struct ClaimedJob {
    pub id: i64,
    pub job_type: String,
    pub payload: serde_json::Value,
    pub attempts: i16,
    pub max_attempts: i16,
}

/// Prise de job — voir docs/plans/phase-0-socle.md, section « Prise de job — requête exacte ».
/// Le `FOR UPDATE SKIP LOCKED` est dans la CTE, avec `ORDER BY` et `LIMIT 1` : c'est ce qui
/// garantit que deux workers ne prennent jamais la même ligne. Ne pas réécrire cette requête
/// autrement.
pub async fn claim_next_job(pool: &PgPool, worker_id: &str) -> sqlx::Result<Option<ClaimedJob>> {
    sqlx::query_as!(
        ClaimedJob,
        r#"
        WITH next_job AS (
            SELECT id
            FROM job
            WHERE status = 'pending'
              AND NOT cancel_requested
              AND scheduled_at <= now()
            ORDER BY priority DESC, scheduled_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        UPDATE job j
        SET status     = 'running',
            locked_at  = now(),
            locked_by  = $1,
            attempts   = j.attempts + 1,
            started_at = COALESCE(j.started_at, now())
        FROM next_job
        WHERE j.id = next_job.id
        RETURNING j.id, j.type AS "job_type", j.payload, j.attempts, j.max_attempts
        "#,
        worker_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn set_progress(
    pool: &PgPool,
    job_id: i64,
    progress: f32,
    message: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE job
        SET progress = $2, progress_message = $3
        WHERE id = $1
        "#,
        job_id,
        progress,
        message,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn complete_job(pool: &PgPool, job_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE job
        SET status      = 'done',
            progress    = 1.0,
            finished_at = now(),
            locked_at   = NULL,
            locked_by   = NULL
        WHERE id = $1
        "#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Au-delà de `max_attempts`, le job passe en `failed` et n'est pas repris — voir
/// docs/plans/phase-0-socle.md.
pub async fn fail_job(pool: &PgPool, job_id: i64, error: &str) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE job
        SET status    = (CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END)::job_status,
            error     = $2,
            locked_at = NULL,
            locked_by = NULL
        WHERE id = $1
        "#,
        job_id,
        error,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Relâche un job en cours sans le faire échouer : utilisé à l'arrêt propre. `attempts` n'est
/// volontairement pas décrémenté (voir docs/plans/phase-0-socle.md, section « Arrêt propre »).
pub async fn release_job(pool: &PgPool, job_id: i64) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        UPDATE job
        SET status    = 'pending',
            locked_at = NULL,
            locked_by = NULL
        WHERE id = $1
        "#,
        job_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Reprise des jobs zombies — voir docs/plans/phase-0-socle.md, section dédiée. Le seuil d'une
/// minute est largement supérieur à la période du battement de cœur (5 s).
pub async fn reclaim_zombie_jobs(pool: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE job
        SET status    = (CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END)::job_status,
            locked_at = NULL,
            locked_by = NULL,
            error     = COALESCE(error, 'reclaimed: worker heartbeat expired')
        WHERE status = 'running'
          AND locked_by IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM worker_heartbeat w
              WHERE w.worker_id = job.locked_by
                AND w.last_seen_at > now() - interval '1 minute'
          )
        "#,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
