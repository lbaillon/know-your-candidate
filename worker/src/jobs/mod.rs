pub mod noop;
pub mod queue;

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

/// Job actuellement traité par ce worker, si aucun n'est en cours. Partagé avec la tâche de
/// battement de cœur pour renseigner `worker_heartbeat.current_job_id`.
pub type CurrentJob = Arc<Mutex<Option<i64>>>;

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const ZOMBIE_RECLAIM_INTERVAL: Duration = Duration::from_secs(60);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const NOTIFY_CHANNEL: &str = "kyc_jobs";

pub fn current_job_id(current_job: &CurrentJob) -> Option<i64> {
    *current_job.lock().unwrap_or_else(PoisonError::into_inner)
}

fn set_current_job_id(current_job: &CurrentJob, value: Option<i64>) {
    *current_job.lock().unwrap_or_else(PoisonError::into_inner) = value;
}

pub fn spawn(
    pool: PgPool,
    worker_id: String,
    current_job: CurrentJob,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(err) = run(&pool, &worker_id, &current_job, &mut shutdown).await {
            tracing::error!(error = %err, "boucle de jobs interrompue par une erreur");
        }
    })
}

async fn run(
    pool: &PgPool,
    worker_id: &str,
    current_job: &CurrentJob,
    shutdown: &mut watch::Receiver<bool>,
) -> anyhow::Result<()> {
    queue::reclaim_zombie_jobs(pool).await?;

    let mut listener = PgListener::connect_with(pool).await?;
    listener.listen(NOTIFY_CHANNEL).await?;

    let mut poll_ticker = tokio::time::interval(POLL_INTERVAL);
    poll_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut zombie_ticker = tokio::time::interval(ZOMBIE_RECLAIM_INTERVAL);
    zombie_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // Vider la file une première fois au démarrage, sans attendre une notification ou un poll.
    drain_queue(pool, worker_id, current_job, shutdown).await;

    loop {
        if *shutdown.borrow() {
            break;
        }

        tokio::select! {
            notification = listener.recv() => {
                if let Err(err) = notification {
                    tracing::warn!(error = %err, "connexion LISTEN perdue, tentative de réabonnement");
                    if let Err(err) = listener.listen(NOTIFY_CHANNEL).await {
                        tracing::error!(error = %err, "échec du réabonnement à kyc_jobs");
                    }
                    continue;
                }
            }
            _ = poll_ticker.tick() => {}
            _ = zombie_ticker.tick() => {
                match queue::reclaim_zombie_jobs(pool).await {
                    Ok(count) if count > 0 => tracing::info!(count, "jobs zombies repris"),
                    Ok(_) => {}
                    Err(err) => tracing::warn!(error = %err, "échec de la reprise des jobs zombies"),
                }
                continue;
            }
            _ = shutdown.changed() => {
                continue;
            }
        }

        if *shutdown.borrow() {
            break;
        }

        drain_queue(pool, worker_id, current_job, shutdown).await;
    }

    Ok(())
}

/// Prend et exécute des jobs tant qu'il y en a en attente, en s'arrêtant dès que l'arrêt est
/// demandé (sans abandonner un job déjà pris en cours de route : voir `run_one_job`).
async fn drain_queue(
    pool: &PgPool,
    worker_id: &str,
    current_job: &CurrentJob,
    shutdown: &mut watch::Receiver<bool>,
) {
    loop {
        if *shutdown.borrow() {
            break;
        }

        let claimed = match queue::claim_next_job(pool, worker_id).await {
            Ok(Some(job)) => job,
            Ok(None) => break,
            Err(err) => {
                tracing::error!(error = %err, "échec de la prise de job");
                break;
            }
        };

        run_one_job(pool, claimed, current_job, shutdown).await;
    }
}

/// Exécute un job déjà pris. Sur arrêt demandé, laisse une grâce de 30 s avant de relâcher le job
/// (voir docs/plans/phase-0-socle.md, section « Arrêt propre »).
async fn run_one_job(
    pool: &PgPool,
    claimed: queue::ClaimedJob,
    current_job: &CurrentJob,
    shutdown: &mut watch::Receiver<bool>,
) {
    let job_id = claimed.id;
    set_current_job_id(current_job, Some(job_id));

    let execution = execute(pool, &claimed);
    tokio::pin!(execution);

    let outcome = loop {
        tokio::select! {
            result = &mut execution => break Some(result),
            _ = shutdown.changed() => {
                if !*shutdown.borrow() {
                    continue;
                }
                tracing::info!(job_id, "arrêt demandé pendant un job en cours, grâce de 30 s");
                let grace = tokio::time::sleep(SHUTDOWN_GRACE);
                tokio::pin!(grace);
                tokio::select! {
                    result = &mut execution => break Some(result),
                    _ = &mut grace => break None,
                }
            }
        }
    };

    set_current_job_id(current_job, None);

    match outcome {
        Some(Ok(())) => {
            if let Err(err) = queue::complete_job(pool, job_id).await {
                tracing::error!(error = %err, job_id, "échec du passage en done");
            }
        }
        Some(Err(err)) => {
            tracing::warn!(error = %err, job_id, "échec du job");
            if let Err(err) = queue::fail_job(pool, job_id, &err.to_string()).await {
                tracing::error!(error = %err, job_id, "échec de l'écriture de l'erreur");
            }
        }
        None => {
            tracing::info!(job_id, "grâce expirée, job relâché");
            if let Err(err) = queue::release_job(pool, job_id).await {
                tracing::error!(error = %err, job_id, "échec du relâchement du job");
            }
        }
    }
}

async fn execute(pool: &PgPool, job: &queue::ClaimedJob) -> anyhow::Result<()> {
    match job.job_type.as_str() {
        "noop" => noop::run(pool, job.id, &job.payload).await,
        other => anyhow::bail!("type de job inconnu : {other}"),
    }
}
