# Phase 1 — Ingestion des scrutins et des mandats

**Statut : ✅ validé** · Dépend de : phase 0 · Bloque : phases 2, 3, 4

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
| Dates d'appartenance fiables ? | Oui : 7 759 mandats de groupe, tous avec une date de début, 588 en cours | Mais **95 chevauchements intra-groupe + ~74 inter-groupes** à normaliser avant toute contrainte `EXCLUDE` (chiffre inter-groupes corrigé le 14 août 2026, voir data-sources.md — le spike initial n'en avait mesuré qu'un) |
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
groupe et sont tous des **inclusions** (une plage en contient une autre, aucune n'est partielle), 1 seul
croise deux groupes — en 2007, hors corpus. Interdire *tout* recouvrement entre groupes différents aurait
fait échouer l'ingestion sur une donnée réelle de l'Assemblée. On préfère une base qui accepte la donnée et
signale, à une base qui refuse et ne dit rien.

`period` se construit en `daterange(date_debut, date_fin + 1, '[)')` : **la date de fin est incluse dans le
mandat**, la source l'établit sans ambiguïté (voir [data-sources.md](../data-sources.md)). Deux
normalisations en découlent, toutes deux journalisées :

- **inclusion** — quand deux mandats d'une même personne dans un même organe se chevauchent, la plage
  englobante est conservée et la plage incluse écartée ;
- **charnière** — quand deux mandats consécutifs d'un même organe partagent leur date de charnière
  (10 cas, tous antérieurs à 2017), la fin du premier est ramenée à la veille.

Le brut reste dans `source_document` : rien n'est perdu.

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
vote(scrutin_id, person_id, position, cause_non_vote, groupe_organe_id, groupe_from_mandat,
     mandat_an_uid, par_delegation, PRIMARY KEY (scrutin_id, person_id))
```

Vérifié sur les 2,3 M de votes du corpus : **aucune personne n'apparaît deux fois dans un même scrutin**,
la clé primaire est sûre. `position` est un enum fermé (`pour`, `contre`, `abstention`, `non_votant`) : la
source n'a que ces quatre blocs nominatifs. `cause_non_vote` est en revanche un `text` et **pas** un enum,
malgré ses trois seules valeurs connues (`PSE`, `PAN`, `MG`) : un code inédit doit produire une anomalie,
pas une ingestion qui échoue. `groupe_from_mandat` distingue le groupe lu dans le scrutin de celui
reconstitué depuis les mandats.

`mandat_an_uid` est le `mandatRef` du vote. Vérifié : il pointe **toujours** vers le mandat de siège
(`ASSEMBLEE`), jamais vers le mandat de groupe, et les 1 270 476 références de la 17e se résolvent dans le
référentiel. Il sert donc à distinguer un titulaire d'un suppléant, pas à retrouver un groupe.

```
vote_mise_au_point(scrutin_id, person_id, position_declaree, source_document_id)
```

Table séparée, jamais fusionnée dans `vote` : l'Assemblée publie la correction sans modifier le résultat,
nous faisons pareil. `position_declaree` est un `text` contraint par `CHECK` et non l'enum `vote_position`,
parce que la source y admet une cinquième valeur absente des blocs de vote : le non-votant volontaire.

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

## Décisions arbitrées

Elles sont tranchées. La session qui implémente ne les rediscute pas ; si l'une se révèle fausse au contact
du code, elle le dit plutôt que de la contourner.

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
| D1.11 | `mandat` est un identifiant français de plus | **Accepté** : le projet est français, le terme mappe 1:1 sur la source et « mandate » est un faux ami. Ajouté à la liste de CLAUDE.md |
| D1.12 | Que faire des mandats non parlementaires ? | **Hors phase 1** : on n'ingère que `GP`, `PARPOL` et `ASSEMBLEE`. Commissions, missions et organismes extra-parlementaires attendront un besoin réel |
| D1.13 | Où lire la licence d'une photo Commons ? | **API `imageinfo` / `extmetadata` de Commons**, par lots de 50 fichiers. Une photo sans licence exploitable = pas de photo |
| D1.14 | Convention de bornes des plages | **`daterange(debut, fin + 1, '[)')`** : la date de fin est incluse dans le mandat, la source l'établit (870 votes le confirment, 0 le contredisent) |
| D1.15 | Qui gagne sur le groupe au moment du vote ? | **Le fichier de scrutin**, toujours. Le référentiel se contredit sur 324 votes du corpus (mandats de non-inscrit d'un jour). Les mandats ne servent qu'à combler les 14 scrutins au groupe `PO0` |

## Plan d'exécution

Cette section est destinée à la session qui implémentera la phase. Elle est autoportante : **tout ce qui
est nécessaire est ici, dans [data-sources.md](../data-sources.md), dans
[methodology.md](../methodology.md) ou dans [CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de
conversation à retrouver. Les décisions ci-dessus sont arbitrées et ne sont pas à rediscuter.

Tous les chiffres cités viennent d'un spike qui a réellement téléchargé et parcouru les archives. Ce ne
sont pas des ordres de grandeur : ils servent de tests d'acceptation. **Si le code produit un autre
chiffre, c'est le code qu'il faut regarder d'abord** — puis la source, qui a pu bouger, auquel cas le dire
et mettre à jour ces documents dans le même commit.

Développement sur le tronc : **commits directs sur `main`**, pas de branche ni de PR. Un commit par étape,
`main` vert (lint, typage, tests) à chaque fois. Ne pas modifier les plans des autres phases.

### Arborescence cible

```
db/migrations/
  0002_referentiel.sql      source_document, person, organe, mandat, person_photo
  0003_scrutins.sql         scrutin, scrutin_groupe, vote, vote_mise_au_point
  0004_ingestion.sql        colonnes d'ingestion_run, ingestion_anomaly

worker/src/
  main.rs                   + sous-commande `enqueue` (voir plus bas)
  an/
    mod.rs
    json.rs                 les deux primitives de normalisation — à écrire en premier
    http.rs                 téléchargement poli, ETag, cache disque, hash
    scrutin.rs              structures et parsing d'un scrutin
    acteur.rs               structures et parsing d'un acteur, d'un organe, d'un mandat
  jobs/
    ingest_acteurs.rs
    ingest_scrutins.rs
    enrich_wikidata.rs
  anomaly.rs                écriture dans ingestion_anomaly, vocabulaire des `kind`
  run.rs                    ouverture et clôture d'un ingestion_run

worker/tests/
  fixtures/
    README.md               comment les extraits ont été fabriqués, pour pouvoir les refabriquer
    scrutins-extrait.zip
    amo30-extrait.zip
  ingest_acteurs.rs
  ingest_scrutins.rs
```

### Migration `0002_referentiel.sql`

```sql
CREATE TABLE source_document (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    source           text        NOT NULL,  -- 'an_scrutin', 'an_acteur', 'an_organe', 'wikidata'
    uid              text        NOT NULL,  -- uid AN du document, ou QID
    url              text        NOT NULL,
    content_hash     text        NOT NULL,  -- sha256 du payload, calculé sur ce que nous avons reçu
    payload          jsonb       NOT NULL,
    ingestion_run_id bigint      REFERENCES ingestion_run (id),
    fetched_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (source, uid, content_hash)
);

CREATE INDEX source_document_courant_idx ON source_document (source, uid, fetched_at DESC);

CREATE TABLE person (
    id                    bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid                text NOT NULL UNIQUE,
    wikidata_qid          text UNIQUE,
    civilite              text,
    prenom                text,
    nom                   text,
    date_naissance        date,
    ville_naissance       text,
    departement_naissance text,
    pays_naissance        text,
    date_deces            date,
    uri_hatvp             text,
    created_at            timestamptz NOT NULL DEFAULT now(),
    updated_at            timestamptz NOT NULL DEFAULT now()
);

-- Recherche par nom dès la phase 2 ; pg_trgm est déjà activé par la migration 0001.
CREATE INDEX person_nom_trgm_idx ON person USING gin (nom gin_trgm_ops);

CREATE TABLE organe (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid         text NOT NULL UNIQUE,
    code_type      text NOT NULL,          -- 'GP', 'PARPOL', 'ASSEMBLEE'
    libelle        text NOT NULL,
    libelle_abrege text,
    libelle_abrev  text,
    legislature    smallint,
    period         daterange,
    is_non_inscrit boolean NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX organe_type_idx ON organe (code_type, legislature);

CREATE TABLE mandat (
    id          bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid      text NOT NULL UNIQUE,
    person_id   bigint NOT NULL REFERENCES person (id),
    organe_id   bigint NOT NULL REFERENCES organe (id),
    type_organe text NOT NULL,
    period      daterange NOT NULL,
    qualite     text,
    legislature smallint,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mandat_gp_sans_chevauchement
        EXCLUDE USING gist (person_id WITH =, organe_id WITH =, period WITH &&)
        WHERE (type_organe = 'GP')
);

CREATE INDEX mandat_person_idx ON mandat (person_id, type_organe);
CREATE INDEX mandat_period_idx ON mandat USING gist (period) WHERE type_organe = 'GP';

-- Pas de photo sans licence affichable : la contrainte porte la règle, pas le code applicatif.
CREATE TABLE person_photo (
    person_id    bigint PRIMARY KEY REFERENCES person (id) ON DELETE CASCADE,
    url          text NOT NULL,
    commons_file text,
    licence      text NOT NULL,
    licence_url  text,
    auteur       text,
    fetched_at   timestamptz NOT NULL DEFAULT now()
);
```

`btree_gist` (déjà activé en `0001`) est ce qui permet le `person_id WITH =` dans une contrainte `EXCLUDE`.

### Migration `0003_scrutins.sql`

```sql
CREATE TYPE vote_position AS ENUM ('pour', 'contre', 'abstention', 'non_votant');
CREATE TYPE chambre AS ENUM ('assemblee', 'congres');

CREATE TABLE scrutin (
    id                      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    an_uid                  text NOT NULL UNIQUE,
    numero                  integer NOT NULL,
    legislature             smallint NOT NULL,
    date_scrutin            date NOT NULL,
    chambre                 chambre NOT NULL DEFAULT 'assemblee',
    organe_id               bigint REFERENCES organe (id),
    session_ref             text,
    seance_ref              text,
    type_code               text NOT NULL,
    type_libelle            text,
    type_majorite           text,
    sort_code               text,
    titre                   text NOT NULL,
    demandeur               text,
    dossier_ref             text,
    mode_publication        text NOT NULL,
    lieu_vote               text,
    nombre_votants          integer NOT NULL,
    suffrages_exprimes      integer NOT NULL,
    pour                    integer NOT NULL,
    contre                  integer NOT NULL,
    abstentions             integer NOT NULL,
    non_votants             integer NOT NULL,
    non_votants_volontaires integer NOT NULL DEFAULT 0,
    effectif                integer NOT NULL,
    participation           numeric GENERATED ALWAYS AS (
                                nombre_votants::numeric / NULLIF(effectif, 0)
                            ) STORED,
    source_document_id      bigint NOT NULL REFERENCES source_document (id),
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX scrutin_date_idx ON scrutin (date_scrutin DESC);
CREATE INDEX scrutin_participation_idx ON scrutin (participation) WHERE chambre = 'assemblee';
CREATE INDEX scrutin_titre_fts_idx ON scrutin USING gin (to_tsvector('french', titre));

-- `rang` et non `organe_id` dans la clé : les 14 scrutins au groupe fantôme publient plusieurs
-- lignes de groupe portant toutes le même `PO0`, une clé (scrutin, organe) les écraserait.
CREATE TABLE scrutin_groupe (
    scrutin_id          bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    rang                smallint NOT NULL,
    organe_id           bigint REFERENCES organe (id),
    nombre_membres      integer NOT NULL,
    position_majoritaire text,
    pour                integer NOT NULL,
    contre              integer NOT NULL,
    abstentions         integer NOT NULL,
    non_votants         integer NOT NULL,
    PRIMARY KEY (scrutin_id, rang)
);

CREATE TABLE vote (
    scrutin_id         bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    person_id          bigint NOT NULL REFERENCES person (id),
    position           vote_position NOT NULL,
    cause_non_vote     text,
    groupe_organe_id   bigint REFERENCES organe (id),
    groupe_from_mandat boolean NOT NULL DEFAULT false,
    mandat_an_uid      text,
    par_delegation     boolean NOT NULL DEFAULT false,
    PRIMARY KEY (scrutin_id, person_id),
    CONSTRAINT vote_cause_reservee_aux_non_votants
        CHECK (cause_non_vote IS NULL OR position = 'non_votant')
);

CREATE INDEX vote_person_idx ON vote (person_id, scrutin_id);
CREATE INDEX vote_groupe_idx ON vote (groupe_organe_id) WHERE groupe_organe_id IS NOT NULL;

CREATE TABLE vote_mise_au_point (
    scrutin_id         bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    person_id          bigint NOT NULL REFERENCES person (id),
    position_declaree  text NOT NULL,
    source_document_id bigint NOT NULL REFERENCES source_document (id),
    PRIMARY KEY (scrutin_id, person_id, position_declaree),
    CONSTRAINT mise_au_point_position_connue CHECK (position_declaree IN (
        'pour', 'contre', 'abstention', 'non_votant', 'non_votant_volontaire'
    ))
);
```

`position` est un mot-clé PostgreSQL non réservé : il s'utilise sans guillemets comme nom de colonne. Ne
pas le renommer pour éviter un problème qui n'existe pas.

### Migration `0004_ingestion.sql`

```sql
ALTER TABLE ingestion_run
    ADD COLUMN job_id       bigint REFERENCES job (id) ON DELETE SET NULL,
    ADD COLUMN params       jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN url          text,
    ADD COLUMN content_hash text,
    ADD COLUMN error        text;

CREATE TABLE ingestion_anomaly (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    ingestion_run_id bigint NOT NULL REFERENCES ingestion_run (id) ON DELETE CASCADE,
    kind             text NOT NULL,
    subject_uid      text,
    detail           jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at       timestamptz NOT NULL DEFAULT now(),
    resolved_at      timestamptz,
    resolved_by      text
);

CREATE INDEX ingestion_anomaly_kind_idx ON ingestion_anomaly (kind, created_at DESC);
```

Vocabulaire fermé des `kind`, à définir comme constantes dans `anomaly.rs` plutôt qu'en chaînes disséminées :

| `kind` | Quand | Attendu au premier passage |
| --- | --- | --- |
| `acteur_inconnu` | un vote cite un `acteurRef` absent du référentiel | 2 |
| `mandat_inclus` | une plage de mandat en contient une autre, même personne et même organe | 85 |
| `mandat_charniere` | deux mandats consécutifs d'un même organe partagent leur date de charnière | 10 |
| `mandat_chevauchement` | chevauchement résiduel entre deux organes différents (pseudo-groupe non-inscrit exclu du contrôle, voir data-sources.md) | ~74, dominé par la scission UMP/R-UMP de 2012 |
| `groupe_fantome` | ligne de groupe portant `PO0` | 146 lignes, 14 scrutins |
| `cause_non_vote_inconnue` | code hors `PSE`, `PAN`, `MG` | 0 |
| `bloc_nominatif_inconnu` | nom de bloc hors singulier et pluriel connus | 0 |
| `compteurs_incoherents` | `total nominatif <> nombreVotants + nonVotants` | 1 (`VTANR5L17V1`) |
| `photo_sans_licence` | Commons ne rend pas de licence exploitable | à mesurer |
| `wikidata_qid_ambigu` | deux QID revendiquent le même `P4123` | à mesurer |

Ces attendus sont des tests d'acceptation, pas de la documentation d'ambiance : un `groupe_fantome` à 0
signifie que le cas n'est pas détecté, pas qu'il a disparu.

### Les deux primitives de parsing — à écrire en premier

Elles conditionnent tout le reste, et une seule ligne de parsing qui les contourne ramène la classe de bugs
entière. Le JSON de l'Assemblée est un XML converti mécaniquement :

```rust
/// Un scalaire est soit une chaîne, soit `{"@xsi:type": ..., "#text": "PA267551"}`,
/// soit `{"@xsi:nil": "true"}` qui vaut None.
fn scalaire(v: &serde_json::Value) -> Option<&str>;

/// Une collection est publiée comme un objet quand elle a un seul élément,
/// comme un tableau au-delà. Mesuré : 95 180 tableaux contre 35 671 objets en 17e législature.
fn collection(v: &serde_json::Value) -> impl Iterator<Item = &serde_json::Value>;
```

Écrire les tests de ces deux fonctions **avant** le reste, avec les trois formes de scalaire et les trois
formes de collection (absente, objet, tableau). Les booléens de la source sont des chaînes : `parDelegation`
vaut `"true"` ou `"false"`, jamais un booléen JSON.

### Client HTTP

`User-Agent` identifiant le projet et son URL — ce sont des serveurs publics et la politesse est une règle
du projet, pas une option. Une requête à la fois, pas de parallélisme. `If-None-Match` sur l'`ETag` pour
éviter un retéléchargement inutile ; le MD5 publié à côté de l'archive est relevé et stocké **à titre
indicatif seulement**, il ne correspond pas toujours à ce qui est servi.

Cache disque en développement (`worker/.cache/`, ajouté au `.gitignore`) : sans lui, chaque itération
retélécharge 60 Mo. Décompression en flux, sans écrire l'archive décompressée sur disque.

Table de correspondance des URLs, explicite parce que les noms ne se déduisent pas :

| Législature | Fichier |
| --- | --- |
| 17 | `17/loi/scrutins/Scrutins.json.zip` |
| 16 | `16/loi/scrutins/Scrutins.json.zip` |
| 15 | `15/loi/scrutins/Scrutins_XV.json.zip` |
| référentiel | `17/amo/tous_acteurs_mandats_organes_xi_legislature/AMO30_tous_acteurs_tous_mandats_tous_organes_historique.json.zip` |

### Job `ingest_acteurs { force_refetch? }`

Pas de paramètre de législature : `AMO30` est global.

1. Ouvrir un `ingestion_run` (`source = 'an_amo30'`, `params`, `job_id`).
2. Télécharger l'archive, calculer son `sha256`, le noter dans le run.
3. **Organes d'abord** : ne retenir que `code_type IN ('GP', 'PARPOL', 'ASSEMBLEE')` (D1.12). Positionner
   `is_non_inscrit = true` quand `code_type = 'GP'` et `libelle_abrev = 'NI'`. `upsert` sur `an_uid`.
4. **Acteurs ensuite** : archiver le payload de chacun dans `source_document`, puis `upsert` de `person`
   sur `an_uid`. `uid` de l'acteur est un objet, pas une chaîne — c'est là que la primitive sert.
5. **Mandats enfin**, uniquement des trois types retenus. Construire
   `daterange(date_debut, date_fin + 1, '[)')`, ou `daterange(date_debut, NULL, '[)')` si le mandat est en
   cours. Si `date_debut` manque ou si `date_fin < date_debut` — jamais vu dans les données actuelles —
   journaliser et écarter la ligne plutôt que de laisser PostgreSQL lever une erreur de plage.
6. **Normaliser les mandats de groupe** avant insertion, par personne et par organe, plages triées :
   - une plage contenue dans une autre est écartée (`mandat_inclus`) ;
   - deux plages consécutives partageant leur date de charnière : la fin de la première est ramenée à la
     veille (`mandat_charniere`) ;
   - un chevauchement qui subsiste entre deux organes différents est journalisé
     (`mandat_chevauchement`) et la plage la plus courte n'est pas insérée.
7. Clôturer le run avec ses compteurs.

Attendus au premier passage : **3 117 personnes**, **≥ 63 organes `GP`**, **58 organes `PARPOL`**,
**7 759 mandats `GP`**, **3 670 mandats `PARPOL`**, **3 954 mandats `ASSEMBLEE`**. Le référentiel vit :
prendre ces chiffres comme des planchers et signaler un écart important plutôt que d'y ajuster le code.

### Job `ingest_scrutins { legislature, since?, force_refetch? }`

1. Ouvrir un `ingestion_run` (`source = 'an_scrutins'`).
2. Télécharger l'archive de la législature demandée. Rejeter tout autre numéro que 15, 16 ou 17 avec un
   message explicite : la 14e n'est pas ingérable en l'état (D1.7).
3. Pour chaque fichier, `since` filtrant sur `dateScrutin` s'il est fourni :
   a. archiver le payload (`source = 'an_scrutin'`, `uid` = uid du scrutin) ;
   b. parser l'entête et les compteurs, `chambre = 'congres'` si l'`uid` commence par `VTCGR` ;
   c. `effectif` = somme des `nombreMembresGroupe` de la ventilation ;
   d. insérer les lignes de `scrutin_groupe` dans l'ordre du fichier (`rang` = index) ;
   e. pour chaque bloc nominatif, **accepter les deux graphies** : `pours`/`pour`, `contres`/`contre`,
      `abstentions`/`abstention`, `nonVotants`. Tout autre nom de bloc est une anomalie
      `bloc_nominatif_inconnu` — sans cette règle, le scrutin du Congrès s'ingère à zéro vote sans erreur ;
   f. résoudre le groupe : celui de la ligne du fichier, sauf `PO0` ; dans ce cas, chercher le mandat `GP`
      de la personne couvrant la date du scrutin, poser `groupe_from_mandat = true`, et si plusieurs
      mandats couvrent la date, prendre celui dont la date de début est la plus tardive ;
   g. si l'`acteurRef` est inconnu, créer une `person` réduite à son `an_uid` et journaliser
      `acteur_inconnu` — on ne perd pas un vote ;
   h. vérifier `total nominatif = nombreVotants + nonVotants`, journaliser l'écart sans échouer ;
   i. n'insérer les entrées de `miseAuPoint` que si elles portent réellement un `acteurRef` : le bloc est
      presque toujours présent et rempli de `null`.
4. Écrire par lots (`COPY` vers une table temporaire puis `INSERT ... ON CONFLICT DO UPDATE`), en
   rapportant la progression tous les 500 scrutins environ.
5. Clôturer le run.

Attendus : **8 434 scrutins et 1 270 476 votes en 17e** (législature en cours, ces nombres augmentent),
**4 106 et 602 911 en 16e**, **4 417 et 472 631 en 15e** (législatures closes, ces nombres sont stables).
Le scrutin du Congrès `VTCGR5L16V1` doit porter **902 votes**, pas zéro.

### Job `enrich_wikidata { person_uids? }`

Une seule requête SPARQL sur <https://query.wikidata.org/sparql>, `User-Agent` identifiant le projet
(Wikimedia le refuse sinon) :

```sparql
SELECT ?p ?an ?img WHERE {
  ?p wdt:P4123 ?an .
  OPTIONAL { ?p wdt:P18 ?img }
}
```

`P4123` porte le **suffixe numérique** de l'`uid` AN : la jointure est `person.an_uid = 'PA' || ?an`. Si
deux QID revendiquent le même `P4123`, ne rien écrire et journaliser `wikidata_qid_ambigu` — Wikidata ne
tranche jamais un conflit à notre place, et l'AN gagne toujours.

Pour la licence, interroger l'API de Commons **par lots de 50 fichiers** :
`action=query&prop=imageinfo&iiprop=url|extmetadata&titles=File:A|File:B|…`, puis lire
`extmetadata.LicenseShortName`, `LicenseUrl`, `Artist`. Pas de licence lisible → **pas de ligne dans
`person_photo`** et une anomalie `photo_sans_licence`. La contrainte `NOT NULL` sur `licence` est là pour
que ce soit impossible d'oublier.

Attendu : environ 84 % des député·es ont une photo (549 sur 652 mesurés sur la 15e législature).

### Déclencher un job sans route publique

Aucune route publique ne crée de job (règle de CLAUDE.md), et le back-office n'arrive qu'en phase 3. Le
worker reçoit donc une sous-commande :

```
cargo run -- enqueue ingest_acteurs
cargo run -- enqueue ingest_scrutins '{"legislature": 17}'
```

Sans argument, le binaire démarre la boucle de jobs comme aujourd'hui. Parsing des arguments à la main via
`std::env::args` : deux sous-commandes ne justifient pas une dépendance. Ajouter une cible
`make ingest` qui enchaîne le référentiel puis les trois législatures dans l'ordre.

### Fixtures et stratégie de test

Les tests d'intégration Rust suivent la convention de la phase 0 : `#[sqlx::test]`, une base fraîche par
test, migrations jouées automatiquement.

Les fixtures sont des **extraits d'archives réelles**, pas des données inventées, et leur budget est de
**1 Mo au total**. Choisir les scrutins pour couvrir les cas qui cassent, pas pour faire nombre :

| Cas à couvrir | Où le trouver |
| --- | --- |
| Scrutin ordinaire, plusieurs groupes | n'importe lequel |
| Bloc `votant` objet (un seul votant) et bloc tableau | fréquents, vérifier les deux |
| Vote par délégation | fréquent en 17e |
| Mise au point réellement remplie | `VTANR5L15V3415` |
| Non-votants des trois causes | `PSE`, `PAN`, `MG` |
| Groupe fantôme `PO0` | `VTANR5L17V501` |
| Blocs nominatifs au singulier | `VTCGR5L16V1` (Congrès) |
| Compteurs incohérents | `VTANR5L17V1` |

L'extrait du référentiel doit contenir les acteurs et organes cités par ces scrutins, plus un acteur aux
mandats de groupe qui s'incluent, et un aux mandats qui partagent une charnière.
`worker/tests/fixtures/README.md` documente la commande qui a produit chaque extrait, pour pouvoir les
refabriquer quand la source bouge.

Le test d'idempotence est le plus important de la phase :

```sql
SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM vote t;
```

Ingérer, relever le condensé de chaque table métier, réingérer le même extrait, comparer. **Toute
différence est un échec**, y compris sur `updated_at` — ce qui impose de ne pas toucher `updated_at`
quand rien d'autre ne change (`ON CONFLICT DO UPDATE ... WHERE` avec une comparaison des colonnes, ou
`IS DISTINCT FROM` sur la ligne entière).

Après toute modification d'une requête `sqlx::query!` / `query_as!`, régénérer les métadonnées avec
`cargo sqlx prepare -- --all-targets` — sans `--all-targets`, les requêtes des tests d'intégration perdent
leurs entrées `.sqlx` et la CI casse en mode hors ligne.

### Vérifications à exécuter avant de déclarer la phase terminée

Ce ne sont pas des cases à cocher de confort : ce sont les seules preuves que l'ingestion dit vrai.

1. Ingestion complète du référentiel puis des trois législatures, sans intervention.
2. Les compteurs correspondent aux attendus ci-dessus pour les 15e et 16e, closes donc stables.
3. `VTCGR5L16V1` porte 902 votes et `chambre = 'congres'`.
4. Relancer les quatre jobs ne change **aucune** ligne (condensés identiques).
5. Ingérer la seule 16e sur une base portant déjà les trois : rien d'autre n'est modifié.
6. `SELECT count(*) FROM vote v LEFT JOIN person p ON p.id = v.person_id WHERE p.id IS NULL` renvoie 0.
7. Aucune anomalie d'un `kind` non prévu par le tableau ci-dessus ; les nombres attendus sont atteints.
8. **Vérification manuelle** : pour trois député·es choisi·es à la main — dont un·e ayant changé de groupe
   en cours de législature — comparer la liste des votes et l'historique de groupe à ce qu'affiche le site
   de l'Assemblée nationale. Consigner le résultat dans le message du commit final.
9. CI verte.

### Hors périmètre — ne pas ajouter

Pas de page publique, pas de route d'API, pas de catégorisation, pas de score, pas de vue matérialisée
d'agrégat, pas de back-office. Pas d'entité éditoriale « parti » ni de lien de succession entre partis :
c'est la phase 4, et le faire ici figerait des choix éditoriaux sans les avoir discutés. Pas de Sénat, pas
de Parlement européen, pas de 14e législature.

En revanche, terminer par la mise à jour de la section « Commandes » de [CLAUDE.md](../../CLAUDE.md) avec
les cibles réellement disponibles, et par la consignation des chiffres réellement obtenus — s'ils diffèrent
de ceux annoncés ici, corriger [data-sources.md](../data-sources.md) dans le même commit.

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
