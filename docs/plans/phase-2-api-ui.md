# Phase 2 — Pages publiques

**Statut : ✅ validé** · Dépend de : phases 0 et 1 · Bloque : rien (mais éclaire les phases 3 et 4)

## Objectif

Rendre les données visibles. À la fin de cette phase, le site est déjà utile sans aucun score : on peut
parcourir les candidat·es, ouvrir une fiche, et lire l'historique réel de ses votes et de ses
appartenances politiques.

Cette phase est placée avant la catégorisation exprès : regarder les vraies données à l'écran révèle des
problèmes de modèle qu'aucune relecture de plan ne détecte.

## Périmètre

**Dedans** : accueil avec la liste des candidat·es et leur photo, annuaire des personnes ingérées, fiche
d'une personne, page d'un scrutin, recherche, API JSON, design de base, accessibilité, mentions de
sources.

**Dehors** : scores par thème et explications (phase 4), back-office (phase 3), authentification.

## Livrables

| Route | Contenu |
| --- | --- |
| `/` | Grille des candidat·es : photo, nom, dernier groupe connu, nombre de votes connus. Recherche en HTMX |
| `/deputes` | Annuaire des personnes ingérées : recherche, filtre par législature, pagination |
| `/personne/{slug}` | En-tête d'identité, frise des appartenances politiques, votes récents, zone « orientations » avec un état vide explicite en attendant la phase 4 |
| `/personne/{slug}/votes` | Liste paginée et filtrable (thème quand il existera, période, position) |
| `/scrutin/{legislature}/{numero}` | Détail d'un scrutin : titre, date, résultat, répartition par groupe, lien vers la source AN |
| `/methodologie`, `/sources` | Rendu de [methodology.md](../methodology.md) et [data-sources.md](../data-sources.md) — accessibles depuis chaque page |
| `/api/v1/…` | Équivalents JSON des pages, pour la réutilisation des données |

