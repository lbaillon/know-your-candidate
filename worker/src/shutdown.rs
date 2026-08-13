use std::sync::Arc;

use tokio::signal;
use tokio::sync::watch;

/// Écoute Ctrl+C (et SIGTERM sous Unix) et bascule `tx` à `true` une fois le signal reçu.
/// Les autres tâches (jobs, heartbeat) observent ce canal pour s'arrêter proprement.
///
/// Ne pas pouvoir installer les handlers de signal est une condition fatale, pas une simple
/// erreur à journaliser : sans handlers, le processus ne s'arrêterait jamais proprement. On
/// déclenche donc l'arrêt malgré tout, plutôt que de laisser tourner un processus sans aucun
/// moyen de le terminer proprement (voir F3 dans docs/plans/phase-0.1-fix.md).
pub fn spawn(tx: Arc<watch::Sender<bool>>) -> tokio::task::JoinHandle<anyhow::Result<()>> {
    tokio::spawn(async move {
        if let Err(err) = wait_for_shutdown_signal().await {
            tracing::error!(error = %err, "échec de l'installation des handlers de signal, arrêt forcé");
            let _ = tx.send(true);
            return Err(err);
        }
        tracing::info!("signal d'arrêt reçu");
        let _ = tx.send(true);
        Ok(())
    })
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

/// Attend le prochain changement sur `rx` et indique si l'arrêt est demandé. Un récepteur ne doit
/// jamais dépendre du bon comportement de l'émetteur : si tous les émetteurs ont disparu
/// (`changed()` renvoie `Err`), plus rien ne viendra jamais, ce qui équivaut à un arrêt. Les
/// appelants doivent sortir de leur boucle sur `true`, jamais faire `continue` en ignorant le
/// résultat — c'est ce qui causait la boucle à 100 % de CPU corrigée par F3
/// (docs/plans/phase-0.1-fix.md).
pub async fn shutdown_requested(rx: &mut watch::Receiver<bool>) -> bool {
    match rx.changed().await {
        Ok(()) => *rx.borrow(),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_true_once_sender_sends_true() {
        let (tx, mut rx) = watch::channel(false);
        tokio::spawn(async move {
            let _ = tx.send(true);
        });

        assert!(shutdown_requested(&mut rx).await);
    }

    #[tokio::test]
    async fn returns_true_without_looping_when_sender_is_dropped() {
        let (tx, mut rx) = watch::channel(false);
        drop(tx);

        assert!(shutdown_requested(&mut rx).await);
    }
}
