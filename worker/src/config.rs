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
    resolve_worker_id(std::env::var("WORKER_ID").ok().as_deref())
}

/// Séparée de la lecture de l'environnement pour être testable sans muter les variables du
/// processus : `std::env::set_var` est `unsafe` en édition 2024 précisément parce que cargo
/// exécute les tests d'un même crate en parallèle dans un seul processus.
fn resolve_worker_id(raw: Option<&str>) -> String {
    match raw {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => default_worker_id(),
    }
}

/// Utilisé quand WORKER_ID n'est pas fourni par l'environnement.
///
/// Le pid seul ne suffit pas : dans un conteneur il vaut presque toujours 1, donc deux instances
/// déployées partageraient une identité et donc une ligne de `worker_heartbeat` — celle qui vit
/// maintiendrait le battement de cœur de celle qui est morte, et les jobs de cette dernière ne
/// seraient jamais repris. C'est exactement la panne que F8 corrigeait, décalée en production.
/// On y ajoute donc le nom d'hôte quand il est disponible et un suffixe temporel, ce qui rend une
/// collision improbable sans ajouter de dépendance.
fn default_worker_id() -> String {
    let host = std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    format!("worker-{}-pid{}-{nanos}", host.trim(), std::process::id())
}

#[cfg(test)]
mod tests {
    use super::{default_worker_id, resolve_worker_id};

    #[test]
    fn falls_back_to_default_when_env_var_is_blank() {
        assert!(resolve_worker_id(Some("   ")).starts_with("worker-"));
        assert!(resolve_worker_id(Some("")).starts_with("worker-"));
        assert!(resolve_worker_id(None).starts_with("worker-"));
    }

    #[test]
    fn keeps_an_explicit_worker_id() {
        assert_eq!(
            resolve_worker_id(Some(" render-worker-1 ")),
            "render-worker-1"
        );
    }

    #[test]
    fn default_ids_differ_between_calls() {
        // Deux conteneurs ont le même nom d'hôte potentiel et le même pid 1 : c'est le suffixe
        // temporel qui porte l'unicité.
        assert_ne!(default_worker_id(), default_worker_id());
    }
}
