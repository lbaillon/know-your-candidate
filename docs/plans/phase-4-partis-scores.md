# Phase 4 — Scores, positions de groupe et explications

**Statut : ✅ validé** · Décisions arbitrées le 19/08/2026 · Dépend de : phase 3 · Bloque : rien

## Objectif

Transformer les votes catégorisés en orientations lisibles — pour une personne et pour les groupes
parlementaires où elle a siégé — **avec l'explication attachée**. C'est la phase qui livre la promesse
du projet : voir qu'une personne est classée d'un côté d'un axe, cliquer, et lire les scrutins qui l'y
ont mise.

## Périmètre

**Dedans** : calcul des scores par personne et par thème, calcul des scores de groupe par période,
restriction aux périodes d'appartenance, table des contributions, affichage et explications.

**Dehors** : comparaison entre candidat·es, évolution dans le temps, sources hors Assemblée (phase 6),
et **toute notion de continuité entre partis** (D4.8).

## Ce que les données disent, et qui commande cette phase

Mesuré le 19/08/2026 sur la base réelle, à l'issue de la phase 3. Ces chiffres ont changé trois
décisions du plan initial.

| Fait | Conséquence |
| --- | --- |
| **149 scrutins** portent une position, répartis 35 / 42 / 66 sur les législatures 15, 16 et 17 | Corpus petit mais **temporellement équilibré** : on ne score pas une seule législature |
| Par thème : sécurité 31, social-fiscalité 29, environnement 21, agriculture 20, institutions 15, santé 12, travail 10, immigration 9, **Europe 2** | Le seuil de D4.2 protège la personne, pas le thème. D'où **D4.7**, un second seuil : un thème sous 10 scrutins ne s'affiche pour personne |
| **1 101 personnes** atteignent 5 contributions sur au moins un thème, **274** sur les huit ; 105 385 votes couverts | Il y a de quoi produire des scores réels, pas une démonstration |
| Candidat·es : Le Pen **120 contributions / 8 thèmes**, Attal 62 / 6, Mélenchon 36 / 4 | Trois fiches sur cinq auront quelque chose à montrer dès le premier calcul |
| **Retailleau : zéro mandat de groupe, un seul vote** — le Congrès sur l'IVG, en tant que sénateur, exclu du corpus. **Arthaud : rien** | Deux fiches sur cinq resteront vides, et le repli « positions du parti » ne les sauve pas (D4.8) |
| Les 236 catégorisations sont **toutes `import`, aucune relue** | D4.1 devient existentiel : appliqué tel qu'écrit, il produit un site qui n'affiche rien (voir D4.1) |
| `mandat.period` est un `daterange` avec index GiST et contrainte `EXCLUDE` ; 7 674 mandats de groupe sur 63 organes | La restriction d'un score de groupe à la période d'appartenance se calcule directement, sans rien ajouter au schéma de la phase 1 |

## Livrables

1. Migrations : `person_theme_score`, `groupe_theme_score`, `mandat_theme_score`,
   `score_contribution`, `score_run`, `score_parametre`, et la colonne `bipolarite` sur
   `scrutin_axis_estimate`.
2. Job `recompute_scores { scope }` : recalcul complet, idempotent, avec versionnage de la formule.
3. Vues matérialisées pour la lecture rapide, rafraîchies en `CONCURRENTLY`.
4. Fiche candidat enrichie : orientations par thème, positions des groupes sur les périodes concernées.
5. **Page d'explication** : pour un couple (personne, thème), la liste des scrutins contributeurs, leur
   poids, la position votée, et le lien vers le scrutin.
6. Affichage explicite du niveau de preuve : nombre de scrutins, part de catégorisations relues, mention
   « données insuffisantes » le cas échéant.
7. Extension de l'API `/api/v1/` aux scores et aux contributions, servie par les mêmes fonctions que
   les pages.

## Règles de calcul

