use std::sync::{Arc, Mutex};

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

    tracing::info!(worker_id = %config.worker_id, version = VERSION, "démarrage du worker");

    let pool = db::connect(&config.database_url).await?;

    // Protégé en interne par un verrou consultatif Postgres : deux instances qui démarrent en
    // même temps ne migrent pas simultanément.
    sqlx::migrate!("../db/migrations").run(&pool).await?;

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    shutdown::spawn(shutdown_tx);

    let current_job: jobs::CurrentJob = Arc::new(Mutex::new(None));

    let heartbeat_handle = heartbeat::spawn(
        pool.clone(),
        config.worker_id.clone(),
        VERSION.to_string(),
        current_job.clone(),
        shutdown_rx.clone(),
    );
    let jobs_handle = jobs::spawn(pool.clone(), config.worker_id.clone(), current_job, shutdown_rx);

    let (heartbeat_result, jobs_result) = tokio::join!(heartbeat_handle, jobs_handle);
    heartbeat_result?;
    jobs_result?;

    tracing::info!("arrêt propre terminé");

    Ok(())
}
