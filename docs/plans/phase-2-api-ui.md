# Phase 2 — Pages publiques

**Statut : 📝 à relire** · Dépend de : phases 0 et 1 · Bloque : rien (mais éclaire les phases 3 et 4)

## Objectif

Rendre les données visibles. À la fin de cette phase, le site est déjà utile sans aucun score : on peut
parcourir les candidat·es, ouvrir une fiche, et lire l'historique réel de ses votes et de ses
appartenances politiques.

Cette phase est placée avant la catégorisation exprès : regarder les vraies données à l'écran révèle des
problèmes de modèle qu'aucune relecture de plan ne détecte.

## Périmètre

**Dedans** : accueil avec la liste des candidat·es et leur photo, fiche candidat, page d'un scrutin,
recherche, design de base, accessibilité, mentions de sources.

**Dehors** : scores par thème et explications (phase 4), back-office (phase 3), authentification.

## Livrables

| Route | Contenu |
| --- | --- |
| `/` | Grille des candidat·es : photo, nom, parti actuel, nombre de votes connus. Filtre et recherche en HTMX |
| `/candidat/{slug}` | En-tête d'identité, frise des appartenances politiques, votes récents, zone « orientations » avec un état vide explicite en attendant la phase 4 |
| `/candidat/{slug}/votes` | Liste paginée et filtrable (thème quand il existera, période, position) |
| `/scrutin/{id}` | Détail d'un scrutin : titre, date, résultat, répartition par groupe, lien vers la source AN |
| `/methodologie` | Rendu de [methodology.md](../methodology.md) — accessible depuis chaque page |
| `/api/…` | Équivalents JSON des pages, pour la réutilisation des données |

## ⚠️ À concevoir ici, pas ailleurs : l'architecture front

La phase 0 ne pose qu'un `style.css` volontairement pauvre et deux conventions (fragments sur routes
explicites, gabarits `_` n'étendant pas `base.html.jinja`). **Tout le reste du front se décide dans cette
phase, avec le mainteneur, avant d'écrire les gabarits** — sans quoi il s'installera par accident :

- **structure de la feuille de style** : variables CSS, `@layer`, découpage en plusieurs fichiers
  concaténés ou fichier unique, convention de nommage des classes ;
- **échelles** typographique et d'espacement, palette, contrastes — sachant la contrainte de neutralité :
  aucune couleur ne doit suggérer un jugement, et les couleurs de parti viennent d'une table documentée ;
- **stratégie responsive** : la grille de candidat·es et surtout **la frise des appartenances** sont les
  deux composants difficiles sur petit écran. La frise est l'élément signature de l'UI, son implémentation
  (grille CSS, SVG généré côté serveur ?) est une vraie question de conception ;
- **organisation des gabarits** : blocs, macros, nommage, découpage — et à quel moment on extrait un
  fragment ;
- **stratégie de test des pages** : que vérifie-t-on d'un rendu Jinja/HTMX, et à quel niveau ? La phase 0
  s'arrête volontairement au test de fumée.

## Points de conception

- **La frise des appartenances est un élément central**, pas un ornement : c'est la représentation
  directe des `daterange` et c'est ce qui rendra lisibles les positions de parti par période en phase 4.
- **Les états vides sont du contenu.** « Cette personne n'a jamais siégé à l'Assemblée nationale, ses
  votes personnels ne sont donc pas disponibles » est une information utile, pas une page cassée.
- **Chaque bloc affiche sa source** et sa date de récupération.
- **HTMX pour la recherche, les filtres et la pagination.** Si une interaction semble exiger du JS,
  chercher d'abord la solution HTMX ; le JS ponctuel reste possible mais doit être justifié.
- **Accessibilité et sobriété** : contrastes suffisants, navigation clavier, pages qui fonctionnent sans
  JS pour l'essentiel, pas de police ni de script distant. Un site de transparence ne pose pas de
  mouchards.
- **Pas de couleurs politiques implicites.** Les couleurs de parti sont acceptables si elles viennent
  d'une table documentée ; aucune palette « bon / mauvais ».

## Étapes

1. Définir la liste des candidat·es v1 (voir décisions) et la table `candidate` associée.
2. Gabarits Jinja : layout, en-tête, pied de page avec licences et sources.
3. Accueil + recherche HTMX.
4. Fiche candidat : identité, frise, votes récents.
5. Page scrutin et page votes filtrée.
6. API JSON en miroir des pages.
7. Passe accessibilité et performance (mesurer le temps de rendu des pages, viser une page candidat en
   moins de 100 ms côté serveur).

## Décisions à trancher

| # | Question | Proposition |
| --- | --- | --- |
| D2.1 | Qui apparaît sur l'accueil en 2026 ? | Une table `candidate` alimentée **manuellement** avec un statut (`déclaré`, `pressenti`, `retiré`) et une source pour chaque entrée. Aucune liste automatique : la sélection des candidat·es est un choix éditorial, il doit être explicite et sourcé |
| D2.2 | Les personnes non candidates sont-elles consultables ? | Oui par URL directe (utile pour comparer), mais non listées sur l'accueil |
| D2.3 | Slug d'URL | `prenom-nom` avec suffixe en cas d'homonymie, table de redirection pour la stabilité des liens |
| D2.4 | CSS | Feuille écrite à la main, sans framework, sans build — cohérent avec « pas de bundler » |
| D2.5 | Mise en cache HTTP | `Cache-Control` court + `ETag` sur les pages publiques ; à réévaluer en phase 5 |
| D2.6 | Architecture CSS | À trancher au début de la phase (voir l'avertissement ci-dessus) : variables, `@layer`, nommage, découpage |
| D2.7 | Implémentation de la frise | Grille CSS pure, ou SVG rendu côté serveur ? Conditionne le comportement mobile et l'accessibilité |
| D2.8 | Niveau de test des pages | Test de fumée par route, assertions sur le HTML, ou vérification d'accessibilité automatisée ? |

## Fini quand

- On peut ouvrir la fiche d'une personne et retrouver un vote précis, correct, avec le lien vers la source
  officielle.
- La frise affiche correctement un cas à plusieurs partis successifs (le cas PS → LFI du cahier des
  charges sert de test de recette).
- Les pages passent sans JS pour la lecture, et sans erreur d'accessibilité bloquante.
- Aucune requête vers un domaine tiers au chargement d'une page.
- Une page candidat se rend en moins de 100 ms côté serveur sur des données réelles.

## Risques

- **Photos** : couverture partielle et licences variables. Prévoir un placeholder digne et l'affichage de
  l'auteur.
- **Volume de votes par personne** : plusieurs centaines de lignes. La pagination et les index doivent
  être pensés dès le départ, pas ajoutés après.
- **Tentation d'afficher un score trop tôt**, avant que la catégorisation soit sérieuse. Ne pas céder :
  un score faux vu une fois décrédibilise durablement le projet.
