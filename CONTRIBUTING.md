# Contribuer

Merci de l'intérêt porté au projet. Deux formes de contribution, la première étant la plus précieuse.

## 1. Contribuer aux données (aucune compétence technique requise)

La partie la plus difficile du projet n'est pas le code, c'est de **rattacher chaque scrutin à un thème et
à une orientation** de façon honnête. Un scrutin mal catégorisé fausse toutes les fiches qui en dépendent.

Vous pouvez donc :

- **Signaler une catégorisation erronée** : ouvrez une issue avec le numéro du scrutin, la catégorisation
  actuelle, celle que vous proposez, et surtout **pourquoi** (contenu réel du texte voté, pas votre
  ressenti sur le camp politique concerné).
- **Signaler une donnée fausse** sur une fiche : appartenance à un parti sur une mauvaise période, photo
  erronée, vote mal attribué. Joignez la source.
- **Proposer une source open data** que nous n'exploitons pas encore.

Les règles que suit le projet pour catégoriser sont écrites dans [docs/methodology.md](docs/methodology.md).
Une contribution qui contredit ces règles sera refusée même si elle est factuellement défendable — dans ce
cas, proposez plutôt une modification des règles.

## 2. Contribuer au code

1. Lisez [CLAUDE.md](CLAUDE.md) : il contient les conventions, y compris pour les contributions humaines.
2. Regardez [docs/plans/](docs/plans/) : le travail est organisé par phases. Une contribution qui
   n'appartient à aucune phase se discute d'abord dans une issue.
3. Une PR = une phase ou une tâche identifiée dans un plan. Les PR fourre-tout sont refusées. (Les
   mainteneurs, eux, travaillent en développement sur le tronc et commitent directement sur `main` ; la
   PR reste la porte d'entrée pour tout le monde d'autre.)
4. Lint, typage et tests doivent passer (`ruff`, `ty`, `pytest`, `cargo clippy`, `cargo test`).
5. Si votre changement modifie une décision documentée, mettez à jour la doc dans la même PR.

## Ce que le projet refuse

Pour que l'outil garde une valeur, certaines contributions ne seront pas acceptées, quelle que soit leur
qualité technique :

- toute fonctionnalité qui note, classe ou recommande un·e candidat·e ;
- tout affichage d'une conclusion non traçable jusqu'à un scrutin ou une source ;
- tout contenu éditorial, satirique ou militant, dans le code comme dans l'UI ;
- toute donnée personnelle hors mandat public (vie privée, santé, patrimoine hors déclarations légales
  publiques, origine, religion).

## Signalement

Un problème de neutralité ou une donnée qui vous semble diffamatoire : ouvrez une issue avec le label
`neutralite`. Ces issues sont traitées en priorité, avant les bugs et les fonctionnalités.
