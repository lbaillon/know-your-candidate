# Phase 1 — Ingestion des scrutins et des mandats

**Statut : 📝 à valider** · Dépend de : phase 0 · Bloque : phases 2, 3, 4

## Objectif

Avoir en base, de façon fiable et rejouable, **qui a voté quoi et quand**, ainsi que **qui siégeait dans
quel groupe et était rattaché à quel parti à quelle période**, sur les législatures 15 à 17 (depuis le
4 juillet 2017).

C'est la phase qui porte le plus de risque : tout le reste dépend de la qualité de ces données.

## Périmètre

**Dedans** : téléchargement et parsing des scrutins de l'Assemblée nationale, du référentiel historique des
acteurs / mandats / organes, enrichissement Wikidata (QID, photo), modèle de données correspondant, jobs
d'ingestion paramétrables et idempotents.

**Dehors** : toute catégorisation, tout score, toute page publique. Sénat, Parlement européen et
14e législature (phase 6). L'entité éditoriale « parti » avec ses successions (renommages, fusions,
scissions) relève de la phase 4 : la phase 1 ingère le référentiel brut de l'Assemblée, elle ne tranche pas
qu'un parti en continue un autre.

## Le spike est fait

Réalisé le 13 août 2026 en téléchargeant et en parcourant les archives réelles. Les chiffres et les pièges
sont consignés dans [data-sources.md](../data-sources.md) ; les conséquences méthodologiques dans
[methodology.md](../methodology.md). Résumé de ce qu'il a **changé** par rapport au plan initial :

| Question du plan initial | Réponse mesurée | Conséquence |
| --- | --- | --- |
| Existe-t-il un jeu historique ? | Oui, `AMO30`, toutes législatures depuis la XIe, 3 117 acteurs et 10 813 organes | Un seul téléchargement pour tout le référentiel ; le job acteurs n'est **pas** paramétré par législature |
| Structure et volume des scrutins ? | 16 957 scrutins, 2 346 018 votes nominatifs sur L15-L17, ~300 Mo décompressés | Volume confortable, `COPY` par lots suffit largement |
| Corrections de vote ? | 4 799 mises au point, 0,2 % des votes, bloc presque toujours présent mais vide | Stockées à part, affichées, hors calcul |
| Délégations ? | 12,6 % du corpus, 15,1 % sur la seule 17e | Impossible à ignorer ; drapeau par vote et mention systématique dans l'UI |
| Dates d'appartenance fiables ? | Oui : 7 759 mandats de groupe, tous avec une date de début, 588 en cours | Mais **86 chevauchements** à normaliser avant toute contrainte `EXCLUDE` |
| Dénominateur de la participation ? | La somme des `nombreMembresGroupe` du scrutin donne l'effectif du jour | Colonne générée, aucune reconstruction |
| Combien de QID avec photo ? | 549/652 ont une photo, **651/652 ont un `P4123`** | La réconciliation d'identités est une jointure exacte, pas un rapprochement flou |

Trois découvertes non anticipées, qui déplacent le plan :

1. **L'Assemblée publie ses propres rattachements à un parti** (`PARPOL`) : 58 partis, 3 670 rattachements
   datés. Wikidata n'est plus la source primaire des partis, seulement le complément.
2. **Le MD5 publié n'est pas utilisable comme somme de contrôle** : il ne correspondait pas à l'archive
   servie, et l'archive est régénérée plusieurs fois par jour. L'idempotence ne peut donc reposer que sur
   les clés naturelles.
3. **La 14e législature est inexploitable en l'état** : 710 de ses 1 354 scrutins n'ont pas de détail
   nominatif. Elle sort du corpus v1.

Le spike a aussi **supprimé du travail** : la file de réconciliation manuelle par nom et date de naissance,
prévue comme un chantier, se réduit à quelques cas isolés traités comme des anomalies (2 acteurs sur 1 524
sont absents du référentiel).

## Livrables

1. Migrations `0002` à `0004` : référentiel, scrutins, ingestion.
2. Client HTTP du worker : téléchargement poli, décompression en flux, cache local de développement.
3. Job `ingest_acteurs { force_refetch? }` — personnes, organes, mandats de groupe et de parti.
4. Job `ingest_scrutins { legislature, since?, force_refetch? }` — scrutins, ventilation par groupe, votes.
5. Job `enrich_wikidata { person_uids? }` — QID, photo et licence.
6. Rapport d'ingestion en base : compteurs, anomalies, durée, hash de ce qui a été reçu.
7. Tests d'intégration sur un jeu réduit d'archives réelles versionnées comme fixtures.

