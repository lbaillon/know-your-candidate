//! Tests d'intégration de la file de jobs, sur une vraie base (voir docs/plans/phase-0-socle.md,
//! section « Stratégie de test »). `#[sqlx::test]` crée une base fraîche par test et joue les
//! migrations : pas besoin d'isolation manuelle ici.

// Fichier entièrement composé de tests : `unwrap()` y est autorisé (voir CLAUDE.md).
#![allow(clippy::unwrap_used)]

use kyc_worker::jobs::queue;
use sqlx::PgPool;

async fn insert_job(pool: &PgPool, job_type: &str, max_attempts: i16) -> i64 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO job (type, payload, max_attempts)
        VALUES ($1, '{}'::jsonb, $2)
        RETURNING id
        "#,
        job_type,
        max_attempts,
    )
    .fetch_one(pool)
    .await
    .expect("l'insertion du job de test ne doit pas échouer")
}

#[sqlx::test(migrations = "../db/migrations")]
async fn claim_next_job_marks_it_running_and_increments_attempts(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 3).await;

    let claimed = queue::claim_next_job(&pool, "worker-a")
        .await?
        .expect("un job en attente doit être pris");

    assert_eq!(claimed.id, job_id);
    assert_eq!(claimed.job_type, "noop");
    assert_eq!(claimed.attempts, 1);

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", locked_by FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "running");
    assert_eq!(row.locked_by.as_deref(), Some("worker-a"));

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn claim_next_job_returns_none_when_queue_is_empty(pool: PgPool) -> sqlx::Result<()> {
    let claimed = queue::claim_next_job(&pool, "worker-a").await?;

    assert!(claimed.is_none());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn concurrent_claims_never_take_the_same_job(pool: PgPool) -> sqlx::Result<()> {
    for _ in 0..20 {
        insert_job(&pool, "noop", 3).await;
    }

    let mut handles = Vec::new();
    for worker_index in 0..4 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let worker_id = format!("worker-{worker_index}");
            let mut claimed_ids = Vec::new();
            while let Some(job) = queue::claim_next_job(&pool, &worker_id).await.unwrap() {
                claimed_ids.push(job.id);
            }
            claimed_ids
        }));
    }

    let mut all_claimed = Vec::new();
    for handle in handles {
        all_claimed.extend(handle.await.expect("la tâche ne doit pas paniquer"));
    }

    let unique_count = {
        let mut sorted = all_claimed.clone();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len()
    };
    assert_eq!(all_claimed.len(), 20, "les 20 jobs doivent tous être pris");
    assert_eq!(unique_count, 20, "aucun job ne doit être pris deux fois");

    let pending_count: i64 =
        sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM job WHERE status = 'pending'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(pending_count, 0, "aucun job ne doit rester en attente");

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn complete_job_marks_it_done(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 3).await;
    queue::claim_next_job(&pool, "worker-a").await?;

    queue::complete_job(&pool, job_id).await?;

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", finished_at FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "done");
    assert!(row.finished_at.is_some());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn fail_job_requeues_when_attempts_below_max(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 3).await;
    queue::claim_next_job(&pool, "worker-a").await?; // attempts = 1, max_attempts = 3

    queue::fail_job(&pool, job_id, "boom").await?;

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", error, locked_by FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "pending");
    assert_eq!(row.error.as_deref(), Some("boom"));
    assert!(row.locked_by.is_none());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn fail_job_gives_up_once_max_attempts_is_reached(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 1).await;
    queue::claim_next_job(&pool, "worker-a").await?; // attempts = 1 = max_attempts

    queue::fail_job(&pool, job_id, "boom").await?;

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!" FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "failed");

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reclaim_zombie_jobs_requeues_jobs_of_dead_workers(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 3).await;
    queue::claim_next_job(&pool, "dead-worker").await?;

    sqlx::query!(
        r#"
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at)
        VALUES ('dead-worker', '0.1.0', now() - interval '5 minutes')
        "#
    )
    .execute(&pool)
    .await?;

    let reclaimed = queue::reclaim_zombie_jobs(&pool).await?;
    assert_eq!(reclaimed, 1);

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!", locked_by, error FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "pending");
    assert!(row.locked_by.is_none());
    assert!(row.error.is_some());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reclaim_zombie_jobs_fails_job_once_attempts_exhausted(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 1).await;
    queue::claim_next_job(&pool, "dead-worker").await?; // attempts = 1 = max_attempts

    sqlx::query!(
        r#"
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at)
        VALUES ('dead-worker', '0.1.0', now() - interval '5 minutes')
        "#
    )
    .execute(&pool)
    .await?;

    queue::reclaim_zombie_jobs(&pool).await?;

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!" FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "failed");

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reclaim_zombie_jobs_leaves_jobs_of_live_workers_alone(pool: PgPool) -> sqlx::Result<()> {
    let job_id = insert_job(&pool, "noop", 3).await;
    queue::claim_next_job(&pool, "live-worker").await?;

    sqlx::query!(
        r#"
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at)
        VALUES ('live-worker', '0.1.0', now())
        "#
    )
    .execute(&pool)
    .await?;

    let reclaimed = queue::reclaim_zombie_jobs(&pool).await?;
    assert_eq!(reclaimed, 0);

    let row = sqlx::query!(
        r#"SELECT status::text AS "status!" FROM job WHERE id = $1"#,
        job_id,
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(row.status, "running");

    Ok(())
}
