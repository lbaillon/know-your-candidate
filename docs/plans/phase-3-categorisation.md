# Phase 3 — Catégorisation des scrutins

**Statut : 📝 à relire** · Dépend de : phases 1 et 2 · Bloque : phase 4

## Objectif

Pouvoir rattacher chaque scrutin à un ou plusieurs thèmes et à un pôle, par trois voies complémentaires :
une première passe automatique, une saisie/correction humaine, et un cycle export → travail hors ligne →
import. Le tout historisé et traçable.

C'est la phase la plus délicate du projet : c'est là que se joue sa crédibilité.

## Périmètre

**Dedans** : modèle des thèmes et des catégorisations, heuristique automatique, back-office admin,
export/import, historique des révisions, consultation publique des catégorisations.

**Dehors** : le calcul des scores et leur affichage sur la fiche (phase 4), l'appel à un LLM depuis
l'application (volontairement : le travail LLM se fait **hors ligne**, sur un export).

## Livrables

1. Migrations : `theme`, `theme_axis` (les deux pôles nommés), `scrutin_label`, `label_revision`.
2. Job `label_scrutins_heuristic { strategy, scope }` : première passe automatique.
3. Back-office `/admin` : file des scrutins non catégorisés, formulaire de catégorisation, correction,
   historique.
4. Export : `GET /admin/export?status=uncategorized&format=csv|json` avec un schéma stable et documenté.
5. Import : dépôt de fichier, **validation stricte**, prévisualisation des changements (créations,
   modifications, conflits), application transactionnelle, rapport.
6. Pages publiques : liste des scrutins catégorisés, détail d'une catégorisation avec sa méthode, son
   auteur et son historique.
7. Authentification admin (voir décisions).
8. **Suivi des jobs dans le back-office** : reprise du fragment de progression HTMX écrit en phase 0 et
   retiré des routes publiques en phase 2. C'est ici qu'il trouve son vrai usage — suivre un import de
   catégorisations ou une ingestion, derrière l'authentification.
9. **Déclenchement du job `noop` réservé aux admins** : il est conservé comme test de fumée en production
   (« le worker traite-t-il réellement quelque chose ? »), mais n'est déclenchable que depuis le
   back-office authentifié. Jamais de route publique de création de job, à aucun moment.

## L'heuristique automatique