## Modèle de données

### Le référentiel (migration `0002`)

```
source_document(id, source, uid, url, fetched_at, content_hash, payload jsonb, ingestion_run_id)
    UNIQUE (source, uid, content_hash)
```

Un document par scrutin et par acteur, archivé **avant** tout parsing. La clé unique porte le hash : un
payload modifié ajoute une ligne, il n'en écrase jamais une. C'est ce qui permet de rejouer le parsing d'un
scrutin précis et de faire remonter une ligne d'UI jusqu'à son payload exact.

```
person(id, an_uid UNIQUE, wikidata_qid UNIQUE, civilite, prenom, nom, date_naissance,
       ville_naissance, date_deces, uri_hatvp, created_at, updated_at)
```

`an_uid` est la clé naturelle (`PA267551`). Tous les autres champs sont nullables : une personne peut
naître d'un vote dont l'acteur est inconnu du référentiel (2 cas mesurés), et vaut alors une anomalie à
traiter, pas un vote perdu.

```
organe(id, an_uid UNIQUE, code_type, libelle, libelle_abrev, legislature, period daterange)
```

Le référentiel brut de l'Assemblée, une seule table pour les groupes (`GP`), les partis (`PARPOL`) et
l'Assemblée elle-même (`ASSEMBLEE`). On ne crée pas deux tables `party` et `groupe` là où la source n'en a
qu'une : la distinction est portée par `code_type`.

```
mandat(id, an_uid UNIQUE, person_id, organe_id, type_organe, period daterange, qualite, legislature)
    EXCLUDE USING gist (person_id WITH =, organe_id WITH =, period WITH &&)
        WHERE (type_organe = 'GP')
```

**La contrainte est posée sur (personne, organe), pas sur (personne, type)**, et c'est un changement par
rapport au plan initial. Le spike a mesuré 86 chevauchements de mandats de groupe : 85 concernent le même
groupe (doublons exacts et périodes emboîtées, artefacts de la source), 1 seul concerne deux groupes
différents — en 2007, hors corpus. La normalisation à l'ingestion fusionne les chevauchements d'un même
groupe en leur union, ce qui les fait disparaître ; un chevauchement entre deux groupes différents reste
donc possible en théorie et sera **journalisé comme anomalie**, la ligne la plus courte n'étant pas
insérée. Le brut reste dans `source_document` : rien n'est perdu.

Interdire *tout* recouvrement entre deux groupes différents par contrainte aurait fait échouer l'ingestion
sur une donnée réelle de l'Assemblée. On préfère une base qui accepte la donnée et signale, à une base qui
refuse et ne dit rien.

### Les scrutins (migration `0003`)

```
scrutin(id, an_uid UNIQUE, numero, legislature, date_scrutin, chambre, organe_id, seance_ref, session_ref,
        type_code, type_libelle, sort_code, titre, demandeur, dossier_ref, mode_publication,
        nombre_votants, suffrages_exprimes, non_votants, pour, contre, abstentions, effectif,
        participation GENERATED ALWAYS AS (nombre_votants::numeric / NULLIF(effectif, 0)) STORED,
        source_document_id)
```

`effectif` est la somme des `nombreMembresGroupe`, donc l'effectif réel au jour du scrutin.
`participation` est générée et indexée : le seuil devient un filtre de requête, jamais un filtre
d'ingestion. `chambre` vaut `assemblee` ou `congres` — le scrutin du Congrès de mars 2024 mélange députés
et sénateurs et doit rester exclu des calculs par défaut.

```
scrutin_groupe(scrutin_id, organe_id, nombre_membres, position_majoritaire,
               pour, contre, abstentions, non_votants)
```

La ventilation par groupe, nécessaire dès la phase 3 pour l'alignement et déjà utile pour afficher qu'un
groupe s'est divisé. `organe_id` est **nullable** : 14 scrutins de la 17e publient leurs groupes sous
l'identifiant fantôme `PO0`. Les votes de ces scrutins restent nominatifs et exploitables, seul le
rattachement au groupe est perdu — il est alors reconstitué depuis les mandats à la date du scrutin, et la
méthode employée est tracée.

```
vote(scrutin_id, person_id, position, cause_non_vote, groupe_organe_id, groupe_source,
     mandat_an_uid, par_delegation, PRIMARY KEY (scrutin_id, person_id))
```