> **`/personne/` et non `/candidat/`** (changé le 17/08/2026, en même temps que le plan d'exécution) :
> D2.2 rend la fiche consultable pour n'importe quelle personne ingérée, dont l'immense majorité n'est
> candidate à rien. Servir la fiche d'un·e député·e sous une URL qui la déclare candidate serait une
> affirmation non sourcée — exactement ce que la méthodologie interdit. Être candidat·e est un **statut
> affiché sur la fiche**, avec sa source et sa date, pas une propriété de son adresse.

## ⚠️ À concevoir ici, pas ailleurs : l'architecture front

> **Fait le 17/08/2026.** Les cinq points ci-dessous ont été tranchés avec le mainteneur avant l'écriture
> du plan d'exécution : voir D2.6 (structure de la feuille), D2.7 (frise), D2.8 (tests), D2.11 (palette)
> et les sections « Architecture CSS », « La frise » et « Stratégie de test » du plan d'exécution. Cette
> section reste ici pour dire **ce qui devait être décidé et pourquoi** ; elle n'est plus ouverte.

La phase 0 ne pose qu'un `style.css` volontairement pauvre et deux conventions (fragments sur routes
explicites, gabarits `_` n'étendant pas `base.html.jinja`). **Tout le reste du front se décide dans cette
phase, avec le mainteneur, avant d'écrire les gabarits** — sans quoi il s'installera par accident :

- **structure de la feuille de style** : variables CSS, `@layer`, découpage en plusieurs fichiers
  concaténés ou fichier unique, convention de nommage des classes ;
- **échelles** typographique et d'espacement, palette, contrastes — sachant la contrainte de neutralité :
  aucune couleur ne doit suggérer un jugement, et les couleurs de parti viennent d'une table documentée ;
- **stratégie responsive** : la grille de candidat·es et surtout **la frise des appartenances** sont les
  deux composants difficiles sur petit écran. La frise est l'élément signature de l'UI, son implémentation
  (grille CSS, SVG généré côté serveur ?) est une vraie question de conception ;
- **organisation des gabarits** : blocs, macros, nommage, découpage — et à quel moment on extrait un
  fragment ;
- **stratégie de test des pages** : que vérifie-t-on d'un rendu Jinja/HTMX, et à quel niveau ? La phase 0
  s'arrête volontairement au test de fumée.

## Points de conception

- **La frise des appartenances est un élément central**, pas un ornement : c'est la représentation
  directe des `daterange` et c'est ce qui rendra lisibles les positions de parti par période en phase 4.
- **Les états vides sont du contenu.** « Cette personne n'a jamais siégé à l'Assemblée nationale, ses
  votes personnels ne sont donc pas disponibles » est une information utile, pas une page cassée.
- **Chaque bloc affiche sa source** et sa date de récupération.
- **HTMX pour la recherche, les filtres et la pagination.** Si une interaction semble exiger du JS,
  chercher d'abord la solution HTMX ; le JS ponctuel reste possible mais doit être justifié.
- **Accessibilité et sobriété** : contrastes suffisants, navigation clavier, pages qui fonctionnent sans
  JS pour l'essentiel, pas de police ni de script distant. Un site de transparence ne pose pas de
  mouchards.
- **Pas de couleurs politiques implicites.** Les couleurs de parti sont acceptables si elles viennent
  d'une table documentée ; aucune palette « bon / mauvais ».

## Étapes

1. Définir la liste des candidat·es v1 (voir décisions) et la table `candidate` associée.
2. Gabarits Jinja : layout, en-tête, pied de page avec licences et sources.
3. Accueil + recherche HTMX.
4. Fiche candidat : identité, frise, votes récents.
5. Page scrutin et page votes filtrée.
6. API JSON en miroir des pages.
7. Passe accessibilité et performance (mesurer le temps de rendu des pages, viser une page candidat en
   moins de 100 ms côté serveur).
8. ~~Supprimer les routes de démonstration de la phase 0~~ — **fait par anticipation en phase 1.1**
   (F11, [phase-1.1-fix.md](phase-1.1-fix.md)) : `POST /dev/jobs` créait un job depuis une route
   publique non authentifiée, ce que CLAUDE.md interdit sans condition ; ça n'avait pas de raison
   d'attendre la phase 2. `routers/dev.py`, son montage, et le réglage `enable_dev_routes` ont déjà
   disparu. Les gabarits de suivi de progression (`dev_job.html.jinja`, `_job_status.html.jinja`)
   sont volontairement restés, non montés : c'est cette étape qui les reprend, derrière le
   back-office de la [phase 3](phase-3-categorisation.md). Le déplacer, pas le détruire.

## Décisions arbitrées

Toutes tranchées le 17/08/2026, avant le plan d'exécution. **Elles ne sont pas à rediscuter en cours
d'implémentation** : si l'une s'avère fausse au contact du code, le dire et proposer une révision.

| # | Question | Décision |
| --- | --- | --- |
| D2.1 | Qui apparaît sur l'accueil en 2026 ? | Une table `candidate` alimentée par un **seed versionné** (`db/seeds/candidates.toml`), une ligne par personne avec son statut (`declare`, `pressenti`, `retire`), sa source et la date de cette source. Appliqué par un job worker, jamais par le backend : la sélection des candidat·es est un choix éditorial, il doit être explicite, sourcé et relisible en diff |
| D2.2 | Les personnes non candidates sont-elles consultables ? | Oui, et pas seulement par URL directe : un **annuaire `/deputes`** les rend parcourables. Sans lui la phase 2 n'afficherait qu'une poignée de fiches largement vides, ce qui la priverait de sa raison d'être — voir les vraies données à l'écran |
| D2.3 | Slug d'URL | `prenom-nom`, suffixe numérique en cas d'homonymie. Table `person_slug` **historisée** : un slug attribué n'est jamais réattribué ni réécrit, les anciens restent et servent de table de redirection (301). La stabilité des liens sort du même mécanisme, sans seconde table |
| D2.4 | CSS | Feuille écrite à la main, sans framework — cohérent avec « pas de bundler » |
| D2.5 | Mise en cache HTTP | `Cache-Control` court + `ETag` sur les pages publiques ; à réévaluer en phase 5 |
| D2.6 | Architecture CSS | Plusieurs fichiers sources numérotés, **concaténés par `make css`** en un `style.css` unique, commité, et dont la CI vérifie la fraîcheur. Voir « Architecture CSS » dans le plan d'exécution |
| D2.7 | Implémentation de la frise | **HTML + grille CSS**, positions calculées côté serveur par une fonction pure. Chaque mandat est un élément de liste réel — texte sélectionnable, lisible par un lecteur d'écran sans travail supplémentaire — et sous le point de rupture la grille se replie en liste chronologique. Pas de SVG : son texte se redimensionne mal et son accessibilité demande une table de repli qu'il faudrait maintenir en double |
| D2.8 | Niveau de test des pages | Trois niveaux, sans navigateur : fonctions pures, routes avec assertions sur le HTML **parsé**, et invariants structurels appliqués à toutes les routes. Pas de Playwright ni d'axe-core en CI : le coût (navigateur à installer, minutes de test) n'est pas justifié à ce stade |
| D2.9 | Photos et domaines tiers | **Lien direct vers Wikimedia Commons**, en vignette dérivée de `commons_file`. Ce qu'on accepte de perdre est réel et doit être écrit : l'adresse IP des visiteurs part chez un tiers à chaque chargement de page. Mitigations : `referrerpolicy="no-referrer"`, `loading="lazy"`, aucun cookie tiers possible sur une image. Décision réévaluable en phase 5 — l'alternative (job worker qui télécharge les vignettes en base, backend qui les sert) reste ouverte et n'invalide ni le schéma ni les gabarits |
| D2.10 | Périmètre de l'API JSON | **Dans la phase 2.** Aucune autre phase ne la porte, et la phase 4 en parle déjà comme d'un existant (« rendre le contexte inséparable du chiffre, y compris dans l'API »). Surface minimale et versionnée `/api/v1/`, calquée sur les quatre objets réels des pages |
| D2.11 | Couleurs de parti | **Aucune en phase 2.** Le plan les autorise « si elles viennent d'une table documentée » ; cette table n'existe pas et l'inventer serait précisément le geste interdit. Les segments de la frise se distinguent par la valeur et le motif, pas par la couleur politique. La table documentée est un sujet de phase 4, où elle aura un usage |

## Plan d'exécution

Cette section est destinée à la session qui implémentera la phase. Elle est autoportante : **tout ce qui
est nécessaire est ici, dans [methodology.md](../methodology.md), dans
[data-sources.md](../data-sources.md), dans [architecture.md](../architecture.md) ou dans
[CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de conversation à retrouver. Les décisions
arbitrées ci-dessus ne sont pas à rediscuter.

Développement sur le tronc : **commits directs sur `main`**, pas de branche ni de PR. Un commit par
étape, `main` vert (lint, typage, tests) à chaque fois. Ne pas modifier les plans des autres phases.

**Langue** (rappel de [CLAUDE.md](../../CLAUDE.md#langue), parce que cette phase produit beaucoup de
noms) : identifiants Python, SQL, classes CSS et noms de gabarits **en anglais**, sauf les termes du
domaine parlementaire qui n'ont pas d'équivalent propre (`scrutin`, `organe`, `mandat`, `groupe`,
`legislature`). Donc `timeline`, pas `frise` ; `person`, pas `personne` ; `.timeline__segment`, pas
`.frise__segment`. En revanche **les URL et tout le texte affiché sont en français** : `/personne/{slug}`,
`/deputes`, `/methodologie`.

### Ce que la phase 2 ne fait pas

Pas de score, pas de thème, pas de catégorisation, pas de back-office, pas d'authentification, pas de
route publique créant un job, pas d'appel HTTP sortant depuis le backend, pas de framework JS, pas de
couleur de parti (D2.11). Le worker ne gagne aucun job d'ingestion : les trois jobs ajoutés ci-dessous
ne touchent qu'à des données dérivées ou éditoriales.

Les gabarits `dev_job.html.jinja` et `_job_status.html.jinja` **restent en place, non montés** : c'est la
phase 3 qui les reprend derrière l'authentification admin (étape 8 ci-dessus). Ne pas les supprimer, ne
pas les monter, et **conserver leurs règles CSS** dans le fichier de composants avec un commentaire
disant pourquoi.

### Arborescence cible

```
db/
  migrations/0005_pages.sql   candidate, person_slug, index de recherche, vue matérialisée
  seeds/candidates.toml       la liste éditoriale, une entrée par candidat·e, avec sa source

worker/src/
  jobs/
    assign_slugs.rs           attribution des slugs, jamais de réattribution
    seed_candidates.rs        application du seed, création des personnes hors AN
    refresh_views.rs          REFRESH MATERIALIZED VIEW CONCURRENTLY
  slug.rs                     fonction pure de translittération, testée seule

worker/tests/
  assign_slugs.rs
  seed_candidates.rs

backend/src/kyc_api/
  queries/                    tout le SQL, un module par agrégat, aucune requête ailleurs
    persons.py  scrutins.py  candidates.py
  schemas/                    modèles Pydantic partagés par les pages et l'API
    person.py  scrutin.py  vote.py  common.py
  timeline.py                 placement de la frise : fonction pure, aucun accès base
  labels.py                   vocabulaire d'affichage (positions, causes de non-vote, statuts)
  photos.py                   URL de vignette Commons dérivée de commons_file
  documents.py                rendu des documents Markdown du dépôt
  http_cache.py               ETag + Cache-Control
  routers/
    health.py  pages.py  fragments.py  api.py
  static/
    css/00-reset.css 10-tokens.css 20-base.css 30-layout.css 40-components.css 50-utilities.css
    style.css                 GÉNÉRÉ par `make css`, commité, jamais édité à la main
  templates/
    base.html.jinja  home.html.jinja  directory.html.jinja  person.html.jinja
    person_votes.html.jinja  scrutin.html.jinja  document.html.jinja
    macros/timeline.html.jinja  macros/vote.html.jinja  macros/person_card.html.jinja
    macros/source.html.jinja  macros/pagination.html.jinja
    _search_results.html.jinja  _vote_list.html.jinja  _directory_list.html.jinja

backend/tests/
  factories.py                insertion de jeux de données réalistes dans la transaction du test
  test_timeline.py  test_photos.py  test_slugs_redirect.py
  test_home.py  test_directory.py  test_person.py  test_votes.py  test_scrutin.py
  test_documents.py  test_api_v1.py  test_http_cache.py
  test_html_invariants.py     invariants structurels, paramétrés sur toutes les routes
```

### Migration `0005_pages.sql`

```sql
-- Phase 2 — pages publiques : identité éditoriale des candidat·es, slugs stables, aperçus
-- pré-calculés pour l'accueil et l'annuaire.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Une personne peut n'avoir jamais siégé -------------------------------------------------------
--
-- La phase 1 supposait que toute personne vient d'AMO30, d'où `an_uid NOT NULL`. C'est faux pour une
-- candidate qui n'a jamais été députée (methodology.md § 7.4 : « beaucoup de candidat·es n'ont jamais
-- siégé »). On lève la contrainte plutôt que de dupliquer l'identité dans une seconde table : une
-- personne reste une personne, seule sa provenance change.

ALTER TABLE person ALTER COLUMN an_uid DROP NOT NULL;

ALTER TABLE person ADD CONSTRAINT person_a_au_moins_un_identifiant
    CHECK (an_uid IS NOT NULL OR wikidata_qid IS NOT NULL);

-- 2. Le statut de candidat·e, éditorial et sourcé --------------------------------------------------
--
-- `source_url` et `source_date` sont NOT NULL par construction, sur le modèle de
-- `person_photo.licence` : la règle « une donnée sans source affichable n'entre pas en base »
-- appartient au schéma, pas au code applicatif qui pourrait l'oublier.

CREATE TYPE candidate_statut AS ENUM ('declare', 'pressenti', 'retire');

CREATE TABLE candidate (
    person_id   bigint PRIMARY KEY REFERENCES person (id) ON DELETE CASCADE,
    statut      candidate_statut NOT NULL,
    source_url  text NOT NULL,
    source_date date NOT NULL,
    note        text,
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT candidate_source_non_vide CHECK (btrim(source_url) <> '')
);

-- 3. Slugs historisés (D2.3) -----------------------------------------------------------------------
--
-- Le slug est la clé : une ligne par slug jamais attribué deux fois. `is_current` distingue le slug
-- servi du ou des anciens, qui restent pour rediriger en 301. L'index partiel garantit qu'une
-- personne n'a jamais deux slugs courants.

CREATE TABLE person_slug (
    slug       text PRIMARY KEY,
    person_id  bigint NOT NULL REFERENCES person (id) ON DELETE CASCADE,
    is_current boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX person_slug_courant_idx ON person_slug (person_id) WHERE is_current;

-- 4. Recherche par nom complet ---------------------------------------------------------------------
--
-- `person_nom_trgm_idx` (migration 0002) ne couvre que `nom` : taper « jean-luc mélenchon » ne
-- trouve rien. pg_trgm est déjà activé par la migration 0001.

CREATE INDEX person_recherche_trgm_idx ON person
    USING gin ((coalesce(prenom, '') || ' ' || coalesce(nom, '')) gin_trgm_ops);

-- 5. Aperçu par personne ---------------------------------------------------------------------------
--
-- L'accueil et l'annuaire ont besoin, par personne, du nombre de votes et du dernier groupe connu.
-- Compter les votes de 1 900 personnes dans une table de 2,3 M de lignes à chaque affichage est
-- exactement ce que l'architecture interdit au backend : le calcul devient donc une vue
-- matérialisée, rafraîchie par le worker (architecture.md § 1).
--
-- « Dernier groupe connu » et non « groupe actuel » : un `CURRENT_DATE` dans une vue matérialisée
-- est figé à l'instant du rafraîchissement et se met à mentir en silence. On classe par fin de
-- mandat, ce qui ne dépend pas de la date de calcul, et l'UI affiche la période plutôt que d'affirmer
-- un présent.
--
-- Les compteurs excluent le Congrès (methodology.md, cas VTCGR5L16V1) : le chiffre affiché dit
-- « votes à l'Assemblée », il doit donc être exactement cela. Le vote du Congrès reste visible dans
-- la liste des votes, signalé comme tel.

CREATE MATERIALIZED VIEW person_apercu AS
SELECT p.id                                            AS person_id,
       s.slug,
       p.civilite, p.prenom, p.nom, p.date_deces,
       ph.url                                          AS photo_url,
       ph.commons_file, ph.licence, ph.licence_url, ph.auteur,
       coalesce(v.votes_total, 0)                      AS votes_total,
       v.premier_vote,
       v.dernier_vote,
       g.organe_id                                     AS dernier_groupe_id,
       g.libelle                                       AS dernier_groupe_libelle,
       g.libelle_abrege                                AS dernier_groupe_abrege,
       g.is_non_inscrit                                AS dernier_groupe_non_inscrit,
       g.period                                        AS dernier_groupe_period,
       coalesce(l.legislatures, '{}')                  AS legislatures,
       (c.person_id IS NOT NULL)                       AS est_candidat
FROM person p
LEFT JOIN person_slug s ON s.person_id = p.id AND s.is_current
LEFT JOIN person_photo ph ON ph.person_id = p.id
LEFT JOIN candidate c ON c.person_id = p.id
LEFT JOIN LATERAL (
    SELECT count(*) AS votes_total,
           min(sc.date_scrutin) AS premier_vote,
           max(sc.date_scrutin) AS dernier_vote
    FROM vote v
    JOIN scrutin sc ON sc.id = v.scrutin_id
    WHERE v.person_id = p.id AND sc.chambre = 'assemblee'
) v ON true
LEFT JOIN LATERAL (
    SELECT o.id AS organe_id, o.libelle, o.libelle_abrege, o.is_non_inscrit, m.period
    FROM mandat m
    JOIN organe o ON o.id = m.organe_id
    WHERE m.person_id = p.id AND m.type_organe = 'GP'
    ORDER BY upper(m.period) DESC NULLS FIRST, lower(m.period) DESC, m.id DESC
    LIMIT 1
) g ON true
LEFT JOIN LATERAL (
    SELECT array_agg(DISTINCT m.legislature ORDER BY m.legislature) AS legislatures
    FROM mandat m
    WHERE m.person_id = p.id AND m.type_organe = 'ASSEMBLEE' AND m.legislature IS NOT NULL
) l ON true;

-- Unique et non partiel : REFRESH ... CONCURRENTLY l'exige.
CREATE UNIQUE INDEX person_apercu_pk_idx ON person_apercu (person_id);
CREATE UNIQUE INDEX person_apercu_slug_idx ON person_apercu (slug);
CREATE INDEX person_apercu_votes_idx ON person_apercu (votes_total DESC);
```

> `person_apercu_slug_idx` est `UNIQUE` sur une colonne qui peut être `NULL` (personne sans slug
> courant, transitoirement) : Postgres autorise plusieurs `NULL` dans un index unique, c'est bien le
> comportement voulu. En revanche l'annuaire et l'accueil **filtrent sur `slug IS NOT NULL`** — une
> personne sans slug n'a pas d'URL, donc rien à lier.

**Un test d'intégration Rust par contrainte**, sur le modèle de
[`worker/tests/referentiel.rs`](../../worker/tests/referentiel.rs) : `person` sans aucun identifiant
refusée, `candidate` sans source refusée, deux slugs courants pour une même personne refusés, deux
personnes partageant un slug refusées, et `person_apercu` qui compte bien zéro vote pour une personne
sans vote (le `coalesce`, qui est le genre de détail qu'on croit acquis).

### Le seed des candidat·es

`db/seeds/candidates.toml` — TOML et non YAML : le format a une bibliothèque Rust maintenue et de
première classe, là où l'écosystème YAML de Rust est en cours d'abandon.

```toml
# Liste éditoriale des candidat·es à l'élection présidentielle de 2027.
#
# Chaque entrée est un choix éditorial : elle doit porter une source publique et vérifiable, et la
# date à laquelle cette source a été publiée. Une candidature annoncée par un tiers ne suffit pas ;
# on cite la déclaration, pas le commentaire qu'on en fait.
#
# Identification : `an_uid` pour toute personne passée par l'Assemblée depuis la XIe législature —
# c'est le cas le plus fiable, la personne existe déjà en base. `wikidata_qid` sinon : le job crée
# alors la personne, avec ce seul identifiant.
#
# statut : declare | pressenti | retire

[[candidate]]
an_uid = "PA642788"
statut = "declare"
source_url = "https://…"
source_date = 2026-05-14
note = "Déclaration au journal télévisé de 20 h"

[[candidate]]
wikidata_qid = "Q3488036"
prenom = "…"
nom = "…"
statut = "pressenti"
source_url = "https://…"
source_date = 2026-07-02
```

Le job `seed_candidates` :

1. lit le fichier (chemin par défaut `db/seeds/candidates.toml`, surchargeable par le payload) ;
2. **valide tout avant d'écrire quoi que ce soit** : statut connu, source non vide, exactement un des
   deux identifiants, `prenom`/`nom` obligatoires quand l'entrée est identifiée par `wikidata_qid`,
   aucun doublon d'identifiant dans le fichier. Un fichier invalide **fait échouer le job** sans
   toucher la base — c'est une donnée éditoriale, pas un flux distant : deviner n'a aucun sens ;
3. résout chaque entrée : `an_uid` → la personne **doit** exister (sinon échec, avec l'uid en clair —
   AMO30 couvre toutes les législatures depuis la XIe, une absence signale une faute de frappe) ;
   `wikidata_qid` → la personne si elle existe, sinon `INSERT INTO person (wikidata_qid, prenom, nom)` ;
4. `INSERT … ON CONFLICT (person_id) DO UPDATE` sur `candidate`, avec la clause
   `WHERE … IS DISTINCT FROM …` habituelle du dépôt pour ne pas toucher `updated_at` sans changement ;
5. **supprime les lignes de `candidate` absentes du fichier** — le seed est la source de vérité de
   cette table, et le retrait d'une candidature doit se voir. C'est la seule suppression autorisée de
   la phase : elle ne touche ni `person`, ni un vote, ni une donnée ingérée, seulement l'index
   éditorial. À écrire dans le journal du run ;
6. journalise dans `ingestion_run` : entrées lues, créées, mises à jour, retirées, personnes créées.

**Pourquoi un fichier tenu à la main et pas une ingestion** — la question mérite sa réponse écrite,
parce qu'elle changera. Il n'existe aujourd'hui **aucune source officielle des candidatures** : se
déclarer candidat·e n'est pas un acte administratif, l'État n'en tient pas registre. Deux sources
officielles apparaîtront début 2027, toutes deux du Conseil constitutionnel : la **publication des
parrainages** pendant la période de recueil (intégrale et bihebdomadaire depuis la loi organique de
2016), puis la **liste définitive arrêtée et publiée au Journal officiel**, environ un mois avant le
premier tour. Jusque-là, la seule source défendable est la déclaration de la personne elle-même — d'où
`source_url` + `source_date` : la ligne n'affirme pas « X est candidat », elle affirme « le JJ/MM/AAAA,
X a déclaré sa candidature ». Quand la liste officielle existera, elle deviendra la source primaire et
le seed n'aura plus à porter que ce qu'elle ne dit pas (les candidatures annoncées puis non validées) ;
un statut supplémentaire, distinguant la déclaration de soi de la validation par le Conseil, sera alors
justifié. Ne pas le prévoir maintenant : un statut sans donnée derrière est un statut qu'on remplira
mal.

Livrer le fichier avec **deux ou trois entrées réelles au maximum**, choisies parce que leur source est
indiscutable, dont **au moins une personne sans mandat à l'Assemblée** — c'est le cas qui casse, il doit
être exercé dès le premier jour. La liste complète est un travail éditorial, pas un travail
d'implémentation : ne pas la remplir de mémoire, et ne jamais inventer une URL de source.

### Les trois jobs du worker

Aucun n'accède au réseau. Chacun suit le contrat de la phase 0 : garde de propriété sur les écritures
de job, progression rapportée, tâche supervisée.

| Job | Payload | Effet |
| --- | --- | --- |
| `assign_slugs` | `{}` | Attribue un slug aux personnes qui n'en ont pas de courant |
| `seed_candidates` | `{ "path"?: string }` | Applique `db/seeds/candidates.toml` |
| `refresh_views` | `{}` | `REFRESH MATERIALIZED VIEW CONCURRENTLY person_apercu` |

`assign_slugs` — **n'écrase jamais un slug existant** ; c'est toute la propriété qui rend les URL
stables. Pour chaque personne sans slug courant, dans l'ordre de `person.id` (déterminisme) :

- slug candidat = `slugify(prenom + " " + nom)` ;
- si le nom est vide ou que le slug est vide après translittération, utiliser `an_uid` ou
  `wikidata_qid` en minuscules, et **journaliser une anomalie** `slug_derive_d_identifiant` — c'est
  laid, ça doit se voir ;
- si le slug est déjà pris par une **autre** personne (y compris par un slug non courant : les anciens
  restent réservés, sinon la redirection pointerait vers quelqu'un d'autre), suffixer `-2`, `-3`, … ;
- `INSERT INTO person_slug (slug, person_id, is_current) VALUES (…, true)`.

`slugify` vit dans `worker/src/slug.rs`, **fonction pure testée seule** : décomposition NFD puis
suppression des marques combinantes (`unicode-normalization`), minuscules, tout ce qui n'est pas
`[a-z0-9]` devient `-`, séquences de `-` réduites, `-` en tête et en queue retirés. Cas à couvrir
explicitement : `Mélenchon` → `melenchon`, `Le Pen` → `le-pen`, `Jean-Luc` → `jean-luc`,
`d'Estaing` → `d-estaing`, `Ó Ríordáin` → `o-riordain`, `Æ` (ne se décompose pas en NFD, doit être
traité ou tomber sur le repli identifiant), chaîne vide, chaîne de ponctuation seule.

`make ingest` gagne trois lignes à la fin, dans cet ordre — les slugs avant le seed (une personne créée
par le seed doit ensuite recevoir son slug, donc **`assign_slugs` tourne aussi après**), et
`refresh_views` en dernier :

```
cargo run --release -- enqueue seed_candidates
cargo run --release -- enqueue assign_slugs
cargo run --release -- enqueue refresh_views
```

Rappel d'outillage : après toute modification d'une requête `sqlx::query!`/`query_as!`, régénérer avec
**`cargo sqlx prepare -- --all-targets`**, sans quoi la CI casse en mode hors ligne.

### Le backend : ce qui change dans le socle

- **`Queryable` (db.py) gagne `fetch`.** Le protocole n'expose que `fetchrow`/`fetchval`/`execute` ;
  toutes les listes de cette phase en ont besoin. Même signature, même raison d'être : le sous-ensemble
  commun à `Pool` et `Connection`, pour que les tests puissent injecter une connexion en transaction.
- **Tout le SQL vit dans `queries/`.** Aucune requête dans un routeur ni dans un gabarit. Chaque
  fonction rend des modèles Pydantic de `schemas/`, jamais des `asyncpg.Record` : les gabarits et l'API
  partagent alors exactement la même donnée, ce qui est la seule façon de garantir qu'ils ne divergent
  pas.
- **Aucun `SELECT` ne compte, n'agrège ni ne boucle sur les votes** ailleurs que par clé indexée. Si
  une page semble l'exiger, c'est une colonne ou une vue matérialisée qui manque.

### Routes exactes

Pages (HTML complet) :

| Méthode et route | Contenu | Réponses |
| --- | --- | --- |
| `GET /` | Candidat·es (`candidate` ⋈ `person_apercu`), champ de recherche | 200 |
| `GET /deputes?q=&legislature=&page=` | Annuaire paginé | 200 |
| `GET /personne/{slug}` | Fiche | 200 · **301** vers le slug courant si `slug` est ancien · 404 |
| `GET /personne/{slug}/votes?…` | Liste complète filtrable | 200 · 301 · 404 |
| `GET /scrutin/{legislature}/{numero}` | Détail | 200 · 404 |
| `GET /scrutin/{an_uid}` | Alias | **301** vers l'URL canonique · 404 |
| `GET /methodologie`, `GET /sources` | Documents du dépôt rendus | 200 |

Fragments HTMX (convention de la phase 0 : routes explicites sous `/fragments/`, gabarits préfixés `_`,
n'étendant pas `base.html.jinja`) :

| Route | Rendu |
| --- | --- |
| `GET /fragments/recherche?q=` | `_search_results.html.jinja` |
| `GET /fragments/deputes?q=&legislature=&page=` | `_directory_list.html.jinja` |
| `GET /fragments/personne/{slug}/votes?…` | `_vote_list.html.jinja` |

**Chaque fragment a une page équivalente qui fonctionne sans JS.** La règle pratique : le `hx-get`
d'un lien ou d'un formulaire pointe sur `/fragments/…`, son `href`/`action` pointe sur la page. HTMX
désactivé, le lien reste un lien.

API (`/api/v1/`) — miroir des mêmes objets, servi par les **mêmes fonctions de `queries/`** :

```
GET /api/v1/candidats
GET /api/v1/personnes?q=&legislature=&page=
GET /api/v1/personnes/{slug}
GET /api/v1/personnes/{slug}/votes?legislature=&position=&du=&au=&avant=
GET /api/v1/scrutins/{legislature}/{numero}
```

Toute réponse porte un bloc `source` : URL du document d'origine à l'Assemblée, date de récupération
(`source_document.fetched_at`), licence (`Licence Ouverte (Etalab)` pour l'AN, `CC0` pour Wikidata,
la licence du fichier pour une photo). C'est le pendant machine de la règle « chaque bloc affiche sa
source » — un consommateur de l'API doit pouvoir citer la donnée sans revenir sur le site. Les listes
portent un bloc `pagination`. Le slug ancien redirige aussi en 301 côté API.

Un vieux slug rend **301 et non 302** : le lien est réputé définitif, et c'est ce qui préserve le
référencement d'une fiche partagée avant un changement de nom.

### La frise : spécification du calcul

`timeline.py`, **fonction pure, aucun accès à la base, `today` injecté** (jamais `date.today()` à
l'intérieur : une frise doit être testable à une date fixe).

```python
def build_timeline(segments: Sequence[Segment], *, today: date) -> Timeline
```

Règles, toutes à couvrir par un test :

1. **Unité de colonne : le mois.** Le domaine va du premier jour du mois du plus ancien début au
   premier jour du mois suivant la fin la plus tardive (ou du mois suivant `today` pour un mandat en
   cours). Une législature fait ~60 colonnes, trois en font ~110 : `grid-template-columns: repeat(N,
   1fr)` reste raisonnable et l'arithmétique est entière, donc exactement testable.
2. `colonne(date) = mois_entre(domaine.debut, date) + 1` (CSS indexe à 1).
3. **Un segment occupe au minimum une colonne.** Un mandat d'un seul jour — il en existe, un mandat de
   non-inscrit d'une journée est mesuré dans [data-sources.md](../data-sources.md) — ne doit pas
   disparaître à l'arrondi. `colonne_fin = max(colonne(fin), colonne_debut + 1)`.
4. `fin = None` (mandat en cours) court jusqu'à la dernière colonne, et le gabarit écrit « depuis le
   … », jamais une date de fin inventée.
5. **Les chevauchements sont réels et se superposent en rangées.** 74 chevauchements inter-organes sont
   conservés en base par décision (D1.16), et les pseudo-mandats de non-inscrit chevauchent en continu
   le groupe suivant. Deux segments qui se chevauchent dans une même piste vont donc sur deux rangées :
   placement glouton, segments triés par début croissant puis fin croissante puis `an_uid` (départage
   déterministe — la leçon de F7 en phase 1.1), chacun sur la première rangée libre.
6. **Trois pistes**, dans cet ordre, chacune avec son intitulé exact tiré de
   [methodology.md](../methodology.md#ce-que-appartenir-à-un-parti-veut-dire-ici) :
   - *Groupes parlementaires* — mandats `GP`. « a siégé au groupe X du … au … » ;
   - *Rattachements* — mandats `PARPOL`. « rattaché·e au parti X au titre du financement de la vie
     politique, déclaration du … ». **Jamais « membre de »** ;
   - *Mandats de député·e* — mandats `ASSEMBLEE`, qui donnent l'échelle et le contexte.
   Wikidata `P102` (adhésion) n'est **pas** ingéré à ce stade : ne pas l'afficher, ne pas l'inventer.
7. **Les non-inscrits ne sont pas une appartenance.** Un mandat `GP` dont l'organe porte
   `is_non_inscrit` s'affiche « non-inscrit·e », avec un motif distinct, et le gabarit n'écrit jamais
   « a siégé au groupe ». Règle de la méthodologie, pas une préférence graphique.
8. **Aucun segment, aucune frise** : liste vide → le gabarit affiche l'état vide et pas une grille de
   zéro colonne.
9. Une rangée d'échelle donne les années, chacune couvrant ses colonnes ; c'est le seul repère
   temporel, il doit rester lisible replié.

Rendu : un `<ol>` par piste, un `<li>` par segment portant `style="grid-column: {{ début }} / {{ fin }}"`
et **le texte complet en clair à l'intérieur** — c'est ce texte qui rend la frise accessible, pas un
`aria-label`. Sous le point de rupture, `grid-template-columns: 1fr` et chaque `li` occupe la ligne
entière : la frise devient la liste chronologique, sans duplication de contenu ni règle `display: none`
sur de l'information.

### Photos (D2.9)

`person_photo.url` est l'URL du **fichier d'origine** sur Commons — plusieurs Mo par image, inaffichable
dans une grille. La vignette se dérive de `commons_file` :

```
https://commons.wikimedia.org/wiki/Special:FilePath/{fichier encodé}?width=400
```

`Special:FilePath` redirige vers la vignette redimensionnée ; c'est un point d'entrée documenté et
stable. Fonction pure dans `photos.py`, testée sur : espaces → `_`, caractères accentués, apostrophes,
`commons_file` absent (→ pas de photo, placeholder), URL sans nom de fichier.

Balise attendue :

```html
<img src="…?width=400" width="400" height="400" alt=""
     loading="lazy" decoding="async" referrerpolicy="no-referrer">
```

`alt=""` est **volontaire et correct** : le nom de la personne est déjà écrit juste à côté, un `alt`
qui le répète fait entendre l'information deux fois à un lecteur d'écran. Le commenter dans le gabarit,
sinon quelqu'un le « corrigera ». `width`/`height` évitent le décalage de mise en page au chargement.

**Auteur et licence sont affichés avec la photo**, pas en pied de page : c'est une obligation
CC-BY-SA, pas une politesse. Une personne sans photo reçoit un cadre de repli dimensionné à
l'identique, jamais un trou dans la grille.

### Les sources affichées

Chaque bloc de contenu affiche d'où il vient et quand il a été récupéré :

- **un scrutin** : `scrutin.source_document_id` → `source_document.url` et `fetched_at` ;
- **une personne** : pas de lien direct en base ; requête
  `SELECT url, fetched_at FROM source_document WHERE source = 'an_acteur' AND uid = $1
   ORDER BY fetched_at DESC LIMIT 1`, servie par `source_document_courant_idx` ;
- **une photo** : `licence`, `licence_url`, `auteur` de `person_photo` ;
- **un statut de candidat·e** : `source_url` et `source_date` de `candidate`.

En plus du lien open data, la page donne le lien vers la page publique de l'Assemblée. **Vérifier les
deux formats d'URL en ouvrant réellement une page de chaque sorte** avant de les coder, et consigner le
format confirmé dans [data-sources.md](../data-sources.md) — l'Assemblée a déjà réorganisé ses URL. Si
un format ne se confirme pas, s'en tenir au lien open data, qui, lui, vient de la base.

### Vocabulaire d'affichage

`labels.py`, table unique, réutilisée par les pages et par l'API. Formulations imposées par
[methodology.md](../methodology.md) — les recopier, ne pas les reformuler :

| Donnée | Affichage |
| --- | --- |
| `pour` / `contre` / `abstention` | « a voté pour » · « a voté contre » · « s'est abstenu·e » |
| `non_votant` + cause `PSE` | « n'a pas pris part au vote : présidait la séance » |
| `non_votant` + cause `PAN` | « n'a pas pris part au vote : présidait l'Assemblée nationale » |
| `non_votant` + cause `MG` | « n'a pas pris part au vote : membre du Gouvernement » |
| `non_votant` sans cause connue | « n'a pas pris part au vote » — **jamais** d'interprétation |
| `par_delegation` | « vote émis par délégation » — signalé systématiquement (12,6 % du corpus) |
| mise au point | « a déclaré après le scrutin avoir voulu voter X » + « le résultat du scrutin n'a pas été modifié » |
| `chambre = 'congres'` | « Congrès du Parlement — députés et sénateurs réunis » |
| `is_non_inscrit` | « non-inscrit·e » |
| statut candidat·e | « candidature déclarée » · « candidature pressentie » · « candidature retirée », suivies de la source et de sa date |

Une cause de non-vote inconnue s'affiche sans glose et **ne fait pas planter la page** : la source peut
introduire un code, l'UI doit dégrader proprement.

### Liste des votes : pagination et filtres

Filtres : `legislature`, `position`, `du`, `au`, `groupe` (uid d'organe). Le filtre par thème est
mentionné dans les livrables **pour la phase 3** : ne pas le préparer, ne pas laisser de champ mort.

Pagination **par curseur** et non par numéro de page : `?avant=AAAA-MM-JJ,{scrutin_id}`, tri
`date_scrutin DESC, scrutin_id DESC`. Le tri par date exige une jointure avec `scrutin`
(`vote_person_idx` est sur `(person_id, scrutin_id)`) : **mesurer avec `EXPLAIN (ANALYZE, BUFFERS)` sur
la base réelle** pour le·la député·e ayant le plus de votes, et n'ajouter un index que si la mesure le
justifie — en consignant la mesure dans le message de commit, avant et après. Pas d'index posé « au
cas où ».

Le bouton « charger plus » est un `<a href>` vers la page avec le curseur, doublé d'un `hx-get` vers le
fragment. Sans JS, c'est un lien ordinaire qui marche.

### Architecture CSS (D2.6)

Sources dans `static/css/`, préfixées par leur ordre de cascade : `00-reset`, `10-tokens`, `20-base`,
`30-layout`, `40-components`, `50-utilities`. `make css` les concatène dans cet ordre dans
`static/style.css`, précédé d'une bannière `/* FICHIER GÉNÉRÉ par `make css` — ne pas éditer */`.

**Le fichier généré est commité**, et `make lint` régénère puis vérifie :

```
css:
	@cat backend/src/kyc_api/static/css/*.css > … (avec la bannière)

lint: css
	git diff --exit-code -- backend/src/kyc_api/static/style.css
```

C'est ce qui répond à la seule vraie objection contre un artefact généré — « qui l'a régénéré avant de
déployer ? ». Personne n'a besoin d'y penser : la CI ne peut pas être verte avec un `style.css` périmé,
et la phase 5 déploie un dépôt qui contient déjà la feuille finale, sans étape de construction.
`make dev` dépend aussi de `css`.

Contenu attendu : `@layer` déclaré dans `00-reset` pour fixer l'ordre ; jetons dans `10-tokens`
(échelle typographique, échelle d'espacement, couleurs, largeurs de contenu, points de rupture en
commentaire — les media queries ne prennent pas de variables) ; nommage BEM allégé
(`.person-card`, `.person-card__name`, `.timeline__segment--non-inscrit`).

Palette : **neutre par construction** (D2.11). Une seule couleur d'accent, non politique, servant aux
liens et au focus. Contraste minimum 4.5:1 pour le texte, 3:1 pour les bordures porteuses de sens — à
vérifier avec un calculateur, pas à l'œil. Aucune information portée par la seule couleur : une
position de vote se lit dans le texte.

### Cache HTTP (D2.5)

`http_cache.py` : pour les `GET` rendant 200 en HTML ou en JSON, calculer un `ETag` fort sur le corps
de la réponse, poser `Cache-Control: public, max-age=60, stale-while-revalidate=300`, et rendre `304`
sur `If-None-Match` correspondant. Les corps sont petits, le hachage est négligeable devant les
requêtes SQL. Les redirections 301 et les 404 ne portent pas d'`ETag`.

### Stratégie de test (D2.8)

**Niveau 1 — fonctions pures**, testées finement et sans base : `build_timeline` (les neuf règles
ci-dessus, chevauchements et mandat d'un jour compris), l'URL de vignette Commons, le vocabulaire
d'affichage, le découpage du curseur de pagination, et côté Rust `slugify`.

**Niveau 2 — routes**, sur la base de test, avec le HTML **parsé** — pas de `assert "…" in
response.text`, qui casse au premier changement de gabarit et ne sait rien exprimer de structurel.
Ajouter `lxml` au groupe `dev` de `backend/pyproject.toml` et écrire un petit helper qui rend un
document interrogeable en CSS ou XPath. Ce qui doit être vérifié :

- la fiche d'une personne affiche un vote précis, sa position en toutes lettres, sa date, et le lien
  vers la source ;
- une personne qui n'a jamais siégé affiche l'état vide **et** son texte exact, pas une page à trous ;
- la zone « orientations » affiche son état d'attente de la phase 4 ;
- un ancien slug rend 301 vers le slug courant ; un slug inconnu rend 404 ;
- le scrutin du Congrès est signalé comme tel et ne compte pas dans le total « votes à l'Assemblée » ;
- un vote par délégation est signalé ; une mise au point est affichée sans modifier le vote ;
- un non-votant affiche sa cause en clair ;
- la recherche HTMX rend un fragment sans `<html>`, et la page équivalente rend une page complète ;
- l'API et la page servent les **mêmes** valeurs pour la même personne (un test qui compare les deux
  est ce qui empêche les deux surfaces de diverger).

**Niveau 3 — invariants structurels**, un seul test paramétré sur la liste de toutes les routes
publiques :

- exactement un `<h1>` ; un `<title>` non vide et distinct d'une page à l'autre ;
- `<html lang="fr">` ; un lien d'évitement vers le contenu principal ;
- tout `<img>` porte un attribut `alt` (vide ou non) ;
- tout champ de formulaire a un `<label>` associé ou un `aria-label` ;
- **aucun `src`/`href` déclenchant une requête au chargement** (`img`, `script`, `link`, `iframe`,
  `source`, `video`) vers un hôte externe, à la seule exception de `commons.wikimedia.org` et
  `upload.wikimedia.org` (D2.9). Les liens `<a>` sortants sont libres : ils ne déclenchent rien ;
- aucun `<script>` en ligne, aucun `style` de page en ligne (les `style="grid-column: …"` de la frise
  sont attendus et explicitement autorisés par le test).

**Jeu de données de test** (`factories.py`) : des insertions dans la transaction du test, pas de dump.
Doivent exister — ce sont les cas qui cassent : une personne à deux groupes successifs (le cas
PS → LFI de la recette), une personne sans aucun vote et sans mandat, une personne non-inscrite, un
scrutin du Congrès, un vote par délégation, un non-votant avec cause, une mise au point, un scrutin au
groupe non identifié (`organe_id IS NULL`, le groupe fantôme `PO0`), une personne à deux slugs dont un
ancien.

**Vues matérialisées et tests** : `REFRESH MATERIALIZED VIEW CONCURRENTLY` **ne peut pas s'exécuter
dans une transaction**, et chaque test tourne dans une transaction annulée. Les tests rafraîchissent
donc `person_apercu` sans `CONCURRENTLY` — le job du worker, lui, garde `CONCURRENTLY`. Écrire ce
piège en commentaire dans la fixture : il coûte une demi-heure à celui qui le rencontre sans
avertissement.

### Mesure de performance

Le critère « moins de 100 ms » n'est pas une intention, c'est une mesure à produire sur la base réelle
peuplée par `make ingest` :

1. `EXPLAIN (ANALYZE, BUFFERS)` sur les trois requêtes les plus lourdes : liste des votes du·de la
   député·e le·la plus prolifique, fiche complète, page d'un scrutin à 577 votants ;
2. temps de rendu côté serveur, 20 appels, p50 et p95, pour `/`, `/deputes`, `/personne/{slug}` et
   `/scrutin/…` ;
3. **consigner les chiffres obtenus dans le message du commit final et dans ce plan.** S'ils dépassent
   100 ms, ne pas contourner : trouver la requête coupable, et transformer le calcul en index, en
   colonne ou en vue matérialisée — jamais en cache applicatif, qui ne ferait que déplacer le
   problème.

### Ordre des commits

Un commit par ligne, `main` vert à chaque fois.

1. **Migration `0005_pages.sql` et ses tests de contraintes.** Rien d'autre : la migration seule doit
   se rejouer sur la base réelle sans rien casser.
2. **`slugify` et le job `assign_slugs`**, avec ses tests d'intégration.
3. **`seed_candidates`, le fichier de seed, `refresh_views`**, et `make ingest` complété.
4. **Socle backend** : `Queryable.fetch`, `http_cache.py`, `documents.py` et les pages
   `/methodologie` et `/sources`, l'architecture CSS et la cible `make css` (avec la garde de CI), le
   test d'invariants structurels branché sur les routes existantes. À partir d'ici, toute route
   ajoutée est vérifiée par construction.
5. **Accueil et annuaire**, avec la recherche HTMX et son équivalent sans JS.
6. **`timeline.py` et la fiche personne** : identité, frise, votes récents, états vides, sources.
7. **Page des votes** paginée et filtrée, avec la mesure d'`EXPLAIN ANALYZE` dans le message de commit.
8. **Page d'un scrutin** : compteurs, ventilation par groupe, mises au point, groupe non identifié.
9. **API `/api/v1/`**, servie par les mêmes fonctions de `queries/`, avec le test qui compare page et
   API.
10. **Passe finale** : accessibilité, contrastes, navigation clavier, mesures de performance
    consignées, mise à jour de [CLAUDE.md](../../CLAUDE.md) (section « Commandes » : `make css`) et de
    ce plan si une décision a bougé.

### Vérifications avant de déclarer la phase terminée

1. `make lint`, `make typecheck`, `make test` verts ; CI verte.
2. `make ingest` rejoué **en entier sur une base vierge** : les trois nouveaux jobs passent, et la
   relance ne change aucune ligne (slugs identiques, `candidate` identique) — l'idempotence vaut aussi
   pour eux.
3. **Vérification manuelle sur trois personnes réelles**, dont Charlotte Parmentier-Lecocq (5 groupes
   sur la seule 17e législature, déjà vérifiée en phase 1) : la frise correspond à l'historique du site
   de l'Assemblée, et trois votes pris au hasard correspondent au fichier de scrutin. **Consigner le
   résultat dans le message du commit final** — le dépôt est en développement sur le tronc, il n'y a
   pas de PR où le déposer.
4. Le cas PS → LFI s'affiche correctement, replié comme déplié (recette du cahier des charges).
5. Chaque page a été ouverte **JavaScript désactivé** : lecture, recherche, filtres et pagination
   fonctionnent.
6. Navigation au clavier seul de bout en bout sur la fiche : focus visible partout, aucun piège.
7. Onglet réseau du navigateur sur trois pages : aucune requête sortante autre que Commons.
8. Les mesures de performance sont consignées, et sous les 100 ms.
9. Aucune route ne crée de job ; `enable_dev_routes` n'est jamais réapparu.

### Hors périmètre — ne pas ajouter

Pas de score, pas de thème, pas de catégorisation, pas de back-office, pas d'authentification, pas de
comparaison entre personnes, pas de graphique de tendance, pas de Sénat, pas de Parlement européen, pas
de 14e législature. Pas de couleur de parti (D2.11). Pas de table `party` éditoriale ni de lien de
succession entre partis : c'est la phase 4, et la préparer ici figerait des choix éditoriaux sans les
avoir discutés. Pas de framework JS, pas de bundler, pas de police distante, pas d'analytique.

En cas de doute sur ce qu'on a le droit d'afficher ou d'en déduire : demander, ne pas deviner.

## Fini quand

- On peut ouvrir la fiche d'une personne et retrouver un vote précis, correct, avec le lien vers la source
  officielle.
- La frise affiche correctement un cas à plusieurs partis successifs (le cas PS → LFI du cahier des
  charges sert de test de recette).
- Les pages passent sans JS pour la lecture, et sans erreur d'accessibilité bloquante.
- Aucune requête vers un domaine tiers au chargement d'une page, **à la seule exception des photos
  servies par Wikimedia Commons** (D2.9) : pas de police distante, pas de script distant, pas
  d'analytique, pas d'iframe. L'exception est écrite ici parce qu'elle a un coût — l'IP du visiteur part
  chez un tiers — et qu'un coût assumé se documente au lieu de se découvrir.
- Une page personne se rend en moins de 100 ms côté serveur sur des données réelles, **mesuré et
  consigné**, pas estimé.
- **Il ne reste aucune route de démonstration dans le code**, et le réglage `enable_dev_routes` a disparu
  avec elles — vérifié dès la phase 1.1 (F11), pas seulement ici.

## Risques

- **Photos** : couverture partielle et licences variables. Prévoir un placeholder digne et l'affichage de
  l'auteur.
- **Volume de votes par personne** : plusieurs centaines de lignes. La pagination et les index doivent
  être pensés dès le départ, pas ajoutés après.
- **Tentation d'afficher un score trop tôt**, avant que la catégorisation soit sérieuse. Ne pas céder :
  un score faux vu une fois décrédibilise durablement le projet.
- **Le seed des candidat·es vieillit vite.** Une liste de candidat·es à seize mois du scrutin est une
  photographie, pas un fait stable : des personnes se déclarent, se retirent, meurent. D'où le statut
  explicite, la date de source obligatoire par ligne, et l'absence de toute automatisation — une liste
  qui se met à jour toute seule est une liste dont personne ne répond.
- **La dépendance à Commons pour les photos** (D2.9) rend l'affichage tributaire d'un service tiers :
  panne, renommage de fichier ou suppression pour raison de licence font apparaître des images cassées.
  Mitigation : `alt` correct et cadre de repli visible, jamais un trou dans la grille.
