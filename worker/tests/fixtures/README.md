# Fixtures d'ingestion

**Fichiers JSON générés, commités en clair — jamais d'archive réelle ni de binaire** (D1.17,
docs/plans/phase-1.1-fix.md). Le `.zip` que lisent les jobs est reconstruit **en mémoire par les
tests**, via `tests/support/mod.rs` — voir « Comment c'est utilisé » ci-dessous.

**Le contenu est inventé, la forme est fidèle.** Aucun nom de personne, aucun libellé de groupe ou
de parti ci-dessous ne correspond à une personne ou une organisation réelle. Ce que les fixtures
reproduisent, ce sont les bizarreries de forme du JSON de l'Assemblée nationale (un XML converti
mécaniquement) et les cas qui ont fait échouer ou mal se comporter le parsing — pas des personnes.

**Les chiffres obtenus en ingérant ces fixtures ne sont pas ceux du corpus réel.** Les vrais
chiffres (nombre de scrutins, de votes, de mandats...) sont ceux de
[data-sources.md](../../../docs/data-sources.md), mesurés par l'ingestion complète du
17 août 2026. Les fixtures vérifient un **comportement** sur chaque cas listé plus bas, jamais un
volume : ne pas prendre les compteurs des tests d'intégration comme une référence de volume.

## Arborescence

```
fixtures/
  README.md
  amo30/json/organe/PO9000NN.json     référentiel des organes
  amo30/json/acteur/PA9000NN.json     référentiel des acteurs et de leurs mandats
  scrutins/json/*.json                cinq scrutins
```

Identifiants : format réel de la source (`PA9000NN`, `PO9000NN`, `PM9000NN`,
`VTANR5L<lég>V9000NN`, `VTCGR5L<lég>V9000NN` pour le Congrès — c'est le préfixe `VTCGR` qui décide
de `chambre`), mais numérotés `9000NN` pour qu'aucun ne puisse coïncider avec un identifiant réel.

## Comment c'est utilisé

