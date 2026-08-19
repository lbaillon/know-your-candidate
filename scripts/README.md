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
python3 scripts/export_batches.py export.json --out lots/ --sans-amendements
```

**`--sans-amendements` n'est pas une option de confort.** Un amendement n'est pas catégorisable
depuis son titre, qui nomme le texte porteur sans jamais dire ce que l'amendement fait : lors de la
première campagne, 557 des 674 scrutins sautés par les agents étaient des amendements — 83 % des
sauts, et plus de la moitié des jetons dépensés pour rien. Sur le corpus actuel, le drapeau fait
passer 988 scrutins à 401. Ils redeviendront catégorisables le jour où le contenu des textes sera
ingéré (phase 6), pas avant.

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

Prompt pour lancer des sous-agents pour tout faire en une fois :

```markdown
Dans ce dépôt, campagne de catégorisation (voir scripts/README.md, étape 3).

Pour chaque fichier lots/lot-NNN.txt, lance UN sous-agent chargé de :
  1. lire scripts/prompt_categorisation.md (le prompt, à suivre à la lettre) ;
  2. lire lots/lot-NNN.txt (les thèmes et les scrutins) ;
  3. écrire sa réponse dans reponses/lot-NNN.tsv, au format décrit par le prompt ;
  4. me répondre UNIQUEMENT « lot-NNN : X lignes écrites, Y scrutins sautés ».

Contraintes, à répéter à chaque sous-agent :
  - ne pas modifier le prompt, ne pas l'améliorer, ne pas discuter ses règles ;
  - un scrutin dont le sens du vote n'est pas déterminable ne reçoit AUCUNE ligne ;
  - ne rien inventer du contenu d'un texte : seul le titre est disponible ;
  - écrire le fichier, ne jamais recopier son contenu dans la réponse.

Six sous-agents en parallèle au maximum. À la fin, liste les lots traités et le
total de lignes. Ne lance pas l'import : je m'en charge.
```

### 4. Assembler et vérifier

```bash
python3 scripts/build_import.py --export export.json --reponses reponses/ \
    --modele claude-sonnet-5 --out import.json
```

Le script refuse d'écrire quoi que ce soit tant qu'une erreur subsiste, et les liste toutes d'un
coup : `uid` inconnu, thème inconnu, doublon, valeur hors bornes, plus de trois décimales, somme des
poids différente de 1, justification trop courte, position posée sur un thème sans axe ou manquante
sur un thème qui en a un. Il affiche enfin combien de scrutins de l'export restent non catégorisés —
c'est la mesure du taux de saut du modèle.

**Le format de sortie est du JSON, et `--modele` est obligatoire.** Le schéma d'échange ne transporte
le modèle utilisé qu'en JSON : un import CSV arrive en base avec `label_import.generateur` vide, et
« ces neuf cents lignes viennent de tel modèle » ne se reconstitue pas après coup. Or methodology.md
§ 5.c impose qu'une catégorisation produite par un modèle soit signalée comme telle dans l'interface.
`--format csv` reste possible pour un aller-retour par tableur ; le script prévient alors, sur la
sortie d'erreur, que la trace sera perdue.

Déposer ensuite `import.json` sur `/admin/import`, **regarder l'aperçu** (créations, modifications,
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

## Ce que la première campagne a appris (19/08/2026)

988 scrutins donnés à des agents, 674 sautés, et sur les 314 traités **251 étiquetés `autre`** : 63
catégorisations substantielles seulement. Le dépouillement a séparé trois causes, qu'il vaut la peine
de connaître avant de relancer :

1. **Les amendements — 557 des 674 sauts.** La règle d'honnêteté qui fonctionne, pas un défaut. D'où
   `--sans-amendements` ci-dessus.
2. **Quatre thèmes manquaient — environ 98 scrutins** : institutions/démocratie (63), agriculture
   (14), Europe (11), travail/entreprise (7). Exactement ceux que methodology.md § 4 annonçait. Ils
   ont été ajoutés à `db/seeds/themes.toml`.
3. **Deux défauts du prompt, pour la part la plus coûteuse.** Son exemple classait une motion de
   rejet en `autre` (« le vote porte sur la procédure ») : les agents l'ont recopié 51 fois, et 51
   des votes les plus clivants du corpus sont partis à la poubelle. Et sa règle de saut confondait
   « je ne sais pas ce que fait ce texte » avec « ce texte fait trop de choses » — d'où des lois de
   finances sautées. Les règles 6 et 7 du prompt répondent aux deux.

Le reste — environ 90 scrutins — est une traîne légitime : Mayotte, programmation militaire, fin de
vie, contenus haineux en ligne, JO, covid. Là, `autre` est la bonne réponse, et c'est précisément à
cela qu'il sert (D3.5).

**Leçon transposable** : un exemple dans un prompt pèse plus lourd qu'une règle. Celui de la motion
disait le contraire de ce que la méthodologie impose, et il a gagné 51 fois contre le texte des
règles. Toute modification de `prompt_categorisation.md` doit être relue comme une page de
méthodologie — ses exemples en premier.

## Ce que ces scripts ne font pas

Ils n'appellent aucun modèle et n'accèdent à aucun réseau : l'appel au modèle est un geste manuel,
tracé, hors de l'application — c'est tout l'intérêt du cycle. Ils n'écrivent pas en base. Ils ne
marquent rien comme relu : une ligne importée reste `method = 'import'` avec `reviewed_at` vide,
donc protégée contre l'écrasement par un import ultérieur, et exclue par défaut des scores publics
de la phase 4 (D4.1). Marquer relu est un geste humain, dans le back-office, scrutin par scrutin.
