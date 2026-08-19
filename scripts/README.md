# Outils du cycle export → travail hors ligne → import

Ces scripts servent le cycle prévu par la [phase 3](../docs/plans/phase-3-categorisation.md) :
catégoriser en masse avec l'aide d'un modèle de langage, **sans que l'application ne dépende d'un
modèle**. Ils vivent hors du backend et hors du worker, n'ont aucune dépendance (bibliothèque
standard de Python uniquement) et ne touchent jamais la base : ils manipulent des fichiers.

La règle qui les gouverne tous : **la validation qui fait autorité est celle de l'application.**
Elle revérifie tout à l'import et refuse le fichier entier à la moindre anomalie (D3.15). Ces
scripts ne font que rendre l'erreur visible sur le poste, avec le numéro de ligne du lot fautif,
avant l'aller-retour de dépôt.

## Le cycle en cinq temps

### 1. Exporter

Depuis le back-office, connecté :

```
http://localhost:8000/admin/export?statut=non_categorises&format=json
```

Enregistrer le fichier, par exemple sous `export.json`. Il porte les scrutins **du corpus de
travail** — participation supérieure au seuil de `corpus_parametre` — et la liste des thèmes avec
leurs deux pôles.

### 2. Découper en lots

```bash
python3 scripts/export_batches.py export.json --out lots/
```

Le découpage **groupe par texte** et non par scrutin. Sur le corpus mesuré le 19/08/2026 — 988
scrutins — il n'y a que **244 textes distincts** : le thème est une propriété du texte, pas du vote,
et grouper garantit qu'un même texte reçoit un thème unique sans avoir à l'espérer. Chaque lot
contient la liste des thèmes recopiée depuis l'export, puis `uid`, date et titre des scrutins. Rien
d'autre : le bloc `groupes` de l'export pèse l'essentiel des octets et ne sert pas à choisir un
thème.

`--textes-par-lot` règle la taille (25 par défaut, soit une centaine de scrutins). Le script
affiche un ordre de grandeur du coût en jetons.

### 3. Faire travailler le modèle

Donner [`prompt_categorisation.md`](prompt_categorisation.md) puis le contenu d'un `lots/lot-NNN.txt`.
Un lot par conversation ou par sous-agent ; **le même prompt partout**, la cohérence entre lots
comptant plus que la finesse d'un lot.

Enregistrer chaque réponse en `reponses/lot-NNN.tsv`. Le format est du TSV à six colonnes — la
tabulation plutôt que la virgule parce qu'une justification contient des virgules et que le
guillemetage est ce qu'un modèle rate en premier.

Deux points du prompt méritent d'être connus de qui lance la campagne :

- **le modèle a pour consigne de sauter un scrutin dont il ne peut pas déterminer le sens.** C'est
  le cas courant des amendements, dont le titre nomme le texte porteur sans dire ce qu'ils font. Un
  scrutin absent du fichier n'est pas touché par l'import (D3.16) : sauter est sans danger, deviner
  ne l'est pas ;
- **la position ne décrit pas le texte mais ce que voter *pour* veut dire.** C'est la notion que le
  prompt explique le plus longuement, parce que c'est celle qu'on peut inverser sans s'en rendre
  compte.

### 4. Assembler et vérifier

```bash
python3 scripts/build_import.py --export export.json --reponses reponses/ --out import.csv
```

Le script refuse d'écrire quoi que ce soit tant qu'une erreur subsiste, et les liste toutes d'un
coup : `uid` inconnu, thème inconnu, doublon, valeur hors bornes, plus de trois décimales, somme des
poids différente de 1, justification trop courte, position posée sur un thème sans axe ou manquante
sur un thème qui en a un. Il affiche enfin combien de scrutins de l'export restent non catégorisés —
c'est la mesure du taux de saut du modèle.

Déposer ensuite `import.csv` sur `/admin/import`, **regarder l'aperçu** (créations, modifications,
inchangés, conflits) et appliquer.

### 5. Contrôler

```bash
podman compose exec -T postgres psql -U kyc -d kyc -f - < scripts/disagreements.sql
```

Trois résultats : le **taux d'accord** entre le signe des positions enregistrées et celui de la
mesure automatique d'axe (mesure 6 du plan), la **file de relecture** des désaccords triés par
qualité de la mesure, et la **couverture** du corpus.

C'est le bon usage de la mesure automatique, et le seul : elle est calculée sur les votes réels des
groupes, indépendamment du titre — donc indépendamment de ce sur quoi le modèle s'est appuyé. Un
désaccord signale que l'un des deux se trompe, ce qui vaut infiniment mieux qu'un échantillon tiré
au hasard. **Ne jamais faire l'inverse** : pré-remplir la position à partir de la mesure ferait
publier une position machine sous signature humaine, et le taux d'accord ne mesurerait plus que sa
propre influence (D3.7, D3.8, et le premier risque de la phase 3).

## Ce que ces scripts ne font pas

Ils n'appellent aucun modèle et n'accèdent à aucun réseau : l'appel au modèle est un geste manuel,
tracé, hors de l'application — c'est tout l'intérêt du cycle. Ils n'écrivent pas en base. Ils ne
marquent rien comme relu : une ligne importée reste `method = 'import'` avec `reviewed_at` vide,
donc protégée contre l'écrasement par un import ultérieur, et exclue par défaut des scores publics
de la phase 4 (D4.1). Marquer relu est un geste humain, dans le back-office, scrutin par scrutin.
