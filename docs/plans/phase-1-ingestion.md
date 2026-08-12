# Phase 1 — Ingestion des scrutins et des mandats

**Statut : 📝 à relire** · Dépend de : phase 0 · Bloque : phases 2, 3, 4

## Objectif

Avoir en base, de façon fiable et rejouable, **qui a voté quoi et quand**, ainsi que **qui appartenait à
quel groupe ou parti à quelle période**, sur les dix dernières années.

C'est la phase qui porte le plus de risque : tout le reste dépend de la qualité de ces données.

## Périmètre

**Dedans** : téléchargement et parsing des scrutins de l'Assemblée nationale, des acteurs/mandats/organes,
enrichissement Wikidata (identité, partis hors AN, photos), modèle de données correspondant, jobs
d'ingestion paramétrables et idempotents.

**Dehors** : toute catégorisation, tout score, toute page publique. Sénat et Parlement européen (phase 6).

## Étape préalable obligatoire : le spike

Avant d'écrire du code de production, une exploration **jetable** (script Rust ou notebook Python, non
mergé) qui répond par des faits, pas par des suppositions :

1. Quelles URLs répondent réellement aujourd'hui, pour les législatures 15, 16 et 17 ? Existe-t-il un jeu
   de données *historique* couvrant plusieurs législatures d'un coup ?
2. Quelle est la structure exacte d'un fichier de scrutin ? Combien de scrutins par législature, quel
   poids d'archive ?
3. Comment sont représentées les `misesAuPoint` (corrections de vote a posteriori) et les délégations ?
4. Les dates de début et de fin d'appartenance à un groupe sont-elles fiables et présentes dans AMO ?
5. Quel dénominateur permet de calculer une participation cohérente ?
6. Combien de personnes ont un `QID` Wikidata avec une photo sous licence utilisable ?

**Livrable du spike** : une mise à jour de [data-sources.md](../data-sources.md) et de la section
« corpus » de [methodology.md](../methodology.md) avec des chiffres réels. Le reste de la phase peut
changer selon ce qu'on trouve — c'est normal et prévu.

## Livrables

1. Migrations : `person`, `party`, `party_membership`, `scrutin`, `vote`, `source_document`.
2. Job `ingest_scrutins { legislature, since?, min_participation? }` — idempotent.
3. Job `ingest_actors { legislature }` — personnes, mandats, appartenances aux groupes.
4. Job `enrich_wikidata { person_ids? }` — QID, partis hors AN, photo + licence.
5. Un rapport d'ingestion consultable : compteurs, anomalies, durée, hash de la source.
6. Tests d'intégration sur un jeu réduit d'archives réelles versionnées comme fixtures.

## Modèle de données — points saillants

- `party_membership(person_id, party_id, period daterange, source, is_parliamentary_group)` avec une
  contrainte `EXCLUDE USING gist` empêchant deux appartenances au même type de structure de se chevaucher.
  C'est ce qui garantit qu'on peut répondre sans ambiguïté à « quel parti au moment de ce vote ».
- `vote(scrutin_id, person_id, position, groupe_id_at_vote, is_delegated, was_corrected)` : on garde le
  groupe **au moment du vote**, pas le groupe actuel.
- `scrutin` porte les compteurs bruts *et* un taux de participation en colonne générée, pour pouvoir
  filtrer par index.
- `source_document` archive le payload brut avec son URL et son hash, avant tout parsing.

## Étapes

1. Spike (ci-dessus).
2. Migrations du modèle, avec les contraintes et les index dès le départ.
3. Client HTTP du worker : téléchargement, vérification MD5, décompression en flux, `User-Agent` du
   projet, mise en cache locale pour ne pas retélécharger pendant le développement.
4. Parsing des scrutins → archivage brut → `upsert` scrutins et votes, en `COPY` par lots.
5. Parsing AMO → personnes, groupes, appartenances datées.
6. Enrichissement Wikidata (SPARQL), en respectant la priorité : l'AN prime en cas de conflit.
7. Réconciliation des personnes entre sources (AN ↔ Wikidata) : par identifiant quand il existe, sinon
   nom + date de naissance, et **file d'attente manuelle pour les cas douteux** — pas de fusion
   automatique approximative.
8. Rapport d'ingestion et jeu de tests.

## Décisions à trancher

| # | Question | Proposition |
| --- | --- | --- |
| D1.1 | Ingérer tous les députés ou seulement les candidat·es pressenti·es ? | **Tous** : le calcul des positions de parti a besoin de l'ensemble des votants, et la liste des candidat·es n'est pas stable en 2026 |
| D1.2 | Filtrer les scrutins à l'ingestion ou au calcul ? | **Au calcul** : on ingère tout ce qui est disponible sur 10 ans, le seuil de participation devient un filtre de scoring. Rendre le seuil additif comme demandé, sans réingestion |
| D1.3 | Suivre les corrections de vote (`misesAuPoint`) ? | Les stocker dans tous les cas, et décider après le spike si elles modifient la position retenue. Un député qui déclare s'être trompé est une information |
| D1.4 | Traiter les votes par délégation ? | Les marquer et les compter, en les signalant dans l'UI |
| D1.5 | Photos : stocker le binaire ou l'URL ? | **URL + métadonnées de licence** d'abord ; mise en cache locale seulement si Commons se révèle lent |
| D1.6 | Dénominateur de la participation | À fixer après le spike, puis figer dans methodology.md |

## Fini quand

- Une ingestion complète sur 10 ans s'exécute sans intervention et produit un rapport.
- **Relancer exactement le même job ne change aucune ligne** (vérifié par un test qui compte les
  modifications).
- Une ingestion partielle (une seule législature) n'abîme pas les données déjà présentes.
- Pour trois député·es choisi·es à la main, la liste des votes et l'historique de groupe correspondent à
  ce qu'affiche le site de l'Assemblée nationale — vérification manuelle documentée dans la PR.
- Aucune personne en double dans `person`.

## Risques

- **Formats hétérogènes entre législatures** : c'est le risque principal. L'archivage du brut permet de
  rejouer le parsing, mais il faudra probablement du code par législature.
- **Réconciliation des identités** : homonymes, particules, noms d'usage, changements de nom. Prévoir la
  file manuelle dès le début plutôt que de la découvrir dans les données.
- **Continuité des partis** : renommages, fusions, scissions. Modéliser un lien de succession entre partis
  plutôt que de fusionner les entités.
- **Volume** : à évaluer au spike. Si les archives sont trop lourdes pour l'hébergement visé, l'ingestion
  se fait en local avec import du dump — à décider en phase 5.
