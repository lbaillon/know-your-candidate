use sqlx::PgPool;

use super::JobContext;

/// Journalise un avertissement quand une écriture censée porter sur le job détenu par
/// `worker_id` n'a affecté aucune ligne : ça signifie que le job a été repris par un autre
/// worker entre-temps (reprise de zombie). Ce n'est pas fatal, mais ça ne doit jamais passer
/// inaperçu (voir F2, docs/plans/phase-0.1-fix.md).
fn warn_if_not_owned(job_id: i64, worker_id: &str, operation: &str, rows_affected: u64) {
    if rows_affected == 0 {
        tracing::warn!(
            job_id,
            worker_id,
            operation,
            "job repris par un autre worker, écriture ignorée"
        );
    }
}

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

/// Garde de propriété : n'affecte que la ligne encore verrouillée par `ctx.worker_id`. La prise
/// de job (`claim_next_job`) est atomique, mais sans cette garde une écriture de restitution ne
/// l'est pas — c'est le compagnon classique de `SKIP LOCKED` (voir F2,
/// docs/plans/phase-0.1-fix.md).
pub async fn set_progress(ctx: &JobContext, progress: f32, message: &str) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE job
        SET progress = $3, progress_message = $4
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
        ctx.job_id,
        ctx.worker_id,
        progress,
        message,
    )
    .execute(&ctx.pool)
    .await?;
    let rows_affected = result.rows_affected();
    warn_if_not_owned(ctx.job_id, &ctx.worker_id, "set_progress", rows_affected);
    Ok(rows_affected)
}

/// Garde de propriété — voir `set_progress`.
pub async fn complete_job(pool: &PgPool, job_id: i64, worker_id: &str) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE job
        SET status      = 'done',
            progress    = 1.0,
            finished_at = now(),
            locked_at   = NULL,
            locked_by   = NULL
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
        job_id,
        worker_id,
    )
    .execute(pool)
    .await?;
    let rows_affected = result.rows_affected();
    warn_if_not_owned(job_id, worker_id, "complete_job", rows_affected);
    Ok(rows_affected)
}

/// Au-delà de `max_attempts`, le job passe en `failed` et n'est pas repris — voir
/// docs/plans/phase-0-socle.md. Garde de propriété — voir `set_progress`.
///
/// `attempts` a déjà été incrémenté à la prise (`claim_next_job`) : on applique donc un report
/// exponentiel plafonné (5 s, 15 s, 45 s… plafonné à 5 min) avant de rendre le job repris,
/// sinon un job qui échoue à cause d'une panne courte épuise ses tentatives en une milliseconde,
/// avant même que la panne ne soit terminée (voir F4, docs/plans/phase-0.1-fix.md). La requête de
/// prise filtre déjà sur `scheduled_at <= now()`, le report est donc respecté sans autre
/// changement.
pub async fn fail_job(
    pool: &PgPool,
    job_id: i64,
    worker_id: &str,
    error: &str,
) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE job
        SET status       = (CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END)::job_status,
            error        = $3,
            scheduled_at = now() + make_interval(
                               secs => least(300, (5 * power(3, greatest(attempts - 1, 0)))::int)
                           ),
            locked_at    = NULL,
            locked_by    = NULL
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
        job_id,
        worker_id,
        error,
    )
    .execute(pool)
    .await?;
    let rows_affected = result.rows_affected();
    warn_if_not_owned(job_id, worker_id, "fail_job", rows_affected);
    Ok(rows_affected)
}

/// Relâche un job en cours sans le faire échouer : utilisé à l'arrêt propre. `attempts` n'est
/// volontairement pas décrémenté (voir docs/plans/phase-0-socle.md, section « Arrêt propre »).
/// Garde de propriété — voir `set_progress`.
pub async fn release_job(pool: &PgPool, job_id: i64, worker_id: &str) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        r#"
        UPDATE job
        SET status    = 'pending',
            locked_at = NULL,
            locked_by = NULL
        WHERE id = $1 AND locked_by = $2 AND status = 'running'
        "#,
        job_id,
        worker_id,
    )
    .execute(pool)
    .await?;
    let rows_affected = result.rows_affected();
    warn_if_not_owned(job_id, worker_id, "release_job", rows_affected);
    Ok(rows_affected)
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
