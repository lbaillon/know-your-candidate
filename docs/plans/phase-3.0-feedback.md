# Phase 3.0 — Retours d'implémentation du lot A

**Statut : ✅ consigné** · Dépend de : [phase 3](phase-3-categorisation.md), lot A · Ne bloque rien, sauf
F2 qui bloque `make ingest` tant qu'elle n'est pas résolue.

## Objectif

Ce n'est pas une phase de correction comme [0.1](phase-0.1-fix.md) ou [2.1](phase-2.1-fix.md) : le lot A
n'a pas de défaut caché à corriger, il tient (`make lint`, `make typecheck`, `make test` verts à chaque
commit). Ce document consigne **deux points où l'implémentation a rencontré une limite du plan lui-même**,
comme demandé par [CLAUDE.md](../../CLAUDE.md) (« si un plan s'avère faux au contact du code, dis-le et
propose une révision — ne contourne pas silencieusement »). Autoportant comme les autres plans : tout ce
qui est nécessaire est ici.

## F1 — La séparation du scrutin 17/2653 est élevée, pas médiocre

**Où** : [phase-3-categorisation.md](phase-3-categorisation.md), section « L'heuristique automatique » et
D3.9. Corrigé dans le commit *Ancrage gauche-droite et job label_scrutins_heuristic*, test
`scrutin_17_2653_isolated_rn_udr_gives_high_separation_not_poor` (`worker/tests/axis.rs`).

**Ce que le plan disait.** Le scrutin 17/2653 (RN et UDR pour, tout le reste contre, de LFI à EPR) est
présenté comme le cas qui justifie la colonne `separation` : « la direction sortie est juste, la
séparation est mauvaise ». Le test à écrire à l'avance, listé dans la section algorithme, portait
l'attente explicite : « scrutin 17/2653 → position positive, séparation médiocre ».

**Ce que l'algorithme, tel qu'arbitré (D3.9), produit réellement.** `separation` cherche, parmi tous les
seuils possibles entre deux coordonnées de groupe consécutives, celui qui classe le plus de votants
correctement (camp majoritaire de chaque côté du seuil), et transforme ce taux en `max(0, 2a − 1)`. Pour
17/2653, le seuil placé juste entre RN/UDR et le reste de l'hémicycle classe la quasi-totalité des
votants : RN+UDR sont unanimement pour, et le reste — bien qu'idéologiquement hétérogène — est unanime
dans son vote contre. Un test construit sur ce cas (six groupes, RN et UDR isolés à l'extrême droite,
quatre autres groupes couvrant le reste du spectre, tous contre) donne une séparation de 1,0, pas
médiocre.

**Pourquoi c'est le comportement correct, pas un bug.** Le rôle de `separation` est de dire si l'axe
*explique* le partage pour/contre, pas si le camp contre est idéologiquement homogène. Sur ce scrutin,
l'axe l'explique très bien : c'est justement un vote où seule l'extrême droite s'est isolée. Le vrai
problème que la section « Ce que les données disent » voulait pointer — `position_pour` sous-estime à
quel point le camp pour est extrême, parce que `μ−` (moyenne du camp contre) est proche de zéro par
annulation des extrêmes — reste réel, mais ce n'est pas ce que `separation` mesure. Une recherche de seuil
libre récompense structurellement l'isolement d'un petit camp homogène à une extrémité ; c'est une
propriété du calcul tel qu'écrit, pas un biais de l'implémentation.

**Décision.** Implémenter l'algorithme exactement comme spécifié (recherche du meilleur seuil, meilleure
des deux orientations) et corriger le récit plutôt que de plier le calcul pour forcer un résultat
présupposé. Le test documente cette correction en toutes lettres ; aucune ligne du schéma ou de la
migration n'a changé.

## F2 — La grille des nuances politiques n'a pas pu être vérifiée cette session

**Où** : [phase-3-categorisation.md](phase-3-categorisation.md), section « Le seed d'ancrage » ;
`db/seeds/group_axis.toml`.

**Ce qui a été vérifié.** Les 40 `an_uid` et libellés des organes `GP` des législatures 15 à 17
(`is_non_inscrit = false`) se lisent en base et sont recopiés tels quels — un de plus que les 39 comptés
par le plan au moment de l'arbitrage (18/08/2026), écart à noter, pas une erreur du fichier.

**Ce qui n'a pas pu l'être.** Une recherche a été menée pour la grille 2026 à 26 nuances / 6 blocs citée
par [data-sources.md](../data-sources.md#4-ministère-de-lintérieur--grille-des-nuances-politiques) :

- une circulaire du ministère de l'Intérieur du 02/02/2026 (NOR INTP2602966C, Légifrance) existe pour les
  municipales 2026 et détaille 26 nuances en 6 blocs, mais son contenu chiffré est dans un PDF que l'outil
  de lecture web disponible n'a pas su extraire ;
- la grille voisine des législatives 2024 (24 nuances, instruction du 27/06/2024, Légifrance
  `id/45565`) a bien été retrouvée, avec quelques codes confirmés par des sources secondaires (`FI`,
  `LUG`, `LREN`…) — mais ni sa version ni ses codes ne sont garantis identiques à la grille 2026, et le
  rattachement nuance → groupe parlementaire (que le ministère ne publie qu'au niveau des candidat·es,
  jamais des groupes) restait de toute façon un travail à faire, pas à recopier.

**Décision**, conforme au plan (« un fichier de source à moitié inventé est pire qu'un fichier
incomplet ») : `db/seeds/group_axis.toml` porte les 40 lignes avec `an_uid`/`libelle` renseignés et tous
les champs de source (`version`, `grille_version`, `grille_date`, `source_url`, `nuance`, `bloc` par
groupe) vides, sous un bandeau `TODO` explicite. Le job `label_scrutins_heuristic` refuse de charger ce
fichier tant que ces champs sont vides — vérifié à la main (`cargo run -- enqueue
label_scrutins_heuristic` puis `run-once` échoue avec un message nommant le champ manquant).

**Conséquence sur le lot A.** Les mesures 1 et 2 du plan (durée du job sur le corpus réel, distribution de
la séparation) ne peuvent pas être produites tant que ce fichier n'est pas complété — elles ne sont donc
pas consignées dans le commit du lot A. `make ingest` échoue à cette étape en l'état, ce qui est le
comportement voulu plutôt qu'un `make ingest` vert sur une donnée inventée.

**Travail restant**, dans l'ordre du bandeau TODO du fichier :

1. retrouver la publication officielle de la grille des nuances 2026 (26 nuances, 6 blocs) — version
   exacte, date, URL ;
2. renseigner `grille_version`, `grille_date`, `source_url` ;
3. renseigner `nuance` et `bloc` pour chacun des 40 groupes, avec une `note` partout où le rattachement
   n'est pas évident (les doublons de libellé déjà repérés dans le fichier : Agir/UDI législature 15,
   Socialistes législature 16, UDR/Union des droites législature 17) ;
4. retirer le bandeau `TODO` — c'est ce retrait, pas une date, que le job vérifie.
