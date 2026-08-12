# Phase 4 — Scores, positions de parti et explications

**Statut : 📝 à relire** · Dépend de : phase 3 · Bloque : rien

## Objectif

Transformer les votes catégorisés en orientations lisibles — pour une personne et pour ses partis
successifs — **avec l'explication attachée**. C'est la phase qui livre la promesse du projet : voir qu'une
personne est classée d'un côté d'un axe, cliquer, et lire les scrutins qui l'y ont mise.

## Périmètre

**Dedans** : calcul des scores par personne et par thème, calcul des scores de parti par période,
restriction aux périodes d'appartenance, table des contributions, affichage et explications.

**Dehors** : comparaison entre candidat·es, évolution dans le temps, sources hors Assemblée (phase 6).

## Livrables

1. Migrations : `person_theme_score`, `party_theme_score` (avec période), `score_contribution`.
2. Job `recompute_scores { scope }` : recalcul complet, idempotent, avec versionnage de la formule.
3. Vues matérialisées pour la lecture rapide, rafraîchies en `CONCURRENTLY`.
4. Fiche candidat enrichie : orientations par thème, positions des partis sur les périodes concernées.
5. **Page d'explication** : pour un couple (personne, thème), la liste des scrutins contributeurs, leur
   poids, la position votée, et le lien vers le scrutin.
6. Affichage explicite du niveau de preuve : nombre de scrutins, part de catégorisations relues, mention
   « données insuffisantes » le cas échéant.

## Règles de calcul

Détaillées dans [methodology.md](../methodology.md#6-du-vote-au-score). Rappel des points structurants :

- score par thème = moyenne pondérée des contributions, projetée sur l'axe du thème ;
- pondération = type de scrutin × position (abstention affaiblie) × confiance de la catégorisation ;
- non-votants exclus, abstentions conservées et signalées ;
- **en dessous du seuil minimal de contributions, aucun score n'est affiché** ;
- score de parti sur une période = calculé sur les votes de ses membres pendant cette période, puis
  restreint à la période d'appartenance de la personne consultée ;
- **le score personnel et le score de parti ne sont jamais fusionnés.**

Chaque calcul enregistre la version de la formule utilisée. Quand la formule change, les scores changent :
il faut pouvoir dire pourquoi, et rejouer l'ancien calcul.

## L'explication est le produit

Une orientation affichée sans explication accessible en un clic est un défaut bloquant, pas une
imperfection. Forme cible :

> **Social / fiscalité — plutôt « maîtrise de la fiscalité »** (12 scrutins, 9 relus par un humain)
> *Notamment parce que :*
> · a voté **contre** la taxe Zucman, le JJ/MM/AAAA — [scrutin n° 4321](#)
> · a voté **contre** le rétablissement de l'ISF, le JJ/MM/AAAA — [scrutin n° 3987](#)
> · s'est **abstenu** sur … *(pondération réduite)*
> [Voir les 12 scrutins et le détail du calcul](#)

## Le cas des candidat·es sans mandat parlementaire

C'est la limite la plus sérieuse du projet et elle doit être traitée frontalement en phase 4, pas
contournée.

- **Ne jamais inventer un score personnel** à partir du parti. La fiche affiche « aucun vote personnel
  disponible » et, séparément, les positions du ou des partis auxquels la personne a appartenu.
- L'écart entre score personnel et score de parti est en soi une information intéressante : quand les deux
  existent, on l'affiche.
- Les autres sources de votes (Sénat, Parlement européen) sont renvoyées en [phase 6](phase-6-backlog-v2.md).

## Étapes

1. Modèle des scores et des contributions.
2. Implémentation du calcul dans le worker, avec des tests sur des cas construits à la main dont le
   résultat attendu est écrit à l'avance.
3. Vues matérialisées et stratégie de rafraîchissement.
4. Affichage sur la fiche candidat.
5. Page d'explication et détail du calcul.
6. Recette sur 3 à 5 personnalités connues : le résultat est-il défendable, et surtout, l'explication
   est-elle convaincante pour quelqu'un qui n'est pas d'accord avec le résultat ?

## Décisions à trancher

| # | Question | Proposition |
| --- | --- | --- |
| D4.1 | Les catégorisations `heuristic` non relues comptent-elles dans un score affiché publiquement ? | **Non par défaut.** Un basculement d'affichage permet de les inclure, avec un avertissement visible. Mieux vaut peu de scores fiables que beaucoup de scores douteux |
| D4.2 | Seuil minimal de contributions | Proposition : 5 scrutins par thème, à réajuster une fois le volume réel connu |
| D4.3 | Représentation visuelle | Curseur sur un axe avec intervalle d'incertitude, plutôt qu'une note ou une étoile. Aucune métaphore de notation |
| D4.4 | Score de parti : tous les membres ou seulement les votes majoritaires ? | **Tous les membres**, avec un indicateur de cohésion. Une majorité écrasante et une division à 51 % ne se valent pas |
| D4.5 | Rafraîchissement des scores | Job déclenché à la main en v1, planifié plus tard. Éviter les recalculs surprises |
| D4.6 | Historique des scores | Conserver les résultats de chaque exécution pour pouvoir expliquer un changement d'affichage |

## Fini quand

- Pour une personne ayant assez de votes, chaque thème affiche une orientation et l'explication associée.
- Le cas PS 2015-2018 → LFI 2018-2026 s'affiche correctement, avec les positions de chaque parti sur la
  bonne période.
- Une personne sans vote personnel a une fiche cohérente et honnête, pas une page vide ni un score inventé.
- Le recalcul complet est reproductible : deux exécutions consécutives donnent des scores identiques.
- Quelqu'un qui conteste un résultat peut, en trois clics, voir exactement ce qui l'a produit.

## Risques

- **Faux sentiment de précision.** Un curseur bien dessiné donne une impression de rigueur que les données
  ne soutiennent pas toujours. Afficher l'incertitude, pas seulement la valeur.
- **Instabilité des scores** au fil des ingestions et des recatégorisations : c'est attendu et sain, mais
  déroutant. D'où l'historique et l'affichage de la date de calcul.
- **Récupération politique** des chiffres sortis de leur contexte. On ne peut pas l'empêcher ; on peut
  rendre le contexte inséparable du chiffre, y compris dans l'API.
