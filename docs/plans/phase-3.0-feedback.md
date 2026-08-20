# Phase 3.0 — Retours d'implémentation

**Statut : ✅ consigné** · Dépend de : [phase 3](phase-3-categorisation.md), lots A, B et C · F2 est
résolue depuis le lot C ; F6, trouvée par la vérification 2 le 19/08/2026, est corrigée dans le
`Makefile`. Plus rien ne bloque `make ingest`.

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

## F2 — La grille des nuances politiques n'a pas pu être vérifiée cette session (résolue au lot C)

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

**Résolution (lot C).** La même impasse s'est reproduite en commençant le lot C : Légifrance renvoie
`403 Forbidden` à toute requête non-navigateur (confirmé par deux outils de lecture web différents et un
`curl` direct), et le seul fichier disponible sur data.gouv.fr pour le « Nuancier politique du ministère
de l'Intérieur » est un CSV de 2014 (nomenclature « Front National », « UMP » — inutilisable pour les
législatures 15 à 17 sans se tromper de parti). Plutôt que d'assembler une grille plausible à partir de
fragments de sources secondaires (Wikipédia, presse) — la même impasse que le bandeau `TODO` avait
identifiée comme le seul endroit de la phase où « remplir de mémoire » serait tentant et grave — la
question a été posée à l'utilisateur, qui a récupéré et fourni le PDF de l'instruction du 11 juin 2024
(NOR IOMA2415630C, Légifrance `id/45565`).

Deux corrections à l'hypothèse de départ, une fois la source primaire lue :

