# Plans

Le travail est découpé en phases. **Chaque phase a un plan qu'on relit et qu'on valide avant d'écrire la
moindre ligne de code.** C'est le seul processus imposé par le projet.

## Statut des phases

| Phase | Objet | Statut |
| --- | --- | --- |
| [0](phase-0-socle.md) | Socle technique : uv, FastAPI, Rust, PostgreSQL, file de jobs, CI | ✔️ terminé |
| [0.1](phase-0.1-fix.md) | Corrections du socle relevées à la revue de la phase 0 | ✅ validé |
| [1](phase-1-ingestion.md) | Ingestion des scrutins et des mandats depuis l'open data | ✅ validé |
| [2](phase-2-api-ui.md) | Pages publiques : liste des candidats, fiche candidat | 📝 à relire |
| [3](phase-3-categorisation.md) | Catégorisation : heuristique, back-office, export/import | 📝 à relire |
| [4](phase-4-partis-scores.md) | Scores par thème, positions de parti par période, explications | 📝 à relire |
| [5](phase-5-deploiement.md) | Déploiement, observabilité Logfire, sauvegardes | 📝 à relire |
| [6](phase-6-backlog-v2.md) | Backlog v2 : enquêtes, autres sources, comparaisons | 💭 idées |

Légende : 📝 à relire · ✅ validé · 🚧 en cours · ✔️ terminé · 💭 idées non engagées

## Ordre et dépendances

```
0 socle ──► 0.1 corrections ──► 1 ingestion ──► 2 pages publiques (données brutes visibles)
                  │
                  └──────► 3 catégorisation ──► 4 scores & explications ──► 5 déploiement
```

La phase 2 vient avant la catégorisation volontairement : voir les vraies données à l'écran tôt évite de
concevoir des scores sur des données mal comprises. À la fin de la phase 2, l'application est utile même
sans aucun score — elle montre déjà les votes réels d'une personne.

## Format d'un plan

Chaque plan suit la même trame :

1. **Statut et objectif** — ce que la phase apporte, en une phrase.
2. **Périmètre** — ce qui est dedans, et surtout ce qui est explicitement dehors.
3. **Livrables** — les artefacts concrets attendus.
4. **Étapes** — le découpage d'exécution.
5. **Décisions à trancher** — les questions ouvertes qui nécessitent un arbitrage *avant* le code. C'est
   la section à lire en priorité lors de la relecture.
6. **Fini quand** — critères vérifiables, pas d'appréciation subjective.
7. **Risques** — ce qui peut faire dérailler la phase.

Certains plans portent en plus une section **« ⚠️ À concevoir ici, pas ailleurs »** : un sujet qu'une phase
antérieure a délibérément laissé à l'état d'ébauche pour ne pas le figer par accident, et qui doit être
tranché au début de la phase concernée. Aujourd'hui : l'architecture front en [phase 2](phase-2-api-ui.md),
la stratégie de test de l'import en [phase 3](phase-3-categorisation.md).

Un plan est un document vivant : s'il s'avère faux au contact du code, on le corrige dans le commit qui le
contredit.
