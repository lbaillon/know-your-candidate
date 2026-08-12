# worker/

Worker de données en Rust : récupération de l'open data, parsing, normalisation, calcul des scores,
écriture dans PostgreSQL.

Deux tâches tokio :

- **boucle de jobs** — `LISTEN kyc_jobs`, réveil sur `NOTIFY`, prise de job avec `FOR UPDATE SKIP LOCKED`,
  exécution, écriture idempotente, poll de secours toutes les 30 s ;
- **battement de cœur** — mise à jour de `worker_heartbeat` toutes les 5 s, c'est ainsi que le backend
  sait qu'un worker tourne.

Aucun serveur réseau : le worker n'expose rien et ne parle qu'à PostgreSQL.

Contenu à venir (phase 0) : `Cargo.toml`, `src/main.rs`, modules `jobs/`, `sources/`, `scoring/`.
