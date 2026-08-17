# Phase 1.1 — Corrections de l'ingestion

**Statut : ✅ validé** · Dépend de : phase 1 · Bloque : phase 2

## Objectif

Corriger les onze défauts relevés à la revue de la phase 1, et remplacer les fixtures d'archives réelles
par des fixtures **générées**. Aucune fonctionnalité nouvelle : la phase 1 reste l'ingestion, elle doit
juste tenir ses garanties — à commencer par la plus élémentaire, qui n'est pas tenue aujourd'hui : **`main`
ne compile pas ses tests depuis un clone propre**.

Cette section est autoportante : **tout ce qui est nécessaire est ici, dans
[phase-1-ingestion.md](phase-1-ingestion.md), dans [data-sources.md](../data-sources.md) ou dans
[CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de conversation à retrouver. Développement sur le
tronc, commits directs sur `main`, un commit par correctif ou par groupe cohérent, `main` vert à chaque
commit.

Chaque diagnostic ci-dessous a été **vérifié dans le code** : les causes décrites sont établies, pas
supposées. Si l'une se révèle fausse au contact du code, le dire plutôt que de contourner.

## Ce que la revue a validé — ne pas y toucher

Le gros de la phase 1 est bon et n'a pas à être retouché :

- les quatre migrations `0001`–`0004` (immuables de toute façon : toute correction passe par une nouvelle
  migration numérotée) ;
- les deux primitives de normalisation de [`worker/src/an/json.rs`](../../worker/src/an/json.rs) et le fait
  que **tout** le parsing passe par elles — c'est la bonne discipline, la conserver ;
- les `ON CONFLICT DO UPDATE ... WHERE ... IS DISTINCT FROM` qui protègent `updated_at` et rendent le
  réingest idempotent, ainsi que les tests de condensé qui le vérifient réellement ;
- `normalize_gp_mandats` comme **fonction pure** testée hors base, y compris ses deux cas de régression
  découverts au contact des vraies données (inclusion inversée, pseudo-groupe non-inscrit) ;
- l'ingestion complète réelle du 17 août 2026 et sa vérification manuelle contre le site de l'Assemblée,
  consignées dans le commit `e13bbc8` et dans [data-sources.md](../data-sources.md). **Ces chiffres restent
  la référence** : ils ne sont pas remis en cause par le changement de fixtures (voir F1).

Les correctifs ci-dessous ne doivent rien changer à ces points, sauf mention explicite.

---

## Décisions arbitrées

Elles sont tranchées. La session qui implémente ne les rediscute pas ; si l'une se révèle fausse au contact
du code, elle le dit plutôt que de la contourner.

| # | Question | Décision |
| --- | --- | --- |
| D1.16 | Que faire d'un chevauchement de mandats de groupe entre **organes différents** ? | **Conserver les deux mandats et journaliser** `mandat_chevauchement`. On n'écarte plus la plage la plus courte. Voir F5 |
| D1.17 | Forme des fixtures de test | **Fichiers JSON générés, commités en clair**, jamais d'archive réelle ni de binaire. Le `.zip` est reconstruit **en mémoire par le test**. Voir F1 |
| D1.18 | Le cache disque du client HTTP | **Actif seulement si `AN_CACHE_DIR` est renseigné**, plus de `.cache` par défaut. Voir F6 |

---

## Bloquants

### F1 — Les fixtures ne sont pas commitées : `main` ne compile pas depuis un clone propre

**Fichiers** : `.gitignore:25`, `worker/tests/fixtures/`, `worker/tests/ingest_acteurs.rs:10`,
`worker/tests/ingest_scrutins.rs:11-12`

**Symptôme** : `worker/tests/fixtures/amo30-extrait.zip` et `scrutins-extrait.zip` existent sur la machine
qui a implémenté la phase 1, mais **pas dans l'arbre commité** : la règle `*.zip` de `.gitignore:25` les
avale, et `git status` reste propre parce qu'elles sont ignorées. Les deux fichiers de tests d'intégration
les chargent par `include_bytes!`. Vérifié en extrayant `HEAD` dans un répertoire vierge :

```
error: couldn't read `tests/fixtures/amo30-extrait.zip`: No such file or directory (os error 2)
  --> tests/ingest_scrutins.rs:11:30
error: couldn't read `tests/fixtures/scrutins-extrait.zip`: No such file or directory (os error 2)
  --> tests/ingest_scrutins.rs:12:33
```

La CI est donc **rouge** depuis le commit `74baa96`, et la vérification n° 9 de la phase 1 (« CI verte ») a
été affirmée sur la foi d'un `make test` local, où les fichiers ignorés sont bien présents. Le livrable 7
(« tests d'intégration sur un jeu réduit d'archives réelles versionnées comme fixtures ») n'est pas tenu.

**Correctif** — il ne s'agit pas seulement de commiter les zips : la forme des fixtures change (D1.17).

1. **Supprimer** `worker/tests/fixtures/build_fixtures.py`, `amo30-extrait.zip` et `scrutins-extrait.zip`.
   Le script de fabrication est un échafaudage qui n'a plus d'objet une fois les fixtures écrites à la main
   (CLAUDE.md, « Aucun échafaudage de développement ne survit dans le produit final »).

2. **Écrire les fixtures en JSON clair**, une arborescence qui reproduit celle des archives de l'Assemblée
   (c'est ce que filtrent `read_amo30_docs` et `read_scrutin_docs`, sur les préfixes `json/organe/`,
   `json/acteur/` et `json/`) :

   ```
   worker/tests/fixtures/
     README.md
     amo30/json/organe/PO9000NN.json
     amo30/json/acteur/PA9000NN.json
     scrutins/json/VTANR5L17V900001.json
   ```

3. **Le `.zip` est reconstruit en mémoire par un helper de test**, jamais commité. `zip::ZipWriter` est
   disponible avec les features actuelles du crate (`default-features = false, features = ["deflate"]`) —
   vérifié. Utiliser `CompressionMethod::Stored`, la compression n'apporte rien ici. Le helper parcourt le
   répertoire à partir de `env!("CARGO_MANIFEST_DIR")` et conserve les chemins relatifs, pour qu'ajouter un
   cas de test soit l'ajout d'**un fichier** et rien d'autre.

   Conséquence importante : les tests continuent d'appeler `ingest_bytes` de bout en bout, donc le lecteur
   de zip et le filtre `since` restent couverts. **Ne pas** refactorer les jobs pour contourner le zip :
   c'est inutile et ça ajoute du risque.

4. **Le contenu est inventé, la forme est fidèle.** C'est tout l'enjeu : ce que les fixtures doivent
   reproduire, ce sont les bizarreries du JSON de l'Assemblée (qui est un XML converti mécaniquement), pas
   des personnes.

   - Identifiants inventés mais au **format** de la source, parce que le code s'appuie dessus :
     `PA9000NN`, `PO9000NN`, `PM9000NN`, `VTANR5L17V9000NN`, et `VTCGR5L16V9000NN` pour le Congrès (c'est
     le préfixe `VTCGR` qui décide de `chambre`, voir `parse_scrutin`).
   - Noms de personnes **inventés**, qui ne doivent ressembler à aucun·e député·e réel·le : ce projet parle
     de personnes identifiables dans un contexte électoral (CLAUDE.md, « Sensibilité du sujet »), et une
     fixture n'a aucune raison de porter un vrai nom.
   - Chaque quirk de forme doit être présent au moins une fois, et le `README.md` dit **de quel scrutin
     réel la forme est tirée**, pour qu'on puisse la revérifier contre l'archive sans que la fixture
     contienne la moindre donnée réelle.

5. **Cas à couvrir** — reprendre le tableau de la phase 1 et y ajouter ce que la fixture réelle couvrait
   par accident :

   | Cas | Comment le fabriquer |
   | --- | --- |
   | Scalaire sous ses quatre formes | chaîne nue, `{"@xsi:type": ..., "#text": ...}`, `{"@xsi:nil": "true"}`, `null` littéral |
   | Collection sous ses trois formes | absente, objet singleton, tableau |
   | Booléen chaîne | `parDelegation: "true"` / `"false"` |
   | Scrutin ordinaire, plusieurs groupes | un scrutin à 3 groupes |
   | Vote par délégation | au moins un `parDelegation: "true"` |
   | Mise au point réellement remplie | un bloc `miseAuPoint` avec de vraies entrées **et** des blocs `[null, null]` à ignorer |
   | Non-votants des trois causes | `PSE`, `PAN`, `MG` |
   | Cause de non-vote inédite | un code hors des trois → anomalie `cause_non_vote_inconnue` (jamais couvert jusqu'ici) |
   | Bloc nominatif inconnu | un nom de bloc hors des huit graphies → `bloc_nominatif_inconnu` (jamais couvert) |
   | Groupe fantôme `PO0` | plusieurs lignes `PO0` sur un même scrutin |
   | Groupe d'un type non retenu | un `organeRef` absent du référentiel → même chemin que `PO0` |
   | Blocs nominatifs au singulier | un scrutin `VTCGR...` en `pour`/`contre`/`abstention`/`nonVotant` |
   | Compteurs incohérents | `syntheseVote` volontairement décalé du nominatif |
   | Acteur cité mais absent du référentiel | un `acteurRef` qui n'existe dans aucun fichier `amo30/` |
   | Mandats de groupe qui s'incluent | un acteur avec deux mandats même organe, l'un contenu dans l'autre |
   | Mandats qui partagent une charnière | fin de l'un = début du suivant, même organe |
   | Chevauchement inter-organes réel | deux vrais groupes qui se recouvrent → **conservés** et journalisés (D1.16) |
   | Chevauchement avec un non-inscrit | `libelleAbrev: "NI"` recouvrant un vrai groupe → conservé, **sans** anomalie |
   | Mandat en cours | `dateFin` absente |
   | Mandat à écarter | `dateDebut` manquante, et `dateFin < dateDebut` |

6. **Réécrire `worker/tests/fixtures/README.md`** : ce que chaque fichier couvre, la forme réelle dont il
   s'inspire, et **le fait que les chiffres de ces tests ne sont pas ceux du corpus réel**. Les chiffres
   réels sont ceux de [data-sources.md](../data-sources.md), mesurés par l'ingestion du 17 août 2026 ; les
   fixtures vérifient un **comportement**, jamais un volume. Le dire explicitement évite qu'on croie plus
   tard que la CI valide les compteurs.

7. **Adapter les assertions** des deux fichiers de tests aux nouveaux effectifs. Elles doivent rester des
   assertions sur le comportement (« ce vote n'est pas perdu », « ces 5 lignes coexistent »), pas des
   totaux opaques.

8. **`.gitignore`** : garder `*.zip` (les archives téléchargées ne doivent jamais être commitées) — avec
   des fixtures en JSON, il n'y a plus rien à excepter. Vérifier après coup que `git status` ne cache rien :
   `git status --ignored --short worker/tests/` ne doit lister aucun fichier de fixture.

**Vérification** : `git archive HEAD | tar -x -C <répertoire vierge>` puis `cargo test` dans ce répertoire
doit compiler et passer. C'est la seule preuve qui vaille — un `make test` local ne prouve rien ici, c'est
précisément ce qui a laissé passer le défaut.

### F2 — `enrich_wikidata` n'a jamais été ni testé ni exécuté

**Fichier** : `worker/src/jobs/enrich_wikidata.rs` (407 lignes), `Makefile`

**Symptôme** : le job est le livrable 5 et l'étape 7 de la phase 1. Il n'a **aucun test** — ni unitaire, ni
d'intégration — il n'est pas dans `make ingest`, et aucun de ses chiffres n'est consigné, alors que le plan
donne des cibles d'acceptation explicites (« environ 84 % des député·es ont une photo, 549 sur 652 ») et
deux `kind` d'anomalie marqués « à mesurer ». C'est du code réseau non exercé dans un livrable déclaré
terminé.

Deux défauts concrets s'y cachent :

**F2a — collision de QID → violation d'unicité.** `person.wikidata_qid` est `UNIQUE`
(`db/migrations/0002_referentiel.sql:23`). Le job groupe par `an_uid` et refuse d'écrire quand plusieurs
QID revendiquent le même `P4123` (`enrich_wikidata.rs:95-104`) — c'est le cas prévu par le plan. Mais le
cas **miroir** n'est pas gardé : un QID portant **deux** valeurs `P4123` produit deux lignes `resolved`
de même `qid` visant deux personnes différentes, et l'`UPDATE ... FROM UNNEST` de `update_wikidata_qids`
viole alors l'index unique. Le job entier échoue.

*Correctif* : après le groupement par `an_uid`, faire le groupement **par `qid`** ; si un `qid` revendique
plusieurs `an_uid`, n'écrire **aucun** des deux et journaliser `wikidata_qid_ambigu` (même `kind`, le
`detail` précisant de quel côté vient l'ambiguïté). L'AN gagne toujours, Wikidata ne tranche jamais un
conflit à notre place — c'est la règle déjà posée par le plan, appliquée symétriquement.

**F2b — perte silencieuse de photos sur la normalisation des titres Commons.**
`commons_title_from_image_url` (`enrich_wikidata.rs:194`) percent-décode le nom de fichier de l'URL `P18`.
Quand cette URL porte des soulignés (`Special:FilePath/Nom_Du_Fichier.jpg`), le titre construit est
`File:Nom_Du_Fichier.jpg` alors que l'API Commons renvoie `File:Nom Du Fichier.jpg`. Le `title_to_an_uid`
ne retrouve alors pas la ligne (`enrich_wikidata.rs:284`) et la photo est **abandonnée sans anomalie** :
elle ne compte ni en `enregistrees`, ni en `sans_licence`. Une perte muette, exactement ce que le projet
s'interdit.

*Correctif* : normaliser `_` en espace après le percent-décodage, des deux côtés de la correspondance
(construction de la table **et** lecture de `page.title`). Et faire de tout candidat non réconcilié une
anomalie `photo_sans_licence` explicite plutôt qu'un `continue` : compter les entrées attendues et les
entrées reçues, et journaliser la différence.

**F2c — jamais exécuté.**

*Correctif* : ajouter `cargo run --release -- enqueue enrich_wikidata` à la cible `make ingest`, après les
trois législatures (le job a besoin des `person` du référentiel). Puis **l'exécuter réellement** et
consigner dans [data-sources.md](../data-sources.md) et dans le message de commit : nombre de résultats
SPARQL, personnes uniques, QID écrits, photos enregistrées, `photo_sans_licence`,
`wikidata_qid_ambigu`. Comparer au ~84 % annoncé par le plan ; si l'écart est important, regarder le code
d'abord, puis la source — et mettre à jour les documents dans le même commit.

**Tests à ajouter** :

- unitaires sur les fonctions pures — pour cela, **extraire** la résolution des `WikidataHit` (groupement
  par `an_uid`, groupement par `qid`, arbitrage) dans une fonction pure prenant `&[WikidataHit]` et rendant
  les couples retenus et les ambiguïtés. La tester sur : cas nominal, deux QID pour un `P4123`, deux
  `P4123` pour un QID, hit sans `P18` ;
- unitaires sur `commons_title_from_image_url` : URL percent-encodée, URL à soulignés, URL sans nom de
  fichier ;
- unitaire sur la lecture d'`extmetadata` : `LicenseShortName` présent, absent, `extmetadata` absent ;
- d'intégration sur `update_wikidata_qids` et `upsert_photos` avec des lignes construites à la main (pas de
  réseau), **dont un test d'idempotence par condensé** sur `person` et `person_photo` : c'est la garantie
  centrale de la phase et elle n'est aujourd'hui vérifiée sur aucune des deux tables.

Le chemin HTTP lui-même (SPARQL, Commons) reste vérifié par l'exécution réelle de F2c, pas par un mock :
introduire une abstraction de client pour deux appels ne vaut pas son coût.

---

## Importants

### F3 — `make ingest` sort en 0 même quand un job échoue

**Fichier** : `worker/src/jobs/mod.rs:189-226`

**Symptôme** : `drain_once` appelle `run_one_job`, qui journalise l'échec, marque le job `failed` en base…
et rend `()`. `drain_once` rend alors `Ok(())`, `main` sort avec le code 0, et `make ingest` se termine en
succès. Sur une commande non surveillée de plusieurs dizaines de minutes, une législature entière peut
manquer sans que rien ne le signale. C'est le contraire de la règle de CLAUDE.md : « une écriture qui
n'affecte aucune ligne est journalisée, jamais ignorée en silence ».

**Correctif** :

1. `run_one_job` rend son issue (job terminé / échoué / relâché à l'expiration de la grâce) au lieu de
   `()`.
2. `drain_queue` — la boucle normale du worker — **ignore** cette issue et continue : un job qui échoue ne
   doit jamais tuer le worker. Comportement inchangé, c'est le bon.
3. `drain_once` **accumule** les issues non réussies, vide la file jusqu'au bout (pour ne pas masquer les
   erreurs suivantes derrière la première), puis rend `Err` en nommant les jobs concernés — type et id.
   `main` sort donc en code non nul, et `make ingest` s'arrête.

**Vérification** : test d'intégration — mettre en file un job d'un type inconnu (`execute` rend déjà
`Err` dessus), appeler `drain_once`, vérifier qu'il rend `Err` et que le job est bien `failed` en base ;
puis un job `noop` seul, et vérifier `Ok`.

### F4 — `groupe_from_mandat` affirme une provenance qui n'a pas eu lieu

**Fichier** : `worker/src/jobs/ingest_scrutins.rs:508-514`

**Symptôme** : quand la ligne de groupe est fantôme, le code pose `groupe_from_mandat = true`
inconditionnellement, que `resolve_groupe_from_mandat` ait rendu `Some` ou `None`. Les 902 votes du Congrès
`VTCGR5L16V1` sont émis par des sénateurs, qui n'ont évidemment aucun mandat `GP` à l'Assemblée : ces
lignes finissent en `groupe_organe_id = NULL, groupe_from_mandat = true`, c'est-à-dire que la colonne
déclare « ce groupe a été reconstitué depuis les mandats » alors qu'aucune reconstitution n'a abouti. La
colonne existe précisément pour tracer la méthode employée (phase 1, modèle de `vote`), et la phase 3 la
lira pour l'alignement.

**Correctif** : `groupe_from_mandat` ne vaut `true` que si un mandat a effectivement été trouvé —
`(mandat_organe_id, mandat_organe_id.is_some())`.

**Vérification** : test d'intégration sur la fixture — un vote de groupe fantôme dont l'auteur n'a aucun
mandat `GP` doit ressortir en `groupe_organe_id IS NULL AND groupe_from_mandat = false` ; un vote de groupe
fantôme dont l'auteur **a** un mandat `GP` couvrant la date doit ressortir en `groupe_from_mandat = true`
avec l'organe résolu. Les deux cas doivent être dans la fixture (F1).

### F5 — 74 mandats réels sont supprimés là où la base était faite pour les accepter

**Fichier** : `worker/src/an/acteur.rs:182-215`

**Symptôme** : le plan de la phase 1 se contredit. La section « Le référentiel » explique que la contrainte
`EXCLUDE` porte volontairement sur *(personne, organe)* et non *(personne, type)* pour que la base
**accepte** les recouvrements entre groupes différents — « On préfère une base qui accepte la donnée et
signale, à une base qui refuse et ne dit rien » — tandis que l'étape 6 dit « la plage la plus courte n'est
pas insérée ». L'implémentation a suivi l'étape 6 : 74 mandats de groupe réels sont absents de `mandat`,
dont la scission UMP / Rassemblement-UMP de novembre 2012, que le commentaire du code qualifie lui-même de
« vrai fait politique et non une anomalie de saisie ».

**Décision (D1.16) : on conserve les deux mandats et on se contente de journaliser.** Trois raisons :

1. C'est ce que la contrainte de base permet déjà, et c'est la raison explicite pour laquelle elle a été
   écrite ainsi.
2. Écarter la plage la plus courte, c'est **trancher** laquelle de deux appartenances contradictoires est
   vraie — une interprétation, décidée silencieusement au moment de l'ingestion. CLAUDE.md l'interdit :
   « Les données brutes ne sont jamais écrasées par une interprétation », et « toute ingestion est
   idempotente et additive ». L'arbitrage éditorial des appartenances relève de la phase 4.
3. L'impact sur le corpus v1 est nul (2012 est hors des législatures 15 à 17), mais la règle est générale :
   elle mordra à la prochaine scission de groupe, cette fois dans le corpus.

**Correctif** :

1. Dans `normalize_gp_mandats`, la passe **inter-organes** cesse d'écarter : elle parcourt les mandats,
   émet un `NormalizationEvent::Chevauchement` pour chaque paire qui se recouvre (toujours en excluant les
   paires dont l'un des deux est le pseudo-groupe non-inscrit), et **retient tout**. La suppression de
   `retained.remove(...)` fait disparaître au passage le `.expect("kept vient de retained")` de la ligne
   209 (voir F9).
2. **La passe intra-organe continue d'écarter**, elle. C'est indispensable : la contrainte `EXCLUDE` porte
   sur *(personne, organe)* et rejetterait deux plages qui se recouvrent dans le même organe. Ne pas
   confondre les deux passes en lisant ce correctif.
3. Ajuster le commentaire de doc de `NormalizationEvent::Chevauchement` : ce n'est plus « le mandat écarté
   parce qu'il chevauche », c'est « un mandat signalé comme recouvrant un autre organe, conservé ».
4. Le compteur `normalisation_chevauchements` du run garde son nom mais change de sens : il compte des
   signalements, plus des suppressions. Le dire dans le code.

**Conséquences à répercuter dans le même commit** :

- l'attendu de mandats passe de **15 224** à **15 298** (15 383 − 85 inclusions ; les charnières
  raccourcissent sans supprimer, les chevauchements ne suppriment plus) ;
- mettre à jour la table des `kind` de [phase-1-ingestion.md](phase-1-ingestion.md) (la ligne
  `mandat_chevauchement` dit « la plage la plus courte n'est pas insérée ») et son étape 6, ainsi que
  [data-sources.md](../data-sources.md) ;
- ajouter D1.16 à la table des décisions de [phase-1-ingestion.md](phase-1-ingestion.md) ;
- **noter pour la phase 3** qu'une personne peut désormais porter deux mandats `GP` couvrant la même date.
  `resolve_groupe_from_mandat` (`ingest_scrutins.rs:318-330`) le gère déjà de façon déterministe
  (`ORDER BY lower(period) DESC LIMIT 1`, le mandat commencé le plus tard gagne) — le vérifier, ne pas le
  changer, et le mentionner dans le plan.

**Vérification** : le test unitaire `chevauchement_reel_entre_deux_vrais_groupes_est_toujours_detecte`
change d'attendu — les deux mandats sont retenus, l'événement est toujours émis. Renommer le test en
conséquence (« …est signalé sans écarter »). Ajouter un test d'intégration qui prouve que les deux lignes
coexistent en base après ingestion de la fixture correspondante.

---

## Mineurs

### F6 — Le cache disque est actif en production

**Fichiers** : `worker/src/jobs/ingest_acteurs.rs:108-113`,
`worker/src/jobs/ingest_scrutins.rs:189-194`, `worker/src/an/http.rs:29-34`

**Symptôme** : `AnClient` documente `cache_dir: None` comme étant le cas de production (« pas de disque
persistant à en attendre entre deux déploiements »). Mais les deux jobs construisent leur chemin par
`std::env::var("AN_CACHE_DIR").ok().map(...).or_else(|| Some(PathBuf::from(".cache")))` : la branche `None`
est **inatteignable**. En production, le worker écrit donc ~300 Mo d'archives dans son répertoire courant,
sans moyen de l'en empêcher — `AN_CACHE_DIR` ne fait que déplacer le cache, jamais le désactiver.

**Correctif (D1.18)** : le cache est actif **si et seulement si** `AN_CACHE_DIR` est renseigné. Supprimer
le `.or_else(...)`. Factoriser la fonction `cache_dir()`, aujourd'hui dupliquée à l'identique dans les deux
jobs, dans `worker/src/an/http.rs`. Ajouter à `.env.example` :

```
# Répertoire de cache des archives téléchargées. Laisser vide en production : le cache n'a de sens
# qu'en développement, pour ne pas retélécharger 300 Mo à chaque itération.
AN_CACHE_DIR=worker/.cache
```

L'entrée `worker/.cache/` de `.gitignore` reste.

### F7 — `fetch_latest_document_ids` n'a pas de départage déterministe

**Fichier** : `worker/src/an/document.rs:64-74`

**Symptôme** : `SELECT DISTINCT ON (uid) ... ORDER BY uid, fetched_at DESC` — deux lignes du même `uid`
archivées par la **même** instruction partagent leur `fetched_at` (`now()` est constant dans une
transaction), et PostgreSQL choisit alors arbitrairement laquelle référencer dans
`scrutin.source_document_id`. Le cas est rare mais il rend le condensé d'idempotence instable, c'est-à-dire
qu'il peut faire échouer le test le plus important de la phase de façon intermittente.

**Correctif** : `ORDER BY uid, fetched_at DESC, id DESC`. Régénérer `.sqlx` (voir la note en fin de plan).

### F8 — Un aller-retour SQL par vote de groupe fantôme

**Fichier** : `worker/src/jobs/ingest_scrutins.rs:503-514`

**Symptôme** : `resolve_groupe_from_mandat` est appelée **dans** la boucle de construction des lignes, une
fois par vote. Sur le corpus réel c'est ~1 050 requêtes (146 lignes fantômes en 17e, 9 en 16e, plus les
groupes sénatoriaux du Congrès) — supportable aujourd'hui, mais c'est une requête par ligne au cœur d'une
boucle d'écriture par lots, ce qui contredit l'esprit du reste du job.

**Correctif** : en amont de la boucle d'écriture, collecter les couples `(person_id, date_scrutin)` dont le
groupe est à reconstituer pour le lot courant, les résoudre en **une** requête, puis lire le résultat dans
la boucle :

```sql
SELECT DISTINCT ON (t.person_id, t.d) t.person_id, t.d, m.organe_id
FROM UNNEST($1::bigint[], $2::date[]) AS t(person_id, d)
JOIN mandat m ON m.person_id = t.person_id AND m.type_organe = 'GP' AND m.period @> t.d
ORDER BY t.person_id, t.d, lower(m.period) DESC
```

Le `ORDER BY ... lower(m.period) DESC` reproduit exactement la règle actuelle (le mandat commencé le plus
tard gagne) ; ne pas la changer en passant. Un couple sans ligne dans le résultat vaut « aucun mandat »,
donc `groupe_organe_id = NULL` et `groupe_from_mandat = false` (F4).

### F9 — `.expect()` hors tests, contre la convention du dépôt

**Fichiers** : `worker/src/jobs/ingest_acteurs.rs:439-445` (trois occurrences),
`worker/src/an/acteur.rs:209`, `worker/src/an/http.rs:103`

**Symptôme** : CLAUDE.md pose « pas d'`unwrap()` hors tests et hors démarrage ». Le lint
`unwrap_used = "warn"` de `worker/Cargo.toml` ne couvre pas `expect`, qui panique pareil. Les invariants
tiennent aujourd'hui — les trois de `ingest_acteurs` sont commentés « filtré ci-dessus » — mais ils sont
maintenus **à distance** du point de panique, ce qui est exactement la configuration où une modification
ultérieure les casse.

**Correctif** :

1. `ingest_acteurs.rs:438-451` : au lieu de refiltrer `mandat` puis de ré-extraire ses champs par
   `.expect()`, construire dès la boucle de filtrage une structure portant les valeurs **déjà validées**
   (`organe_id`, `debut`, `type_organe` non optionnels), et pousser celle-ci dans `autres`. L'invariant
   passe alors dans le type et le `.expect()` n'a plus de raison d'être.
2. `acteur.rs:209` : disparaît avec le correctif F5.
3. `http.rs:103` : `self.cache_dir.as_ref().expect(...)` est inutile — le chemin parent est déjà dans
   `body_path`, utiliser `body_path.parent()`.
4. Ajouter `expect_used = "warn"` à `[lints.clippy]` dans `worker/Cargo.toml`, et compléter les
   `#![allow(clippy::unwrap_used)]` des tests en y ajoutant `expect_used`.

### F10 — Chemins du plan non couverts par un test

**Fichiers** : `worker/tests/`, `worker/src/an/http.rs:57-71`

**Symptôme** : trois exigences de la phase 1 n'ont aucun test, et l'une n'est même pas consignée comme
vérifiée à la main.

**Correctif** :

1. **`since`** — le filtre est appliqué dans `read_scrutin_docs` (`ingest_scrutins.rs:219-223`). Ajouter un
   test d'intégration : ingérer la fixture avec `since` postérieur à une partie des scrutins, vérifier que
   seuls les scrutins attendus entrent, et que les autres **restent intacts** s'ils étaient déjà en base.
2. **Vérification n° 5 de la phase 1** (« ingérer la seule 16e sur une base portant déjà les trois : rien
   d'autre n'est modifié ») — jamais automatisée, jamais mentionnée comme faite à la main dans le commit
   `e13bbc8`. Ajouter un test : ingérer les fixtures des trois législatures, relever les condensés,
   réingérer les seuls scrutins d'une législature, vérifier que les condensés des deux autres sont
   inchangés.
3. **`force_refetch`** — extraire la décision « quel ETag envoie-t-on ? » de `fetch_zip` dans une fonction
   pure (`force_refetch` vrai → aucun ; sinon l'ETag en cache s'il existe) et la tester unitairement. Le
   reste du chemin HTTP reste vérifié manuellement ; monter un serveur de test pour deux en-têtes ne vaut
   pas son coût, mais le dire dans le commentaire plutôt que de laisser croire à une couverture.
4. **Dérive de documentation** : la table des `kind` de [phase-1-ingestion.md](phase-1-ingestion.md) décrit
   `groupe_fantome` comme « ligne de groupe portant `PO0` », alors que le code y range aussi toute ligne
   dont l'`organeRef` est absent du référentiel (`ingest_scrutins.rs:473-476`) — c'est le bon comportement,
   c'est ce qui rattrape les groupes sénatoriaux du Congrès, mais le plan ne le dit pas. Corriger le
   libellé.

### F11 — Les routes de démonstration de la phase 0 n'ont plus lieu d'être

> Ce point n'était pas dans la liste des dix de la revue : il est apparu en la rédigeant. Il est ici pour
> décision, et peut être retiré du lot sans conséquence sur les autres correctifs.

**Fichiers** : `backend/src/kyc_api/routers/dev.py`, `backend/src/kyc_api/main.py:29`,
`backend/src/kyc_api/config.py:25-28`, `backend/src/kyc_api/routers/pages.py:20`,
`backend/tests/test_jobs.py`, `.env.example`

**Symptôme** : `POST /dev/jobs` crée un job depuis une route publique non authentifiée, derrière le drapeau
`enable_dev_routes` (à `true` dans `.env.example`). CLAUDE.md l'interdit deux fois : « aucune route
publique ne crée de job », et « aucun échafaudage de développement ne survit dans le produit final […] pas
neutralisé derrière un drapeau : un drapeau finit toujours par être activé "temporairement" ». La phase 1 a
livré le remplaçant explicitement prévu pour ça (`cargo run -- enqueue`), et la phase 5 déploie `main`
automatiquement.

**Correctif** : supprimer `routers/dev.py`, son montage dans `main.py`, le réglage `enable_dev_routes`, la
variable de `.env.example` et le `show_demo` de `pages.py`. Le suivi de progression, lui, est **promu** en
phase 3 derrière l'authentification admin, comme le prévoit CLAUDE.md — ne rien en supprimer d'autre que la
route de création. Adapter `backend/tests/test_jobs.py` : ce qui testait la création par HTTP teste
désormais la lecture d'un job inséré directement en base.

---

## Ordre des commits

Chaque commit laisse `main` vert (lint, typage, tests).

1. **F1** — fixtures JSON générées, helper de zip en mémoire, suppression de `build_fixtures.py` et des
   deux archives, README réécrit, assertions adaptées. À faire **en premier** : sans lui, aucun autre
   correctif n'est vérifiable en CI.
2. **F5** — conservation des chevauchements inter-organes, mise à jour du plan de la phase 1 et de
   data-sources.md dans le même commit (CLAUDE.md, règle 3).
3. **F4 + F8** — résolution du groupe depuis les mandats : correction du drapeau et passage en une requête
   par lot. Les deux touchent le même bloc, les séparer n'apporte rien.
4. **F3** — issue des jobs remontée par `drain_once`.
5. **F2** — corrections et tests de `enrich_wikidata`, ajout à `make ingest`.
6. **F6 + F7 + F9** — cache conditionnel, départage du `DISTINCT ON`, suppression des `.expect()` et lint.
7. **F10** — tests manquants et corrections de documentation.
8. **F11** — suppression des routes de démonstration (si retenu).
9. **Exécution réelle** — relancer `make ingest` en entier sur une base vierge, consigner les chiffres
   obtenus (dont ceux de `enrich_wikidata`, jamais mesurés) dans data-sources.md et dans le message de
   commit, comme l'a fait `e13bbc8`.

## Vérifications avant de déclarer la phase 1.1 terminée

1. `git archive HEAD | tar -x -C <répertoire vierge>` puis `cargo test` **dans ce répertoire** : compile et
   passe. C'est la vérification centrale de ce lot.
2. **CI verte sur `main`** — vérifiée sur GitHub Actions, pas déduite d'un `make test` local. Les trois
   jobs (`python`, `rust`, `migrations`) doivent passer.
3. `make lint`, `make typecheck`, `make test` verts en local.
4. `make ingest` sur une base vierge : les quatre jobs s'exécutent, `enrich_wikidata` compris, et la
   commande sort en **code non nul** si l'un échoue (vérifiable en mettant temporairement en file un job de
   type inconnu — ne pas commiter ce test manuel).
5. Les compteurs correspondent aux attendus de la phase 1, avec **15 298 mandats** au lieu de 15 224
   (D1.16). Tout autre écart : regarder le code d'abord, puis la source, et mettre à jour les documents
   dans le même commit.
6. Relancer les quatre jobs ne change **aucune** ligne — condensés identiques sur `person`, `organe`,
   `mandat`, `scrutin`, `scrutin_groupe`, `vote`, `vote_mise_au_point` et `person_photo`. Cette dernière
   n'a jamais été vérifiée.
7. Aucune anomalie d'un `kind` hors de la table du plan.
8. Les chiffres de `enrich_wikidata` sont consignés dans data-sources.md et dans le message de commit.
9. `git status --ignored --short worker/tests/` ne cache aucun fichier de fixture.

## Note d'outillage

Plusieurs correctifs touchent des requêtes `sqlx::query!` (F2, F3, F7, F8). Après **chaque** modification,
régénérer les métadonnées avec `cargo sqlx prepare -- --all-targets` — sans `--all-targets`, les requêtes
des tests d'intégration perdent leurs entrées `.sqlx` et la CI casse en mode hors ligne. La CI le vérifie
explicitement (`cargo sqlx prepare --check -- --all-targets`).