Vérifié sur les 2,3 M de votes du corpus : **aucune personne n'apparaît deux fois dans un même scrutin**,
la clé primaire est sûre. `position` est un enum fermé (`pour`, `contre`, `abstention`, `non_votant`) : la
source n'a que ces quatre blocs nominatifs. `cause_non_vote` est en revanche un `text` et **pas** un enum,
malgré ses trois seules valeurs connues (`PSE`, `PAN`, `MG`) : un code inédit doit produire une anomalie,
pas une ingestion qui échoue. `groupe_source` dit si le groupe vient du fichier ou d'une reconstitution.

```
vote_mise_au_point(scrutin_id, person_id, position_declaree, source_document_id)
```

Table séparée, jamais fusionnée dans `vote` : l'Assemblée publie la correction sans modifier le résultat,
nous faisons pareil.

### L'ingestion (migration `0004`)

`ingestion_run` reçoit ce qui lui manque : `job_id`, `params jsonb`, `url`, `content_hash`, `error`.

```
ingestion_anomaly(id, ingestion_run_id, kind, subject_uid, detail jsonb, resolved_at, resolved_by)
```

Une seule table pour tout ce qui cloche : acteur inconnu, compteurs incohérents, chevauchement de mandats,
groupe `PO0`, code de non-vote inédit. C'est elle qui remplace la file de réconciliation manuelle prévue
initialement.

## Le worker

**Client HTTP.** `User-Agent` identifiant le projet et son URL, une requête à la fois, `If-None-Match` sur
l'`ETag` pour les retéléchargements, cache local sur disque en développement pour ne pas taper les serveurs
de l'Assemblée à chaque itération. Le MD5 publié est relevé et stocké **à titre indicatif** ; le hash qui
fait foi est calculé sur ce que nous recevons.

**Parsing.** Le JSON de l'Assemblée est un XML converti mécaniquement, ce qui impose deux primitives avant
tout le reste, et tout le parsing passe par elles :

- un normalisateur de scalaire : une valeur est soit une chaîne, soit un objet à `#text`, soit
  `{"@xsi:nil": "true"}` qui vaut `NULL` ;
- un normalisateur de collection : un singleton est publié comme un objet, plusieurs éléments comme une
  liste.

Les booléens sont des chaînes (`"true"` / `"false"`). Les URLs d'archives ne se déduisent pas du numéro de
législature : une table de correspondance explicite, `Scrutins.json.zip` pour les 16e et 17e,
`Scrutins_XV.json.zip` pour la 15e.

**Écriture.** Décompression en flux, archivage du brut, puis `COPY` par lots dans des tables temporaires et
`INSERT ... ON CONFLICT DO UPDATE` sur les clés naturelles. Jamais de `TRUNCATE`, jamais de `DELETE` de
masse. Les jobs rapportent leur progression via le contexte de job de la phase 0.

**Ordre d'exécution.** `ingest_acteurs` avant `ingest_scrutins` : le référentiel porte les personnes et les
groupes. Si un vote cite un acteur inconnu, une `person` réduite à son `an_uid` est créée et l'anomalie est
journalisée — on ne perd pas un vote parce qu'un référentiel est en retard.

## Étapes

Chaque étape est un commit qui laisse `main` vert.

1. Migration `0002` (référentiel) et ses tests de contrainte.
2. Client HTTP du worker et ses primitives de normalisation, testés sur des extraits réels versionnés.
3. Job `ingest_acteurs` : organes, personnes, mandats, normalisation des chevauchements.
4. Migration `0003` (scrutins).
5. Job `ingest_scrutins` : archivage brut, ventilation par groupe, votes, mises au point.
6. Migration `0004` et remontée des anomalies dans les deux jobs.
7. Job `enrich_wikidata` : jointure sur `P4123`, photo et licence lue sur Commons.
8. Ingestion complète des trois législatures, vérification manuelle, consignation des chiffres obtenus.

## Décisions

### Tranchées

