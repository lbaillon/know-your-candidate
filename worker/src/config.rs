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
            worker_id: worker_id_from_env(),
            log_level: std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
        })
    }
}

/// `std::env::var` renvoie `Ok("")` pour une variable définie mais vide (ce qu'un `.env.example`
/// avec `WORKER_ID=` laisse en local) : il faut donc traiter une valeur vide ou blanche comme
/// absente, sans quoi le repli ci-dessous ne se déclenche jamais et deux workers lancés en local
/// partagent la même identité (voir F8, docs/plans/phase-0.1-fix.md).
fn worker_id_from_env() -> String {
    match std::env::var("WORKER_ID") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => default_worker_id(),
    }
}

/// Utilisé quand WORKER_ID n'est pas fourni par l'environnement (typiquement en local). Le pid
/// suffit à garantir l'unicité par processus sur un même hôte ; pas de dépendance ajoutée pour un
/// nom d'hôte, ce serait disproportionné pour ce seul repli local (voir F8,
/// docs/plans/phase-0.1-fix.md).
fn default_worker_id() -> String {
    format!("worker-pid{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::worker_id_from_env;

    #[test]
    fn falls_back_to_default_when_env_var_is_blank() {
        // SAFETY: modifie une variable d'environnement de processus dans un test mono-thread —
        // aucune autre tâche ne lit WORKER_ID en parallèle ici.
        unsafe {
            std::env::set_var("WORKER_ID", "   ");
        }
        let worker_id = worker_id_from_env();
        unsafe {
            std::env::remove_var("WORKER_ID");
        }

        assert!(
            worker_id.starts_with("worker-"),
            "une valeur blanche doit retomber sur le repli, pas être utilisée telle quelle"
        );
    }
}
