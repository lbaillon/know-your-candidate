# Architecture

État : proposition, à valider avec les plans de phases. Ce document décrit la cible ; les plans décrivent
le chemin pour y arriver.

## 1. Principe directeur

Une seule contrainte structure tout le reste : **les pages doivent être rapides, et les calculs sont
lourds**. On sépare donc strictement le temps de lecture du temps de calcul.

- Le **worker Rust** fait tout ce qui est long : télécharger des archives de plusieurs dizaines de Mo,
  parser des dizaines de milliers de votes, recalculer des scores sur l'ensemble du corpus.
- **PostgreSQL** est la frontière entre les deux mondes, et la seule source de vérité.
- Le **backend FastAPI** ne fait que des `SELECT` sur des tables et des vues matérialisées déjà prêtes.
  Une page candidat doit être une poignée de requêtes indexées, pas une agrégation.

Corollaire : si une page a besoin d'un calcul, ce calcul devient une colonne, une vue matérialisée ou un
job du worker. Jamais une boucle Python.

## 2. Composants

```
┌────────────────────┐                                        ┌────────────────────┐
│  backend FastAPI   │   les deux ne se parlent jamais         │    worker Rust     │
│                    │        directement                      │                    │
│  • pages HTMX      │                                         │  • fetch open data │
│  • API JSON        │                                         │  • parsing         │
│  • back-office     │                                         │  • scoring         │
│    catégorisation  │                                         │  • LISTEN kyc_jobs │
└─────────┬──────────┘                                         └─────────┬──────────┘
          │ SELECT (+ INSERT job / saisie admin)                         │ INSERT/UPDATE
          ▼                                                              ▼
    ┌──────────────────────────────────────────────────────────────────────────┐
    │                              PostgreSQL                                   │
    │  données brutes · normalisées · catégorisations · scores · jobs · heartbeat│
    └──────────────────────────────────────────────────────────────────────────┘
                                       ▲
                                       │ HTTP
                  open data AN (Scrutins, AMO) · Wikidata · Wikimedia Commons
```

### Backend FastAPI

Rendu serveur avec Jinja2, interactions via HTMX (fragments HTML renvoyés par le serveur). Pas de bundler,
pas de framework JS. L'API JSON existe surtout pour rendre les données réutilisables par d'autres — c'est
un projet de transparence, l'ouverture des données fait partie du produit.

Il porte aussi le back-office de catégorisation : c'est la seule partie de l'application qui écrit des
données métier, parce qu'elle est pilotée par un humain et que le volume est faible.

### Worker Rust

Un binaire, deux tâches tokio :

