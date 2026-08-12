# Phase 0 — Socle technique

**Statut : ✅ validé** (décisions D0.1 à D0.6 arbitrées) · Dépend de : rien · Bloque : toutes les autres
phases

## Objectif

Avoir un squelette qui tourne de bout en bout — une page FastAPI, un worker Rust, une base PostgreSQL, un
job qui transite de l'un à l'autre — et une CI qui refuse le code non conforme. Aucune donnée réelle à ce
stade : on valide la plomberie.

## Périmètre

**Dedans** : configuration des deux projets, connexion à PostgreSQL, première migration, mécanisme de jobs
`LISTEN`/`NOTIFY`, battement de cœur du worker, outillage local, CI.

**Dehors** : toute donnée réelle, toute page métier, tout déploiement (phase 5), toute authentification
(phase 3).

## PostgreSQL comme unique canal entre le backend et le worker

Pas de gRPC, pas de HTTP interne, pas de broker : **le backend et le worker ne se parlent jamais
directement**. Ils partagent une base, et c'est tout.

| Besoin | Mécanisme |
| --- | --- |
| Créer un job | `INSERT INTO job` ; un trigger émet `pg_notify('kyc_jobs', …)` |
| Réveiller le worker | `LISTEN kyc_jobs`, avec un poll de secours toutes les 30 s |
| Prendre un job sans collision | `SELECT … FOR UPDATE SKIP LOCKED` |
| Suivre la progression | colonnes `progress` et `progress_message` mises à jour par le worker, lues en `SELECT` |
| Annuler un job | colonne `cancel_requested`, que le worker consulte entre deux lots |
| Savoir si le worker est vivant | table `worker_heartbeat` (identifiant, version, `last_seen_at`, job en cours), mise à jour toutes les 5 s |

Ce que ça nous fait gagner par rapport à la version gRPC initialement prévue :

- **le backend ne dépend que de PostgreSQL.** Worker éteint ou injoignable : les pages publiques
  fonctionnent, les jobs s'empilent et partent au redémarrage ;
- aucun port interne à exposer, aucune découverte de service à configurer au déploiement ;
- aucune génération de code, donc pas de désynchronisation entre le `.proto`, le Python et le Rust ;
- l'état d'un job est **durable et interrogeable en SQL**, y compris a posteriori.

Ce qu'on accepte de perdre : la réponse strictement synchrone (« le worker est-il vivant *à cet
instant* ? »). Le battement de cœur répond avec quelques secondes de retard, ce qui est sans conséquence
pour un écran d'administration.

Le contrat entre les deux langages devient le **schéma SQL plus la forme des payloads JSON de job**,
validés côté Python par Pydantic et côté Rust par serde, avec les mêmes cas de test des deux côtés.

## Livrables

1. `backend/` : projet `uv`, FastAPI, Jinja2, HTMX, Ruff + ty configurés, une page `/` et un `/healthz`.
2. `worker/` : crate Rust, tokio, sqlx, connexion base, boucle `LISTEN kyc_jobs` + poll de secours +
   battement de cœur.
3. `db/migrations/0001_init.sql` : extensions (`btree_gist`, `pg_trgm`), tables `job`, `worker_heartbeat`,
   `ingestion_run`, fonction + trigger émettant `NOTIFY kyc_jobs`.
4. `compose.yaml` (Podman) : PostgreSQL local uniquement (l'application tourne en natif, itération plus
   rapide). Choix du mainteneur : Podman plutôt que Docker Desktop, piloté via `podman compose` (le
   sous-commande intégrée qui délègue à `podman-compose`, installé séparément) — le fichier suit le
   compose-spec standard et n'est pas nommé `docker-compose.yml` pour ne pas laisser croire qu'il dépend
   de Docker. Nécessite Podman ≥ 4.0 (packagé nativement à partir d'Ubuntu 24.04 ; sur des distributions
   plus anciennes, `podman-compose <cmd>` avec le tiret fonctionne identiquement).
5. `Makefile` (ou `justfile`) : `make dev`, `make lint`, `make test`, `make migrate`.
6. `.github/workflows/ci.yml` : lint + typage + tests Python, `clippy` + tests Rust, migrations rejouées
   sur une base vierge.
