# Méthodologie

Ce document est le cœur du projet. Le code n'est qu'une manière d'appliquer ces règles ; si le code et ce
document divergent, c'est le code qui a tort.

## 1. Ce qu'on affirme, et ce qu'on n'affirme pas

**On affirme** : « le JJ/MM/AAAA, cette personne a voté *contre* le scrutin n° 4321 portant sur X. Ce
scrutin a été catégorisé *fiscalité / redistribution*, pôle *maîtrise de la fiscalité*, par [méthode]. »

**On n'affirme pas** : que cette personne « est » libérale, qu'elle votera de même demain, qu'elle pense
ce que son vote suggère, ni que ce vote est bon ou mauvais.

Trois règles en découlent, non négociables :

1. **Aucune conclusion sans trace.** Tout score affiché est cliquable et mène à la liste des scrutins qui
   l'ont produit, avec leur poids. Une conclusion non traçable est un bug bloquant.
2. **Le fait et l'interprétation sont séparés.** Le vote est une donnée ; sa catégorisation est une
   opinion méthodologique, datée, signée, révisable, historisée.
3. **Le vocabulaire reste descriptif.** Les pôles sont nommés de façon symétrique et non péjorative. Si un
   pôle ne peut pas être nommé sans jugement, l'axe est mal construit.

## 2. Corpus retenu

| Critère | Valeur v1 | Pourquoi c'est un paramètre et pas une constante |
| --- | --- | --- |
| Profondeur | législatures 15 à 17, soit du 04/07/2017 à aujourd'hui | On veut pouvoir remonter plus loin sans réécrire de code |
| Participation minimale | 50 % de l'effectif | Un scrutin à 16 votants sur 577 n'est pas représentatif ; le seuil pourra être abaissé pour enrichir le corpus, la donnée étant conservée |
| Type de scrutins | scrutins publics (ordinaires, solennels, motions de censure) | C'est ce que publie l'open data |

**Pourquoi 2017 et pas « 10 ans glissants ».** L'ambition initiale était de dix ans. Le spike de la
[phase 1](plans/phase-1-ingestion.md) a montré que la 14e législature (2012-2017) ne publie de vote
nominatif que pour 644 de ses 1 354 scrutins — les 710 autres ne donnent que la position du groupe et le
nom des dissidents — et que son archive s'arrête en novembre 2016. L'inclure reviendrait à afficher, pour
la même personne, des votes personnels sur une période et des positions de groupe sur une autre : deux
natures de fait sous une seule étiquette. On préfère un corpus **plus court et homogène** : neuf ans,
16 957 scrutins, 2 346 018 votes nominatifs. La 14e législature est au backlog v2.

**Dénominateur de la participation : l'effectif au jour du scrutin**, obtenu en sommant les
`nombreMembresGroupe` publiés dans le fichier du scrutin (574 à 577 en pratique). Ni l'effectif légal
(577, faux dès qu'un siège est vacant), ni une reconstruction à partir des mandats (fragile et inutile
puisque la source donne le chiffre). Participation = `nombreVotants / effectif`.

**Ce que le dénominateur ne dit pas** : les absents ne figurent pas dans les fichiers de l'Assemblée. Un
scrutin à 30 % de participation ne permet donc **pas** de dire qui était absent, seulement qui était
présent et comment il a voté. On n'affiche jamais de « taux de présence » par personne : la source ne le
porte pas.

### Ce que « appartenir à un parti » veut dire ici

Trois liens différents sont mesurés par trois sources différentes, et **ils ne se disent pas de la même
façon** :

| Lien | Source | Formulation imposée |
| --- | --- | --- |
| Siéger dans un groupe parlementaire | mandats `GP` de l'Assemblée | « a siégé au groupe X du JJ/MM/AAAA au JJ/MM/AAAA » |
| Être rattaché à un parti | organes `PARPOL` de l'Assemblée | « rattaché·e au parti X au titre du financement de la vie politique, déclaration du JJ/MM/AAAA » |
| Adhérer à un parti | Wikidata (`P102`) | « adhésion au parti X selon Wikidata, du … au … » |

Le rattachement au titre du financement est un **acte administratif annuel**, pas une adhésion : l'écrire
« membre de X » serait une inférence, exactement le genre que ce document interdit. Le lien à afficher en
priorité est le groupe parlementaire, parce que c'est lui qui a un rapport avec les votes.