Détaillées dans [methodology.md](../methodology.md#6-du-vote-au-score). Rappel des points structurants :

- score par thème = moyenne pondérée des contributions, projetée sur l'axe du thème ;
- pondération = confiance de la catégorisation × poids du thème dans le scrutin × informativité du
  scrutin (D4.9) ;
- non-votants exclus, **abstentions conservées, affichées, et de poids nul** (D4.10) ;
- **en dessous du seuil minimal de contributions, aucun score n'est affiché** ;
- score de groupe sur une période = calculé sur les votes de ses membres pendant cette période, puis
  restreint à la période d'appartenance de la personne consultée ;
- **le score personnel et le score de groupe ne sont jamais fusionnés.**

Chaque calcul enregistre la version de la formule utilisée. Quand la formule change, les scores changent :
il faut pouvoir dire pourquoi, et rejouer l'ancien calcul.

## L'explication est le produit

Une orientation affichée sans explication accessible en un clic est un défaut bloquant, pas une
imperfection. Forme cible :

> **Social / fiscalité — plutôt « maîtrise de la fiscalité »** (12 scrutins, 0 relu par un humain)
> *Notamment parce que :*
> · a voté **contre** la taxe Zucman, le JJ/MM/AAAA — [scrutin n° 4321](#)
> · a voté **contre** le rétablissement de l'ISF, le JJ/MM/AAAA — [scrutin n° 3987](#)
> · s'est **abstenu** sur … *(n'entre pas dans le calcul)*
> [Voir les 12 scrutins et le détail du calcul](#)

## Un fait mesuré en phase 3, à traiter ici : l'opposition bipolaire

La phase 3 a mesuré l'accord entre les catégorisations et l'estimation d'axe des groupes. Sur les
textes budgétaires, l'accord tombait à **29,3 %** — *sous* le hasard, donc une inversion
systématique. La cause n'est pas un défaut de calcul, elle est structurelle : sur un budget, ceux qui
votent pour sont la majorité gouvernementale, donc le centre, et ceux qui votent contre sont **les
deux extrêmes à la fois**. La moyenne d'axe du camp « contre » retombe vers le centre, et le signe
s'inverse.

Ce cas n'est pas propre aux budgets : il apparaît chaque fois qu'un texte est rejeté par la gauche
parce qu'il ne va pas assez loin et par la droite parce qu'il va trop loin — la loi climat de 2021 et
les textes « pouvoir d'achat » de 2022 en sont d'autres exemples relevés. **Un tel vote ne situe
personne sur un axe** : deux personnes aux positions opposées y votent identiquement.

Conséquence pour le calcul des scores : la pondération doit **dévaluer ces scrutins**, et
`scrutin_axis_estimate.separation` ne suffit pas à les détecter — elle mesure si un seuil sépare bien
les deux camps, pas si le camp minoritaire est idéologiquement homogène (F1,
[phase-3.0-feedback.md](phase-3.0-feedback.md)). La mesure manquante est définie en D4.9 et calculée
par le job de la phase 3, dans la même passe que la séparation.

## Le cas des candidat·es sans mandat parlementaire

C'est la limite la plus sérieuse du projet et elle doit être traitée frontalement en phase 4, pas
contournée. La mesure du 19/08/2026 la rend concrète : **deux candidat·es sur cinq** n'ont aucun vote
exploitable — Bruno Retailleau (sénateur ; son unique vote en base est celui du Congrès sur l'IVG,
hors corpus) et Nathalie Arthaud (jamais élue).

- **Ne jamais inventer un score personnel** à partir d'un groupe. La fiche affiche « aucun vote
  personnel disponible » et, séparément, les positions des groupes où la personne a **effectivement
  siégé**.
- Pour qui n'a jamais siégé, il n'y a donc rien à afficher, et c'est ce que la fiche dit — en toutes
  lettres, avec la raison. Le repli « positions du parti » supposerait de relier une personne à un
  parti hors Assemblée, ce que la phase 4 refuse explicitement (D4.8).
- L'écart entre score personnel et score de groupe est en soi une information intéressante : quand les
  deux existent, on l'affiche.
- Les autres sources de votes (Sénat, Parlement européen) sont renvoyées en
  [phase 6](phase-6-backlog-v2.md) — c'est la seule réponse honnête au cas Retailleau, et elle a un
  coût qui n'est pas celui de cette phase.

## Étapes

1. Modèle des scores et des contributions, et la mesure de bipolarité qui manque à la phase 3.
2. Implémentation du calcul dans le worker, avec des tests sur des cas construits à la main dont le
   résultat attendu est écrit à l'avance.
3. Vues matérialisées et stratégie de rafraîchissement.
4. Affichage sur la fiche candidat.
5. Page d'explication et détail du calcul.
6. Recette sur 3 à 5 personnalités connues : le résultat est-il défendable, et surtout, l'explication
   est-elle convaincante pour quelqu'un qui n'est pas d'accord avec le résultat ?
7. **Validation croisée de l'axe — `principal_axis`, reportée de la [phase 3](phase-3-categorisation.md)
   (D3.10).** Dès qu'au moins 200 scrutins sont relus par un humain : extraire l'axe principal de la
   matrice votants × scrutins et le confronter à l'ancrage des nuances du ministère de l'Intérieur. Deux
   méthodes indépendantes qui se contredisent sur un scrutin sont le meilleur signal de relecture qu'on
   ait, et c'est le seul contrôle qui puisse dire que l'ancrage officiel place un groupe au mauvais
   endroit. La colonne `scrutin_axis_estimate.strategy` l'attend sans migration ; la clé étrangère vers
   `group_axis` est à relâcher à ce moment-là. Si la phase 4 déborde, ce point part en
   [phase 6](phase-6-backlog-v2.md) — nommé, pas oublié.

## Décisions arbitrées

Toutes tranchées le 19/08/2026, avant le plan d'exécution. **Elles ne sont pas à rediscuter en cours
d'implémentation** : si l'une s'avère fausse au contact du code, le dire et proposer une révision.

| # | Question | Décision |
| --- | --- | --- |
| D4.1 | Les catégorisations non relues comptent-elles dans un score affiché publiquement ? | **Oui, étiquetées.** La question a changé de nature depuis la rédaction initiale : les 236 catégorisations en base sont **toutes** issues d'un modèle et **aucune n'est relue**, donc « non par défaut » ne signifie plus « peu de scores fiables » mais « aucun score du tout ». On affiche, et chaque orientation porte son nombre de scrutins **et sa part relue par un humain** — zéro aujourd'hui, et l'UI l'écrit. C'est exactement ce que methodology.md § 5.c prévoit : « le projet ne prétend pas qu'une catégorisation LLM vaut une relecture humaine ; il prétend qu'elle est traçable et corrigeable ». La relecture fait monter un indicateur visible, elle ne débloque pas un affichage |
| D4.2 | Seuil minimal de contributions par personne et par thème | **5**, comme proposé, désormais confirmé par la mesure : 1 101 personnes l'atteignent sur au moins un thème, 274 sur les huit. Paramètre en base (`score_parametre`), pas une constante |
| D4.3 | Représentation visuelle | **Curseur sur un axe**, avec les deux pôles nommés en toutes lettres de part et d'autre et un intervalle d'incertitude. Aucune métaphore de notation : pas d'étoiles, pas de note sur 10, pas de couleur « bon/mauvais ». Réemploi direct des règles d'apparence de D3.21 : piste non remplie, dégradé d'orientation, pôle écrit avant le chiffre |
| D4.4 | Score de groupe : tous les membres ou seulement les votes majoritaires ? | **Tous les membres**, avec un **indicateur de cohésion** = part des votes du groupe alignés sur sa position majoritaire, sur les scrutins du thème. Une majorité écrasante et une division à 51 % ne se valent pas, et la division est souvent l'information la plus intéressante |
| D4.5 | Rafraîchissement des scores | Job déclenché à la main depuis le back-office (liste blanche de la phase 3, à étendre à `recompute_scores`). Pas de planification en v1 : éviter les recalculs surprises |
| D4.6 | Historique des scores | **Une ligne `score_run` par exécution**, portant la version de formule, les paramètres et les compteurs ; les tables de score portent le `run_id` qui les a produites. On garde les runs, pas seulement le dernier : c'est ce qui permet d'expliquer qu'un chiffre a bougé |
| D4.7 | Seuil au niveau du thème | **10 scrutins catégorisés minimum** pour qu'un thème soit affiché, pour qui que ce soit. Aujourd'hui cela masque l'Europe (2) et l'immigration (9), et laisse passer les sept autres. Paramètre en base : il s'ouvre tout seul à mesure que la catégorisation avance. Sans lui, une orientation européenne reposerait sur deux textes, ce qu'aucune formulation prudente ne rattrape |
| D4.8 | Que score-t-on : des groupes ou des partis ? | **Les groupes parlementaires uniquement**, tels que l'Assemblée les publie (63 organes `GP`), sans aucune table de continuité entre eux. Affirmer que LaREM, Renaissance et Ensemble pour la République « sont le même parti » est un choix éditorial contestable que rien n'oblige à faire ici, et methodology.md § 2 désigne le groupe comme le lien à afficher en priorité « parce que c'est lui qui a un rapport avec les votes ». Coût assumé et écrit : le repli par le parti n'existe pas, donc les candidat·es n'ayant jamais siégé n'ont rien à afficher. La question des continuités part en [phase 6](phase-6-backlog-v2.md) avec sa condition de déclenchement : le jour où une source de votes hors Assemblée est ingérée, elle devient utile — pas avant |
| D4.9 | Comment détecter un scrutin qui ne situe personne ? | Une colonne **`bipolarite`** sur `scrutin_axis_estimate`, calculée dans la même passe que la séparation : part du camp « contre » située **de part et d'autre** du camp « pour ». Formellement `2 × min(g, d)` où `g` et `d` sont les parts du camp « contre » à gauche et à droite de la position moyenne du camp « pour ». Vaut 0 quand l'opposition est d'un seul bord, 1 quand elle est également répartie des deux. Elle entre dans la pondération en facteur `(1 − bipolarite)` : un scrutin rejeté par les deux extrêmes ne pèse rien, ce qui est exactement ce que la mesure de la phase 3 a démontré (F9) |
| D4.10 | Traitement de l'abstention | **Conservée, affichée, poids nul dans le calcul.** methodology.md § 3 laisse le choix entre « faible ou nulle » : on prend nulle, et voici pourquoi. Une abstention n'a pas de direction ; lui donner une position 0 tirerait mécaniquement tous les scores vers le centre, ce qui est une affirmation que la donnée ne soutient pas. Elle reste comptée et **listée dans l'explication**, avec la mention « n'entre pas dans le calcul » — le lecteur voit qu'elle existe et pourquoi elle ne compte pas |
| D4.11 | Pondération par type de scrutin | **Aucune en v1**, et methodology.md § 6 est amendée en conséquence (« la pondération *peut* dépendre du type »). Le plan initial la mentionnait ; nous n'avons aucun élément permettant d'affirmer qu'un scrutin solennel est plus révélateur qu'un scrutin ordinaire, et inventer ce coefficient serait exactement le geste que la phase 3 vient de payer cher (F9). Ce que le type apporterait est déjà capté par la bipolarité et la confiance |
| D4.12 | Score de groupe restreint à une période | Une table **`mandat_theme_score`**, une ligne par (mandat de groupe, thème) : le score du groupe calculé **sur la seule période du mandat de la personne consultée**. 7 674 mandats × 9 thèmes bornent la table à ~69 000 lignes, donc le calcul est précalculé et non fait à la volée — la règle d'architecture ne souffre pas d'exception ici. `groupe_theme_score` garde en parallèle le score du groupe sur toute son existence, pour la page du groupe |
| D4.13 | Que fait-on des scrutins étiquetés `autre` ? | **Rien : ils n'entrent dans aucun score**, par construction (pas de thème à axe, pas de position). Ils restent visibles sur la page du scrutin et comptent dans la couverture affichée. Un scrutin relu et déclaré hors axes est un travail fait, pas un trou |

## Plan d'exécution

Cette section est destinée à la session qui implémentera la phase. Elle est autoportante : **tout ce
qui est nécessaire est ici, dans [methodology.md](../methodology.md), dans
[architecture.md](../architecture.md), dans [phase-3-categorisation.md](phase-3-categorisation.md) ou
dans [CLAUDE.md](../../CLAUDE.md)**. Les décisions arbitrées ci-dessus ne sont pas à rediscuter.

Développement sur le tronc : **commits directs sur `main`**, `main` vert (lint, typage, tests) à
chaque commit.

**Langue** : identifiants en anglais sauf les termes du domaine (`scrutin`, `groupe`, `organe`,
`mandat`, `legislature`) et le vocabulaire méthodologique déjà en français dans le schéma
(`poids`, `confiance`, `justification`, `position_pour`, `separation`, `bipolarite`, `cohesion`).

### Ce que la phase 4 ne fait pas

Pas de table de partis ni de continuité entre groupes (D4.8). Pas de comparaison entre candidat·es.
Pas d'évolution d'un score dans le temps (les runs sont historisés, mais aucune courbe n'est
affichée). Pas de source de votes hors Assemblée. Pas de `principal_axis` tant que les 200 relectures
n'existent pas. Pas de recalcul automatique ni planifié (D4.5). Pas de couleur de parti — les seules
couleurs politiques du dépôt restent celles du curseur du back-office (D3.21), et elles n'en sortent
pas.

### Arborescence cible

```
db/
  migrations/0009_scores.sql        bipolarite, score_parametre, score_run, les quatre tables de score

worker/src/
  jobs/recompute_scores.rs          le calcul complet, idempotent
  scoring.rs                        fonctions pures : contribution, moyenne pondérée, cohésion
  axis.rs                           + la bipolarité, dans la même passe que la séparation

worker/tests/
  scoring.rs                        cas construits à la main, résultats écrits à l'avance
  recompute_scores.rs               intégration : idempotence, seuils, périodes

backend/src/kyc_api/
  queries/scores.py                 lectures des scores et des contributions
  schemas/score.py
  routers/pages.py                  + /personne/{slug}/theme/{slug}
  templates/macros/score.html.jinja  le curseur et sa légende
  templates/person_theme.html.jinja  la page d'explication

backend/tests/
  test_scores_pages.py  test_scores_api.py
```

### Migration `0009_scores.sql`

```sql
-- Phase 4 — scores : bipolarité des scrutins, paramètres, exécutions, et les quatre tables de
-- score (personne, groupe, mandat, contributions).
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. La mesure qui manquait à la phase 3 (D4.9) ---------------------------------------------------
--
-- `separation` dit si un seuil sépare bien les deux camps ; elle ne dit pas si le camp « contre »
-- est idéologiquement homogène. Sur un budget, l'opposition vient des deux extrêmes à la fois : le
-- vote ne situe personne, et la phase 3 l'a payé (F9, phase-3.0-feedback.md). `bipolarite` mesure
-- exactement cela, et entre dans la pondération en facteur (1 − bipolarite).
--
-- NULL est permis : les estimations calculées avant cette migration n'en ont pas, et le job les
-- recalculera. Un score ne se calcule que sur des scrutins dont la bipolarité est connue.

ALTER TABLE scrutin_axis_estimate
    ADD COLUMN bipolarite numeric(4,3) CHECK (bipolarite BETWEEN 0 AND 1);

-- 2. Les paramètres du calcul, en base et non en dur ------------------------------------------------
--
-- Même raison qu'en phase 3 pour `corpus_parametre` : déplacer un seuil est une décision éditoriale,
-- elle doit être visible et datée. `formula_version` est incrémentée à la main quand la formule
-- change — c'est elle qui permet de dire pourquoi un chiffre a bougé.

CREATE TABLE score_parametre (
    id                        boolean PRIMARY KEY DEFAULT true CHECK (id),
    contributions_min         smallint NOT NULL DEFAULT 5  CHECK (contributions_min > 0),
    scrutins_min_par_theme    smallint NOT NULL DEFAULT 10 CHECK (scrutins_min_par_theme > 0),
    formula_version           smallint NOT NULL DEFAULT 1,
    updated_at                timestamptz NOT NULL DEFAULT now(),
    updated_by                text
);

INSERT INTO score_parametre (id) VALUES (true);

-- 3. Une exécution de calcul (D4.6) -----------------------------------------------------------------

CREATE TABLE score_run (
    id                     bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    formula_version        smallint NOT NULL,
    contributions_min      smallint NOT NULL,
    scrutins_min_par_theme smallint NOT NULL,
    started_at             timestamptz NOT NULL DEFAULT now(),
    finished_at            timestamptz,
    counters               jsonb NOT NULL DEFAULT '{}'::jsonb,
    is_current             boolean NOT NULL DEFAULT false
);

CREATE UNIQUE INDEX score_run_courant_idx ON score_run (is_current) WHERE is_current;

-- 4. Les contributions — la table qui rend l'explication possible -----------------------------------
--
-- architecture.md § 3 : « score_contribution n'est pas un détail d'implémentation : c'est le
-- produit. Sans elle, on affiche un verdict sans preuve, ce que le projet refuse. »
--
-- `apport` est la position sur l'axe que CE vote attribue à la personne : +position_pour si elle a
-- voté pour, −position_pour si elle a voté contre, NULL pour une abstention (D4.10). `poids` est nul
-- pour une abstention : elle est listée, elle ne pèse pas.

CREATE TABLE score_contribution (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    person_id     bigint   NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    scrutin_id    bigint   NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    position      vote_position NOT NULL,
    apport        numeric(4,3) CHECK (apport BETWEEN -1 AND 1),
    poids         numeric(6,5) NOT NULL CHECK (poids >= 0),
    PRIMARY KEY (run_id, person_id, theme_id, scrutin_id),
    CONSTRAINT score_contribution_abstention_sans_apport
        CHECK ((position = 'abstention') = (apport IS NULL)),
    CONSTRAINT score_contribution_abstention_sans_poids
        CHECK (position <> 'abstention' OR poids = 0)
);

CREATE INDEX score_contribution_lecture_idx
    ON score_contribution (run_id, person_id, theme_id, scrutin_id);

-- 5. Les scores ---------------------------------------------------------------------------------------
--
-- `contributions` compte les votes qui pèsent ; `abstentions` celles qui ne pèsent pas mais qui sont
-- affichées ; `relues` la part des catégorisations sous-jacentes validées par un humain (D4.1) —
-- aujourd'hui zéro partout, et l'UI l'écrit.

CREATE TABLE person_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    person_id     bigint   NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    incertitude   numeric(4,3) NOT NULL CHECK (incertitude >= 0),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    abstentions   integer  NOT NULL DEFAULT 0 CHECK (abstentions >= 0),
    relues        integer  NOT NULL DEFAULT 0 CHECK (relues >= 0),
    PRIMARY KEY (run_id, person_id, theme_id)
);

CREATE TABLE groupe_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    organe_id     bigint   NOT NULL REFERENCES organe (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    cohesion      numeric(4,3) NOT NULL CHECK (cohesion BETWEEN 0 AND 1),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    membres       integer  NOT NULL CHECK (membres >= 0),
    PRIMARY KEY (run_id, organe_id, theme_id)
);

-- Le score du groupe restreint à la période d'appartenance de la personne (D4.12, methodology.md
-- § 6). C'est « tout l'intérêt des daterange » : le même groupe ne dit pas la même chose selon la
-- tranche de temps qu'on regarde.

CREATE TABLE mandat_theme_score (
    run_id        bigint   NOT NULL REFERENCES score_run (id) ON DELETE CASCADE,
    mandat_id     bigint   NOT NULL REFERENCES mandat (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    score         numeric(4,3) NOT NULL CHECK (score BETWEEN -1 AND 1),
    cohesion      numeric(4,3) NOT NULL CHECK (cohesion BETWEEN 0 AND 1),
    contributions integer  NOT NULL CHECK (contributions >= 0),
    PRIMARY KEY (run_id, mandat_id, theme_id)
);
```

Les vues matérialisées de lecture (`person_theme_score_courant`, `mandat_theme_score_courant`) ne
portent que le run courant, et sont rafraîchies `CONCURRENTLY` par le job — même piège qu'en phase 2 :
`REFRESH … CONCURRENTLY` ne s'exécute pas dans une transaction, les tests rafraîchissent donc sans.

### La formule, écrite une fois pour toutes

Pour une personne `p`, un thème `t`, un scrutin `s` catégorisé sur `t` :

```
apport(p, s, t)  = +position_pour(s, t)   si p a voté pour
                 = −position_pour(s, t)   si p a voté contre
                 = (rien)                 si p s'est abstenu·e   (D4.10)
                 = (exclu)                si p est non-votant·e  (methodology.md § 3)

poids(p, s, t)   = poids(s, t) × confiance(s, t) × (1 − bipolarite(s))

score(p, t)      = Σ apport × poids / Σ poids
incertitude(p,t) = écart-type pondéré des apports / √(nombre de contributions)
```

Quatre remarques, toutes déjà arbitrées mais qu'il faut avoir en tête en écrivant le code :

- **`apport` s'inverse avec le vote, pas la position du scrutin.** `position_pour` décrit ce que veut
  dire voter *pour* ; voter *contre*, c'est se placer à l'opposé. C'est l'erreur qu'on peut commettre
  sans que rien ne la signale : un test la cible explicitement.
- **Pas de facteur de type de scrutin** (D4.11) : il vaudrait 1 partout, autant ne pas l'écrire.
- **`bipolarite` à 1 annule le poids.** Un scrutin rejeté également par les deux bords ne contribue à
  rien. Un scrutin dont la bipolarité est inconnue (`NULL`) est **exclu**, pas traité comme 0 : on ne
  score pas sur une mesure absente.
- **L'incertitude n'est pas décorative** : c'est elle qui donne sa largeur à l'intervalle affiché
  autour du curseur (D4.3). Une personne dont les votes se contredisent sur un thème a un score
  central *et* un intervalle large ; les deux se lisent ensemble, jamais le chiffre seul.

Le score d'un groupe suit la même formule sur les votes de **tous** ses membres pendant la période
considérée (D4.4). La **cohésion** est calculée à part : part des votes des membres qui suivent la
position majoritaire du groupe sur les scrutins du thème, dans `[0, 1]`.

### Job `recompute_scores`

`{ "scope"?: { "theme"?: string, "person_uid"?: string } }` — le périmètre ne sert qu'au débogage :
**le calcul est toujours complet et repart de zéro** (architecture.md § 6, « même corpus + mêmes
règles = mêmes scores »).

1. Ouvre un `score_run` avec la version de formule et les seuils lus dans `score_parametre`.
2. Détermine les thèmes éligibles : au moins `scrutins_min_par_theme` scrutins catégorisés avec une
   position (D4.7). Les autres ne produisent **aucune ligne**, pour personne — ils ne sont pas
   calculés puis masqués à l'affichage, ils n'existent pas dans le run.
3. Balaye les votes des scrutins catégorisés — 105 385 lignes aujourd'hui, un ordre de grandeur en
   dessous de ce que la phase 1 traite —, écrit `score_contribution`, puis agrège en
   `person_theme_score` en écartant les couples sous `contributions_min`.
4. Agrège les scores de groupe (`groupe_theme_score`) et les scores restreints par mandat
   (`mandat_theme_score`), en filtrant les votes par `mandat.period @> scrutin.date_scrutin`.
5. Bascule `is_current` sur le nouveau run, rafraîchit les vues matérialisées, journalise les
   compteurs dans `score_run.counters` et dans `ingestion_run`.
6. **Ne supprime aucun run antérieur** : c'est l'historique de D4.6. Une purge éventuelle sera une
   décision séparée, prise quand le volume le justifiera.

Le calcul lui-même vit dans `worker/src/scoring.rs`, **fonctions pures testées seules**. Cas à écrire
avant le code :

- une personne qui vote systématiquement pour des textes à `position_pour` négative → score négatif
  franc, incertitude faible ;
- **la même personne, mais qui vote contre** → score positif de même amplitude. C'est le test qui
  attrape l'inversion de signe ;
- des votes contradictoires → score proche de 0 **et incertitude large** : vérifier que les deux
  sortent, pas seulement le score ;
- un scrutin à `bipolarite = 1` → poids nul, aucune influence ;
- un scrutin à `bipolarite` inconnue → exclu, et compté comme tel dans les compteurs du run ;
- une abstention → apparaît en contribution, poids nul, n'entre pas dans la moyenne, et le compteur
  `abstentions` l'enregistre ;
- un non-votant → n'apparaît nulle part ;
- exactement `contributions_min − 1` contributions → aucune ligne de score ;
- un mandat de groupe d'un seul jour → le score de mandat ne prend que les scrutins de ce jour ;
- un groupe non-inscrit (`organe.is_non_inscrit`) → **aucun score de groupe** (methodology.md § 2 :
  n'appartenir à aucun groupe n'est pas appartenir au groupe des sans-groupe).

### Pages et affichage

| Route | Rendu |
| --- | --- |
| `GET /personne/{slug}` | **enrichie** : bloc « orientations », un curseur par thème éligible |
| `GET /personne/{slug}/theme/{slug}` | La page d'explication : tous les scrutins contributeurs, leur poids, la position votée, le lien vers le scrutin |
| `GET /groupe/{an_uid}` | Le score du groupe par thème et sa cohésion |
| `GET /api/v1/personnes/{slug}/scores` | Les mêmes valeurs que la fiche |
| `GET /api/v1/personnes/{slug}/themes/{slug}/contributions` | Les mêmes valeurs que la page d'explication |

Règles d'affichage, non négociables et testées :

- **le pôle est nommé avant le chiffre**, toujours : « plutôt *maîtrise de la dépense et de la
  fiscalité* (+0,42) », jamais « +0,42 » seul ;
- **chaque orientation porte son niveau de preuve** : nombre de scrutins, nombre d'abstentions
  écartées, et **part relue par un humain**. Quand elle vaut zéro, la phrase le dit : « aucune de ces
  catégorisations n'a encore été relue par un humain » ;
- **en dessous du seuil, on écrit « données insuffisantes »** et on donne le compte réel — pas de
  case vide, pas de zéro affiché comme un score ;
- une personne sans aucun vote exploitable voit une phrase qui **dit pourquoi** : jamais siégé à
  l'Assemblée, ou n'a siégé qu'avant le corpus. Le cas Retailleau est le cas de test : son unique
  vote est celui du Congrès, et la fiche doit l'expliquer plutôt que d'afficher un vide ;
- **le score personnel et le score de groupe ne sont jamais dans le même curseur** : deux lignes
  distinctes, deux libellés distincts, et l'écart entre les deux est commenté quand les deux existent ;
- la page d'explication liste **toutes** les contributions, pas les cinq premières — c'est la preuve,
  elle ne se tronque pas. La fiche, elle, en montre trois et renvoie vers la page.

### Stratégie de test

**Niveau 1 — le calcul, sans base** (`worker/tests/scoring.rs`) : la liste de cas de la section
précédente, chacun avec son résultat écrit à l'avance. C'est le niveau qui compte le plus : une
erreur de signe ou de pondération y est visible, alors qu'elle est indétectable à l'écran.

**Niveau 2 — le job, sur base jetable** (`worker/tests/recompute_scores.rs`) : idempotence (deux
exécutions consécutives donnent des scores identiques — critère de « Fini quand »), respect des deux
seuils, restriction par période de mandat, exclusion des non-inscrits, bascule de `is_current`.

**Niveau 3 — les pages** (`backend/tests/test_scores_pages.py`) : le pôle nommé avant le chiffre, le
niveau de preuve présent sur chaque orientation, « données insuffisantes » sous le seuil, la page
d'explication qui liste toutes les contributions, l'égalité page/API sur les mêmes valeurs (le test
qui empêche les deux surfaces de diverger, comme en phase 2).

**Un test de recette documenté**, distinct des précédents : un jeu de données construit à la main
reproduisant **le cas PS → LFI** (deux mandats de groupe successifs, des scrutins dans chaque
période), avec les deux scores de mandat attendus écrits dans le test. C'est le cas du cahier des
charges ; il mérite son test nommé.

### Mesures à produire, pas à estimer

À exécuter sur la base réelle et à consigner dans ce plan et dans le message du commit final :

1. durée du job `recompute_scores` sur le corpus complet, et volume écrit (contributions, scores) ;
2. **nombre de personnes affichant au moins une orientation**, et distribution du nombre de thèmes
   par personne — à comparer à la projection de ce plan (1 101 personnes sur au moins un thème) ;
3. les scores des trois candidat·es qui en ont, **relus à la main** : sont-ils défendables ? C'est la
   recette, et elle ne se délègue pas à une mesure ;
4. `EXPLAIN (ANALYZE, BUFFERS)` sur la fiche personne enrichie et sur la page d'explication ;
5. p50/p95 de rendu des deux routes — le seuil de 100 ms de la phase 2 s'applique.

### Ordre des commits

1. **Migration `0009_scores.sql`** seule, avec ses tests de contraintes.
2. **`bipolarite`** : calcul dans `axis.rs`, écriture par `label_scrutins_heuristic`, tests purs, et
   la mesure sur le corpus réel (combien de scrutins au-dessus de 0,5 ?) dans le message de commit.
3. **`scoring.rs`** : les fonctions pures et toute leur liste de cas. Aucun accès base dans ce commit.
4. **Job `recompute_scores`** et ses tests d'intégration, `make ingest` complété, liste blanche du
   back-office étendue (D4.5).
5. **Vues matérialisées** et lectures backend (`queries/scores.py`, `schemas/score.py`).
6. **Fiche personne enrichie** : curseurs, niveau de preuve, états vides expliqués.
7. **Page d'explication** et test de recette PS → LFI.
8. **Scores de groupe** sur la fiche et page `/groupe/{an_uid}`.
9. **API** et test d'égalité page/API.
10. **Passe finale** : recette sur les candidat·es, mesures consignées, mise à jour de
    [methodology.md](../methodology.md) § 6 (pondération par type — D4.11 ; abstention de poids nul —
    D4.10) et de ce plan si une décision a bougé.

### Vérifications avant de déclarer la phase terminée

1. `make lint`, `make typecheck`, `make test` verts ; CI verte.
2. Deux exécutions consécutives de `recompute_scores` produisent des scores **identiques**, vérifié
   par empreinte sur la table entière et pas par sondage.
3. Le cas PS → LFI s'affiche correctement, avec le bon score de groupe sur chaque période.
4. **Recette à la main sur les trois candidat·es qui ont des votes** : le résultat est-il défendable,
   et l'explication est-elle convaincante pour quelqu'un qui n'est pas d'accord ? Consigner les trois
   réponses, y compris les désaccords.
5. La fiche de Bruno Retailleau et celle de Nathalie Arthaud disent **pourquoi** elles sont vides.
6. Aucune orientation affichée sans son niveau de preuve, vérifié par un test sur toutes les routes.
7. Un thème sous le seuil de 10 scrutins n'apparaît nulle part — aujourd'hui, ni Europe ni
   immigration.
8. Les mesures sont consignées, et les pages sous les 100 ms.

### Hors périmètre — ne pas ajouter

Pas de comparateur entre candidat·es. Pas de courbe d'évolution. Pas de table de partis (D4.8). Pas de
score sur les scrutins `autre`. Pas de recalcul planifié. Pas de source hors Assemblée. Pas de
`principal_axis` avant 200 relectures. Pas de couleur de parti sur une page publique.

En cas de doute sur ce qu'on a le droit d'afficher ou d'en déduire : demander, ne pas deviner.

## Fini quand

- Pour une personne ayant assez de votes, chaque thème éligible affiche une orientation, son intervalle
  d'incertitude, son niveau de preuve et l'explication associée.
- Le cas PS 2015-2018 → LFI 2018-2026 s'affiche correctement, avec les positions de chaque groupe sur la
  bonne période.
- Une personne sans vote personnel a une fiche cohérente et honnête, **qui dit pourquoi elle est vide** —
  pas une page à trous, pas un score inventé.
- Le recalcul complet est reproductible : deux exécutions consécutives donnent des scores identiques.
- Quelqu'un qui conteste un résultat peut, en trois clics, voir exactement ce qui l'a produit — et
  **toutes** les contributions, pas un extrait.
- Aucune orientation n'est affichée sans le nombre de scrutins qui la soutiennent et la part relue par
  un humain, fût-elle nulle.

## Risques

- **Faux sentiment de précision.** Un curseur bien dessiné donne une impression de rigueur que les
  données ne soutiennent pas toujours. Afficher l'incertitude, pas seulement la valeur — et rappeler
  que 149 scrutins catégorisés, ce n'est pas un corpus abondant.
- **Zéro relecture humaine.** C'est le risque nouveau, et le plus sérieux : la phase 4 va publier des
  orientations fondées sur des catégorisations produites par un modèle, qu'aucun humain n'a validées
  (D4.1). L'étiquetage est la seule protection, il doit être visible partout et non relégué en bas de
  page. Une campagne de relecture reste le meilleur investissement possible sur ce projet.
- **Instabilité des scores** au fil des ingestions et des recatégorisations : c'est attendu et sain, mais
  déroutant. D'où l'historique des runs et l'affichage de la date de calcul.
- **Récupération politique** des chiffres sortis de leur contexte. On ne peut pas l'empêcher ; on peut
  rendre le contexte inséparable du chiffre, y compris dans l'API.
- **Deux candidat·es sur cinq sans rien à afficher.** Le produit sera visiblement incomplet là où on
  l'attend le plus. C'est honnête, ce n'est pas satisfaisant, et la seule vraie réponse — ingérer le
  Sénat et le Parlement européen — est en phase 6.
