# worker/

Worker de données en Rust : récupération de l'open data, parsing, normalisation, calcul des scores,
écriture dans PostgreSQL.

Deux tâches tokio :

- **boucle de jobs** — `LISTEN kyc_jobs`, réveil sur `NOTIFY`, prise de job avec `FOR UPDATE SKIP LOCKED`,
  exécution, écriture idempotente, poll de secours toutes les 30 s ;
- **battement de cœur** — mise à jour de `worker_heartbeat` toutes les 5 s, c'est ainsi que le backend
  sait qu'un worker tourne.

Aucun serveur réseau : le worker n'expose rien et ne parle qu'à PostgreSQL.

## Développement

```
cargo run                                    # lance le worker (DATABASE_URL, WORKER_ID via .env)
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo sqlx prepare                           # après toute modification d'une requête sqlx::query!
SQLX_OFFLINE=true cargo test
```

Nécessite `sqlx-cli` (`cargo install sqlx-cli --no-default-features --features postgres,rustls`) pour
`cargo sqlx prepare` et pour jouer les migrations manuellement (`sqlx migrate run --source
../db/migrations`).

## Structure

```
src/
  lib.rs                point d'entrée de la bibliothèque (le binaire main.rs est un point d'entrée fin,
                         pour que tests/ puisse appeler les modules directement)
  main.rs                démarrage, tâches tokio, arrêt propre
  config.rs
  db.rs
  heartbeat.rs
  shutdown.rs
  jobs/
    mod.rs               boucle de jobs : LISTEN, poll de secours, reprise des zombies, arrêt propre
    queue.rs              prise, achèvement, échec, reprise des jobs zombies
    noop.rs                job factice de la phase 0
tests/
  queue.rs               tests d'intégration sur une vraie base (#[sqlx::test])
```
