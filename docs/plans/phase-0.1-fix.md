# Phase 0.1 — Corrections du socle

**Statut : ✅ implémenté** · Dépend de : phase 0 · Bloque : phase 1

## Objectif

Corriger les dix défauts relevés à la revue de la phase 0. Aucune fonctionnalité nouvelle : la phase 0
reste le socle, elle doit juste tenir ses garanties.

Cette section est autoportante : **tout ce qui est nécessaire est ici ou dans
[CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de conversation à retrouver. Développement sur le
tronc, commits directs sur `main`, un commit par correctif ou par groupe cohérent, `main` vert à chaque
commit.

Le diagnostic de chaque point a été vérifié dans le code : les causes décrites ci-dessous sont établies,
pas supposées. Si l'une se révèle fausse au contact du code, le dire plutôt que de contourner.

## Ce que la revue a validé — ne pas y toucher

La migration `0001`, la requête de prise de job (`FOR UPDATE SKIP LOCKED` dans la CTE), les tests
d'intégration Rust et les gabarits HTMX sont corrects. Les correctifs ci-dessous ne doivent rien y
changer, sauf mention explicite.

---

## Bloquants

### F1 — Aucune supervision des tâches de fond

**Fichier** : `worker/src/main.rs:48`, `worker/src/jobs/mod.rs:35`, `worker/src/heartbeat.rs`

**Symptôme** : si la boucle de jobs meurt (par exemple `PgListener::connect_with` qui échoue sur un
hoquet de la base au démarrage), `tokio::join!` continue d'attendre la tâche de battement de cœur, qui
tourne toujours. Le processus reste vivant, **le heartbeat continue d'écrire « je vais bien », `/healthz`
répond `worker: ok`, et plus aucun job n'est traité**. Comme la reprise des jobs zombies se fonde sur la
fraîcheur du heartbeat, les jobs bloqués en `running` ne sont jamais repris non plus.

C'est le plus grave des dix, parce qu'il est totalement silencieux.

**Correctif** :

1. `jobs::spawn` et `heartbeat::spawn` renvoient `JoinHandle<anyhow::Result<()>>` et **propagent**
   l'erreur au lieu de la journaliser puis de renvoyer `()`.
2. Le `watch::Sender` est partagé : `let (tx, rx) = watch::channel(false); let tx = Arc::new(tx);`
   (`watch::Sender` n'est pas `Clone`, mais ses méthodes prennent `&self`, donc `Arc` convient).
   `shutdown::spawn(tx.clone())`, et `main` garde `tx`.
3. Dans `main`, remplacer `tokio::join!` par un `tokio::select!` sur les deux `JoinHandle`. Dès que l'une
   des deux tâches se termine — quelle qu'en soit la raison :
   - journaliser laquelle et pourquoi ;
   - `tx.send(true)` pour demander l'arrêt de l'autre ;
   - attendre l'autre avec un délai maximal supérieur à `SHUTDOWN_GRACE` (35 s) ;
   - **sortir en erreur** (`Err`) si la tâche s'est terminée sur une erreur, pour que le code de sortie
     soit non nul et que l'hébergeur redémarre le processus.

Un arrêt normal (signal reçu, les deux tâches se terminent proprement) reste un `Ok(())`.

**Vérification** : test d'intégration ou vérification manuelle documentée — démarrer le worker avec une
`DATABASE_URL` valide, puis rendre le `LISTEN` impossible (couper la base) ; le processus doit sortir avec
un code non nul en quelques secondes, et non rester vivant.

### F2 — Écritures sur un job sans garde de propriété

**Fichier** : `worker/src/jobs/queue.rs:69` (et `fail_job`, `release_job`, `set_progress`)

**Symptôme** : les quatre fonctions font `WHERE id = $1` sans vérifier `locked_by`. Après une reprise de
zombie, l'ancien propriétaire peut donc écrire `done` sur un job qu'un autre worker est en train
d'exécuter. La prise de job est atomique, la restitution ne l'est pas — c'est le compagnon classique de
`SKIP LOCKED`.

**Correctif** :

1. Ces quatre fonctions prennent le `worker_id` et gardent leur `UPDATE` :
   `WHERE id = $1 AND locked_by = $2 AND status = 'running'`.
2. Elles récupèrent `rows_affected()` ; **si zéro, journaliser un avertissement explicite** (« job repris
   par un autre worker, écriture ignorée ») avec l'identifiant du job. Ce n'est pas une erreur fatale,
   mais ça ne doit jamais passer inaperçu.
3. Pour éviter de faire circuler trop de paramètres, introduire un petit contexte partagé — par exemple
   `struct JobContext { pool: PgPool, worker_id: String, job_id: i64 }` — passé à `noop::run` et aux
   futurs jobs de la phase 1. C'est ce contexte qui portera la progression.

**Test de non-régression** (intégration Rust, `worker/tests/queue.rs`) : prendre un job comme worker A,
forcer sa reprise par un worker B (mettre `last_seen_at` de A dans le passé, appeler
`reclaim_zombie_jobs`, puis `claim_next_job` avec B), puis appeler `complete_job` avec A. Vérifier que le
job **reste** en `running` pour B, et que l'appel signale zéro ligne affectée.

### F3 — Boucle serrée à 100 % de CPU si les handlers de signal ne s'installent pas

**Fichiers** : `worker/src/shutdown.rs:10`, `worker/src/jobs/mod.rs:85` et `:143`, `worker/src/heartbeat.rs`

**Symptôme** : dans `shutdown.rs`, le `return` sur échec d'installation détruit le `watch::Sender`. Dès
lors, `changed()` renvoie `Err` **immédiatement et pour toujours** ; or les boucles font
`_ = shutdown.changed() => { continue; }`, qui ignore le `Result`. Résultat : boucle sans jamais dormir,
dans le worker *et* dans le heartbeat.

**Correctif, des deux côtés** :

1. `shutdown.rs` : ne pas se contenter de journaliser et sortir. Ne pas pouvoir installer les handlers est
   une condition fatale : journaliser l'erreur **puis `tx.send(true)`**, ce qui provoque un arrêt propre
   plutôt qu'un processus sans gestion de signal.
2. Les récepteurs ne doivent de toute façon jamais dépendre du bon comportement de l'émetteur. Remplacer
   `_ = shutdown.changed()` par une gestion explicite : **`Err` signifie que tous les émetteurs ont
   disparu, donc plus rien ne viendra — c'est un arrêt.** Sortir de la boucle, ne pas `continue`.
3. Extraire cette logique dans un petit helper (par exemple
   `async fn shutdown_requested(rx: &mut watch::Receiver<bool>) -> bool`) et **le tester unitairement** :
   sender vivant qui envoie `true` → renvoie `true` ; sender détruit → renvoie `true` sans boucler.

Le correctif F1 fait que `main` garde un `Sender` vivant, ce qui rend le scénario moins probable — mais
les deux corrections restent nécessaires, la seconde étant la seule qui protège vraiment.

### F4 — Les trois tentatives sont consommées en une milliseconde

**Fichier** : `worker/src/jobs/queue.rs:89` (`fail_job`), consommé par `worker/src/jobs/mod.rs:113`

**Symptôme** : `fail_job` remet le job en `pending` sans repousser `scheduled_at`, et la boucle de
`drain_queue` le reprend immédiatement. Un job qui échoue à cause d'une coupure réseau de deux secondes
épuise ses trois tentatives et passe en `failed` avant même que la coupure ne soit terminée. Le mécanisme
de tentatives ne protège donc de rien.

**Correctif** : report exponentiel plafonné dans `fail_job`. `attempts` ayant déjà été incrémenté à la
prise, on obtient 5 s, 15 s, 45 s… plafonné à 5 minutes :

```sql
SET status       = (CASE WHEN attempts >= max_attempts THEN 'failed' ELSE 'pending' END)::job_status,
    error        = $3,
    scheduled_at = now() + make_interval(
                       secs => least(300, (5 * power(3, greatest(attempts - 1, 0)))::int)
                   ),
    locked_at    = NULL,
    locked_by    = NULL
WHERE id = $1 AND locked_by = $2 AND status = 'running'
```

La requête de prise filtre déjà sur `scheduled_at <= now()`, le report est donc respecté sans autre
changement. La reprise après report s'appuie sur le poll de 30 s, ce qui est acceptable ; ne pas chercher
à émettre un `NOTIFY` différé.

**Test** : faire échouer un job, vérifier que `scheduled_at > now()` et qu'un `claim_next_job` immédiat
renvoie `None`.

---

## Sérieux — le parcours documenté ne fonctionne pas depuis un clone neuf

### F5 — La base de test n'existe pas

**Fichier** : `backend/tests/conftest.py:11`

**Symptôme** : les tests visent `kyc_test`, que rien ne crée en local — `compose.yaml` ne provisionne que
`kyc`. `make test` échoue en `InvalidCatalogNameError`. Ça contredit le critère « depuis un clone neuf »
de la phase 0.

**Correctif** : la fixture de session crée la base si elle manque, plutôt que de dépendre d'un script
d'init de conteneur (les scripts d'init de l'image Postgres ne s'exécutent qu'à la création du volume :
un volume déjà existant ne les rejouerait pas).

1. Dériver une URL de maintenance depuis `TEST_DATABASE_URL` en remplaçant le nom de base par `postgres`.
2. S'y connecter, vérifier l'existence de `kyc_test` dans `pg_database`, la créer si absente.
3. Conserver la remise à zéro du schéma et le rejeu des migrations tels qu'ils sont.
4. Si le serveur lui-même est injoignable, échouer avec un message actionnable qui nomme la commande à
   lancer (`make db-up`), pas une trace asyncpg brute.

### F6 — `make dev` court contre l'initialisation de Postgres

**Fichier** : `Makefile:6`

**Symptôme** : `podman compose up -d` rend la main avant que Postgres accepte les connexions ; au premier
démarrage à froid, `make migrate` échoue en « connection refused ».

**Correctif** : une cible `db-up` qui démarre le conteneur **et attend qu'il réponde** :

- passer `--wait` si la version de `podman compose` le prend en charge, mais ne pas s'y fier seule (le
  support varie selon les versions) ;
