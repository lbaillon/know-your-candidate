# CLAUDE.md

Contexte et conventions pour Claude Code sur ce repo. À lire avant toute modification.

## Le projet en trois phrases

Know Your Candidate produit des fiches factuelles sur les candidat·es à la présidentielle française de
2027, dérivées de leurs votes à l'Assemblée nationale et de ceux de leurs partis successifs. La règle
absolue : **toute affirmation affichée doit être traçable jusqu'à un scrutin identifié**. Un chiffre sans
explication cliquable est un bug, pas une fonctionnalité.

## Manière de travailler sur ce repo

1. **Les plans passent avant le code.** Chaque phase a un fichier dans [docs/plans/](docs/plans/). On
   discute et on affine le plan, l'utilisateur valide, puis on implémente. N'écris pas le code d'une phase
   dont le plan n'a pas été validé (l'en-tête `Statut` du plan fait foi).
2. **Développement sur le tronc.** On commit directement sur `main`, pas de branche ni de PR pour le
   travail des mainteneurs. En contrepartie, chaque commit doit laisser `main` dans un état qui tient :
   lint, typage et tests verts. Un commit = un changement cohérent et racontable — plutôt plusieurs petits
   commits qu'un seul fourre-tout en fin de phase.
3. **Quand une décision change, mets à jour le plan** dans le même commit que le code. Un plan périmé est
   pire que pas de plan.
4. **Ne pousse pas et ne déploie pas** sans demande explicite. Commiter localement est libre, pousser ne
   l'est pas — d'autant qu'à partir de la phase 5, `main` se déploie automatiquement : un `push` sera une
   mise en ligne.
5. Si un plan s'avère faux au contact du code, dis-le et propose une révision — ne contourne pas
   silencieusement.

## Langue

- **Documentation, UI, commentaires expliquant un choix métier : français.**
- **Identifiants de code, noms de fichiers, messages de commit : anglais**, sauf les termes du domaine
  parlementaire qui n'ont pas d'équivalent propre et qui mappent 1:1 sur les données sources. On garde
  donc en français : `scrutin`, `groupe` (groupe parlementaire), `legislature`, `dossier`, `organe`,
  `mandat` (« mandate » est un faux ami en anglais).
- Tout le reste en anglais : `person`, `party`, `vote`, `theme`, `score`, `job`, `ingestion_run`.

## Structure du repo

```
backend/    API FastAPI + templates Jinja/HTMX. Lecture seule sur la BDD (sauf tables admin).
worker/     Worker Rust : fetch open data, parsing, calcul des scores, écritures lourdes.
db/         Migrations SQL versionnées, seeds, fonctions PL/pgSQL.
docs/       Architecture, sources de données, méthodologie.
docs/plans/ Un plan par phase. C'est là que se fait la conception.
```

## Règles d'architecture à ne pas casser

- **Le backend FastAPI ne fait aucun traitement lourd.** Pas d'appel HTTP sortant vers l'open data, pas de
  boucle sur des dizaines de milliers de votes, pas de recalcul de score à la volée. Il lit des tables et
  des vues matérialisées déjà calculées par le worker. Si une page a besoin d'un calcul, ce calcul devient
  une colonne, une vue matérialisée ou un job.
- **Le worker Rust est le seul à écrire les données métier.** Le backend n'écrit que les données saisies
  par un admin (catégorisations manuelles) et les demandes de job.
- **Toute ingestion est idempotente et additive.** Rejouer un fetch ne doit ni dupliquer, ni détruire :
  `ON CONFLICT DO UPDATE` sur des clés naturelles (l'`uid` de l'AN), jamais de `TRUNCATE`.
- **Les données brutes ne sont jamais écrasées par une interprétation.** Le vote source et sa
  catégorisation vivent dans des tables séparées ; les catégorisations sont historisées, pas remplacées.
- **Pas de Redis, pas de RabbitMQ, pas de Celery.** File de jobs = table PostgreSQL + `LISTEN`/`NOTIFY` +
  `FOR UPDATE SKIP LOCKED`.
- **Le backend et le worker ne se parlent jamais directement.** Pas de gRPC, pas de HTTP interne : tout
  passe par PostgreSQL — création de job, progression, annulation, battement de cœur. Si un besoin semble
  exiger un appel direct, c'est presque toujours une colonne qui manque.
- **Pas de framework JS, pas de build front.** HTMX + Jinja, rendu serveur. Si une interaction semble
  exiger du JS, on cherche d'abord la solution HTMX.
- **Toute écriture sur un job porte le garde de propriété** (`locked_by`, `status`), et une écriture qui
  n'affecte aucune ligne est journalisée, jamais ignorée en silence. La prise de job (`SKIP LOCKED`) est
  atomique ; sans cette garde, une restitution (achèvement, échec, relâchement, progression) ne l'est pas,
  et un worker qui a perdu son job après une reprise de zombie peut écraser le travail de celui qui l'a
  repris (leçon de la revue de la phase 0, voir [phase-0.1-fix.md](docs/plans/phase-0.1-fix.md), F2).
- **Toute tâche de fond est supervisée** : sa mort arrête le processus avec un code non nul. Un processus à
  moitié vivant qui continue de signaler qu'il va bien (heartbeat qui tourne, boucle de jobs qui ne tourne
  plus) est pire qu'un processus mort (même revue, F1).
