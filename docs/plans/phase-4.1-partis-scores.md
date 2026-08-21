# Phase 4.1 — Corrections des scores

**Statut : ✅ validé** · Décision arbitrée le 19/08/2026 · Dépend de : phase 4 · Bloque : phase 5

## Objectif

Corriger les quatre défauts relevés à la revue de la phase 4. **Aucune fonctionnalité nouvelle.**

Le bloquant n'est pas un bug d'ingénierie — le code fait exactement ce que le plan de la phase 4
demandait. C'est **le plan qui avait un trou**, et ce trou produit aujourd'hui, sur la fiche publique
d'une personne réelle et identifiable en contexte électoral, une affirmation que nos propres données
contredisent. C'est la faute la plus grave que ce projet puisse commettre : elle ne casse rien, elle
ment (même diagnostic qu'en [phase 2.1](phase-2.1-fix.md), pour une raison différente).

Cette section est autoportante : **tout ce qui est nécessaire est ici, dans
[phase-4-partis-scores.md](phase-4-partis-scores.md), dans [methodology.md](../methodology.md), dans
[phase-3.0-feedback.md](phase-3.0-feedback.md) ou dans [CLAUDE.md](../../CLAUDE.md)**. Développement
sur le tronc, un commit par correctif, `main` vert à chaque commit.

Chaque diagnostic ci-dessous a été **reproduit** : requête sur la base réelle ou rendu de la page par
HTTP. Les causes sont établies, pas supposées.

## Ce que la revue a validé — ne pas y toucher

La phase 4 est bien faite et la plus grande partie ne doit pas être retouchée :

- **le calcul est exact et idempotent.** Cinq exécutions successives, empreintes `md5` identiques sur
  `person_theme_score` **et** sur `mandat_theme_score` (`8e44ceff…` et `2cb15df6…` pour les runs 4
  et 5). Les compteurs de run sont identiques à la ligne près ;
- **le piège nommé par le plan a été évité** : `apport` inverse le signe avec le *vote*, jamais
  `position_pour`, et un test le cible explicitement ;
- **les seuils sont respectés** : 7 thèmes éligibles, Europe (2 scrutins) et immigration (9) écartés
  par D4.7, `contributions_min` appliqué par couple personne-thème ;
- **les règles d'affichage sont tenues**, vérifiées en rendant les pages : le pôle est nommé avant le
  chiffre (« plutôt redistribution et protection sociale étendue (-0.27) »), chaque orientation porte
  son niveau de preuve (« 5 scrutins — aucune de ces catégorisations n'a encore été relue par un
  humain »), les abstentions écartées sont comptées, « données insuffisantes : seulement 3 scrutins
  exploitables » remplace tout score sous le seuil ;
- **les états vides disent pourquoi.** La fiche de Bruno Retailleau explique que son unique vote est
  celui du Congrès et que le corpus ne couvre que l'Assemblée ; celle de Nathalie Arthaud dit
  l'absence de mandat. C'est exactement ce que la phase 2.1 avait dû corriger ailleurs, et la leçon a
  été retenue ;
- **la bipolarité** est conforme à D4.9, testée sur les quatre cas limites, y compris le votant
  exactement sur `mu_plus` ;
- **la correction de performance** trouvée pendant la passe finale (JIT déclenché par une mauvaise
  estimation de cardinalité sur `jsonb_array_elements_text`) est bien diagnostiquée, corrigée à la
  cause et remesurée : 218 ms → 9 ms.

`make lint`, `make typecheck` et `make test` sont verts (324 tests Python, tous les tests Rust).

## Bloquants

### F1 — Le score affirme le contraire de ce qu'un vote signifie, sur des personnes nommées

**Reproduit** en rendant `/personne/jean-luc-melenchon`. La page affiche aujourd'hui :

> **Environnement — plutôt priorité à l'activité économique et à la transition graduée (+0.21)**

**Ce qui produit ce chiffre**, lu dans `score_contribution` : sur ses quatre contributions qui pèsent,
trois sont des votes **contre** — contre la loi climat de 2021 (deux lectures) et contre l'inscription
de la préservation de l'environnement à l'article 1er de la Constitution. La catégorisation de ces
textes est correcte (`position_pour` négatif : voter pour, c'est la contrainte réglementaire). La
formule en déduit mécaniquement que voter contre, c'est se placer au pôle opposé.

Or Jean-Luc Mélenchon et son groupe ont voté contre ces textes en les jugeant **insuffisants**. Le
projet affirme donc l'inverse de la réalité, sur une personne nommée, sur le thème le plus identitaire
de son mouvement.

**Ce n'est pas un cas isolé, et le signal qui le détecte existe déjà.** La phase 3 a construit
`scrutin_axis_estimate`, une mesure de la position d'un scrutin dérivée des votes des groupes,
indépendante du titre. Quand elle contredit la catégorisation en signe, c'est précisément que
« voter contre = tenir la position opposée » ne s'applique pas. Mesuré sur le run courant :

| | |
| --- | --- |
| Contributions qui pèsent dans un score | 52 848 |
| Dont issues d'un scrutin où catégorisation et mesure **se contredisent en signe** | **20 107, soit 38,0 %** |

Part du **poids** total provenant de ces scrutins, par thème :

| Thème | Part du poids en désaccord |
| --- | --- |
| institutions-democratie | 57,6 % |
| social-fiscalite | 48,6 % |
| agriculture | 28,2 % |
| sante | 19,5 % |
| travail-entreprise | 18,8 % |
| environnement | 13,4 % |
| securite | 8,4 % |

**Pourquoi la bipolarité ne l'attrape pas.** `bipolarite` mesure si le camp « contre » se répartit **de
part et d'autre** du camp « pour » (D4.9) : elle vise le budget rejeté par les deux extrêmes. Ici
l'opposition vient d'**un seul bord — le même que celui du texte**. Sur la loi climat, les groupes
ayant voté contre sont à −0,6 (SOC, FI, GDR) contre un camp « pour » à −0,1 : la bipolarité ne vaut
que 0,286, le poids reste à 0,50, et le signe reste faux.

**Correctif proposé** — à valider, voir « Décision à trancher » ci-dessous : **une contribution issue
d'un scrutin où les deux lectures se contredisent ne compte pas**. Elle reste écrite dans
`score_contribution` avec un poids nul et une raison, donc visible dans l'explication, et le compteur
affiché à côté de l'orientation le dit (« 4 scrutins écartés : notre mesure automatique contredit la
catégorisation »). Formulé en une phrase : **on ne score que là où nos deux lectures indépendantes
concordent.**

Restriction impérative : ce filtre ne s'applique **que sur les thèmes dont l'axe se lit gauche-droite**.
Sur `institutions-democratie`, `agriculture` et `europe`, la mesure est du bruit par construction (voir
la réserve écrite dans `db/seeds/themes.toml`) : elle ne peut arbitrer quoi que ce soit, et l'utiliser
comme filtre y serait aussi faux que de l'ignorer ailleurs. Ces thèmes relèvent de F2.

**Ce que le correctif coûte, et qu'il faut assumer** : entre un tiers et la moitié du poids disparaît
sur les thèmes les plus fournis, des personnes repassent sous `contributions_min` et perdent une
orientation. C'est le prix d'un chiffre défendable, et le « Fini quand » de la phase 4 le demandait
déjà — « quelqu'un qui conteste un résultat peut voir exactement ce qui l'a produit ».

**Tests à écrire avant le code** : le cas Mélenchon-environnement reconstruit à la main (trois votes
contre sur des textes à `position_pour` négatif dont la mesure est positive) doit produire « données
insuffisantes », pas une orientation inversée ; une contribution écartée apparaît bien dans
l'explication avec sa raison ; le compteur d'écartés est exact ; un thème non gauche-droite n'est pas
filtré.

## Importants

### F2 — `institutions-democratie` classe les trois candidat·es du même côté

**Reproduit** par requête sur le run courant :

| Personne | Score `institutions-democratie` | Contributions |
| --- | --- | --- |
| Jean-Luc Mélenchon | −0,589 | 5 |
| Gabriel Attal | −0,416 | 7 |
| Marine Le Pen | −0,365 | 10 |

Trois responsables politiques que tout oppose se retrouvent du même côté de l'axe, à des valeurs
proches. Ce n'est pas une coïncidence : sur ce thème, **57,6 % du poids vient de scrutins contredits**
(F1), et l'axe lui-même — « pouvoirs du Parlement élargis » contre « autorité de l'exécutif » — n'est
pas un axe gauche-droite. Tout parlementaire d'opposition, quel que soit son bord, vote contre les
textes institutionnels du gouvernement et atterrit donc au même endroit. Le score ne mesure pas une
position sur les institutions, il mesure **le fait d'être dans l'opposition**.

C'est le défaut que la réserve de `db/seeds/themes.toml` annonçait, matérialisé.

**Correctif proposé** : les thèmes dont l'axe ne se lit pas gauche-droite
(`institutions-democratie`, `agriculture`, `europe`) **ne produisent pas d'orientation publique** tant
qu'aucune relecture humaine n'existe. Deux façons de le faire, à trancher à l'implémentation : une
colonne `theme.axe_gauche_droite` alimentée par le seed (explicite, relisible en diff), ou un
paramètre dans `score_parametre`. La première est préférable — c'est une propriété du thème, pas du
calcul.

Le calcul continue de les produire en base : ils restent consultables dans le back-office, et
redeviendront publiables le jour où la relecture humaine leur donnera un fondement. Ce qui disparaît,
c'est l'affirmation publique, pas la donnée.

### F3 — `recompute_scores` charge tous les votes en mémoire

`compute_person_scores` fait un `fetch_all` sur la jointure votes × catégorisations : **105 385 lignes
aujourd'hui**, ce qui passe sans difficulté. Mais 230 scrutins sur 988 sont catégorisés, et le seuil de
participation du corpus est destiné à baisser (`corpus_parametre`, phase 3) : les deux facteurs se
multiplient, et la [phase 5](phase-5-deploiement.md) vise un hébergement dont elle dit elle-même que
« mémoire et CPU sont serrés ».

**Correctif** : streamer avec `fetch(...)` plutôt que `fetch_all`. Le code est déjà écrit pour :
il consomme les lignes triées par `(person_id, theme_id)` et n'accumule qu'un groupe à la fois. Les
trois autres calculs (groupe, mandat) travaillent sur `scrutin_groupe`, dont la volumétrie est bornée
par construction — ils n'ont pas ce problème.

## Mineurs

### F4 — La cohésion n'utilise jamais la moitié basse de son échelle

`cohesion` vaut « part des voix alignées sur la position majoritaire du groupe » : par construction
elle ne peut pas descendre sous 0,5. Mesuré sur le run courant : minimum 0,583, médiane 1,000.

Le libellé est exact — « cohésion : 58 % des voix du groupe alignées sur sa position majoritaire » —
mais un lecteur qui voit 58 % sur une échelle qu'il suppose aller de 0 à 100 % lit « moyennement
cohésif » là où il faudrait lire « aussi divisé qu'un groupe peut l'être ».

**Correctif** : ajouter au libellé la borne basse (« 50 % correspondant à un groupe coupé en deux »),
ou publier `2 × cohesion − 1`. La première option est préférable : elle ne touche ni au schéma ni au
calcul, et elle explique au lieu de transformer.

## Décision arbitrée

F1 admettait quatre réponses. **L'option A est retenue** (19/08/2026) : une contribution issue d'un
scrutin où les deux lectures se contredisent **ne compte pas**. Les trois autres ont été écartées, et
leurs raisons sont conservées ici parce qu'elles reviendront :

| Option | Ce qu'elle fait | Pourquoi elle n'est pas retenue |
| --- | --- | --- |
| **A — écarter les contributions contredites** | On ne score que là où les deux lectures concordent ; les écartées restent visibles dans l'explication avec leur raison | **Retenue.** Seule option qui cesse d'affirmer ce que nos données contredisent sans renoncer à publier, et la seule qui s'explique en une phrase |
| B — les pondérer sans les écarter | Un facteur (0,25) au lieu de zéro | Atténue le signe faux sans le corriger : Mélenchon resterait du mauvais côté, en plus pâle. Un mensonge pâle reste un mensonge |
| C — ne rien changer, afficher le désaccord | « Sur 4 de ces 5 scrutins, notre mesure contredit la catégorisation » | Transparent, mais le projet continue d'affirmer publiquement ce qu'il sait douteux. La transparence ne rachète pas l'affirmation |
| D — ne pas publier de score par personne avant relecture | Coupe le problème à la racine | L'application perdrait sa promesse principale pour une durée indéterminée. Reste la meilleure réponse *si* la 4.1 ne suffit pas — à réexaminer à la vérification 6 |

**Ce que le correctif coûte, et qu'il faut assumer** : entre un tiers et la moitié du poids disparaît
sur les thèmes les mieux fournis, des personnes repassent sous `contributions_min` et perdent une
orientation. C'est le prix d'un chiffre défendable.

**Ce qu'il ne règle pas** : un vote contre pour insuffisance reste indétectable quand nos deux
lectures **concordent** à tort. C'est une limite structurelle du corpus binaire, elle appartient à
[methodology.md § 7.2](../methodology.md), et les seules vraies réponses sont la relecture humaine
puis l'ingestion du contenu des textes ([phase 6](phase-6-backlog-v2.md)). Le correctif réduit la
surface du problème, il ne l'élimine pas — et le plan ne doit pas laisser croire le contraire.

## Plan d'exécution

Autoportant. Les décisions ci-dessus ne sont pas à rediscuter ; si l'une s'avère fausse au contact du
code, le dire et proposer une révision plutôt que contourner.

### Migration `0010_score_accord.sql`

```sql
-- Phase 4.1 — F1 et F2 : une contribution dont les deux lectures se contredisent ne compte pas, et
-- un thème dont l'axe ne se lit pas gauche-droite ne produit pas d'orientation publique.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Quels axes la mesure peut-elle arbitrer ? (F2) -----------------------------------------------
--
-- L'estimation d'axe situe un scrutin sur l'axe gauche-droite des groupes. Elle peut donc contredire
-- utilement une catégorisation sur `securite` ou `social-fiscalite` ; elle ne dit rien sur l'Europe
-- (intégration contre souveraineté), l'agriculture ou les institutions, dont les axes sont
-- orthogonaux à celui-là. La réserve était écrite dans db/seeds/themes.toml depuis la phase 3 ;
-- elle devient une colonne, parce qu'elle commande maintenant du calcul et de l'affichage.
--
-- `DEFAULT true` puis correction explicite : un thème ajouté plus tard sans y penser serait traité
-- comme gauche-droite, donc filtré par F1 — le comportement prudent. Le seed, lui, rend le champ
-- obligatoire : ajouter un thème force à répondre à la question.

ALTER TABLE theme ADD COLUMN axe_gauche_droite boolean NOT NULL DEFAULT true;

UPDATE theme SET axe_gauche_droite = false
WHERE slug IN ('institutions-democratie', 'agriculture', 'europe', 'autre');

-- 2. Pourquoi une contribution ne pèse pas (F1) ----------------------------------------------------
--
-- Une contribution écartée reste écrite : elle est la trace de ce qui a été regardé puis mis de côté,
-- et l'explication l'affiche avec sa raison. C'est la différence entre « ce vote ne compte pas » et
-- « ce vote n'existe pas », que le projet ne confond jamais.

CREATE TYPE contribution_exclusion AS ENUM ('abstention', 'desaccord_mesure');

ALTER TABLE score_contribution ADD COLUMN exclusion contribution_exclusion;

-- Toute exclusion annule le poids. L'inverse n'est pas vrai : un poids nul peut aussi venir d'une
-- bipolarité de 1 (D4.9), qui n'est pas une exclusion mais une pondération qui tombe à zéro.
ALTER TABLE score_contribution
    ADD CONSTRAINT score_contribution_exclusion_sans_poids
        CHECK (exclusion IS NULL OR poids = 0);

-- PIÈGE À NE PAS REPRODUIRE : écrire cette contrainte
--     CHECK ((position = 'abstention') = (exclusion = 'abstention'))
-- laisserait passer une abstention sans exclusion, parce que `exclusion = 'abstention'` vaut NULL
-- quand la colonne est NULL, que la comparaison entière vaut alors NULL, et qu'un CHECK à NULL
-- passe. `IS NOT DISTINCT FROM` compare en traitant NULL comme une valeur.
ALTER TABLE score_contribution
    ADD CONSTRAINT score_contribution_abstention_toujours_exclue
        CHECK ((position = 'abstention') = (exclusion IS NOT DISTINCT FROM 'abstention'));

-- 3. Ce que l'orientation doit pouvoir dire ---------------------------------------------------------

ALTER TABLE person_theme_score
    ADD COLUMN ecartes_desaccord integer NOT NULL DEFAULT 0 CHECK (ecartes_desaccord >= 0);
```

Le compteur n'est ajouté qu'à `person_theme_score` : c'est la seule surface où l'on explique un
chiffre scrutin par scrutin. Les scores de groupe et de mandat appliquent **le même filtre** — sans
quoi la position d'une personne et celle de son groupe ne porteraient plus sur le même corpus et ne
seraient plus comparables — mais leur nombre de contributions reflète déjà le retrait.

### Le filtre, écrit une fois

Pour une contribution `(personne, thème, scrutin)` :

```
mesure_utilisable = theme.axe_gauche_droite
                    AND estimate.position_pour IS NOT NULL
                    AND estimate.position_pour <> 0

ecartee = mesure_utilisable
          AND sign(label.position_pour) <> sign(estimate.position_pour)
```

Trois cas limites, tous décidés :

- **`estimate.position_pour = 0`** (exactement) : la mesure ne tranche rien, elle ne peut donc pas
  contredire. La contribution est **retenue**. Même règle que `scripts/disagreements.sql`, qui exclut
  déjà les positions nulles des deux côtés — les deux doivent rester cohérents ;
- **thème dont l'axe n'est pas gauche-droite** : aucune contribution n'est écartée par ce motif. Le
  calcul continue comme avant ; c'est l'**affichage** qui change (F2) ;
- **`label.position_pour = 0`** : déjà impossible en pratique (le prompt l'interdit) mais le code ne
  doit pas s'y fier — signe nul, donc pas de contradiction possible, contribution retenue.

Le filtre s'applique aux **quatre** calculs : personne, groupe, mandat, et l'écriture des
contributions. Pour les trois derniers, il se traduit par une condition supplémentaire dans la
jointure SQL ; pour le premier, par un `exclusion = 'desaccord_mesure'` et un poids nul.

### Le seed des thèmes

`db/seeds/themes.toml` gagne un champ **obligatoire** `axe_gauche_droite` sur chaque entrée, et
`seed_themes` refuse un fichier où il manque — c'est ce qui force la question à être posée pour tout
thème futur. Valeurs à écrire, cohérentes avec la migration : `true` pour `social-fiscalite`,
`environnement`, `sante`, `education`, `securite`, `immigration` ; `false` pour
`institutions-democratie`, `agriculture`, `europe`, `autre`.

La réserve déjà présente en tête du fichier depuis la phase 3 est à compléter d'une phrase : ce champ
ne dit pas qu'un thème est moins légitime, il dit que **notre mesure automatique ne sait pas
l'arbitrer**, et qu'il n'est donc ni filtré par F1 ni publié tant qu'aucune relecture humaine ne lui
donne un fondement.

### Affichage

- **À côté de chaque orientation concernée** : « · 4 scrutins écartés : notre mesure automatique
  contredit la catégorisation ». La phrase dit *ce qui s'est passé*, pas *qui a tort* — on ne sait pas
  laquelle des deux lectures se trompe, seulement qu'elles ne concordent pas.
- **Sur la page d'explication** : les scrutins écartés sont listés **avec les autres**, dans une
  section distincte intitulée « Écartés du calcul », chacun avec sa raison (abstention, ou désaccord).
  Ils ne disparaissent pas : c'est la preuve de ce qui a été regardé.
- **Thèmes non gauche-droite** : ils n'apparaissent plus dans le bloc « Orientations », ni dans l'API
  publique, ni sur la page d'un groupe. Ils restent calculés, consultables dans le back-office, et
  redeviendront publiables après relecture humaine. Une phrase le dit en bas du bloc : « Trois thèmes
  sont calculés mais non publiés : notre mesure de contrôle ne sait pas les valider. »
- **[methodology.md](../methodology.md) § 6** gagne le paragraphe correspondant, dans le commit qui
  livre le filtre : le document de référence ne doit pas décrire une formule que le code ne calcule
  plus.

### F3 — streamer les votes

`compute_person_scores` passe de `fetch_all` à un flux (`.fetch(pool)` + `try_next()`), ce qui ne
demande aucune restructuration : la boucle consomme déjà les lignes triées par
`(person_id, theme_id)` et n'accumule qu'un groupe à la fois.

**Deux pièges à connaître avant d'écrire** :

1. le flux **retient une connexion du pool** pendant toute sa durée, et la boucle écrit par lots sur
   ce même pool. Avec un pool à une seule connexion, cela s'interbloque. Vérifier la taille configurée
   et, au moindre doute, acquérir explicitement une seconde connexion pour les écritures ;
2. `try_next()` vient de `futures_util::TryStreamExt`, qui n'est pas encore une dépendance du worker.
   L'ajouter si nécessaire — une dépendance pour supprimer un pic mémoire est un échange raisonnable,
   mais qu'il faut faire sciemment.

### F4 — le libellé de cohésion

Une phrase à compléter dans `kyc_api.labels.cohesion_label` : la borne basse. Ni le schéma ni le
calcul ne bougent.

### Tests à écrire avant le code

- **`melenchon_environnement_case`** (`worker/tests/scoring.rs` ou intégration) : trois votes contre
  sur des textes à `position_pour` négatif dont la mesure est positive → aucune orientation, pas une
  orientation inversée. C'est le cas qui a motivé cette phase, il porte son nom ;
- une contribution écartée est **écrite** avec `exclusion = 'desaccord_mesure'` et `poids = 0` ;
- le compteur `ecartes_desaccord` est exact ;
- un thème `axe_gauche_droite = false` n'est **pas** filtré par F1 (le calcul est inchangé) mais
  **n'apparaît pas** dans la lecture publique ;
- une mesure exactement nulle n'écarte rien ;
- l'idempotence tient toujours : deux exécutions, mêmes empreintes ;
- côté pages : le compteur d'écartés s'affiche, la section « Écartés du calcul » liste les scrutins
  avec leur raison, et l'API ne rend aucun thème non publié.

## Ordre des commits

1. **F3** — streamer les votes. Indépendant du reste, aucun effet sur les valeurs : à faire d'abord
   pour que les empreintes d'idempotence servent de garde-fou aux commits suivants. **Relever
   l'empreinte `md5` de `person_theme_score` et de `mandat_theme_score` avant et après** : elles
   doivent être identiques, c'est ce qui prouve que le passage au flux n'a rien changé.
2. **F4** — le libellé de cohésion.
3. **Migration `0010`** seule, avec ses tests de contraintes — dont celui qui vérifie qu'une
   abstention sans exclusion est bien refusée (le piège `IS NOT DISTINCT FROM`).
4. **F2** — `themes.toml` et `seed_themes` (champ obligatoire), puis le filtre d'affichage public et
   la phrase de bas de bloc.
5. **F1** — le filtre d'accord dans les quatre calculs, les tests écrits avant, le compteur
   d'écartés, la section « Écartés du calcul » sur la page d'explication, et la mise à jour de
   [methodology.md](../methodology.md) § 6 **dans ce commit**.
6. **Passe finale** — remesurer et consigner : combien d'orientations subsistent, combien de personnes
   en perdent une, le poids écarté par thème, et le nouveau tableau des candidat·es. Mettre à jour
   [phase-4-partis-scores.md](phase-4-partis-scores.md) — D4.9 gagne un voisin, et les mesures
   consignées de la phase 4 sont désormais périmées : le dire plutôt que les laisser.

## Vérifications avant de déclarer la phase 4.1 terminée

1. `make lint`, `make typecheck`, `make test` verts ; CI verte.
2. Deux exécutions consécutives de `recompute_scores` donnent des empreintes identiques — la propriété
   de la phase 4 ne doit pas être perdue en la corrigeant.
3. `/personne/jean-luc-melenchon` n'affiche plus d'orientation « environnement » inversée, et ce qui
   s'affiche à la place est explicable en une phrase.
4. Aucune orientation publique sur un thème dont l'axe n'est pas gauche-droite.
5. Le nombre de contributions écartées est visible à côté de chaque orientation concernée, et la page
   d'explication liste les scrutins écartés **avec leur raison**.
6. Les trois candidat·es ayant des scores sont relus à la main une seconde fois : les orientations
   restantes sont-elles défendables ? Consigner les réponses, désaccords compris. **C'est ici que
   l'option D se réexamine** : si une orientation reste indéfendable après le correctif, alors le
   filtre ne suffit pas, et la bonne réponse devient de ne rien publier avant relecture humaine. Ne
   pas trancher cette question à l'instinct — la poser explicitement avec les cas trouvés.
7. Les mesures sont consignées : poids écarté par thème, orientations perdues, personnes affectées.
