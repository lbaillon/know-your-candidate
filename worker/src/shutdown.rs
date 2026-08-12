use tokio::signal;
use tokio::sync::watch;

/// Écoute Ctrl+C (et SIGTERM sous Unix) et bascule `tx` à `true` une fois le signal reçu.
/// Les autres tâches (jobs, heartbeat) observent ce canal pour s'arrêter proprement.
pub fn spawn(tx: watch::Sender<bool>) {
    tokio::spawn(async move {
        if let Err(err) = wait_for_shutdown_signal().await {
            tracing::error!(error = %err, "échec de l'installation des handlers de signal");
            return;
        }
        tracing::info!("signal d'arrêt reçu");
        let _ = tx.send(true);
    });
}

async fn wait_for_shutdown_signal() -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())?;
        tokio::select! {
            result = signal::ctrl_c() => result?,
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        signal::ctrl_c().await?;
    }

    Ok(())
}
