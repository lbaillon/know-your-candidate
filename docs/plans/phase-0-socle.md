# Phase 0 — Socle technique

**Statut : 📝 à relire** · Dépend de : rien · Bloque : toutes les autres phases

## Objectif

Avoir un squelette qui tourne de bout en bout — une page FastAPI, un worker Rust, une base PostgreSQL, un
job qui transite de l'un à l'autre — et une CI qui refuse le code non conforme. Aucune donnée réelle à ce
stade : on valide la plomberie.

## Périmètre

**Dedans** : configuration des deux projets, connexion à PostgreSQL, première migration, contrat gRPC
minimal, mécanisme de jobs `LISTEN`/`NOTIFY`, outillage local, CI.

**Dehors** : toute donnée réelle, toute page métier, tout déploiement (phase 5), toute authentification
(phase 3).

## Livrables

1. `backend/` : projet `uv`, FastAPI, Jinja2, HTMX, Ruff + ty configurés, une page `/` et un `/healthz`.
2. `worker/` : crate Rust, tokio, sqlx, tonic, connexion base, boucle `LISTEN kyc_jobs` + poll de secours.
3. `proto/kyc/v1/worker.proto` : `Ping`, `EnqueueJob`, `GetJobStatus`. Génération de code des deux côtés.
4. `db/migrations/0001_init.sql` : extensions (`btree_gist`, `pg_trgm`), table `job`, table
   `ingestion_run`, fonction + trigger émettant `NOTIFY kyc_jobs`.
5. `docker-compose.yml` : PostgreSQL local uniquement (l'application tourne en natif, itération plus
   rapide).
6. `Makefile` (ou `justfile`) : `make dev`, `make lint`, `make test`, `make migrate`.
7. `.github/workflows/ci.yml` : lint + typage + tests Python, `clippy` + tests Rust, migrations rejouées
   sur une base vierge.
8. `.env.example` documenté.

## Étapes

1. **Base et migration.** Docker compose PostgreSQL, migration `0001`, choix de l'outil de migration
   (voir décisions). Vérifier que `NOTIFY` part bien à l'insertion d'un job.
2. **Worker minimal.** Connexion sqlx, boucle : `LISTEN` → réveil → `SELECT … FOR UPDATE SKIP LOCKED` →
   exécution d'un job factice `noop` → passage à `done`. Poll de secours toutes les 30 s. Arrêt propre sur
   SIGTERM (un job en cours n'est pas perdu, il est relâché).
3. **Contrat gRPC.** `.proto`, génération Rust (tonic-build) et Python, serveur côté worker, client côté
   backend, `Ping` fonctionnel.
4. **Backend minimal.** FastAPI, layout Jinja de base, HTMX chargé localement (pas de CDN), page d'accueil
   provisoire, `/healthz` qui vérifie la base et le worker.
5. **Outillage et CI.** Ruff, ty, pytest, clippy, workflow GitHub Actions avec un service PostgreSQL.
6. **Documentation.** Remplir les sections « Démarrage rapide » du README et « Commandes » de CLAUDE.md
   avec ce qui existe réellement.

## Décisions à trancher

| # | Question | Options | Proposition |
| --- | --- | --- | --- |
| D0.1 | Outil de migration | SQL brut + petit runner maison · Alembic · `sqlx migrate` | **`sqlx migrate`** : le worker Rust est déjà le propriétaire du schéma, les migrations sont du SQL pur, et ça évite d'introduire un ORM côté Python |
| D0.2 | Accès base côté Python | SQLAlchemy Core · asyncpg + SQL écrit à la main | **asyncpg + SQL** : les requêtes sont peu nombreuses, en lecture, et on veut voir le SQL |
| D0.3 | gRPC dès la phase 0 ? | Oui · Reporter et se contenter de la base | **Oui, minimal** : le poser tôt évite une migration douloureuse plus tard, mais on se limite à trois méthodes |
| D0.4 | Un seul processus ou deux ? | Worker = gRPC + jobs dans un binaire · deux binaires | **Un binaire, deux tâches tokio** : moins d'hébergement à payer, et l'état est partagé |
| D0.5 | Nom du paquet Python | `kyc_api` · `know_your_candidate` | `kyc_api` (`kyc` est ambigu avec *Know Your Customer* — à confirmer) |
| D0.6 | Version de Python | 3.13 · 3.14 | Celle qui est supportée par la cible d'hébergement, à vérifier en phase 5 |

## Fini quand

- `make dev` lance base + worker + backend en une commande.
- Un `POST` sur une route de test crée un job, le worker le traite en moins d'une seconde sans polling.
- Le worker est arrêté → le job reste en attente ; il redémarre → le job est traité.
- Deux workers lancés simultanément ne traitent jamais le même job.
- La CI est verte sur une PR, et rouge si on introduit une erreur de type ou de lint.

## Risques

- **`ty` est un outil jeune** (préversion) : faux positifs et évolutions de comportement possibles. Le
  choix est assumé, il n'y a pas de repli vers mypy. Si une règle bloque, on la neutralise localement avec
  un commentaire justifiant, et on remonte le cas en amont — un projet ouvert peut se permettre de
  contribuer à son outillage.
- **Génération gRPC en Python** : la chaîne `grpcio-tools` s'intègre mal avec `uv` et les chemins
  d'import. Prévoir un script de génération explicite plutôt qu'une magie de build.
- **Perte de `NOTIFY`** en cas de coupure de connexion : c'est attendu, le poll de secours est la
  protection. À tester réellement, pas seulement à documenter.