7. `.env.example` documenté.

### Esquisse de la table `job`

```
id, type, payload jsonb, status, priority, attempts, max_attempts,
locked_at, locked_by, cancel_requested, progress, progress_message,
error, created_by, created_at, started_at, finished_at
```

`status` : `pending`, `running`, `done`, `failed`, `cancelled`. Un index partiel sur les jobs `pending`
suffit à rendre la prise de job triviale à l'échelle du projet.

## Étapes

1. **Base et migration.** PostgreSQL via `podman compose`, migration `0001`, `sqlx migrate`. Vérifier
   que `NOTIFY` part bien à l'insertion d'un job.
2. **Worker minimal.** Connexion sqlx, boucle : `LISTEN` → réveil → `SELECT … FOR UPDATE SKIP LOCKED` →
   exécution d'un job factice `noop` → passage à `done`. Poll de secours toutes les 30 s. Battement de
   cœur en tâche tokio séparée. Arrêt propre sur SIGTERM : un job en cours n'est pas perdu, il est
   relâché.
3. **Backend minimal.** FastAPI, layout Jinja de base, HTMX chargé localement (pas de CDN), page d'accueil
   provisoire, `/healthz` qui vérifie la base et lit le battement de cœur du worker.
4. **Boucle complète.** Une route de test crée un job, une page suit sa progression en HTMX
   (`hx-trigger="every 2s"` tant que le job n'est pas terminé).
5. **Outillage et CI.** Ruff, ty, pytest, clippy, workflow GitHub Actions avec un service PostgreSQL.
6. **Documentation.** Remplir les sections « Démarrage rapide » du README et « Commandes » de CLAUDE.md
   avec ce qui existe réellement.

## Décisions arbitrées

| # | Question | Décision |
| --- | --- | --- |
| D0.1 | Outil de migration | **`sqlx migrate`**. Le worker Rust est propriétaire du schéma, les migrations sont du SQL pur, aucun ORM côté Python. Alembic reste plus familier mais imposerait des modèles SQLAlchemy dont on n'a pas besoin |
| D0.2 | Accès base côté Python | **asyncpg + SQL écrit à la main**. Requêtes peu nombreuses, en lecture, et on veut voir le SQL |
| D0.3 | ~~gRPC~~ | **Abandonné.** Tout passe par PostgreSQL (voir la section dédiée). Le répertoire `proto/` est supprimé |
| D0.4 | Un seul processus ou deux ? | **Un binaire worker**, avec deux tâches tokio : boucle de jobs et battement de cœur |
| D0.5 | Nom du paquet Python | **`kyc_api`** |
| D0.6 | Version de Python | **3.14**. Render l'utilise par défaut depuis février 2026 (3.14.3, avec uv 0.10.2) — aucun risque côté hébergement |

## Plan d'exécution

Cette section est destinée à la session qui implémentera la phase. Elle est autoportante : **tout ce qui
est nécessaire est ici ou dans [CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de conversation à
retrouver. Les décisions ci-dessus sont arbitrées et ne sont pas à rediscuter.

Le projet est en développement sur le tronc : **commits directs sur `main`**, pas de branche ni de PR.
Découper en commits cohérents (migration, worker, backend, outillage, CI) plutôt qu'en un seul bloc, et
laisser `main` vert à chaque commit. Ne pas modifier les plans des autres phases.

### Arborescence cible

```
backend/
  pyproject.toml            projet uv, dépendances, config Ruff + ty
  .python-version           3.14
  src/kyc_api/
    __init__.py
    main.py                 création de l'app FastAPI, montage des routeurs
    config.py               réglages via variables d'environnement (Pydantic Settings)
    db.py                   pool asyncpg, dépendance FastAPI
    jobs.py                 création d'un job et lecture de son état (SQL écrit à la main)
    routers/
      health.py             /healthz
      dev.py                routes de démonstration de la boucle de jobs
    templates/
      base.html.jinja
      index.html.jinja
      _job_status.html.jinja   fragment rechargé par HTMX
    static/
      htmx.min.js           vendoré, pas de CDN
      style.css
  tests/
    conftest.py
    test_health.py
    test_jobs.py

worker/
  Cargo.toml                édition 2024
  .sqlx/                    requêtes préparées, commitées (voir mode offline)
  src/
    main.rs                 démarrage, tâches tokio, arrêt propre
    config.rs
    db.rs
    heartbeat.rs
    shutdown.rs
    jobs/
      mod.rs
      queue.rs              prise, achèvement, échec, reprise des jobs zombies
      noop.rs               job factice de la phase 0
  tests/
    queue.rs                tests d'intégration sur une vraie base

db/migrations/0001_init.sql
compose.yaml
Makefile
.env.example
.github/workflows/ci.yml
```

### Versions

Python **3.14**, PostgreSQL **16 minimum**, Rust **édition 2024**. Pour tout le reste (FastAPI, asyncpg,
sqlx, uv, HTMX), prendre la dernière version stable au moment de l'implémentation et **la figer dans les
fichiers de verrouillage** (`uv.lock`, `Cargo.lock`, commités tous les deux). Ne pas inventer de numéros
de version : vérifier ce qui est réellement publié.

### Migration `0001_init.sql`

Extensions : `btree_gist`, `pg_trgm` (utilisés dès la phase 1, autant les activer maintenant).

Table `job` :

```sql
CREATE TYPE job_status AS ENUM ('pending', 'running', 'done', 'failed', 'cancelled');

CREATE TABLE job (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    type             text        NOT NULL,
    payload          jsonb       NOT NULL DEFAULT '{}'::jsonb,
    status           job_status  NOT NULL DEFAULT 'pending',
    priority         smallint    NOT NULL DEFAULT 0,
    attempts         smallint    NOT NULL DEFAULT 0,
    max_attempts     smallint    NOT NULL DEFAULT 3,
    scheduled_at     timestamptz NOT NULL DEFAULT now(),
    locked_at        timestamptz,
    locked_by        text,
    cancel_requested boolean     NOT NULL DEFAULT false,
    progress         real,                 -- 0.0 à 1.0, NULL si inconnu
    progress_message text,
    error            text,
    created_by       text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    started_at       timestamptz,
    finished_at      timestamptz
);

CREATE INDEX job_pending_idx
    ON job (priority DESC, scheduled_at, id)
    WHERE status = 'pending';
```

Table `worker_heartbeat` :

```sql
CREATE TABLE worker_heartbeat (
    worker_id      text PRIMARY KEY,
    version        text        NOT NULL,
    started_at     timestamptz NOT NULL DEFAULT now(),
    last_seen_at   timestamptz NOT NULL DEFAULT now(),
    current_job_id bigint REFERENCES job (id) ON DELETE SET NULL
);
```

Table `ingestion_run` : créer un squelette minimal (id, source, statut, compteurs jsonb, horodatages).
Elle ne sert qu'en phase 1, il s'agit juste de ne pas laisser la migration `0001` incomplète.

Déclencheur de notification :

```sql
CREATE FUNCTION notify_job() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_notify('kyc_jobs', NEW.id::text);
    RETURN NEW;
END;
$$;

CREATE TRIGGER job_notify
    AFTER INSERT ON job
    FOR EACH ROW WHEN (NEW.status = 'pending')
    EXECUTE FUNCTION notify_job();
```

`pg_notify` n'est délivré qu'au `COMMIT` : le worker ne peut donc pas être réveillé pour un job qu'il ne
verrait pas encore. C'est le comportement voulu, ne pas chercher à le contourner.

### Prise de job — requête exacte

C'est le seul morceau de concurrence de la phase. L'écrire ainsi, pas autrement :

```sql
WITH next_job AS (
    SELECT id
    FROM job
    WHERE status = 'pending'
      AND NOT cancel_requested
      AND scheduled_at <= now()
    ORDER BY priority DESC, scheduled_at, id
    FOR UPDATE SKIP LOCKED
    LIMIT 1
)
UPDATE job j
SET status     = 'running',
    locked_at  = now(),
    locked_by  = $1,
    attempts   = j.attempts + 1,
    started_at = COALESCE(j.started_at, now())
FROM next_job
WHERE j.id = next_job.id
RETURNING j.id, j.type, j.payload, j.attempts, j.max_attempts;
```

Points à ne pas rater :

- `FOR UPDATE SKIP LOCKED` est dans la CTE, avec `ORDER BY` **et** `LIMIT 1` — c'est ce qui garantit que
  deux workers ne prennent jamais la même ligne ;
- `attempts` est incrémenté à la prise, pas à l'échec : un job qui fait planter le worker doit finir par
  s'épuiser plutôt que de boucler ;
- au-delà de `max_attempts`, le job passe en `failed` avec son message d'erreur, il n'est pas repris.

### Reprise des jobs zombies

Un worker tué brutalement laisse un job en `running`. Au démarrage puis toutes les 60 s :

```sql
UPDATE job
SET status    = CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END,
    locked_at = NULL,
    locked_by = NULL,
    error     = COALESCE(error, 'reclaimed: worker heartbeat expired')
WHERE status = 'running'
  AND locked_by IS NOT NULL
  AND NOT EXISTS (
      SELECT 1 FROM worker_heartbeat w
      WHERE w.worker_id = job.locked_by
        AND w.last_seen_at > now() - interval '1 minute'
  );
```

Le seuil d'expiration (1 min) doit être largement supérieur à la période du battement de cœur (5 s), sans
quoi un worker occupé se ferait voler ses jobs.

### Arrêt propre

Sur SIGTERM (et SIGINT en local) : arrêter de prendre de nouveaux jobs, laisser le job en cours se
terminer si c'est rapide, sinon le **relâcher** en `pending` avec `locked_at`/`locked_by` remis à `NULL`.
Ne pas décrémenter `attempts` : c'est volontairement conservateur, un job qui se fait interrompre en
boucle finit par échouer plutôt que de tourner indéfiniment. Prévoir un délai de grâce (30 s) avant sortie
forcée.

### Boucle du worker

Trois tâches tokio :

1. **jobs** — `LISTEN kyc_jobs` sur une connexion dédiée ; à chaque réveil, vider la file (boucler tant
   que la requête de prise renvoie une ligne) ; poll de secours toutes les 30 s ; reprise des zombies
   toutes les 60 s ;
2. **heartbeat** — `INSERT … ON CONFLICT (worker_id) DO UPDATE` toutes les 5 s. `worker_id` = nom d'hôte +
   pid, ou un identifiant fourni par l'environnement ;
3. **shutdown** — écoute des signaux, diffusion de l'ordre d'arrêt aux deux autres.

Le job `noop` de la phase 0 se contente d'attendre quelques secondes en publiant sa progression : il
existe pour prouver que la chaîne fonctionne de bout en bout.

### sqlx en mode offline

Indispensable, sinon la CI ne compile pas sans base :

- utiliser les macros `sqlx::query!` / `query_as!` (vérification à la compilation) ;
- générer `.sqlx/` avec `cargo sqlx prepare` et **commiter ce répertoire** ;
- en CI, compiler avec `SQLX_OFFLINE=true` ;
- ajouter une vérification que `.sqlx/` est à jour (`cargo sqlx prepare --check`), sinon la dérive entre le
  code et les métadonnées passera inaperçue.

Migrations : `sqlx migrate run`. Le worker joue les migrations au démarrage, protégé par un
`pg_advisory_lock` pour que deux instances ne migrent pas simultanément.

### Backend

- `/healthz` renvoie l'état de la base **et** la fraîcheur du battement de cœur du worker. Worker absent →
  réponse 200 avec un état `worker: stale`, **pas** une erreur : le site public n'est pas censé tomber
  parce que le worker est éteint.
- La route de démonstration crée un job (`INSERT` uniquement, jamais d'appel au worker) et redirige vers
  une page qui suit sa progression avec `hx-get` + `hx-trigger="every 2s"`, en arrêtant le rechargement
  quand le job est terminé (`hx-swap` sur un fragment qui cesse de porter le déclencheur).
- HTMX est **vendoré dans `static/`**, pas chargé depuis un CDN. Noter la version dans un commentaire.
- Aucun ORM. Le SQL est écrit à la main, dans `jobs.py` et `db.py`.

### Fragment ou page entière : la convention à poser dès maintenant

C'est la convention qui pourrit le plus vite si elle s'installe par accident. On la fixe ici, elle vaudra
pour tout le projet :

- **une route renvoie une page entière, ou un fragment, mais jamais les deux selon l'en-tête `HX-Request`.**
  Les fragments ont leurs propres routes explicites, sous un préfixe lisible (`/fragments/…`) ;
- un gabarit de fragment est préfixé par `_` et **n'étend jamais `base.html.jinja`** ;
- ouvrir directement l'URL d'un fragment dans un navigateur doit fonctionner et afficher le fragment nu.
  C'est ce qui les rend débogables et testables sans outillage particulier.

Raison du choix : pas de branchement invisible dans les vues, et un test qui appelle une URL sait ce qu'il
va recevoir.

### CSS : le strict minimum, volontairement

`style.css` doit rester **délibérément pauvre** à ce stade : quelques variables CSS dans `:root` pour les
couleurs, les espacements et l'échelle typographique, de quoi rendre une page lisible, et rien d'autre.

**Ne pas** créer de design system, de bibliothèque de composants, de classes utilitaires, de thème sombre
ni de stratégie responsive élaborée. L'architecture front — structure de la feuille, échelles, palette,
comportement mobile, et le composant de frise des appartenances — se conçoit en
[phase 2](phase-2-api-ui.md) avec le mainteneur. Tout ce qui serait inventé ici serait à défaire.

### Stratégie de test

Elle est fixée maintenant parce que `conftest.py` va faire jurisprudence pour tout le projet.

**Côté Python** : une base jetable créée une fois, migrations jouées une fois, puis **chaque test s'exécute
dans une transaction annulée à la fin**. Rapide, et aucun test ne dépend de l'ordre des autres.

**L'exception importante** : les tests de la file de jobs ne peuvent pas utiliser cet isolement. `NOTIFY`
n'est délivré qu'au `COMMIT`, et `SKIP LOCKED` n'a de sens qu'entre connexions distinctes voyant des
données validées. Ces tests-là ont donc besoin d'un état réellement commité, et d'un nettoyage explicite
entre chaque cas — les marquer clairement pour que la distinction ne se perde pas.

**Côté Rust** : utiliser `#[sqlx::test]`, qui crée une base fraîche par test et joue les migrations. C'est
l'idiome de sqlx, ne pas réinventer un harnais.

**Ce qu'on teste en phase 0** : la mécanique de la file (prise, achèvement, échec, reprise des zombies,
concurrence), la création de job côté backend, `/healthz` dans ses deux états. **Ce qu'on ne teste pas** :
le rendu HTML au-delà d'un test de fumée — la manière de tester les pages Jinja/HTMX se décide en
phase 2, quand il y aura de vraies pages.

Pas d'objectif chiffré de couverture, ni maintenant ni plus tard : on teste ce qui casse silencieusement.

### Outillage

`Makefile` avec au minimum :

| Cible | Effet |
| --- | --- |
| `make dev` | démarre PostgreSQL (`podman compose`), joue les migrations, lance worker et backend |
| `make lint` | `ruff check`, `ruff format --check`, `cargo clippy -- -D warnings`, `cargo fmt --check` |
| `make typecheck` | `uv run ty check` |
| `make test` | `uv run pytest`, `cargo test` |
| `make migrate` | `sqlx migrate run` |

Pour `ty` : consulter sa documentation à jour pour la commande et les clés de configuration — c'est un
outil en préversion, **ne pas deviner** la syntaxe de `[tool.ty]`. Et ne pas proposer mypy, le choix est
tranché.

`.env.example` documente `DATABASE_URL`, `WORKER_ID`, le niveau de log, et rien d'autre à ce stade.

### CI

Deux jobs GitHub Actions, un service PostgreSQL 16 :

1. **python** — `uv sync`, `ruff check`, `ruff format --check`, `ty check`, `pytest` ;
2. **rust** — `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo sqlx prepare --check`,
   `cargo test` avec `SQLX_OFFLINE=true` pour la compilation ;
3. **migrations** — rejouer toutes les migrations sur une base vierge, puis vérifier qu'un second passage
   ne casse rien.

### Vérifications à exécuter avant de déclarer la phase terminée

Ce ne sont pas des cases à cocher de confort, ce sont les seules preuves que la plomberie tient :

1. `make dev` démarre tout en une commande depuis un clone neuf.
2. Créer un job depuis l'interface : il passe en `done` en moins d'une seconde, sans attendre le poll.
3. Arrêter le worker, créer trois jobs, le redémarrer : les trois sont traités.
4. **Concurrence** — lancer deux workers, injecter 100 jobs, vérifier qu'aucun n'est traité deux fois
   (`SELECT count(*) <> count(DISTINCT id)` doit être faux) et qu'aucun ne reste en `pending`.
5. **Perte de `NOTIFY`** — couper la connexion du worker (`pg_terminate_backend` sur sa connexion
   d'écoute) pendant qu'un job est créé : il doit être traité au poll de secours suivant, et le worker
   doit se réabonner.
6. **Zombie** — `kill -9` sur un worker pendant un job long ; après expiration du battement de cœur, le
   job repasse `pending` et un autre worker le reprend.
7. **Arrêt propre** — SIGTERM pendant un job long : le job est relâché, pas perdu, et le processus sort
   sans erreur.
8. Le worker éteint, `/healthz` répond 200 avec `worker: stale` et les pages publiques s'affichent.
9. CI verte ; puis vérifier qu'une erreur de type introduite volontairement la fait échouer.

Consigner le résultat des points 4 à 7 dans le message du commit final : ce sont ceux qui ne se voient
pas à la lecture du code.

### Hors périmètre — ne pas ajouter

Pas d'authentification, pas de table métier (`person`, `scrutin`…), pas d'appel à l'open data, pas de
Dockerfile de production, pas de configuration Render, pas de Logfire. Tout cela appartient aux phases
suivantes et les ajouter ici rendrait la phase illisible.

En revanche, terminer par la mise à jour de la section « Démarrage rapide » du [README](../../README.md)
et de la section « Commandes » de [CLAUDE.md](../../CLAUDE.md) avec les commandes réellement disponibles.

## Fini quand

- `make dev` lance base + worker + backend en une commande.
- Un `POST` sur une route de test crée un job, le worker le traite en moins d'une seconde sans polling.
- Le worker est arrêté → le job reste en attente ; il redémarre → le job est traité.
- Deux workers lancés simultanément ne traitent jamais le même job.
- Le worker est arrêté → `/healthz` le signale via le battement de cœur périmé, sans erreur ni page
  cassée.
- Un job interrompu par un SIGTERM en cours d'exécution est relâché puis repris, pas perdu.
- La CI est verte sur `main`, et rouge si on introduit une erreur de type ou de lint.

## Risques

- **`ty` est un outil jeune** (préversion) : faux positifs et évolutions de comportement possibles. Le
  choix est assumé, il n'y a pas de repli vers mypy. Si une règle bloque, on la neutralise localement avec
  un commentaire justifiant, et on remonte le cas en amont — un projet ouvert peut se permettre de
  contribuer à son outillage.
- **Perte de `NOTIFY`** en cas de coupure de connexion : c'est attendu, le poll de secours est la
  protection. À tester réellement, pas seulement à documenter.
- **Jobs zombies** : un worker tué brutalement laisse un job en `running` avec un `locked_at` ancien. Il
  faut une règle de reprise (au-delà de N minutes sans battement de cœur du détenteur, le job redevient
  `pending`) et un compteur de tentatives pour ne pas boucler indéfiniment sur un job qui fait planter le
  worker.
- **Écriture depuis le backend** : créer un job est la seule écriture métier autorisée côté Python. À
  garder sous surveillance, c'est par là que les entorses à l'architecture commenceront.
