use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::jobs::{self, CurrentJob};
use crate::shutdown;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

pub fn spawn(
    pool: PgPool,
    worker_id: String,
    version: String,
    current_job: CurrentJob,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if let Err(err) = beat(&pool, &worker_id, &version, &current_job).await {
                        tracing::warn!(error = %err, "échec du battement de cœur");
                    }
                }
                requested = shutdown::shutdown_requested(&mut shutdown) => {
                    if requested {
                        break;
                    }
                }
            }
        }

        Ok(())
    })
}

async fn beat(
    pool: &PgPool,
    worker_id: &str,
    version: &str,
    current_job: &CurrentJob,
) -> sqlx::Result<()> {
    let current_job_id = jobs::current_job_id(current_job);

    sqlx::query!(
        r#"
        INSERT INTO worker_heartbeat (worker_id, version, last_seen_at, current_job_id)
        VALUES ($1, $2, now(), $3)
        ON CONFLICT (worker_id) DO UPDATE
        SET version         = EXCLUDED.version,
            last_seen_at    = now(),
            current_job_id  = EXCLUDED.current_job_id
        "#,
        worker_id,
        version,
        current_job_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}