| # | Question | Décision |
| --- | --- | --- |
| D1.1 | Tous les députés ou seulement les candidat·es pressenti·es ? | **Tous** : le calcul des positions de groupe a besoin de l'ensemble des votants, et la liste des candidat·es n'est pas stable en 2026 |
| D1.2 | Filtrer les scrutins à l'ingestion ou au calcul ? | **Au calcul** : on ingère tout, le seuil de participation est un filtre de requête sur une colonne générée et indexée |
| D1.3 | Suivre les mises au point ? | **Oui**, dans une table séparée, affichées, hors calcul. 0,2 % des votes, mais un député qui déclare s'être trompé est une information |
| D1.4 | Traiter les votes par délégation ? | **Comptés comme des votes** — c'est leur sens juridique — et **signalés systématiquement**. 12,6 % du corpus |
| D1.5 | Photos : binaire ou URL ? | **URL + métadonnées de licence**, lues sur Commons fichier par fichier. Mise en cache seulement si Commons se révèle lent |
| D1.6 | Dénominateur de la participation | **Somme des `nombreMembresGroupe` du scrutin** = effectif du jour. Figé dans methodology.md |
| D1.7 | Profondeur du corpus | **Législatures 15 à 17**. La 14e n'a pas de détail nominatif sur la moitié de ses scrutins ; elle passe au backlog v2 |
| D1.8 | Source primaire des partis | **`PARPOL` de l'Assemblée**, Wikidata en complément pour les personnes n'ayant pas siégé. Le libellé affiché dit « rattaché·e au titre du financement de la vie politique », jamais « membre de » |
| D1.9 | Granularité de l'archivage brut | **Un JSONB par document**, ~20 000 lignes. Le hash de l'archive ne sert pas de clé d'idempotence |
| D1.10 | Scrutin du Congrès | **Ingéré et marqué** `chambre = 'congres'`, exclu des calculs par défaut : il mélange députés et sénateurs |

### À trancher pendant l'implémentation

| # | Question | Piste |
| --- | --- | --- |
| D1.11 | `mandat` est un identifiant français de plus | Le terme mappe 1:1 sur la source et n'a pas d'équivalent anglais propre (« mandate » est un faux ami). À ajouter à la liste de CLAUDE.md si la table est retenue telle quelle |
| D1.12 | Que faire des mandats non parlementaires ? | AMO30 porte aussi commissions, missions, organismes extra-parlementaires. Ingérer seulement `GP`, `PARPOL` et `ASSEMBLEE` en phase 1, quitte à élargir plus tard |
| D1.13 | Où lire la licence d'une photo Commons ? | L'API `imageinfo` / `extmetadata` de Commons. À vérifier au moment du job : une photo sans licence exploitable = pas de photo |

## Fini quand

- Une ingestion complète des trois législatures s'exécute sans intervention et renseigne son
  `ingestion_run` : compteurs, durée, anomalies.
- **Relancer exactement le même job ne change aucune ligne**, vérifié par un test qui compare un
  condensé des tables métier avant et après.
- Une ingestion partielle (une seule législature) n'abîme pas les données déjà présentes.
- Les compteurs par scrutin sont cohérents : `total nominatif = nombreVotants + nonVotants` sur tous les
  scrutins sauf ceux journalisés en anomalie (1 attendu sur la 17e, l'élection du président).
- Pour trois député·es choisi·es à la main, la liste des votes et l'historique de groupe correspondent à ce
  qu'affiche le site de l'Assemblée — vérification manuelle **consignée dans le message de commit** (le
  dépôt est en développement sur le tronc, il n'y a pas de PR où la déposer).
- Aucune personne en double dans `person`, et aucun vote orphelin.

## Risques

- **Le format change sous nos pieds.** L'Assemblée régénère ses archives plusieurs fois par jour et a déjà
  changé ses conventions de nommage entre législatures. Mitigation : archivage systématique du brut, qui
  permet de rejouer un parsing corrigé sans retélécharger, et table de correspondance explicite des URLs.
- **Le référentiel se contredit.** 86 chevauchements de mandats mesurés, un identifiant de groupe fantôme
  sur 14 scrutins, 2 acteurs cités mais absents. Mitigation : normalisation documentée, anomalies
  journalisées, ingestion qui continue.
- **La continuité des partis reste entière.** Renommages, fusions, scissions : le spike n'a rien résolu de
  ce côté, il a seulement fourni une source datée. Le lien de succession entre partis est un travail de
  phase 4, à modéliser comme un lien et jamais comme une fusion d'entités.
- **Wikidata peut contredire l'Assemblée.** Mitigation : l'AN gagne toujours ; Wikidata n'écrit que dans
  des colonnes qui lui sont propres (`wikidata_qid`, photo) ou sur des personnes que l'AN ne connaît pas.
- **Le volume n'est plus un risque.** ~300 Mo de JSON brut, 2,3 M de lignes de vote : un Postgres modeste
  suffit. Le point de vigilance se déplace vers la phase 5, où le coût de stockage de `source_document`
  devra être mesuré avant de choisir l'hébergement.
