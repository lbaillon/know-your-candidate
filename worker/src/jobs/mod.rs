pub mod assign_slugs;
pub mod enrich_wikidata;
pub mod ingest_acteurs;
pub mod ingest_scrutins;
pub mod noop;
pub mod queue;
pub mod refresh_views;
pub mod seed_candidates;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgListener;
use tokio::sync::watch;
use tokio::time::MissedTickBehavior;

use crate::shutdown;

/// Job actuellement traité par ce worker, si aucun n'est en cours. Partagé avec la tâche de
/// battement de cœur pour renseigner `worker_heartbeat.current_job_id`.
pub type CurrentJob = Arc<Mutex<Option<i64>>>;

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const ZOMBIE_RECLAIM_INTERVAL: Duration = Duration::from_secs(60);
/// Délai de grâce laissé à un job en cours avant qu'il ne soit relâché à l'arrêt. Exposé pour que
/// `main` puisse aligner le délai qu'il laisse à cette tâche pour se terminer (voir F1,
/// docs/plans/phase-0.1-fix.md).
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(30);
const NOTIFY_CHANNEL: &str = "kyc_jobs";

/// Contexte passé à l'implémentation d'un job : ce dont un job a besoin pour s'exécuter et
/// rapporter sa progression, sans multiplier les paramètres au fil des jobs à venir en phase 1
/// (voir F2, docs/plans/phase-0.1-fix.md). `worker_id` sert de garde de propriété : toute
/// écriture portée par ce contexte n'affecte que la ligne encore détenue par ce worker.
pub struct JobContext {
    pub pool: PgPool,
    pub worker_id: String,
    pub job_id: i64,
}

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
) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move { run(&pool, &worker_id, &current_job, &mut shutdown).await })
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
                // F10 : pas de `continue` ici — on retombe sur drain_queue comme les autres
                // branches, sinon un job zombie repris attend jusqu'à 30 s de plus (le poll
                // suivant), puisqu'un simple UPDATE n'émet pas de NOTIFY.
            }
            requested = shutdown::shutdown_requested(shutdown) => {
                // Un récepteur ne doit jamais dépendre du bon comportement de l'émetteur : `Err`
                // (tous les émetteurs disparus) est traité comme un arrêt par le helper. Sortir
                // de la boucle plutôt que `continue`, sans quoi un `Err` immédiat et permanent
                // tournerait en boucle serrée (F3, docs/plans/phase-0.1-fix.md).
                if requested {
                    break;
                }
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

        // F3 : l'issue est ignorée volontairement ici — un job qui échoue ne doit jamais tuer le
        // worker normal, contrairement à `drain_once` qui, lui, la fait remonter.
        let _ = run_one_job(pool, worker_id, claimed, current_job, shutdown).await;
    }
}

/// Issue d'un job déjà pris, rendue par `run_one_job` — voir F3, docs/plans/phase-1.1-fix.md :
/// `drain_queue` (boucle normale) l'ignore et continue, `drain_once` l'accumule pour faire échouer
/// la commande en sortie si un job n'a pas terminé en `done`.
enum JobOutcome {
    Done,
    Failed { error: String },
    Released,
}

struct JobResult {
    job_id: i64,
    job_type: String,
    outcome: JobOutcome,
}

