# Phase 3.0 — Retours d'implémentation

**Statut : ✅ consigné** · Dépend de : [phase 3](phase-3-categorisation.md), lots A et B · Ne bloque rien,
sauf F2 qui bloque `make ingest` tant qu'elle n'est pas résolue.

## Objectif

Ce n'est pas une phase de correction comme [0.1](phase-0.1-fix.md) ou [2.1](phase-2.1-fix.md) : les lots A
et B n'ont pas de défaut caché à corriger, ils tiennent (`make lint`, `make typecheck`, `make test` verts à
chaque commit). Ce document consigne les points où l'implémentation a rencontré une limite du plan
lui-même ou de l'architecture existante, comme demandé par [CLAUDE.md](../../CLAUDE.md) (« si un plan
s'avère faux au contact du code, dis-le et propose une révision — ne contourne pas silencieusement »).
Autoportant comme les autres plans : tout ce qui est nécessaire est ici. Mis à jour au fil des lots plutôt
que réécrit à chaque fois.

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

## F3 — Le backend n'avait jamais eu besoin d'une vraie transaction avant la catégorisation

**Où** : `backend/src/kyc_api/db.py` ; commit *File de travail et formulaire de catégorisation* (lot B,
étape 5).

**Ce que le plan ne disait pas, parce que ça n'avait pas de raison de figurer dedans.** Jusqu'à la phase 3,
le backend était en lecture seule (CLAUDE.md : « le backend n'écrit que les données saisies par un admin »)
et n'écrivait qu'une ligne à la fois (`job`, `person_photo`…) : `Queryable`, le protocole partagé par
`asyncpg.Pool` et `asyncpg.Connection` pour permettre l'injection d'une connexion de test, n'exposait donc
que `fetchrow`/`fetchval`/`fetch`/`execute`. L'écriture d'une catégorisation (D3.20) exige, elle,
plusieurs instructions atomiques : lire l'état avant, remplacer les lignes, écrire l'historique
seulement si l'état a changé, journaliser l'action — et `Pool`, contrairement à `Connection`, n'expose pas
`.transaction()` (chaque appel `fetch*`/`execute` y acquiert sa propre connexion, sans mémoire de l'appel
précédent).

**Décision.** Ajout de `WritableQueryable` (étend `Queryable` de `.transaction()`) et de `get_connection`,
qui retient une connexion unique acquise du pool pour toute la durée de la requête — à ne poser que sur
les routes qui écrivent transactionnellement, `get_pool` reste la dépendance par défaut pour tout le
reste. Les tests le surchargent exactement comme `get_pool` (même connexion unique déjà ouverte dans une
transaction annulée en fin de test), une savepoint imbriquée s'ouvrant naturellement à chaque
`conn.transaction()` de `replace_labels`. Aucune ligne du plan n'avait à anticiper ce détail
d'infrastructure ; il est consigné ici pour que la phase 3.2 (import, lot C) le retrouve directement
plutôt que de le redécouvrir.

**Corollaire découvert en écrivant les tests, à connaître avant d'en écrire d'autres qui comparent des
horodatages** : dans un test, deux écritures successives par un même `admin_client` partagent la
transaction externe ouverte par la fixture `db_conn` (voir `conftest.py`) ; `now()` y est *figée* à
l'ouverture de cette transaction, pas rafraîchie par instruction ni par savepoint. Deux révisions créées
dans le même test portent donc le même `created_at` à la microseconde près. `ORDER BY created_at DESC`
seul ne peut pas les départager de façon fiable *dans un test* (en production, chaque requête HTTP est sa
propre transaction, le problème ne s'y pose pas). `kyc_api.queries.labels.get_revisions_for_scrutin`
trie désormais par `created_at DESC, id DESC` pour rester correct dans les deux mondes ; tout futur tri
sur un horodatage d'écriture devrait faire de même par précaution, et tout test qui a besoin d'ordonner
des écritures successives doit trier par `id`, pas par `created_at`.
