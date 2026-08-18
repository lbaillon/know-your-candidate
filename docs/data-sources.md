# Sources de données

Toutes les sources doivent être **ouvertes, officielles ou vérifiables, et citées dans l'UI**. Une donnée
sans source affichable n'entre pas en base.

> **Relevé du 13 août 2026, vérifié en téléchargeant les archives** (spike de la
> [phase 1](plans/phase-1-ingestion.md)). Les chiffres de cette page sont mesurés, pas estimés. Les chemins
> restent à revalider avant chaque campagne d'ingestion : l'Assemblée nationale réorganise ses archives, et
> les noms de fichiers ne sont **pas** uniformes d'une législature à l'autre.
>
> **Confirmé par une ingestion complète réelle le 17 août 2026** (implémentation de la phase 1) :
> `make ingest` a chargé le référentiel puis les trois législatures sans intervention, et les compteurs
> obtenus reproduisent exactement ceux du spike — **4 417 scrutins / 472 631 votes** en 15e, **4 106
> scrutins** en 16e, **8 434 scrutins / 1 270 476 votes** en 17e, `VTCGR5L16V1` avec ses **902 votes** en
> `chambre = 'congres'`. Le total des trois législatures (472 631 + 603 813 + 1 270 476 = 2 346 920 votes,
> Congrès inclus) correspond exactement à 2 346 018 (Assemblée) + 902 (Congrès) ci-dessus. Seul écart
> apparent : le total brut de la 16e ingéré est **603 813**, pas 602 911 — la différence est exactement les
> 902 votes du Congrès, que le chiffre du spike excluait implicitement. Vérification manuelle de trois
> député·es (dont Charlotte Parmentier-Lecocq, 5 groupes différents sur la seule 17e législature) contre le
> site de l'Assemblée : historique de groupe et position de vote strictement identiques.
>
> **Ré-exécuté en entier le 17 août 2026 après les correctifs de la revue** ([phase-1.1-fix.md](plans/phase-1.1-fix.md)),
> sur une base vierge : scrutins et votes des trois législatures **inchangés** (les correctifs ne
> touchaient ni au parsing des scrutins ni au calcul des votes). Deux chiffres bougent, comme prévu par
> le plan :
>
> - **15 298 mandats** au lieu de 15 224 (D1.16, F5) : les 74 chevauchements réels entre organes
>   différents (dont la scission UMP/Rassemblement-UMP de 2012) sont désormais **conservés et
>   journalisés** plutôt que tranchés silencieusement à l'ingestion — `mandat_chevauchement` reste à 74
>   dans `ingestion_anomaly`, mais compte des signalements, plus des suppressions.
> - `enrich_wikidata`, jamais exécuté par `make ingest` avant F2c, mesuré pour la première fois :
>   **2 474 résultats SPARQL**, **2 454 personnes uniques**, **10 `wikidata_qid_ambigu`** (le cas
>   miroir que F2a corrige — un même QID portant deux `P4123` — n'était pas détecté avant le correctif,
>   donc jamais mesuré), **1 684 candidats photo**, dont **85 hors du corpus L15-L17** (`P4123` couvre
>   toutes les législatures depuis la XIe, pas seulement les nôtres — découvert à l'exécution réelle, voir
>   `photos_hors_perimetre` ci-dessous) et **1 599 photos enregistrées, 0 sans licence exploitable** parmi
>   les 1 599 candidats restants. Écart avec le ~84 % annoncé par le plan (mesuré sur la 15e législature
>   seule, 652 personnes) : sur l'ensemble du corpus L15-L17, le ratio réel est proche (1 599 photos pour
>   environ 1 900 personnes avec `wikidata_qid`), l'ordre de grandeur tient.
>
> **Une régression a été trouvée et corrigée pendant cette exécution réelle**, au-delà de ce que F2/F3
> décrivaient : `enrich_photos` ne comptait ni ne journalisait les 85 candidats hors périmètre (silencieux
> avant le correctif — exactement la classe de bug que F2b visait à éliminer, manquée parce que F2b ne
> couvrait que la normalisation de titre, pas l'absence de `person`), et `drain_once` (F3) rendait `Err`
> pour un job qui avait fini par réussir après une reprise automatique (un blip réseau a fait échouer le
> téléchargement de la 15e législature, réparé quelques secondes plus tard dans le même `run-once` —
> `drain_once` gardait la trace du premier échec au lieu de la dernière issue connue par `job_id`). Les
> deux corrigés dans le même commit que cette exécution, voir son message pour le détail.
>
> Idempotence vérifiée sur les **huit** tables métier (`person`, `organe`, `mandat`, `scrutin`,
> `scrutin_groupe`, `vote`, `vote_mise_au_point`, `person_photo`) en relançant `make ingest` sur la base
> déjà peuplée : condensés strictement identiques avant/après — `person_photo` n'avait jamais été
> vérifiée jusqu'ici.

## 1. Assemblée nationale — open data officiel (source principale)

Portail : <https://data.assemblee-nationale.fr> · Licence Ouverte (Etalab) · formats XML, JSON, parfois CSV,
distribués en archives `.zip`.

### Scrutins (les votes)

Pages de référence : <https://data.assemblee-nationale.fr/travaux-parlementaires/votes> pour la législature
en cours, `archives-16e/votes` et `archives-anterieures/archives-15e/scrutins` pour les précédentes. Les
pages ne listent que leur propre législature : il n'existe pas de page unique énumérant tout.

Base commune : `https://data.assemblee-nationale.fr/static/openData/repository/`

| Lég. | Chemin | Archive | Contenu | Période |
| --- | --- | --- | --- | --- |
| 17 | `17/loi/scrutins/Scrutins.json.zip` | 26,3 Mo | 8 434 fichiers, 172,7 Mo | 2024-10-08 → 2026-07-21 |
| 16 | `16/loi/scrutins/Scrutins.json.zip` | 10,1 Mo | 4 106 fichiers, 68,4 Mo | 2022-07-11 → 2024-06-07 |
| 15 | `15/loi/scrutins/Scrutins_XV.json.zip` | 9,2 Mo | 4 417 fichiers, 57,1 Mo | 2017-07-04 → 2022-02-24 |
| 14 | `14/loi/scrutins/Scrutins_XIV.json.zip` | 0,7 Mo | **1 seul fichier**, 1 354 scrutins | 2012-07-03 → 2016-11-24 |

Le nom change (`Scrutins` vs `Scrutins_XV` vs `Scrutins_XIV`) : **construire l'URL par formatage sur le
numéro de législature ne marche pas**, il faut une table de correspondance explicite.

Les législatures 15 à 17 publient un fichier JSON par scrutin, toutes en `modePublicationDesVotes =
DecompteNominatif`, soit **2 346 018 votes nominatifs** à l'Assemblée, plus 902 au Congrès. Chaque scrutin
porte le titre, la date, le type, le sort, les compteurs, et la position de chaque votant, groupe par
groupe.

La **législature 14 est un autre format** : une archive monolithique, et surtout 710 de ses 1 354 scrutins
sont publiés en `DecompteDissidentsPositionGroupe` — on connaît la position du groupe et le nom des
dissidents, pas le vote de chacun. L'archive s'arrête au 24 novembre 2016 alors que la législature a duré
jusqu'en juin 2017. Elle est donc **hors corpus v1** (voir [methodology.md](methodology.md), § 2).

Un MD5 est publié à côté de chaque archive (`Scrutins.json.zip.md5`). **Ce n'est pas une somme de contrôle
utilisable** : le 13/08/2026, le MD5 publié ne correspondait pas à l'archive servie, et l'archive a été
régénérée dans la journée (même taille à l'octet près, `Last-Modified` et `ETag` différents). L'AN
reconstruit ses archives périodiquement. Conséquences pour l'ingestion :

- le MD5 publié sert d'**indice de changement**, jamais de vérification d'intégrité ;
- le hash qui fait foi est celui **que nous calculons sur ce que nous avons réellement reçu**, et c'est
  celui-là qui va en base ;
- deux téléchargements successifs peuvent différer sans que le contenu métier ait bougé : l'idempotence
  doit reposer sur les clés naturelles (`uid`), pas sur le hash de l'archive.

### Acteurs, mandats, organes (AMO) — qui siège, dans quel groupe, à quelles dates

Le jeu **historique existe** et c'est celui qu'il faut : `AMO30`, publié sous le chemin de la législature
en cours mais couvrant toutes les législatures depuis la XIe.

```
17/amo/tous_acteurs_mandats_organes_xi_legislature/AMO30_tous_acteurs_tous_mandats_tous_organes_historique.json.zip
```

13,6 Mo compressés, 13 989 fichiers, 94,5 Mo décompressés : **3 117 acteurs**, **10 813 organes**, 59
déports. Les autres jeux (`AMO10` députés actifs, `AMO20` députés/sénateurs/ministres, `AMO40`, `AMO50`) ne
couvrent que la législature en cours et ne sont pas nécessaires en phase 1.

Couverture mesurée : sur les **1 524 acteurs distincts** apparaissant dans les scrutins des législatures 15
à 17, **2 seulement** sont absents d'AMO30 (`PA429842`, `PA720634`). La réconciliation acteur ↔ vote est
donc quasi complète par simple jointure sur l'`uid`.

**Groupes parlementaires** (`codeType = GP`) : 63 groupes, des législatures 12 à 17, chacun avec ses dates
de vie (`viMoDe.dateDebut` / `dateFin`). Les appartenances sont portées par les mandats de `typeOrgane =
GP` : **7 759 mandats**, tous avec une `dateDebut`, 588 sans `dateFin` (en cours). Qualités observées :
`Membre` (4 643), `Député non-inscrit` (2 770), `Membre apparenté` (261), `Président` (85).

Trois pièges mesurés :

- les **non-inscrits** sont modélisés comme un pseudo-groupe par législature (`PO840056` = NI de la 17e).
  Techniquement un `GP`, politiquement l'absence de groupe : ne jamais l'afficher comme une appartenance ni
  le faire entrer dans un calcul d'alignement de groupe. L'AN ne clôture par ailleurs pas toujours ce
  pseudo-mandat à la date où la personne rejoint un vrai groupe, ce qui le fait chevaucher son groupe
  suivant en continu — sans que ce soit une vraie contradiction (voir point suivant) ;
- **95 chevauchements** de mandats GP existent pour une même personne dans un **même** organe, dont des
  **doublons exacts** (mêmes dates, même groupe, deux `uid` de mandat distincts) : 85 sont des
  **inclusions** — une plage en contient une autre, aucune n'est partielle — et 10 sont des **charnières**
  (deux plages consécutives qui partagent leur jour de bascule, voir plus bas). Une contrainte `EXCLUDE`
  posée naïvement rejetterait l'ingestion : il faut normaliser avant d'insérer, et journaliser ce qui
  reste.
  **Correction du 14 août 2026** (implémentation de la phase 1, `worker/src/an/acteur.rs`) : le spike
  initial n'avait mesuré qu'**un seul** chevauchement entre deux organes différents (« 2007, hors corpus »).
  Une vérification indépendante sur l'archive réelle en trouve **~74 à 77**, pour deux raisons non
  anticipées : (1) le non-clôturage du pseudo-mandat non-inscrit décrit ci-dessus, qui n'est pas une vraie
  contradiction et est donc **exclu** du contrôle de chevauchement inter-organes ; (2) la **scission
  UMP / Rassemblement-UMP de novembre 2012 à janvier 2013** (crise Copé-Fillon), où l'AN a recensé environ
  73 député·es simultanément membres des deux groupes — un fait politique réel, pas une anomalie de saisie.
  Ce second cas reste un chevauchement au sens du plan : les **deux** mandats sont conservés — trancher
  laquelle des deux appartenances est vraie serait une interprétation, pas un fait brut (D1.16,
  [phase-1.1-fix.md](plans/phase-1.1-fix.md), F5) — et journalisés (`mandat_chevauchement`), avec le plus
  court des deux (le groupe éphémère « Rassemblement-UMP ») nommé dans l'anomalie à titre indicatif ;
- **`dateFin` est incluse dans le mandat.** Ce n'est pas une convention supposée, c'est la source qui
  l'établit : sur les votes tombant exactement un jour de fin de mandat, **870 confirment le groupe qui se
  termine ce jour-là et aucun ne confirme le suivant**. Une plage se construit donc en
  `daterange(debut, fin + 1, '[)')`. Corollaire : 10 mandats consécutifs d'un même groupe partagent leur
  date de charnière et se chevaucheraient d'un jour — à raboter à l'ingestion.

Aucun mandat n'a de date de début manquante ni de fin antérieure à son début : les plages sont
constructibles sans garde-fou, ce qui n'empêche pas d'en poser un.

**Le référentiel contredit parfois le fichier de scrutin**, et c'est le fichier qui a raison. Cas mesuré :
un député porte un mandat de non-inscrit d'un seul jour, le 13/11/2025, alors que le scrutin de ce jour-là
le range dans son groupe. 324 votes du corpus sont dans cette situation. Le rattachement au groupe se lit
donc **d'abord dans le scrutin**, et les mandats ne servent qu'à combler ce que le scrutin ne dit pas.

### Partis politiques — l'AN publie ses propres rattachements

Découverte du spike : AMO30 contient un type d'organe `PARPOL`, **58 partis politiques**, avec **3 670
rattachements datés couvrant 1 613 personnes** (574 sans date de fin). C'est une source officielle,
datée et jointe aux acteurs — meilleure que Wikidata pour les personnes passées par l'Assemblée.

**Mais ce n'est pas une adhésion.** Ce sont les déclarations de rattachement **au titre du financement de
la vie politique**, renouvelées périodiquement — d'où des dates communes à des dizaines de lignes
(`2012-12-01`, `2015-12-01`, `2025-12-03`). Le libellé affiché doit dire exactement cela, et jamais
« membre de X » (voir [methodology.md](methodology.md), § 2).

Distinction à conserver dans le modèle : un **groupe parlementaire** n'est pas un **parti**. On peut être
rattaché à un parti sans siéger dans le groupe correspondant, et les groupes se renomment ou se
recomposent.

### Page publique d'un scrutin (lien affiché en plus du lien open data)

Format confirmé le 17 août 2026 (implémentation de la phase 2, page `/scrutin/{legislature}/{numero}`)
en ouvrant réellement une page de chaque législature :

```
https://www.assemblee-nationale.fr/dyn/{legislature}/scrutins/{numero}
```

Vérifié sur `VTANR5L16V2004` → <https://www.assemblee-nationale.fr/dyn/16/scrutins/2004> : même
titre (« amendement n° 113 de M. Dharréville… »), même date (26/06/2023), même ventilation par
groupe que le fichier open data. `{numero}` est le numéro simple du scrutin (`2004`), pas son
`an_uid` complet (`VTANR5L16V2004`) : c'est le même numéro que la colonne `scrutin.numero`.

### Cas particulier : les scrutins du Congrès

L'archive de la 16e législature contient un scrutin `VTCGR5L16V1` (04/03/2024, révision constitutionnelle
sur l'IVG) : **902 votants**, `organeRef` du Congrès (`PO791932`), donc **députés et sénateurs mélangés**.
Son `uid` commence par `VTCGR` et non `VTANR`. À ingérer marqué comme tel et exclu des calculs par défaut,
sans quoi des sénateurs entreraient dans les statistiques de l'Assemblée.

### Le JSON de l'AN est un XML converti mécaniquement

Ce n'est pas un détail de confort : c'est la première source de bugs de parsing.

- **Un scalaire peut être un objet.** `acteur.uid` vaut
  `{"@xsi:type": "IdActeur_type", "#text": "PA267551"}` et non `"PA267551"`.
- **Un null peut être un objet.** Une valeur absente vaut `{"@xsi:nil": "true"}`, pas `null`.
- **Un singleton n'est pas une liste.** Le bloc `votant` est une liste quand le groupe a plusieurs votants
  et un objet quand il n'en a qu'un (mesuré en 17e : 95 180 listes contre 35 671 objets).
- **Les booléens sont des chaînes.** `parDelegation` vaut `"true"` ou `"false"`.
- **Les noms de blocs changent d'un scrutin à l'autre.** Le scrutin du Congrès nomme ses blocs nominatifs
  `pour`, `contre`, `abstention` au **singulier**, là où les 16 956 autres les nomment `pours`, `contres`,
  `abstentions`. Un parseur qui ne connaît que le pluriel ingère **zéro vote pour ce scrutin, sans lever la
  moindre erreur** : accepter les deux graphies et journaliser tout nom de bloc inconnu.

Tout accès à un champ doit donc passer par un normalisateur « texte, objet à `#text`, ou nil », et tout
accès à une collection par un « objet ou liste → liste ».

### Ce que contient une position de vote

Le `decompteNominatif` d'un groupe n'a que **quatre** blocs nominatifs : `pours`, `contres`, `abstentions`,
`nonVotants`. Les « non-votants volontaires » sont comptés mais **jamais nommés** — et les absents
n'apparaissent pas du tout dans le fichier : seuls les votants et les non-votants institutionnels y sont.

Chaque non-votant porte une **cause explicite** (`causePositionVote`), et il n'en existe que trois :

| Code | Signification | Occurrences (L14-L17) |
| --- | --- | --- |
| `PSE` | président de séance | 15 127 |
| `PAN` | président de l'Assemblée nationale | 12 473 |
| `MG` | membre du Gouvernement | 10 723 |

C'est une bonne nouvelle pour la règle « une absence de vote n'est pas une opinion » : la source dit
elle-même pourquoi la personne n'a pas voté, et la réponse est toujours institutionnelle.

**Délégations** : massives et en hausse. 191 629 votes délégués en 17e (15,1 %), 51 737 en 16e (8,6 %),
52 110 en 15e (11,0 %) — **12,6 % du corpus**. Impossible de les ignorer, impossible de les traiter comme
un vote ordinaire sans le dire.

**Mises au point** (`miseAuPoint`) : corrections déclarées après coup. 3 597 scrutins concernés sur les
trois législatures, pour 4 799 entrées seulement — 0,2 % des votes. Attention : le bloc est **presque
toujours présent mais vide**, rempli de `null`, artefact de la conversion XML ; il faut détecter les
entrées réelles, pas la présence du bloc. Sont aussi signalés 160 scrutins avec un `dysfonctionnement`
déclaré.

**Cohérence interne** : sur les 8 434 scrutins de la 17e, 8 433 vérifient
`total nominatif = nombreVotants + nonVotants`. L'unique écart est `VTANR5L17V1`, l'élection du président
de l'Assemblée. Cette égalité fait un bon contrôle d'ingestion — à journaliser comme anomalie, pas à
traiter comme une erreur fatale.

Le spike n'avait vérifié cette égalité que sur la 17e. L'ingestion complète du 17 août 2026 l'a mesurée sur
les trois législatures : **30 écarts sur la 15e, 4 sur la 16e**, en plus de l'unique cas de la 17e — tous
journalisés comme `compteurs_incoherents`, aucun n'a fait échouer l'ingestion. De même, le groupe fantôme
`PO0` (voir plus bas) touche aussi **9 lignes de la 16e**, pas seulement les 146 de la 17e mentionnées par
le spike. Ces trois chiffres n'avaient jamais été mesurés avant l'implémentation de la phase 1 : le spike
n'avait vérifié ces contrôles que sur la 17e législature.

**Effectif et dénominateur** : la somme des `nombreMembresGroupe` d'un scrutin donne l'effectif de
l'Assemblée à sa date (574 à 577 en 17e). Le dénominateur de la participation est donc **dans le fichier**,
sans reconstruction à partir des mandats.

## 2. Wikidata — identité, partis hors Assemblée, photos

Endpoint SPARQL : <https://query.wikidata.org/sparql> · Licence CC0.

Sert à :

- rattacher une personne à un identifiant stable (`QID`) et faire le lien avec d'autres bases ;
- récupérer la **photo** (`P18`), hébergée sur Wikimedia Commons ;
- compléter les **partis** (`P102`) pour les personnes que l'AN ne couvre pas, PARPOL restant primaire pour
  celles qui ont siégé.

**La jointure est exacte, pas approximative.** La propriété `P4123` (identifiant Assemblée nationale) porte
le suffixe numérique de l'`uid` AN : `P4123 = 410` ↔ `PA410` (vérifié sur François Bayrou, Michèle
Alliot-Marie, Jean-François Copé, Ségolène Royal, Guy Drut). Mesure sur la 15e législature : **651 des 652**
député·es ont un `P4123`. Le rapprochement par nom et date de naissance n'est donc qu'un **filet de
sécurité pour une poignée de cas**, pas le mécanisme principal.

Couverture photo mesurée sur la même population : **549 sur 652 ont un `P18`** (84 %). La licence, elle,
n'est pas dans Wikidata — elle se lit sur Commons, fichier par fichier (§ 3).

Wikidata est collaboratif, donc faillible : les données servent d'appoint et de complément, jamais de
contradiction à l'open data officiel. En cas de désaccord, l'AN gagne.

## 3. Wikimedia Commons — photos des candidat·es

Les images référencées par `P18` sont sur Commons, avec une licence par fichier (souvent CC-BY-SA, parfois
domaine public). Conséquences :

- il faut stocker et **afficher l'auteur et la licence** de chaque photo ;
- on stocke une URL et les métadonnées, pas nécessairement le binaire ;
- une photo sans licence exploitable = pas de photo, on affiche un placeholder.

## 4. Ministère de l'Intérieur — grille des nuances politiques

Le ministère de l'Intérieur publie, à chaque élection, une **grille des nuances politiques** servant à
agréger les résultats. Version 2026 : 26 nuances, regroupées en 6 blocs (extrême gauche, gauche, divers,
centre, droite, extrême droite). La nuance est attribuée par les préfectures, **indépendamment de
l'étiquette revendiquée par le candidat**. Les résultats nuancés sont publiés sur data.gouv.fr et
interieur.gouv.fr, et le Répertoire national des élus (RNE) porte la nuance des élu·es.

C'est la seule source **officielle, publiée et datée** qui positionne les formations politiques sur un axe
gauche-droite. Elle est donc un excellent point d'ancrage pour l'heuristique de la
[phase 3](plans/phase-3-categorisation.md) — bien meilleur qu'un classement que nous inventerions.

Trois réserves à afficher avec la donnée, sans quoi on ferait passer un acte administratif pour une
vérité :

1. **C'est une décision de l'exécutif**, qui classe notamment ses propres opposants. Le classement de LFI
   en « extrême gauche » pour les municipales de 2026, contesté devant le Conseil d'État, en est
   l'illustration : la grille est un objet politique, pas une mesure.
2. **Elle évolue d'une élection à l'autre.** Toute utilisation doit donc mentionner la version et la date
   de la grille employée. Une nuance n'est pas une propriété stable d'un parti.
3. **Elle nuance des candidat·es et des listes, pas des groupes parlementaires.** Le rattachement
   nuance → parti → groupe à l'Assemblée est une jointure que nous établissons, et qui doit être
   documentée comme telle.

Usage retenu : la grille alimente le seed d'ancrage gauche-droite, **chaque ligne citant sa source et sa
date**, et reste modifiable par PR argumentée. On cite, on ne délègue pas.

## 5. Sources secondaires envisagées (hors périmètre v1)

| Source | Apport | Réserve |
| --- | --- | --- |
| NosDéputés.fr (Regards Citoyens) | API pratique, données déjà normalisées, synthèses d'activité | Intermédiaire supplémentaire ; à utiliser pour recouper, pas comme source primaire |
| data.gouv.fr | Miroirs et jeux dérivés | Fraîcheur variable |
| Sénat (open data) | Couvre les candidat·es passé·es par le Sénat | Format et modèle différents, coût non négligeable |
| Parlement européen | Couvre les candidat·es eurodéputé·es | Autre modèle de vote, autre échelle politique |
| HATVP (déclarations d'intérêts) | Conflits d'intérêts | Interprétation délicate, à traiter avec les mêmes garde-fous que la v2 |

## 6. Règles d'ingestion

1. **Archiver le brut avant de parser.** Le payload d'origine va en `source_document` (JSONB), **un
   document par scrutin et par acteur**, avec son URL et sa date de récupération. Quand le format change,
   on rejoue le parsing sans retélécharger, et une ligne d'UI remonte jusqu'à son payload exact.
2. **Idempotence par clé naturelle.** L'`uid` de l'AN est la clé. Rejouer une ingestion met à jour, ne
   duplique pas, ne supprime pas. **Pas d'idempotence fondée sur le hash de l'archive** : l'AN régénère ses
   archives, le hash change sans que le contenu métier bouge.
3. **Additivité.** On peut lancer une ingestion sur une seule législature, une période, ou un seuil de
   participation plus bas, sans toucher à ce qui existe déjà.
4. **Traçabilité.** Chaque `ingestion_run` note la source, l'URL, le hash **calculé par nous** sur ce que
   nous avons reçu, les compteurs et les erreurs. Une fiche candidat doit pouvoir remonter jusqu'au fichier
   qui a produit la donnée.
5. **Politesse.** Un `User-Agent` identifiant le projet avec son URL, pas de parallélisme agressif, et une
   requête conditionnelle (`If-None-Match` sur l'`ETag`) pour ne pas retélécharger inutilement — le MD5
   publié ne permet pas de le décider de façon fiable. Ce sont des serveurs publics.
6. **Une anomalie se journalise, elle n'interrompt pas.** Doublons de mandats, incohérence de compteurs,
   acteur inconnu : l'ingestion continue et rend compte. Seule une erreur qui rendrait les données fausses
   justifie d'échouer.