- faire suivre d'une **boucle d'attente bornée** sur `pg_isready` (environ 30 tentatives d'une seconde),
  qui échoue avec un message clair au bout du compte ;
- `dev` et `test` dépendent de `db-up`.

### F7 — `/healthz` renvoie 500 dans le cas exact pour lequel il existe

**Fichier** : `backend/src/kyc_api/routers/health.py:17`

**Symptôme** : seul `asyncpg.PostgresError` est attrapé. Postgres arrêté lève `ConnectionRefusedError`
(ou `asyncpg.InterfaceError`), donc l'endpoint renvoie 500 au lieu du 503 documenté.

**Correctif** : attraper largement (`except Exception as exc`) et renvoyer 503 avec le nom de la classe
d'exception dans la charge utile. **Ajouter un commentaire justifiant la capture large** : un point de
santé qui plante n'a aucune valeur, c'est l'un des rares endroits où c'est le comportement correct.

**Test** : surcharger `get_pool` par un objet dont `fetchval` lève `ConnectionRefusedError`, vérifier le
503 et la charge utile.

### F8 — `WORKER_ID` fixe casse la reprise des zombies

**Fichiers** : `.env.example:9`, `worker/src/config.rs:13`

**Symptôme** : `.env.example` propose `WORKER_ID=local-dev`, en contradiction avec le commentaire
d'unicité juste au-dessus. Deux workers lancés en local partagent alors la même ligne de
`worker_heartbeat` : celui qui reste vivant maintient le heartbeat de celui qui est mort, et les jobs de
ce dernier ne sont **jamais** repris. Le repli à base de pid est par ailleurs inatteignable, puisque la
variable est définie.

**Correctif** :

1. `.env.example` : laisser `WORKER_ID` vide (ou commenté), avec l'explication que le worker en dérive un
   automatiquement, et qu'on ne le fixe qu'en production où l'orchestrateur garantit l'unicité.
2. `config.rs` : traiter une valeur **vide ou blanche** comme absente (`std::env::var` renvoie `Ok("")`
   pour une variable définie mais vide, le repli actuel ne se déclenche donc pas).
3. Le repli doit rester unique par processus. Le pid suffit sur un même hôte ; y ajouter le nom d'hôte est
   un plus si ça n'impose pas de dépendance lourde.

---

## À corriger avant la phase 5

### F9 — Route de création de job publique et non authentifiée

**Fichier** : `backend/src/kyc_api/main.py:28`

**Symptôme** : `dev.router` et `dev.fragments_router` sont montés sans condition. `POST /dev/jobs` permet
à n'importe qui de remplir la file. Le jour où `main` se déploie automatiquement (phase 5), c'est un déni
de service trivial.

**Correctif** : un réglage `enable_dev_routes`, **`False` par défaut**, qui conditionne le montage.
`.env.example` le met à `true` pour le développement local, et `conftest.py` le force à `true` pour les
tests. Un défaut sûr, activé explicitement — pas l'inverse.

**Test** : application créée avec le drapeau à `False` → `POST /dev/jobs` renvoie 404.

### F10 — Un job repris attend jusqu'à 30 s de plus

**Fichier** : `worker/src/jobs/mod.rs:77`

**Symptôme** : la branche de reprise des zombies se termine par `continue`, qui saute l'appel à
`drain_queue` en fin de boucle. Et comme requeuer un job est un `UPDATE`, aucun `NOTIFY` n'est émis : le
job repris attend le poll suivant.

**Correctif** : retirer le `continue` pour que la branche retombe sur `drain_queue`, comme les autres.

---

## Pour finir : inscrire les leçons là où elles serviront

Les quatre bloquants ont un point commun qui vaut d'être noté : **le SQL que le plan dictait mot pour mot
était juste ; les défauts sont tous dans le code de cycle de vie autour**, que le plan ne décrivait pas.
Deux règles en découlent, à ajouter à la section « Règles d'architecture à ne pas casser » de
[CLAUDE.md](../../CLAUDE.md) :

- **Toute écriture sur un job porte le garde de propriété** (`locked_by`, `status`), et une écriture qui
  n'affecte aucune ligne est journalisée, jamais ignorée en silence.
- **Toute tâche de fond est supervisée** : sa mort arrête le processus avec un code non nul. Un processus
  à moitié vivant qui continue de signaler qu'il va bien est pire qu'un processus mort.

Reporter également dans la section « Plan d'exécution » de
[phase-0-socle.md](phase-0-socle.md) le report exponentiel des tentatives et le garde de propriété, pour
que la phase 1 en hérite au lieu de refaire les mêmes erreurs sur de vrais jobs.

## Fini quand

- Les dix points sont corrigés, chacun avec son test de non-régression là où il en est prévu un
  (F2, F3, F4, F5, F7, F9).
- Les neuf vérifications de la phase 0 sont rejouées et passent toujours — en particulier la concurrence
  sur 100 jobs et le `kill -9`, que les gardes de propriété touchent directement.
- **`git clone` puis `make db-up && make migrate && make test` fonctionne sans intervention manuelle.**
- Worker démarré avec une base injoignable : le processus sort en erreur au lieu de rester vivant.
- CLAUDE.md et le plan de la phase 0 portent les deux règles ci-dessus.

## Risques

- **F1 et F3 se recouvrent** : traiter F1 d'abord (le `Sender` partagé change la structure de `main`),
  puis F3 par-dessus. Ne pas les faire en parallèle.
- **F2 touche la signature de tout ce qui écrit un job**, y compris `noop::run` et la progression. C'est le
  correctif le plus étendu ; le faire dans son propre commit, avec `cargo sqlx prepare` rejoué.
- Après toute modification d'une requête `sqlx::query!`, **relancer `cargo sqlx prepare -- --all-targets`**
  et commiter `.sqlx/`, sinon la CI cassera en mode offline. Sans `--all-targets`, la commande régénère
  `.sqlx/` **en écrasant** les entrées des requêtes qui ne vivent que dans les tests d'intégration : la
  compilation offline des tests échoue alors, et la cause n'est pas évidente à lire dans l'erreur.

## Suites : ce qu'une seconde revue a trouvé

L'implémentation de F1 à F10 a fait l'objet d'une relecture, qui a relevé sept points de plus — dont deux
qui annulaient l'effet du correctif qu'ils portaient. Corrigés dans la foulée :

1. **La supervision n'aboutissait pas.** `main` attendait la tâche de signal après avoir détecté la mort
   d'une tâche de fond ; cette tâche étant parquée sur `ctrl_c()`/SIGTERM, le processus se bloquait au lieu
   de sortir. F1 détectait la panne mais n'en tirait aucune conséquence. On ne récupère désormais son
   résultat que si elle a déjà fini, et on l'abandonne sinon.
2. **`GET /` vivait sur `dev.router`.** Conditionner ce routeur démontait donc aussi la page d'accueil :
   404 sur `/` avec le défaut sûr. Les pages publiques sont passées dans `routers/pages.py`, toujours
   monté — **une page publique ne vit jamais sur un routeur qu'on éteint**, et un test le vérifie.
3. Le cast `::int` du report exponentiel était appliqué dans l'argument de `least()` au lieu d'envelopper
   le résultat : débordement possible avant le plafond. `make_interval(secs => …)` prenant un
   `double precision`, le cast a simplement été retiré.
4. Le repli de `WORKER_ID` sur le seul pid redonnait `worker-pid1` à tous les conteneurs — la panne de F8,
   décalée en production. Nom d'hôte et suffixe temporel ajoutés.
5. `pg_isready` interrogeait la socket Unix, sur laquelle le serveur temporaire d'initialisation répond
   aussi ; `-h 127.0.0.1` force une vérification TCP réelle.
6. Le test de `WORKER_ID` mutait l'environnement du processus sous un commentaire `SAFETY` invoquant un
   test mono-thread, alors que cargo exécute les tests en parallèle. La résolution a été extraite en
   fonction pure, testable sans `unsafe`.
