use std::sync::{Arc, Mutex};
use std::time::Duration;

use kyc_worker::{config, db, heartbeat, jobs, shutdown};
use tokio::sync::watch;
use tracing_subscriber::EnvFilter;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = config::Config::from_env()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(&config.log_level).unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("enqueue") {
        let job_type = args
            .next()
            .ok_or_else(|| anyhow::anyhow!("usage: kyc-worker enqueue <type> [payload_json]"))?;
        let payload: serde_json::Value = match args.next() {
            Some(raw) => serde_json::from_str(&raw)?,
            None => serde_json::json!({}),
        };
        let pool = db::connect(&config.database_url).await?;
        sqlx::migrate!("../db/migrations").run(&pool).await?;
        let job_id: i64 = sqlx::query_scalar!(
            "INSERT INTO job (type, payload, created_by) VALUES ($1, $2, 'cli') RETURNING id",
            job_type,
            payload,
        )
        .fetch_one(&pool)
        .await?;
        tracing::info!(job_id, job_type = %job_type, "job créé");
        return Ok(());
    }

    tracing::info!(worker_id = %config.worker_id, version = VERSION, "démarrage du worker");

    let pool = db::connect(&config.database_url).await?;

    // Protégé en interne par un verrou consultatif Postgres : deux instances qui démarrent en
    // même temps ne migrent pas simultanément.
    sqlx::migrate!("../db/migrations").run(&pool).await?;

    // `watch::Sender` n'est pas `Clone`, mais ses méthodes prennent `&self` : `Arc` permet de le
    // partager entre `shutdown::spawn` et `main`, qui garde son propre exemplaire pour déclencher
    // l'arrêt si l'une des deux tâches supervisées meurt (voir F1, docs/plans/phase-0.1-fix.md).
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_tx = Arc::new(shutdown_tx);
    let shutdown_handle = shutdown::spawn(shutdown_tx.clone());

    let current_job: jobs::CurrentJob = Arc::new(Mutex::new(None));

    let mut heartbeat_handle = heartbeat::spawn(
        pool.clone(),
        config.worker_id.clone(),
        VERSION.to_string(),
        current_job.clone(),
        shutdown_rx.clone(),
    );
    let mut jobs_handle = jobs::spawn(
        pool.clone(),
        config.worker_id.clone(),
        current_job,
        shutdown_rx,
    );

    // Supervision : si l'une des deux tâches de fond se termine — pour quelque raison que ce
    // soit —, on ne doit pas rester à moitié vivant en attendant l'autre indéfiniment. On
    // journalise, on demande l'arrêt de l'autre tâche, on lui laisse une grâce, puis on sort en
    // erreur si la cause était une erreur : un processus à moitié vivant qui continue de
    // signaler qu'il va bien (heartbeat qui tourne, jobs qui ne tournent plus) est pire qu'un
    // processus mort (voir F1, docs/plans/phase-0.1-fix.md).
    enum FinishedTask {
        Heartbeat,
        Jobs,
    }
    let (finished_task, result) = tokio::select! {
        result = &mut heartbeat_handle => (FinishedTask::Heartbeat, result),
        result = &mut jobs_handle => (FinishedTask::Jobs, result),
    };
    let finished_name = match finished_task {
        FinishedTask::Heartbeat => "heartbeat",
        FinishedTask::Jobs => "jobs",
    };
    let mut task_error = match result {
        Ok(Ok(())) => {
            tracing::info!(task = finished_name, "tâche terminée normalement");
            None
        }
        Ok(Err(err)) => {
            tracing::error!(task = finished_name, error = %err, "tâche de fond interrompue par une erreur");
            Some(err)
        }
        Err(join_err) => {
            tracing::error!(task = finished_name, error = %join_err, "tâche de fond a paniqué");
            Some(anyhow::anyhow!("{join_err}"))
        }
    };

    let _ = shutdown_tx.send(true);

    // On ne repolle jamais le JoinHandle déjà résolu ci-dessus (le contrat de Future l'interdit) :
    // seule l'autre tâche est attendue ici.
    let grace = jobs::SHUTDOWN_GRACE + Duration::from_secs(5);
    let other_name = match finished_task {
        FinishedTask::Heartbeat => "jobs",
        FinishedTask::Jobs => "heartbeat",
    };
    let other_outcome = tokio::time::timeout(grace, async {
        match finished_task {
            FinishedTask::Heartbeat => jobs_handle.await,
            FinishedTask::Jobs => heartbeat_handle.await,
        }
    })
    .await;
    match other_outcome {
        Ok(Ok(Ok(()))) => tracing::info!(task = other_name, "tâche terminée normalement"),
        Ok(Ok(Err(err))) => {
            tracing::error!(task = other_name, error = %err, "tâche de fond interrompue par une erreur");
            task_error.get_or_insert(err);
        }
        Ok(Err(join_err)) => {
            tracing::error!(task = other_name, error = %join_err, "tâche de fond a paniqué");
            task_error.get_or_insert_with(|| anyhow::anyhow!("{join_err}"));
        }
        Err(_) => {
            tracing::error!(
                task = other_name,
                "n'a pas répondu à l'arrêt dans le délai imparti, sortie forcée"
            );
            task_error
                .get_or_insert_with(|| anyhow::anyhow!("{other_name}: arrêt forcé après délai"));
        }
    }

    // La tâche de signal ne se termine d'elle-même que si un signal est arrivé. Quand on sort
    // parce qu'une tâche de fond est morte — le scénario même de F1 —, elle est toujours parquée
    // sur ctrl_c()/SIGTERM et l'attendre bloquerait le processus indéfiniment : on détecterait la
    // panne, on journaliserait, et on ne sortirait jamais. On ne récupère donc son résultat que si
    // elle a déjà fini, et on l'abandonne sinon.
    if shutdown_handle.is_finished() {
        match shutdown_handle.await {
            Ok(Ok(())) => {}
            // Échec d'installation des handlers : déclaré fatal par le plan. shutdown::spawn a
            // déjà demandé l'arrêt, mais le processus doit sortir en erreur.
            Ok(Err(err)) => {
                tracing::error!(task = "shutdown", error = %err, "tâche de signal en erreur");
                task_error.get_or_insert(err);
            }
            Err(join_err) => {
                tracing::error!(task = "shutdown", error = %join_err, "tâche de signal a paniqué");
                task_error.get_or_insert_with(|| anyhow::anyhow!("{join_err}"));
            }
        }
    } else {
        shutdown_handle.abort();
    }

    if let Some(err) = task_error {
        return Err(err);
    }

    tracing::info!("arrêt propre terminé");

    Ok(())
}