1. **La grille utilisée est celle des élections législatives de 2024 (24 nuances), pas une grille « 2026
   à 26 nuances ».** Cette dernière référence, présente dans une version antérieure de
   [data-sources.md § 4](../data-sources.md#4-ministère-de-lintérieur--grille-des-nuances-politiques) et
   reprise par le plan, n'a jamais été vérifiée par une source primaire — corrigée dans le même commit
   que ce fichier, avec un renvoi ici plutôt qu'une réécriture silencieuse (même traitement que F1).
2. **La grille elle-même ne définit pas de blocs.** L'annexe 1 du document est une liste plate de 24
   nuances (libellé, signification, commentaires), sans colonne de regroupement — seulement un ordre
   implicite de l'extrême gauche à l'extrême droite. Le commentaire du fichier de seed affirmait que
   « la grille ordonne six blocs, elle ne les chiffre pas » : c'était une supposition du plan, jamais
   vérifiée, et elle est fausse pour la source réellement utilisée. Le regroupement en six blocs (pas
   seulement leurs six coordonnées) est donc entièrement de notre fait, dérivé de cet ordre implicite —
   `db/seeds/group_axis.toml` le dit maintenant explicitement.

Le rattachement nuance → groupe parlementaire (40 lignes) a ensuite été construit à la main : les 12
nuances à parti unique (`COM`, `FI`, `SOC`, `RDG`, `VEC`, `REN`, `MDM`, `HOR`, `UDI`, `LR`, `RN`, `REC`)
couvrent la plupart des groupes ; les groupes sans nuance propre (petits groupes centristes ou
régionalistes, alliances électorales comme UDR/RN aux législatives 2024) portent une `note` qui
documente le choix — la même convention que les doublons de libellé déjà présents dans le fichier.
Vérifié en conditions réelles : `cargo run -- enqueue label_scrutins_heuristic` puis `run-once` charge le
fichier (`is_current = true` sur `grille-legislatives-2024-v1`) et calcule 15 621 estimations sur 16 956
scrutins examinés (couverture médiane 99,3 %), y compris pour 17/2653 (position `+0,670`, séparation
`1,000`) — cohérent avec la correction narrative de F1 : RN et UDR, tous deux `extreme_droite`, sont
isolés par le seuil optimal, comme prévu.

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

## F4 — Un guillemet non fermé faisait disparaître une ligne CSV au lieu de la faire refuser

**Où** : `backend/src/kyc_api/labels_io.py::read_csv` ; commit *Import : validation et aperçu* (lot C,
étape 8), trouvé en écrivant `tests/fixtures/imports/guillemets_non_fermes.csv`.

**Ce qui s'est passé.** `csv.DictReader` ne lève aucune exception sur un guillemet non refermé : il avale
tout le reste du fichier dans le champ ouvert par le guillemet, et remplit les colonnes qu'une ligne
devenue trop courte laisse sans valeur avec `None` — pas une chaîne vide. Le code de `read_csv` testait
`if not theme: continue` pour distinguer « scrutin présent mais non catégorisé » (une ligne légitime du
format d'export, D3.6) d'une ligne à importer. `(None or "").strip()` vaut `""` exactement comme une
vraie colonne vide : la ligne corrompue était donc silencieusement traitée comme un scrutin non
catégorisé, et disparaissait de l'import sans un mot — la même classe d'erreur que la « ⚠️ » du plan
prévenait explicitement (« un bug dans la validation… détruit silencieusement des heures de relecture
humaine »), sur son tout premier cas de test.

**Décision.** `read_csv` refuse maintenant toute ligne où `None in record.values()` — signe qu'elle a
moins de champs que l'en-tête — avant même de regarder si le thème est vide. Testé par la fixture
elle-même dans `tests/test_import_validation.py` (le test paramétré vérifie que chaque fixture de refus
est bien refusée, à un stade ou un autre).

## F5 — `fastapi.UploadFile` n'est pas la classe que `request.form()` retourne

**Où** : `backend/src/kyc_api/admin/imports.py` ; même commit, trouvé par un test qui déposait un fichier
CSV valide et recevait un 422 « aucun fichier déposé ».

**Ce qui s'est passé.** `fastapi.UploadFile` est une **sous-classe** de `starlette.datastructures.
UploadFile`, pas un réexport du même objet. `Request.form()` (hérité de Starlette, pas réimplémenté par
FastAPI) construit ses fichiers avec la classe Starlette. `isinstance(upload, fastapi.UploadFile)`
rendait donc `False` pour un fichier réellement déposé — l'import était rejeté avant même d'être lu,
quel que soit son contenu.

**Décision.** Vérifier contre `starlette.datastructures.UploadFile`, la classe effectivement produite par
`request.form()`. Piège à connaître pour toute future route qui lit `request.form()` directement plutôt
que de déclarer un paramètre `UploadFile` typé par FastAPI (qui, lui, gère cette distinction tout seul) —
c'est précisément parce que `admin/imports.py` doit lire les autres champs du formulaire à la main
(nom de fichier, etc.) qu'il passe par `request.form()` plutôt que par l'injection de dépendance standard.

## F6 — Une reprise de job suffisait à faire calculer les dérivés sur un corpus incomplet

**Où** : `Makefile`, cible `ingest`. Trouvé le 19/08/2026 en exécutant la vérification 2 du plan
(« `make ingest` rejoué en entier sur une base vierge »), sur une base `kyc_phase3_check` créée pour
l'occasion.

**Ce qui s'est passé.** Au premier passage, un incident réseau a fait échouer
`ingest_scrutins {"legislature": 16}` (`error decoding response body`). La reprise automatique a
fonctionné — mais un job requis repart en `pending` avec une nouvelle date de planification, donc
**derrière** tous les jobs enfilés après lui. Horodatages relevés dans la table `job` :

| Job | Tentatives | Début | Fin |
| --- | --- | --- | --- |
| `ingest_scrutins {16}` | 2 | 12:22:21 | **12:27:24** |
| `label_scrutins_heuristic` | 1 | 12:25:51 | 12:25:52 |
| `refresh_views` | 1 | 12:25:52 | 12:25:59 |

Les deux jobs dérivés ont donc tourné sur une base à laquelle il manquait encore les 4 105 scrutins
de la 16e législature. Résultat : 11 956 estimations d'axe au lieu de 15 621, deux personnes sans
slug (des votants créés par l'ingestion des scrutins, arrivés après `assign_slugs`), et une
`person_apercu` rafraîchie trop tôt. **Et `make ingest` a rendu 0.**

**Pourquoi les gardes existantes ne l'ont pas vu.** F3 ([phase-1.1-fix.md](phase-1.1-fix.md)) garantit
que `run-once` sort en code non nul si un job ne finit pas en `done`. Ici, *tous* les jobs ont fini
en `done` : la garantie porte sur l'issue de chaque job, jamais sur leur ordre. Or l'ordre du
pipeline n'existait que dans l'ordre des `enqueue`, c'est-à-dire dans une propriété que la file
n'a jamais promis de préserver.

**Le même symptôme existait sur la base de développement**, indépendamment de cette reprise :
`sum(person_apercu.votes_total)` valait 1 743 107 pour 2 346 018 votes réels à l'Assemblée — l'accueil
et l'annuaire sous-comptaient les votes de tout le monde de 602 911. Réparé par un `refresh_views`.
C'est le genre de faute qui ne se voit pas : une vue matérialisée périmée n'a l'air de rien, elle
répond vite et elle ment.

**Décision.** `make ingest` est découpé en **cinq étapes séparées par un `run-once`**, une par niveau
de dépendance : référentiel · scrutins et Wikidata · seed des candidat·es · slugs et thèmes ·
dérivés. `run-once` ne rend la main qu'une fois la file vide, reprises comprises : une étape ne peut
plus doubler celle dont elle dépend, et un échec définitif arrête `make ingest` à l'étape fautive
plutôt qu'après avoir calculé des dérivés faux. Pas de colonne `depends_on` dans la table `job` : la
dépendance est déjà exprimée par l'ordre des étapes du Makefile, la mettre aussi en base ferait deux
sources de vérité pour la même chose.

**Vérifié après correctif** : deuxième et troisième passages sur base vierge, sans incident —
15 621 estimations, empreintes identiques d'un passage à l'autre
(`md5` des estimations `fe768380…`, des thèmes `438590e5…`), **zéro ligne réécrite** au troisième
passage (aucun `computed_at` d'estimation modifié, aucun slug créé). Seules grandissent les deux
tables de journal, par construction : `job` (+10 lignes par passage) et `ingestion_anomaly`
(+1 704 — une trace par anomalie et par exécution ; sa croissance sans borne au fil des ingestions
est à regarder en phase 5, ce n'est pas un défaut de la phase 3).

## F7 — Deux tests passaient uniquement tant que personne n'avait configuré l'authentification

**Où** : `backend/tests/conftest.py` ; trouvé le 19/08/2026, à la première exécution de `make test`
sur une machine où le back-office venait d'être réellement configuré.

**Ce qui s'est passé.** `kyc_api.config.settings` est un singleton Pydantic lu **au chargement du
module**, depuis le `.env` du dépôt. Les tests du back-office n'annulaient pas cette lecture :

- `test_login_returns_503_when_oauth_is_not_configured` vérifiait le 503 « authentification non
  configurée » en comptant sur le fait que la machine de test n'a pas de `ADMIN_GITHUB_CLIENT_ID`.
  Dès qu'un `.env` réel en porte un, `/admin/login` redirige vers GitHub et le test tombe ;
- `test_a_safe_method_is_never_checked` (garde CSRF) suivait alors cette redirection : le transport
  ASGI de `httpx` route **toutes** les URL vers l'application, y compris `github.com`, et rendait un
  404 au lieu du 503 attendu.

Les deux échecs sont arrivés ensemble, sans qu'une ligne de code de production ait changé.

**Pourquoi ça compte plus que deux tests rouges.** Un test dont le résultat dépend de la
configuration locale de qui l'exécute ne teste plus le code : il teste la machine. Ici, il rendait
`make test` rouge exactement au moment où quelqu'un commençait à se servir du back-office —
c'est-à-dire au pire moment, et avec un message qui ne désigne pas la cause. La CI, elle, restait
verte, ce qui aurait fait conclure à un problème de poste de travail.

**Décision.** Une fixture `autouse` dans `conftest.py` vide `admin_github_client_id`,
`admin_github_client_secret` et `admin_github_logins` pour **tous** les tests : l'état par défaut de
la suite est « non configuré », quel que soit le `.env` de la machine. Les tests qui ont besoin du
contraire appellent déjà `configure_admin(monkeypatch)`, qui écrase ces valeurs avec le même
`monkeypatch`, donc avec le même défaisage en fin de test. Règle à retenir pour la suite : **tout
réglage lu depuis l'environnement doit être neutralisé par une fixture, pas supposé absent.**

## F8 — L'apostrophe typographique reléguait des votes sur un texte entier en fin de file

**Où** : `db/migrations/0007_categorisation.sql`, vue `scrutin_a_categoriser` ; corrigé par
`db/migrations/0008_priorite_apostrophe.sql`. Trouvé le 19/08/2026 en relisant un lot produit par
`scripts/export_batches.py`, dont un titre commençait par « l’ensemble » avec une apostrophe
courbe.

**Ce qui s'est passé.** Le `rang_priorite` de la file de travail compare le début du titre à
`'l''ensemble%'`, avec l'apostrophe droite (U+0027) — celle que le plan avait écrite, sur la foi des
titres qu'il avait sous les yeux. L'open data de l'Assemblée mélange en réalité les deux formes :
**118 titres sur 16 956** commencent par « l’ » typographique (U+2019). Ces scrutins tombaient donc
dans le `ELSE` du `CASE`, au rang 4, celui des amendements.

**L'impact réel, cité tel quel plutôt qu'arrondi vers le haut** : sur le corpus de travail actuel,
**deux scrutins** étaient mal classés (16/3048, vote sur l'ensemble d'une proposition de résolution,
et 16/3047, son article unique). Les neuf autres titres concernés du corpus sont des scrutins
solennels, déjà au rang 1 par leur `type_code`, que le défaut n'atteignait pas. Deux lignes ne
justifieraient pas une migration ; ce qui la justifie, c'est que le défaut grandit avec le corpus —
sur l'ensemble des scrutins de l'Assemblée, 12 votes sur un texte entier et 14 articles sont dans ce
cas, et abaisser `corpus_parametre.participation_min`, ce qui est prévu, les ferait entrer d'un coup
dans la file, mal classés.

**Ce que ce défaut apprend, au-delà de son ampleur.** Rien n'était faux à l'écran : une file de
travail mal ordonnée ne ressemble pas à une panne, elle ressemble à une file de travail. Ce sont les
défauts qu'aucun test ne réclame parce que personne ne sait qu'ils existent ; celui-ci n'est sorti
que parce qu'un humain a relu des titres à voix haute. La règle qui en découle vaut pour toute la
suite : **tout filtre posé sur du texte de l'open data doit accepter les deux apostrophes**, et
plus généralement se méfier de l'idée qu'une source publique est typographiquement homogène.

**Décision.** Une classe de caractères (`~* '^l[''’]ensemble'`) plutôt qu'un `replace()` de
normalisation : elle dit ce qu'on accepte là où une normalisation dirait ce qu'on remplace, et si la
source introduit demain une troisième variante, on veut la voir plutôt que l'absorber.

**Test de non-régression** : `test_a_typographic_apostrophe_still_ranks_as_a_vote_on_a_whole_text`
(`backend/tests/test_admin_categorisation.py`). Il insère un amendement **daté plus tard** qu'un vote
sur l'ensemble d'un texte à l'apostrophe courbe : si le rang de priorité cessait de les distinguer,
le départage par date rendrait l'amendement et le test tomberait. Vérifié en rejouant l'ancien `CASE`
à la main sur les deux mêmes lignes — il classe bien l'amendement en tête, le test discrimine donc
réellement le défaut et ne se contente pas de constater le comportement corrigé.

## F9 — Une règle de prompt a fabriqué 99 positions sans fondement, et le contrôle croisé les a trouvées

**Où** : `scripts/prompt_categorisation.md`, règle 7 (retirée) ; 99 lignes de `scrutin_label`
retirées de la base le 19/08/2026.

**Ce que la règle disait.** Après la première campagne, des lois de finances étaient sautées par les
agents au motif qu'elles « contiennent des mesures de sens opposés ». Jugeant qu'un budget est le
vote politique par excellence et qu'il ne pouvait pas rester hors du corpus, une règle 7 a été
ajoutée : un texte budgétaire prend son thème dominant, avec une confiance basse et une position
modérée. Elle a été appliquée à la lettre.

**Ce qu'elle a produit.** Sur les 103 lignes budgétaires importées, **69 portaient exactement
`+0.300`**, 27 exactement `-0.300` (les motions, qui héritaient de l'inversion), et 4 la valeur
`0.200`. Trente-quatre justifications disaient noir sur blanc que « l'orientation d'ensemble n'est
pas précisée par le titre » — **tout en affichant une position**. Les autres se contentaient de
reformuler le titre (« Article 2 du projet de loi de finances pour 2026 »). Une position publiée,
adossée à un aveu d'ignorance : exactement ce que la règle 1 de methodology.md interdit.

**Comment on l'a vu.** Par la mesure 6, le contrôle croisé avec l'estimation d'axe. L'agrégat
donnait 48,5 % d'accord de signe — indiscernable du hasard. Décomposé, il révélait trois
populations :

| Sous-ensemble | n | Accord |
| --- | --- | --- |
| Axes gauche-droite, hors budgets | 95 | 68,4 % |
| **Textes budgétaires** | 99 | **29,3 %** |
| Axes non gauche-droite (institutions, agriculture, Europe) | 35 | 48,6 % |

Les 29,3 % ne sont pas du bruit : *sous* le hasard, donc **inversion systématique**. La raison est
structurelle et vaut d'être retenue — sur un budget, ceux qui votent pour sont la majorité
gouvernementale, donc le centre, et ceux qui votent contre sont **les deux extrêmes à la fois**. La
moyenne d'axe du camp « contre » retombe vers le centre, et le signe s'inverse. Ce vote ne situe
personne.

**Décision.** Règle 7 retirée : un texte budgétaire redevient un cas de saut, et le prompt le dit
explicitement plutôt que de le laisser déduire. Les 99 lignes concernées ont été retirées de la base
(72 scrutins), en conservant les **quatre** qui affirment réellement une orientation citable — les
trois budgets rectificatifs de 2020 « orientés vers la dépense de soutien plutôt que la maîtrise
budgétaire », et une loi de règlement classée `autre` sans position. Le retrait est passé par
`queries.labels.replace_labels`, la fonction du back-office : même transaction, une `label_revision`
par scrutin avec l'état avant, un `apres` vide et le motif, une `admin_action`. Un `DELETE` sec
aurait effacé la trace d'une décision éditoriale, ce que le projet s'interdit.

État après retrait : 236 lignes sur 230 scrutins, et l'accord remonte à **67,0 %** sur le
sous-ensemble comparable (97 lignes).

**Deux leçons, la seconde plus utile que la première.** Une règle qui impose une valeur là où la
donnée n'en porte aucune ne produit pas une nuance, elle produit une constante — et une constante se
voit : 69 valeurs identiques étaient la signature du défaut. Surtout, **c'est le contrôle croisé qui
a trouvé la faute, pas une relecture**. Deux signaux indépendants — l'un lit un titre, l'autre mesure
des votes de groupes — se contredisant de façon systématique, cela ne peut pas être du hasard, et
c'est le seul outil du projet capable de repérer une erreur que personne ne cherchait. Il justifie à
lui seul le temps passé sur `scrutin_axis_estimate`.