Objectif : dégrossir. Deux stratégies décrites dans [methodology.md](../methodology.md#5-les-trois-méthodes-de-catégorisation) :

- `group_alignment` — position par rapport à un ordonnancement gauche-droite des groupes parlementaires,
  lu depuis un **fichier de configuration versionné** (`db/seeds/group_axis.yaml`).

  Cet ordonnancement n'est pas inventé : il est **dérivé de la grille des nuances politiques du ministère
  de l'Intérieur** (26 nuances, 6 blocs — voir [data-sources.md](../data-sources.md#4-ministère-de-lintérieur--grille-des-nuances-politiques)).
  Chaque ligne du seed porte donc la nuance officielle, la version de la grille, sa date et l'URL de la
  source, plus le rattachement nuance → parti → groupe parlementaire que nous établissons.

  On garde un fichier plutôt qu'une ingestion directe pour trois raisons : la grille change à chaque
  élection et n'est pas versionnée de façon exploitable ; elle nuance des candidat·es, pas des groupes,
  la jointure est donc de toute façon notre travail ; et c'est une **décision de l'exécutif classant ses
  propres opposants**, qu'on cite en la datant mais à laquelle on ne délègue pas nos conclusions. Un
  désaccord argumenté se règle par une PR sur ce fichier, visible de tous.

  Le passage à un ancrage dynamique (ingestion de la grille et du RNE, gestion des versions successives)
  est prévu en [phase 6](phase-6-backlog-v2.md).
- `principal_axis` — axe principal extrait de la matrice votants × scrutins, sans postulat de départ, à
  nommer ensuite par un humain.

Proposition : implémenter `group_alignment` d'abord (simple, explicable, contestable ligne à ligne), et
garder `principal_axis` comme validation croisée. Deux méthodes indépendantes qui se contredisent sur un
scrutin, c'est un excellent signal pour prioriser la relecture humaine.

Dans tous les cas, ces catégorisations sont marquées `heuristic` et affichées comme non relues.

## Le cycle export / import

C'est le mécanisme prévu pour catégoriser en masse avec l'aide d'un LLM, sans que l'application ne
dépende d'un LLM :

```
export CSV/JSON  ──►  travail hors ligne (LLM, tableur, plusieurs relecteurs)  ──►  import validé
```

Exigences :

- schéma d'export **versionné** (colonne `schema_version`), avec l'identifiant du scrutin, son titre, sa
  date, son résultat et un extrait de contexte suffisant pour décider ;
- import qui refuse un fichier ambigu plutôt que de deviner ;
- **prévisualisation obligatoire** avant application : combien de créations, de modifications, de
  conflits avec des saisies humaines plus récentes ;
- règle de conflit explicite : une catégorisation `manual` relue n'est jamais écrasée par un import sans
  confirmation ;
- chaque ligne importée conserve sa provenance (`import`, nom du fichier, date, auteur) et, si connu, le
  modèle utilisé.

## ⚠️ À concevoir ici, pas ailleurs : la stratégie de test de l'import

C'est **la cible de test la plus rentable du projet**, et elle doit être définie avant d'écrire l'import.
Un bug dans la validation ou dans la règle de conflit ne provoque pas d'erreur visible : il détruit
silencieusement des heures de relecture humaine, qui ne sont pas régénérables comme le sont les données
ingérées.

À couvrir explicitement, avec des cas écrits avant le code :

- fichier malformé, colonnes manquantes, `schema_version` inconnue, identifiants de scrutin inexistants,
  thèmes inconnus, valeurs hors bornes → **refus propre**, jamais d'interprétation approximative ;
- doublons dans le fichier importé, et lignes contradictoires entre elles ;
- **conflit avec une catégorisation `manual` plus récente** → non écrasée sans confirmation. C'est la
  règle dont la violation coûterait le plus cher ;
- import interrompu en cours → transaction annulée entièrement, aucun état intermédiaire en base ;
- aller-retour complet : export → réimport à l'identique → **aucune modification en base**, et aucune
  ligne d'historique créée ;
- l'historique reflète exactement ce qui a changé, ni plus ni moins.

À décider au moment de détailler la phase : jusqu'où on va sur les propriétés de l'aller-retour, et si on
teste l'ergonomie du back-office autrement qu'à la main.

## Étapes

1. Modèle des thèmes et des axes, alimenté par un seed versionné.
2. Authentification admin et journalisation des actions.
3. Back-office de saisie : la file de travail doit être efficace au clavier, elle sera utilisée des
   centaines de fois.
4. Heuristique `group_alignment` + seed de l'ordonnancement des groupes, construit à partir de la grille
   des nuances du ministère de l'Intérieur, source et date citées ligne par ligne.
5. Export, puis import avec prévisualisation.
6. Historique et pages publiques de consultation.
7. Recette : catégoriser à la main une trentaine de scrutins marquants, dont la taxe Zucman, pour valider
   que le modèle tient sur des cas réels.

## Décisions à trancher

| # | Question | Proposition |
| --- | --- | --- |
| D3.1 | Authentification admin | Compte unique avec mot de passe fort en variable d'environnement + session signée, ou OAuth GitHub. **OAuth GitHub** semble préférable : plusieurs relecteurs à terme, pas de mot de passe à gérer |
| D3.2 | Un scrutin peut-il porter plusieurs thèmes ? | **Oui**, avec un poids par thème (un budget touche à tout). Contrainte : la somme des poids vaut 1 |
| D3.3 | Pôle binaire ou position continue ? | **Position continue** dans `[-1, 1]` avec une confiance séparée : plus expressif, et l'affichage peut toujours arrondir |
| D3.4 | Qui peut catégoriser ? | v1 : admins seulement. Les contributions extérieures passent par des issues. Une modération ouverte est un projet en soi |
| D3.5 | Que faire des scrutins hors thèmes ? | Un thème `autre` explicite, pour distinguer « non catégorisé » de « ne relève d'aucun thème » |
| D3.6 | Contexte fourni dans l'export | Titre + type + résultat suffisent-ils pour un LLM ? Sinon il faut récupérer le texte du dossier législatif, ce qui est une ingestion supplémentaire à chiffrer |

## Fini quand

- Un admin peut catégoriser 30 scrutins en moins de 15 minutes sans quitter le clavier.
- Un export retraité hors ligne se réimporte sans perte, avec prévisualisation, et l'historique montre qui
  a changé quoi.
- Un import ne peut pas écraser silencieusement une catégorisation humaine.
- La page publique d'un scrutin affiche sa catégorisation, sa méthode et son historique.
- L'heuristique tourne sur l'ensemble du corpus et son taux d'accord avec les catégorisations humaines
  est mesuré et documenté — même s'il est mauvais, surtout s'il est mauvais.

## Risques

- **Biais de l'heuristique.** Elle produira des résultats plausibles et faux. D'où l'affichage systématique
  de la méthode et le refus de compter les catégorisations non relues dans les scores affichés (à
  trancher en phase 4).
- **Volume de travail humain.** Des centaines de scrutins à relire. L'ergonomie du back-office n'est pas
  du confort, c'est ce qui rend le projet faisable.
- **Dérive éditoriale.** Le pouvoir de catégoriser est le pouvoir de conclure. Historique public,
  justification obligatoire et méthodologie écrite sont les seuls garde-fous.
