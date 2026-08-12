# proto/

Contrats gRPC partagés entre le backend Python et le worker Rust. Source unique de vérité : le code
généré n'est jamais édité à la main.

Rappel de périmètre : gRPC sert au **synchrone** (déclencher un job, connaître l'état du worker, obtenir
un aperçu). Le travail asynchrone passe par la file de jobs PostgreSQL, pas par gRPC.

Contenu à venir (phase 0) : `kyc/v1/worker.proto`.