- **la boucle de jobs** : il écoute `LISTEN kyc_jobs`, se réveille sur `NOTIFY`, prend un job disponible
  avec `SELECT … FOR UPDATE SKIP LOCKED` et l'exécute. Un `poll` de secours toutes les 30 s couvre les
  notifications perdues (une `NOTIFY` émise pendant une déconnexion est perdue — la table de jobs, elle,
  ne l'est pas) ;
- **le battement de cœur** : il met à jour `worker_heartbeat` toutes les 5 s (identifiant, version, job en
  cours). C'est ce que lit le backend pour savoir si un worker tourne.

Plusieurs instances du worker peuvent tourner en parallèle sans coordination externe : `SKIP LOCKED` fait
office de verrou distribué.

### PostgreSQL est le seul canal entre les deux processus

Décision arbitrée en [phase 0](plans/phase-0-socle.md) : **le backend et le worker ne communiquent jamais
directement**. Pas de gRPC, pas de HTTP interne, pas de broker.

| Besoin | Mécanisme |
| --- | --- |
| Créer un job | `INSERT INTO job` + trigger `pg_notify` |
| Réveiller le worker | `LISTEN` / `NOTIFY`, poll de secours en filet |
| Suivre la progression | colonnes `progress` / `progress_message`, lues en `SELECT` |
| Annuler | colonne `cancel_requested`, consultée entre deux lots |
| Vérifier que le worker est vivant | table `worker_heartbeat` |

Conséquences structurantes :

- **le backend ne dépend que de PostgreSQL.** Worker éteint : les pages publiques fonctionnent, les jobs
  s'empilent et partent au redémarrage ;
- l'état d'un job est durable et interrogeable en SQL, y compris longtemps après ;
- aucun port interne, aucune découverte de service, aucune génération de code à maintenir en double.

Ce qu'on accepte de perdre : la réponse strictement synchrone à « le worker est-il vivant à cet
instant ? ». Le battement de cœur répond avec quelques secondes de retard, sans conséquence pour un écran
d'administration.

Le contrat entre les deux langages est donc le **schéma SQL et la forme des payloads JSON de job**,
validés par Pydantic côté Python et serde côté Rust, avec les mêmes cas de test des deux côtés.

## 3. Modèle de données (esquisse)

Noms en anglais sauf les termes du domaine parlementaire (voir [CLAUDE.md](../CLAUDE.md#langue)).

### Données sources, jamais réinterprétées

| Table | Contenu | Clé naturelle |
| --- | --- | --- |
| `person` | député·es et candidat·es : nom, prénom, date de naissance, identifiants externes | `an_uid`, `wikidata_qid` |
| `party` | partis et groupes parlementaires, avec leur type (`groupe` vs parti hors AN) | `an_uid` / `wikidata_qid` |
| `party_membership` | appartenance d'une personne à un parti sur une **période** | `(person_id, party_id, period)` |
| `scrutin` | un scrutin public : date, législature, titre, type, résultat, compteurs | `an_uid` |
| `vote` | position d'une personne sur un scrutin : `pour` / `contre` / `abstention` / `non_votant` | `(scrutin_id, person_id)` |
| `source_document` | payload brut archivé (JSONB) + URL + date de récupération | `(source, external_id, fetched_at)` |

`party_membership.period` est un `daterange`, avec une contrainte `EXCLUDE` (extension `btree_gist`) qui
interdit deux appartenances contradictoires qui se chevauchent. C'est ce qui permet de répondre
proprement à « quel parti à la date du scrutin ? » — le cas PS 2015-2018 puis LFI 2018-2026.

### Interprétation, séparée et historisée

| Table | Contenu |
| --- | --- |
| `theme` | social, environnement, santé, éducation, sécurité, immigration (extensible) |
| `scrutin_label` | catégorisation d'un scrutin : thème, orientation, méthode (`heuristic` / `manual` / `import`), auteur, confiance, date, commentaire |
| `label_revision` | historique complet des changements de catégorisation (qui, quand, avant/après, pourquoi) |
| `person_theme_score` | score agrégé par personne et par thème, recalculé par le worker |
| `party_theme_score` | score agrégé par parti, par thème **et par période** |
| `score_contribution` | quels scrutins ont pesé dans quel score, et combien — c'est la table qui rend l'explication possible |

`score_contribution` n'est pas un détail d'implémentation : c'est le produit. Sans elle, on affiche un
verdict sans preuve, ce que le projet refuse.

### Infrastructure

| Table | Contenu |
| --- | --- |
| `job` | file de travaux : type, paramètres JSONB, état, tentatives, `locked_at`, progression, annulation, erreur |
| `worker_heartbeat` | présence des workers : identifiant, version, `last_seen_at`, job en cours |
| `ingestion_run` | trace d'une exécution d'ingestion : source, périmètre, compteurs, durée, résultat |

## 4. Flux de données

1. **Ingestion.** Un job `ingest_scrutins` est créé (admin ou planification). Le worker télécharge
   l'archive de la législature, parse, archive le brut dans `source_document`, puis fait un `upsert` sur
   `scrutin` et `vote`. Rejouable à volonté : mêmes clés naturelles, `ON CONFLICT DO UPDATE`.
2. **Filtrage.** Les scrutins retenus sont ceux des 10 dernières années dont la participation atteint le
   seuil configuré (50 % par défaut). Le seuil est une donnée de configuration, pas une constante en dur :
   on veut pouvoir l'abaisser plus tard pour enrichir le corpus sans rien réingérer.
3. **Catégorisation.** Un premier passage automatique (heuristique gauche/droite par comparaison avec le
   reste des votants, voir [methodology.md](methodology.md)) donne une base. Un admin corrige à la main ou
   via un cycle export → travail hors ligne → import.
4. **Scoring.** Un job `recompute_scores` recalcule `person_theme_score`, `party_theme_score` et
   `score_contribution`. Idempotent : il repart toujours de zéro sur les données actuelles, ce qui évite
   les dérives de calcul incrémental.
5. **Lecture.** Le backend sert la liste des candidats et les fiches depuis les tables de scores et de
   contributions.

## 5. Fonctionnalités PostgreSQL exploitées volontairement

| Besoin | Mécanisme |
| --- | --- |
| Appartenances partisanes dans le temps | `daterange` + `EXCLUDE USING gist` + index GiST |
| File de jobs sans broker | `LISTEN`/`NOTIFY` + `SELECT … FOR UPDATE SKIP LOCKED` |
| Éviter deux ingestions concurrentes de la même source | `pg_advisory_lock` |
| Payloads bruts hétérogènes | `JSONB` + index GIN |
| Agrégats de score prêts à lire | vues matérialisées + `REFRESH … CONCURRENTLY` |
| Recherche de candidat / de scrutin | `tsvector` en configuration `french`, colonne générée |
| Ingestion massive | `COPY` en flux depuis le worker, pas d'`INSERT` unitaires |

## 6. Décisions techniques

| # | Décision | Raison | Ce qu'on accepte de perdre |
| --- | --- | --- | --- |
| 1 | PostgreSQL comme unique dépendance d'infra | Un seul système à héberger, sauvegarder, comprendre. Le volume attendu (~10⁵ votes) ne justifie pas plus. | Le débit d'une vraie file de messages, dont on n'a pas besoin. |
| 1 bis | Aucun canal direct entre backend et worker (gRPC abandonné) | Chaque besoin réel — créer, suivre, annuler, vérifier la présence — se traite en SQL. Le backend ne dépend plus que de la base. | La réponse strictement synchrone sur l'état du worker, remplacée par un battement de cœur de 5 s. |
| 2 | Worker en Rust | Parsing et agrégation sur gros volumes, à coût mémoire prévisible sur des hébergements modestes. | Deux langages à maintenir. |
| 3 | HTMX plutôt qu'un SPA | Pages publiques majoritairement en lecture, SEO utile, pas de build. | Les interactions très riches côté client. |
| 4 | Données brutes archivées en JSONB | Pouvoir rejouer le parsing quand le format évolue, sans retélécharger. | De l'espace disque. |
| 5 | Catégorisations historisées | Le sujet est politiquement sensible : on doit pouvoir répondre à « qui a changé cette étiquette, quand et pourquoi ». | De la complexité dans le back-office. |
| 6 | Scores recalculés en entier | Simplicité et reproductibilité : même corpus + mêmes règles = mêmes scores. | Le temps de recalcul, acceptable à cette échelle. |

## 7. Points ouverts

- Périmètre exact des personnes suivies : tou·tes les député·es, ou seulement les candidat·es déclaré·es ?
  (La réponse conditionne le volume et la charge de catégorisation.)
- **Les candidat·es qui n'ont jamais siégé à l'Assemblée n'auront aucun vote personnel.** C'est la limite
  la plus sérieuse du projet ; les pistes sont détaillées dans
  [phase-4-partis-scores.md](plans/phase-4-partis-scores.md).
- Gestion des changements de nom de partis et des scissions (LR/UMP, RN/FN, etc.).
- Traitement des scrutins où le groupe se divise : le score du parti a-t-il encore du sens ?