Cas à ne jamais afficher comme une appartenance : les **députés non-inscrits**. La source les modélise
comme un groupe (un pseudo-groupe « NI » par législature), mais n'appartenir à aucun groupe n'est pas
appartenir au groupe des sans-groupe. On affiche « non-inscrit·e », et aucun alignement de groupe n'est
calculé sur cette période.

## 3. Comment on lit une position de vote

| Position | Traitement |
| --- | --- |
| Pour | contribue au score |
| Contre | contribue au score, en sens inverse |
| Abstention | conservée, **signalée**, pondération faible ou nulle — jamais convertie en « contre » |
| Non-votant | **exclu de tout calcul**, affiché avec sa cause |

L'abstention en France recouvre le refus de choisir comme la nuance politique ; elle mérite d'être
affichée, pas interprétée.

**Le non-votant, lui, est documenté par la source.** L'Assemblée publie la cause de chaque non-vote, et il
n'en existe que trois : président de séance (`PSE`), président de l'Assemblée (`PAN`), membre du
Gouvernement (`MG`). Toutes institutionnelles. On affiche donc la cause en clair — « ne prend pas part au
vote : préside la séance » — plutôt qu'un « non-votant » qui inviterait à imaginer un désengagement. Les
absents, eux, ne figurent pas dans les fichiers : leur absence n'est ni affichée ni comptée.

**Le vote par délégation est un vote, et il est dit.** 12,6 % des votes du corpus sont émis par délégation
(15,1 % sur la seule 17e législature). Il compte comme le vote de la personne au nom de qui il est émis —
c'est son sens juridique — et l'UI le signale systématiquement. Le masquer serait malhonnête ; l'écarter
amputerait le corpus d'un vote sur huit.

**Une mise au point n'écrase pas le vote.** Quand un député déclare après coup s'être trompé, l'Assemblée
publie la correction sans modifier le résultat du scrutin. Nous faisons pareil : le vote enregistré reste
la donnée, la mise au point est **stockée à côté, affichée, et n'entre pas dans le calcul**. Le cas est
rare (0,2 % des votes), et il dit quelque chose — c'est pourquoi on le garde.

## 4. Les axes thématiques

Chaque scrutin peut porter zéro, un ou plusieurs thèmes. Chaque thème a **un axe à deux pôles nommés
symétriquement** :

| Thème | Pôle A | Pôle B |
| --- | --- | --- |
| Social / fiscalité | redistribution et protection sociale étendue | maîtrise de la dépense et de la fiscalité |
| Environnement | contrainte réglementaire et transition rapide | priorité à l'activité économique et à la transition graduée |
| Santé | financement collectif et service public renforcé | maîtrise des dépenses et place accrue du privé |
| Éducation | moyens publics et unification du système | autonomie des établissements, sélection, place du privé |
| Sécurité | extension des pouvoirs de police et fermeté pénale | libertés publiques, prévention, garanties procédurales |
| Immigration | droits des étrangers et facilitation du séjour | restriction de l'entrée et du séjour |

Ces axes sont une proposition de départ, à discuter et à faire évoluer — ils sont stockés en base et
versionnés, pas codés en dur. On ajoutera probablement au moins : institutions/démocratie, Europe,
travail/entreprise, agriculture.

L'exemple donné au départ du projet — voter la taxe Zucman relève du pôle *redistribution*, voter contre du
pôle *maîtrise de la fiscalité* — est exactement le format attendu : un scrutin, un thème, un pôle, et une
phrase qui explique le rattachement.

## 5. Les trois méthodes de catégorisation

Chaque catégorisation porte sa `method`, et l'UI l'affiche. Une catégorisation automatique n'a pas le même
statut qu'une catégorisation relue.

### a. `heuristic` — première passe automatique

Objectif : dégrossir, pas conclure. Deux approches possibles, à trancher en
[phase 3](plans/phase-3-categorisation.md) :

- **Alignement sur des groupes de référence.** On compare le vote d'une personne à la position majoritaire
  de chaque groupe parlementaire, et on situe la personne à partir d'un ordonnancement gauche-droite des
  groupes. Cet ordonnancement est **dérivé de la grille des nuances politiques du ministère de
  l'Intérieur**, seule source officielle publiée qui positionne les formations sur cet axe. Il vit dans un
  fichier versionné où chaque ligne cite sa nuance, la version de la grille et sa date.

  On le cite sans s'y soumettre : cette grille est établie par l'exécutif, elle classe donc aussi ses
  opposants, elle change d'une élection à l'autre, et elle nuance des candidat·es plutôt que des groupes
  parlementaires. Elle reste donc une **entrée documentée et contestable par PR**, pas une vérité.