- **Aucun échafaudage de développement ne survit dans le produit final.** Les routes de démonstration, les
  jeux de données factices et les raccourcis de mise au point sont supprimés dès qu'ils ont servi, pas
  neutralisés derrière un drapeau : un drapeau finit toujours par être activé « temporairement ». Quand un
  échafaudage se révèle réellement utile (le suivi de progression, le job `noop` comme test de fumée), il
  est **promu** — déplacé derrière l'authentification admin et assumé comme fonctionnalité — jamais laissé
  en libre accès. Corollaire : aucune route publique ne crée de job.

## Conventions techniques

**Python** — 3.14, géré par `uv` (jamais `pip` ni `poetry` directement). Typage complet et vérifié par
**`ty`** — c'est un choix assumé, ne propose pas mypy en remplacement. Pydantic v2 pour les schémas. Ruff
pour lint + format, configuré dans `backend/pyproject.toml`. Pas de `Any` sans commentaire justifiant.

**Rust** — édition 2024, `tokio` pour l'async, `sqlx` en mode requêtes vérifiées à la compilation.
`cargo clippy -- -D warnings` doit passer. Pas de `unwrap()` hors tests et hors démarrage.
Après toute modification d'une requête `sqlx::query!`/`query_as!`, régénérer les métadonnées avec
**`cargo sqlx prepare -- --all-targets`** : sans `--all-targets`, les requêtes des tests d'intégration
perdent leurs entrées `.sqlx` et la CI casse en mode offline.

**SQL** — migrations numérotées et immuables une fois mergées (`db/migrations/NNNN_description.sql`).
Toute migration doit être rejouable sur une base existante. On exploite volontairement Postgres :
`daterange` + `EXCLUDE` pour les appartenances partisanes, JSONB + GIN pour les payloads bruts, vues
matérialisées pour les agrégats de score, recherche plein texte en configuration `french`.

**Tests** — `pytest` côté backend, tests d'intégration Rust avec une vraie base Postgres jetable. La
logique de scoring est testée sur des cas réels documentés, pas seulement sur des fixtures inventées.

## Sensibilité du sujet

Ce projet parle de personnes réelles et identifiables dans un contexte électoral.

- On n'affiche **que** des faits vérifiables et sourcés. Pas d'inférence sur les intentions, la vie privée,
  la santé, la religion ou l'origine.
- Une absence de vote n'est pas une opinion : un « non-votant » peut être en mission, président de séance,
  ou absent. On ne l'interprète jamais comme une position.
- Le vocabulaire d'affichage reste descriptif (« a voté pour », « aligné sur les votes du groupe X »),
  jamais évaluatif.
- Les enquêtes et condamnations sont hors périmètre v1 (voir
  [docs/plans/phase-6-backlog-v2.md](docs/plans/phase-6-backlog-v2.md)) et ne seront traitées qu'avec des
  garde-fous explicites (présomption d'innocence, source judiciaire, date, statut de la procédure).

En cas de doute sur ce qu'on a le droit d'afficher ou de déduire : demande, ne devine pas.

## Commandes

Prérequis : `uv`, Rust (édition 2024) + `sqlx-cli`, Podman + `podman-compose`, `make`. Copier
`.env.example` en `.env` avant de lancer quoi que ce soit.

**Tout l'outillage (`cargo`, `uv`, `podman`, `make`, `sqlx`) est installé dans WSL, pas dans Windows**, et
le dépôt lui-même vit dans le système de fichiers WSL (`/home/laury/know-your-candidate`), pas sous
`/mnt/c`. La manière de lancer une commande dépend donc de l'endroit d'où tourne la session :

- **Session déjà dans WSL** (cas normal : `uname -r` contient `microsoft`, plateforme Linux, répertoire de
  travail sous `/home`) : lancer les commandes directement, `make test`. **Ne pas préfixer par `wsl`** —
  depuis WSL, ce préfixe échoue.
- **Session lancée depuis Windows** (PowerShell, `cmd`, plateforme Windows) : préfixer par
  `wsl -d Ubuntu-26.04 --`, par exemple `wsl -d Ubuntu-26.04 -- make test`.

En cas de doute, `uname -r` tranche. La CI GitHub Actions tourne nativement sur Linux et n'est concernée
par aucun des deux cas.

```
make dev          # podman compose up (Postgres), migrations, worker + backend
make lint         # ruff check/format --check + cargo fmt --check + clippy -D warnings
make typecheck    # ty check (backend)
make test         # pytest (backend) + cargo test (worker)
make migrate      # sqlx migrate run sur DATABASE_URL
make ingest       # référentiel (AMO30), législatures 15/16/17, puis enrich_wikidata — plusieurs
                   # dizaines de minutes, voir docs/plans/phase-1-ingestion.md. Sort en code non
                   # nul si un job échoue (F3, docs/plans/phase-1.1-fix.md)
```

Détail par composant : `cd backend && uv run <cmd>` (`ruff`, `ty`, `pytest`, `uvicorn kyc_api.main:app
--reload`) ; `cd worker && cargo <cmd>` (`run`, `test`, `clippy --all-targets -- -D warnings`, `sqlx
prepare` après toute modification d'une requête `sqlx::query!`/`query_as!`).

Sous-commandes du binaire worker, pour déclencher un job sans route publique (voir
docs/plans/phase-1-ingestion.md) : `cargo run -- enqueue <type> [payload_json]` insère un job ;
`cargo run -- run-once` vide la file une fois et rend la main (c'est ce qu'utilise `make ingest`) ;
sans argument, le binaire démarre la boucle de jobs normale.