/// Exécute un job déjà pris. Sur arrêt demandé, laisse une grâce de 30 s avant de relâcher le job
/// (voir docs/plans/phase-0-socle.md, section « Arrêt propre »).
async fn run_one_job(
    pool: &PgPool,
    worker_id: &str,
    claimed: queue::ClaimedJob,
    current_job: &CurrentJob,
    shutdown: &mut watch::Receiver<bool>,
) -> JobResult {
    let job_id = claimed.id;
    let job_type = claimed.job_type.clone();
    set_current_job_id(current_job, Some(job_id));

    let ctx = JobContext {
        pool: pool.clone(),
        worker_id: worker_id.to_string(),
        job_id,
    };
    let execution = execute(&ctx, &claimed);
    tokio::pin!(execution);

    let outcome = loop {
        tokio::select! {
            result = &mut execution => break Some(result),
            requested = shutdown::shutdown_requested(shutdown) => {
                if !requested {
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

    let outcome = match outcome {
        Some(Ok(())) => {
            if let Err(err) = queue::complete_job(pool, job_id, worker_id).await {
                tracing::error!(error = %err, job_id, "échec du passage en done");
            }
            JobOutcome::Done
        }
        Some(Err(err)) => {
            tracing::warn!(error = %err, job_id, "échec du job");
            if let Err(err) = queue::fail_job(pool, job_id, worker_id, &err.to_string()).await {
                tracing::error!(error = %err, job_id, "échec de l'écriture de l'erreur");
            }
            JobOutcome::Failed {
                error: err.to_string(),
            }
        }
        None => {
            tracing::info!(job_id, "grâce expirée, job relâché");
            if let Err(err) = queue::release_job(pool, job_id, worker_id).await {
                tracing::error!(error = %err, job_id, "échec du relâchement du job");
            }
            JobOutcome::Released
        }
    };

    JobResult {
        job_id,
        job_type,
        outcome,
    }
}

/// Vide la file une fois puis rend la main, sans écoute `LISTEN` ni battement de cœur — pour
/// `cargo run -- run-once`, qui sert `make ingest` (voir docs/plans/phase-1-ingestion.md,
/// « Déclencher un job sans route publique »). Réutilise `run_one_job` telle quelle : mêmes
/// garanties de garde de propriété et de report d'échec qu'en fonctionnement normal, avec un canal
/// d'arrêt qui ne se déclenche jamais.
///
/// F3 (docs/plans/phase-1.1-fix.md) : vide la file **jusqu'au bout** même si un job échoue, pour
/// ne pas masquer les erreurs suivantes derrière la première, puis rend `Err` en nommant tous les
/// jobs qui n'ont pas terminé en `done` — sans quoi `main` sort en code 0 et `make ingest` se
/// termine « avec succès » alors qu'une législature entière peut manquer.
///
/// Un échec sous `max_attempts` est requis (`fail_job`) plutôt que définitivement `failed`, donc
/// le même `job_id` peut repasser par `claim_next_job` et réussir plus tard dans cette même
/// boucle (observé en pratique : un blip réseau a fait échouer un `ingest_scrutins` que la reprise
/// automatique a réparé quelques secondes plus tard, dans le même `run-once`). Les issues sont donc
/// indexées par `job_id` — la **dernière** connue pour chacun compte, pas la première — sans quoi
/// `drain_once` rendrait encore `Err` pour un job qui a fini par aboutir.
pub async fn drain_once(pool: &PgPool, worker_id: &str) -> anyhow::Result<()> {
    let current_job: CurrentJob = Arc::new(Mutex::new(None));
    let (_never_shuts_down, mut shutdown) = watch::channel(false);
    let mut last_outcome: HashMap<i64, JobResult> = HashMap::new();
    loop {
        let claimed = match queue::claim_next_job(pool, worker_id).await? {
            Some(job) => job,
            None => break,
        };
        let result = run_one_job(pool, worker_id, claimed, &current_job, &mut shutdown).await;
        last_outcome.insert(result.job_id, result);
    }

    let mut failures: Vec<JobResult> = last_outcome
        .into_values()
        .filter(|r| !matches!(r.outcome, JobOutcome::Done))
        .collect();
    if failures.is_empty() {
        return Ok(());
    }
    failures.sort_by_key(|f| f.job_id);

    let detail = failures
        .iter()
        .map(|f| {
            let reason = match &f.outcome {
                JobOutcome::Failed { error } => format!("échoué : {error}"),
                JobOutcome::Released => "relâché (arrêt pendant l'exécution)".to_string(),
                JobOutcome::Done => unreachable!("filtré ci-dessus"),
            };
            format!("{} (id={}) {reason}", f.job_type, f.job_id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "{} job(s) n'ont pas terminé en done : {detail}",
        failures.len()
    );
}

async fn execute(ctx: &JobContext, job: &queue::ClaimedJob) -> anyhow::Result<()> {
    match job.job_type.as_str() {
        "noop" => noop::run(ctx, &job.payload).await,
        "ingest_acteurs" => ingest_acteurs::run(ctx, &job.payload).await,
        "ingest_scrutins" => ingest_scrutins::run(ctx, &job.payload).await,
        "enrich_wikidata" => enrich_wikidata::run(ctx, &job.payload).await,
        "assign_slugs" => assign_slugs::run(ctx, &job.payload).await,
        "seed_candidates" => seed_candidates::run(ctx, &job.payload).await,
        "refresh_views" => refresh_views::run(ctx, &job.payload).await,
        other => anyhow::bail!("type de job inconnu : {other}"),
    }
}