- **Axe dérivé des votes eux-mêmes.** On construit la matrice votants × scrutins et on en extrait l'axe
  principal (approche classique en science politique). Avantage : aucune position politique n'est postulée
  au départ. Inconvénient : l'axe obtenu est un artefact statistique qu'un humain doit nommer, et il ne
  dit rien des thèmes.

Dans les deux cas, le résultat est étiqueté « automatique, non relu » et le restera tant qu'un humain ne
l'aura pas validé.

### b. `manual` — saisie ou correction par un admin

Via le back-office : thème, pôle, confiance, et **commentaire justifiant le rattachement**. Le commentaire
n'est pas optionnel : c'est lui qui sera affiché en explication.

### c. `import` — export → travail hors ligne → réimport

Cycle prévu explicitement pour permettre un travail de masse, y compris avec l'aide d'un LLM : export CSV
ou JSON des scrutins non catégorisés, traitement à l'extérieur, réimport avec validation stricte du
format, prévisualisation des changements avant application, et conservation de l'historique.

Une catégorisation produite par un LLM est signalée comme telle dans l'UI. Le projet ne prétend pas
qu'elle vaut une relecture humaine ; il prétend qu'elle est traçable et corrigeable.

## 6. Du vote au score

Chaque scrutin catégorisé produit une contribution `(personne, thème, pôle, poids)`. Le score par thème
est la moyenne pondérée de ces contributions, projetée sur l'axe.

- La pondération dépend du type de scrutin, de la position (abstention affaiblie) et de la confiance de la
  catégorisation.
- **En dessous d'un nombre minimal de contributions, aucun score n'est affiché.** On affiche
  « données insuffisantes ». Trois votes ne font pas une orientation.
- Le score d'un parti sur une période se calcule sur les votes de ses membres pendant cette période.
- Le score affiché sur la fiche d'une personne pour un parti est celui du parti **restreint à la période
  d'appartenance** de cette personne — c'est tout l'intérêt des `daterange`.
- **Le score du parti ne se mélange jamais au score personnel.** Ils sont affichés côte à côte, jamais
  fusionnés en un chiffre unique.

Quand un groupe s'est divisé sur un scrutin, l'information est conservée et affichée : c'est souvent plus
intéressant qu'un score.

## 7. Limites assumées, à afficher dans l'application

Elles ne sont pas un avertissement de bas de page : elles font partie de l'honnêteté du produit.

1. **Les scrutins publics ne sont qu'une partie des votes.** Beaucoup de textes sont adoptés à main levée
   et n'apparaissent nulle part. Le corpus est représentatif, pas exhaustif.
2. **Un vote n'est pas une conviction.** Discipline de groupe, stratégie parlementaire, opposition à un
   détail d'un texte par ailleurs approuvé : les raisons de voter contre sont nombreuses.
3. **Un texte est rarement mono-thématique.** Un projet de loi de finances touche à tout. D'où la
   possibilité de rattacher plusieurs thèmes, et la nécessité d'un commentaire explicatif.
4. **Beaucoup de candidat·es n'ont jamais siégé à l'Assemblée.** Élu·es locaux, ministres non
   parlementaires, eurodéputé·es, candidat·es sans mandat : leur fiche sera vide de votes personnels. On
   l'affiche clairement plutôt que de combler le vide par des inférences.
5. **La catégorisation est subjective.** On la rend transparente, discutable et corrigeable — on ne
   prétend pas la rendre neutre.
6. **Les partis changent de nom, fusionnent, se scindent.** Les continuités que nous établissons sont des
   choix documentés.

## 8. Corrections

Toute personne peut contester une catégorisation ou une donnée via une issue (voir
[CONTRIBUTING.md](../CONTRIBUTING.md)). Les corrections sont historisées et l'historique est public : on
doit pouvoir savoir qui a changé quoi, quand et pourquoi. Une donnée factuellement fausse est corrigée
sans débat ; une catégorisation contestée se discute avec ce document en référence.
