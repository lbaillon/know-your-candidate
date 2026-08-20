# Phase 6 — Backlog v2

**Statut : 💭 idées** — rien n'est engagé ici. Ce fichier existe pour éviter que ces sujets ne
contaminent les phases 1 à 5.

## Élargir les sources de votes

**Sénat et Parlement européen.** C'est la réponse la plus solide au trou du modèle : beaucoup de
candidat·es n'ont jamais siégé à l'Assemblée mais ont voté ailleurs. Coût réel : un autre modèle de
données, d'autres axes politiques (un vote au Parlement européen ne se lit pas sur la même échelle), une
autre ingestion. À évaluer une fois la liste des candidat·es plus stable — c'est peut-être le sujet le
plus rentable de la v2.

**14e législature (2012-2017).** Écartée du corpus v1 sur mesure, pas par principe : 710 de ses 1 354
scrutins sont publiés sans détail nominatif — on connaît la position du groupe et le nom des dissidents,
pas le vote de chacun — et l'archive s'arrête en novembre 2016 (voir
[phase-1-ingestion.md](phase-1-ingestion.md), D1.7). L'ajouter suppose donc d'abord de répondre à une
question d'affichage, pas d'ingestion : comment montrer, sur une même fiche, des votes personnels sur une
période et des positions de groupe sur une autre, sans que le lecteur confonde les deux. Sans réponse à
cette question, l'ajout dégrade le produit au lieu de l'enrichir.

**Textes des dossiers législatifs.** Récupérer le contenu réel des textes permettrait une catégorisation
nettement mieux informée, notamment pour le cycle export/LLM. Ingestion supplémentaire, volume important.

**Amendements et interventions.** Plus fin que les scrutins, beaucoup plus volumineux, et le rapport
signal/bruit est incertain.

## Enquêtes et condamnations

Demandé dès le départ, mais explicitement reporté — et c'est le sujet le plus risqué du projet.

Garde-fous à définir **avant** toute ligne de code :

- **présomption d'innocence** : distinguer sans ambiguïté enquête, mise en examen, condamnation en
  première instance, condamnation définitive ;
- source judiciaire ou de presse identifiée, datée, avec le statut de la procédure au jour de l'affichage ;
- **mise à jour obligatoire en cas de relaxe ou d'appel**, sinon la donnée devient fausse et
  potentiellement diffamatoire ;
- droit de réponse et procédure de correction rapide ;
- décision assumée sur la prescription et l'ancienneté des faits.

Ce n'est pas un sujet technique : sans réponse claire à ces questions, la fonctionnalité ne se fait pas.

## Continuité entre partis, et rattachement des personnes hors Assemblée

La [phase 4](phase-4-partis-scores.md) score les **groupes parlementaires** tels que l'Assemblée les
publie, sans jamais affirmer qu'un groupe succède à un autre (D4.8). C'est le choix prudent : dire que
LaREM, Renaissance et Ensemble pour la République « sont le même parti » est un arbitrage éditorial
contestable, et rien n'obligeait à le prendre pour livrer des scores.

Ce que ce choix laisse ouvert est mesuré, pas supposé : **deux candidat·es sur cinq n'ont aucun vote
exploitable** (Bruno Retailleau, sénateur, dont l'unique vote en base est celui du Congrès sur l'IVG ;
Nathalie Arthaud, jamais élue). Le repli prévu à l'origine — afficher les positions du parti auquel la
personne appartient — supposerait deux choses que nous n'avons pas : une table de partis reliant les
groupes successifs, et l'appartenance partisane des personnes hors Assemblée (Wikidata `P102`, non
ingéré).

**Condition de déclenchement** : ce sujet ne devient rentable que le jour où une source de votes hors
Assemblée est ingérée. Tant que Retailleau n'a pas de votes au Sénat dans la base, lui attribuer les
positions du groupe LR ne comble pas le trou — cela le déguise. Les deux sujets se traitent donc
ensemble, ou pas du tout.

## Ancrage gauche-droite dynamique

En phase 3, l'ordonnancement des groupes vit dans un fichier seed dérivé de la grille des nuances
politiques du ministère de l'Intérieur. Le rendre dynamique suppose de résoudre trois choses, aucune
triviale :

- **ingérer la grille et le Répertoire national des élus** (formats et emplacements variables d'une
  élection à l'autre) ;
- **gérer les versions successives** : une nuance change entre deux élections, un score calculé hier ne
  doit pas se mettre à bouger en silence. Il faut donc dater l'ancrage et rejouer explicitement ;
- **conserver la possibilité de diverger** de la grille officielle, avec justification affichée : le seed
  restera la couche d'arbitrage, l'ingestion ne fera que l'alimenter.

Utile aussi pour les nuances des candidat·es aux élections locales, qui couvriraient des personnalités
sans mandat parlementaire.

## Autres idées

| Idée | Intérêt | Réserve |
| --- | --- | --- |
| Comparateur de deux candidat·es | Très demandé, très lisible | Invite à la lecture en « match », à traiter avec soin |
| Évolution d'une orientation dans le temps | Montre les inflexions réelles | Nécessite d'historiser les scores par période |
| Programme déclaré vs votes passés | C'est l'intuition fondatrice du projet | Demande de structurer les programmes : gros travail éditorial |
| Assiduité aux scrutins | Facile à calculer | Facile à mal interpréter — un député en mission n'est pas absent |
| Financement des campagnes (CNCCFP) | Complète le tableau | Données peu structurées |
| Déclarations d'intérêts (HATVP) | Conflits d'intérêts | Interprétation délicate, mêmes garde-fous que les condamnations |
| Export ouvert de toute la base | Cohérent avec la démarche | À faire proprement (licence, format, fréquence) |
| Contributions publiques aux catégorisations | Passe à l'échelle | Demande une modération : un projet en soi |
| Version anglaise de la documentation | Ouvre les contributions | Le domaine reste français |

## Règle de tri

Un sujet ne sort de ce fichier que s'il a son propre plan, avec un périmètre, des décisions tranchées et
des critères de fin. Tant qu'il est ici, il n'est pas engagé — et personne ne devrait écrire de code pour
lui.
