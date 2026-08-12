use anyhow::Context;

pub struct Config {
    pub database_url: String,
    pub worker_id: String,
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL manquant")?,
            worker_id: std::env::var("WORKER_ID").unwrap_or_else(|_| default_worker_id()),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        })
    }
}

/// Utilisé quand WORKER_ID n'est pas fourni par l'environnement (typiquement en local).
fn default_worker_id() -> String {
    format!("worker-pid{}", std::process::id())
}