`tests/support/mod.rs` zippe `amo30/` et `scrutins/` en mémoire au début de chaque test
(`zip_fixture_dir`), avec `CompressionMethod::Stored` (la compression n'apporte rien ici). Les
chemins d'entrée du zip sont les chemins relatifs à `amo30/` ou `scrutins/` : `amo30/json/organe/
PO900001.json` devient l'entrée `json/organe/PO900001.json`, exactement ce que filtrent
`read_amo30_docs` et `read_scrutin_docs`. Ajouter un cas de test, c'est ajouter un fichier JSON,
rien de plus — pas besoin de retoucher le helper.

`zip_fixture_files` construit un zip ne contenant qu'un sous-ensemble nommé des fichiers d'un
répertoire, pour les tests qui doivent réingérer un extrait (voir F10 ci-dessous).

## Le référentiel (`amo30/`)

Organes (8 fichiers, 7 retenus par `ingest_acteurs` — voir D1.12) :

| `an_uid` | `codeType` | Rôle dans les tests |
| --- | --- | --- |
| `PO900001` | GP | groupe « Alpha », vote dans V900001/V900002/V900004, réutilisé pour deux cas de normalisation de mandats |
| `PO900002` | GP | groupe « Beta », vote dans V900001/V900005, réutilisé pour la charnière et le mandat en cours |
| `PO900003` | GP | groupe « Gamma », vote dans V900001 uniquement |
| `PO900004` | GP, `libelleAbrev = "NI"` | pseudo-groupe non-inscrit, `is_non_inscrit = true` |
| `PO900005` | PARPOL | parti fictif, rattachement d'un acteur (D1.8) |
| `PO900006` | GP | groupe « Delta », second organe du cas de chevauchement inter-organes réel |
| `PO900007` | GROUPESENAT | **non retenu** par `ingest_acteurs` (D1.12) : sert le cas « groupe d'un type non retenu » |
| `PO900010` | ASSEMBLEE | organe de rattachement des mandats de siège et `organeRef` de tête des scrutins |

Acteurs (17 fichiers) : `PA900001` à `PA900009` votent dans les scrutins et couvrent les formes de
scalaire/collection (voir tableau plus bas) ; `PA900010` à `PA900015` sont dédiés à la
normalisation des mandats de groupe ; `PA900020`/`PA900021` sont dédiés à F4 (résolution du groupe
depuis les mandats sur une ligne fantôme). Trois acteurs sont **cités par les scrutins mais
absents du référentiel** — aucun fichier `PA900023.json`, `PA900024.json`, `PA900099.json` — pour
exercer `acteur_inconnu`.

## Les scrutins (`scrutins/`)

| Fichier | Cas couvert | Forme réelle inspirée de |
| --- | --- | --- |
| `VTANR5L17V900001.json` | scrutin ordinaire, 3 groupes, vote par délégation, mise au point réellement remplie (avec blocs `[null, null]` à ignorer), les trois causes de non-vote (`PSE`, `PAN`, `MG`), une cause inédite (`XYZ` → `cause_non_vote_inconnue`), un bloc nominatif inconnu (`blocMystere` → `bloc_nominatif_inconnu`) | `VTANR5L15V3415` |
| `VTANR5L17V900002.json` | groupe fantôme `PO0`, trois lignes sur le même scrutin ; F4 : un votant sans mandat de groupe (`groupe_organe_id NULL`, `groupe_from_mandat = false`) et un votant avec un mandat de groupe couvrant la date (`groupe_from_mandat = true`, organe résolu) | `VTANR5L17V501` |
| `VTANR5L17V900003.json` | compteurs incohérents : `syntheseVote` déclare 60 votants/non-votants pour un seul votant nominatif réel | `VTANR5L17V1` |
| `VTCGR5L16V900001.json` | scrutin du Congrès : blocs nominatifs au **singulier** (`pour`/`contre`/`abstention`/`nonVotant`), un groupe d'un type non retenu (`GROUPESENAT`, même chemin que `PO0`), deux acteurs absents du référentiel | `VTCGR5L16V1` |
| `VTANR5L15V900001.json` | troisième législature du corpus (15e), scrutin sans particularité, pour la couverture des trois législatures et les tests `since` / réingestion partielle | forme ordinaire, aucun cas spécifique |

Scalaires sous ses quatre formes, exercés dans le référentiel : chaîne nue (`PA900002.acteur.uid`),
objet à `#text` (`PA900001.acteur.uid`), objet `@xsi:nil` (`PA900002.etatCivil.ident.trigramme`,
`PA900015` : `dateDebut` du premier mandat), `null` littéral (`PA900002.etatCivil.dateDeces`, et de
nombreux blocs de scrutin comme `contres: null`). Collections sous ses trois formes : absente
(`PA900003` sans clé `mandats`, `V900001` groupe A sans clé `abstentions`), objet singleton
(la plupart des blocs à un seul votant), tableau (`V900001` groupe A `pours`, deux votants).

## Mandats de groupe — cas de normalisation (F5, D1.16)

| Acteur | Cas | Organe(s) | Résultat attendu |
| --- | --- | --- | --- |
| `PA900010` | inclusion | `PO900001` × 2 | 1 mandat retenu, `mandat_inclus` journalisé |
| `PA900011` | charnière | `PO900002` × 2 | 2 mandats retenus (fin du premier raccourcie d'un jour), `mandat_charniere` journalisé |
| `PA900012` | chevauchement inter-organes réel | `PO900001` + `PO900006` | **les deux mandats retenus** (D1.16), `mandat_chevauchement` journalisé |
| `PA900013` | chevauchement avec le pseudo-groupe non-inscrit | `PO900004` (NI) + `PO900001` | les deux mandats retenus, **aucune** anomalie |
| `PA900014` | mandat en cours (`dateFin` absente) | `PO900002` | conservé tel quel |
| `PA900015` | mandats à écarter | `PO900001` (dateDebut manquante) + `PO900002` (dateFin < dateDebut) | aucun des deux inséré, aucune anomalie (ce sont des compteurs, pas des `ingestion_anomaly`) |

`PA900012` est la forme inventée du cas réel documenté dans data-sources.md : la scission
UMP / Rassemblement-UMP de novembre 2012, un vrai fait politique qui chevauche deux vrais groupes
et que la base doit accepter et signaler, jamais trancher silencieusement (voir CLAUDE.md, « Les
données brutes ne sont jamais écrasées par une interprétation »).
