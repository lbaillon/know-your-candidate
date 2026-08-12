# worker/

Worker de données en Rust : récupération de l'open data, parsing, normalisation, calcul des scores,
écriture dans PostgreSQL. Expose un service gRPC de commande/statut consommé par le backend.

Boucle principale : `LISTEN kyc_jobs` sur PostgreSQL, réveil sur `NOTIFY`, prise de job avec
`FOR UPDATE SKIP LOCKED`, exécution, écriture idempotente.

Contenu à venir (phase 0) : `Cargo.toml`, `src/main.rs`, modules `jobs/`, `sources/`, `scoring/`.
