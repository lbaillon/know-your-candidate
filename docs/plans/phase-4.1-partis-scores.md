# Phase 4.1 — Corrections des scores

**Statut : 📝 à relire** · Dépend de : phase 4 · Bloque : phase 5

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

## Décision à trancher avant d'implémenter

F1 admet plusieurs réponses, et le choix n'est pas technique.

| Option | Ce qu'elle fait | Ce qu'elle coûte |
| --- | --- | --- |
| **A — écarter les contributions contredites** (proposée) | On ne score que là où les deux lectures concordent ; les écartées restent visibles dans l'explication avec leur raison | 38 % du poids disparaît ; des orientations tombent sous le seuil |
| **B — les pondérer sans les écarter** | Un facteur (0,25 par exemple) au lieu de zéro | Atténue le signe faux sans le corriger : Mélenchon resterait du mauvais côté, en plus pâle |
| **C — ne rien changer au calcul, afficher le désaccord** | « Sur 4 de ces 5 scrutins, notre mesure automatique contredit la catégorisation » | Transparent, mais le projet continue d'affirmer publiquement quelque chose qu'il sait douteux |
| **D — ne pas publier de score par personne** avant relecture humaine (retour sur D4.1) | Coupe le problème à la racine | L'application perd sa promesse principale pour une durée indéterminée |

Recommandation : **A**, plus F2. C'est la seule option qui cesse d'affirmer ce que nos données
contredisent, sans renoncer à publier. Elle est aussi la plus facile à défendre en une phrase, ce qui
est le vrai test sur ce projet.

Ce que ni A ni aucune autre option ne règle : un vote contre pour insuffisance reste indétectable
quand nos deux lectures **concordent** à tort. C'est une limite structurelle du corpus binaire, elle
appartient à [methodology.md § 7](../methodology.md) § 7.2, et la seule vraie réponse est la relecture
humaine — puis, plus tard, l'ingestion du contenu des textes ([phase 6](phase-6-backlog-v2.md)).

## Ordre des commits

1. **F3** — streamer les votes. Indépendant du reste, aucun effet sur les valeurs : à faire d'abord
   pour que les empreintes d'idempotence servent de garde-fou aux commits suivants.
2. **F4** — le libellé de cohésion.
3. **F2** — `theme.axe_gauche_droite` (migration + seed), et le filtre d'affichage public.
4. **F1** — le filtre d'accord dans `recompute_scores`, les tests écrits avant, le compteur
   d'écartés dans l'explication et à côté de l'orientation.
5. **Passe finale** — remesurer et consigner : combien d'orientations subsistent, combien de personnes
   en perdent une, et le nouveau tableau des candidat·es. Mettre à jour
   [phase-4-partis-scores.md](phase-4-partis-scores.md) (D4.9 gagne un voisin) et
   [methodology.md](../methodology.md) § 6.

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
   restantes sont-elles défendables ? Consigner les réponses, désaccords compris.
7. Les mesures sont consignées : poids écarté par thème, orientations perdues, personnes affectées.
