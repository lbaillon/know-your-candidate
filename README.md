# Know Your Candidate

**Des fiches candidats basées sur les votes, pas sur les discours.**

En vue de l'élection présidentielle française de 2027, ce projet construit pour chaque candidat·e une fiche
factuelle et sourcée, dérivée **des votes réellement émis** à l'Assemblée nationale — par la personne
elle-même et par les partis auxquels elle a appartenu, période par période.

Le postulat du projet : un programme de campagne est une promesse, un vote est un fait. On s'appuie donc
d'abord sur les faits, et chaque conclusion affichée est **cliquable jusqu'au scrutin qui la justifie**.

> Statut : 🚧 phase 0 (socle technique) implémentée, aucune donnée réelle. Les plans de réalisation sont
> dans [docs/plans/](docs/plans/) et sont ouverts à la relecture.

---

## Ce que fait (fera) le projet

- **Ingestion** des scrutins publics de l'Assemblée nationale depuis l'open data officiel (10 dernières
  années max, scrutins à participation ≥ 50 % dans un premier temps).
- **Reconstitution des appartenances partisanes** dans le temps : si une personne était au PS de 2015 à
  2018 puis à LFI de 2018 à 2026, sa fiche affiche les positions de chaque parti sur la période concernée.
- **Catégorisation des scrutins** par thème (social, environnement, santé, éducation, sécurité,
  immigration) et par orientation, avec un premier passage automatique et une correction humaine.
- **Fiches candidats** : orientation par thème, et surtout **l'explication** — « classé libéral notamment
  parce qu'il a voté contre la taxe Zucman le JJ/MM/AAAA ».

Ce que le projet ne fait **pas** : noter, classer « bon / mauvais », prédire un vote, ou éditorialiser.
Voir [docs/methodology.md](docs/methodology.md) pour les règles de neutralité et les limites assumées.

## Architecture en une image

```
    open data AN  ──►  worker Rust  ──►  PostgreSQL  ──►  backend FastAPI  ──►  HTMX
   (Scrutins,           (fetch,          (source de       (lecture seule,     (pages
    AMO, Wikidata)       parsing,         vérité +         rapide, rendu       serveur)
                         calculs)         file de jobs)    de templates)
                              ▲                                  │
                              └───── création de job (INSERT) ────┘
```

Pas de Redis ni de RabbitMQ, et pas non plus de canal direct entre les deux processus : le backend et le
worker communiquent **uniquement par PostgreSQL** — file de jobs en `LISTEN`/`NOTIFY` + `SELECT … FOR
UPDATE SKIP LOCKED`, progression et présence lues en SQL. Le backend ne dépend donc que de la base.
Détails et justifications dans [docs/architecture.md](docs/architecture.md).

## Stack

| Brique | Choix |
| --- | --- |
| Backend web | Python 3.14, FastAPI, [uv](https://docs.astral.sh/uv/) |
| Qualité Python | [Ruff](https://docs.astral.sh/ruff/) (lint + format), [ty](https://github.com/astral-sh/ty) (typage) |
| Frontend | HTMX + Jinja2, rendu serveur, CSS minimal (pas de bundler) |
| Worker de données | Rust (tokio, sqlx) |
| Communication | PostgreSQL uniquement : `LISTEN`/`NOTIFY`, `FOR UPDATE SKIP LOCKED`, battement de cœur |
| Stockage | PostgreSQL 16+ (JSONB, `daterange`, vues matérialisées, FTS français) |
| Observabilité | [Logfire](https://logfire.pydantic.dev/) (offre gratuite), OpenTelemetry côté Rust |
| Hébergement | Render.com (cible principale) — voir [la note sur Vercel](docs/plans/phase-5-deploiement.md) |

## Démarrage rapide

Prérequis : [uv](https://docs.astral.sh/uv/), Rust (édition 2024) avec `sqlx-cli`
(`cargo install sqlx-cli --no-default-features --features postgres,rustls`), [Podman](https://podman.io/)
+ `podman-compose` (pour PostgreSQL local, via `podman compose`), `make`.

```
cp .env.example .env
make dev
```

`make dev` démarre PostgreSQL (`podman compose`, voir [compose.yaml](compose.yaml)), joue les migrations,
puis lance le worker et le backend. Le backend écoute sur <http://localhost:8000>.

Autres commandes utiles, voir aussi le [Makefile](Makefile) :

```
make lint        # ruff + clippy + fmt
make typecheck   # ty
make test        # pytest + cargo test
make migrate     # rejoue les migrations sur DATABASE_URL
```

Détails par composant : [backend/README.md](backend/README.md), [worker/README.md](worker/README.md),
[db/README.md](db/README.md).

## Documentation

| Document | Contenu |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | Composants, modèle de données, flux, décisions techniques |
| [docs/data-sources.md](docs/data-sources.md) | Sources open data, formats, licences, pièges connus |
| [docs/methodology.md](docs/methodology.md) | Comment on catégorise, ce qu'on refuse de faire, limites |
| [docs/plans/](docs/plans/) | Un plan par phase, à relire avant implémentation |
| [CLAUDE.md](CLAUDE.md) | Conventions et contexte pour le travail assisté par Claude Code |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Comment contribuer, notamment aux catégorisations |

## Contribuer

Les contributions les plus utiles ne sont pas forcément du code : relire et corriger la catégorisation
d'un scrutin a plus de valeur qu'une optimisation SQL. Voir [CONTRIBUTING.md](CONTRIBUTING.md).

## Licence et données

Code sous **AGPL-3.0** (voir [LICENSE](LICENSE)) : toute réutilisation, y compris via un service en ligne,
doit rester ouverte. C'est volontaire — un outil qui prétend à la transparence doit être auditable.

Les données sources restent sous leurs licences respectives (Licence Ouverte / Etalab pour l'Assemblée
nationale, CC0 pour Wikidata, licences par image pour Wikimedia Commons). Chaque donnée affichée est
accompagnée de sa source.

Ce projet n'est affilié à aucun parti, aucun candidat, aucune institution.
