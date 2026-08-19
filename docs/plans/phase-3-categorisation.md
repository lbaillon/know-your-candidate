# Phase 3 — Catégorisation des scrutins

**Statut : ✅ validé** · Décisions arbitrées le 18/08/2026 · Dépend de : phases 1 et 2 · Bloque : phase 4

## Objectif

Pouvoir rattacher chaque scrutin à un ou plusieurs thèmes et à un pôle, par trois voies complémentaires :
une première passe automatique, une saisie/correction humaine, et un cycle export → travail hors ligne →
import. Le tout historisé et traçable.

C'est la phase la plus délicate du projet : c'est là que se joue sa crédibilité.

## Périmètre

**Dedans** : modèle des thèmes et des catégorisations, mesure automatique d'ancrage gauche-droite,
back-office admin, export/import, historique des révisions, consultation publique des catégorisations.

**Dehors** : le calcul des scores et leur affichage sur la fiche (phase 4), l'appel à un LLM depuis
l'application (volontairement : le travail LLM se fait **hors ligne**, sur un export).

## Livrables

1. Migrations : `theme` (l'axe et ses deux pôles y sont portés, voir D3.12), `group_axis` +
   `group_axis_entry` (l'ancrage gauche-droite, versionné), `scrutin_axis_estimate` (la mesure),
   `scrutin_label`, `label_revision`, `label_import`, `admin_user`, `admin_action`.
2. Job `label_scrutins_heuristic { strategy, scope }` : calcule, pour chaque scrutin, **où se situent
   l'un par rapport à l'autre les camps du « pour » et du « contre »** sur un ancrage gauche-droite
   documenté. Une mesure, pas une catégorisation (D3.7).
3. Back-office `/admin` : file des scrutins non catégorisés, formulaire de catégorisation, correction,
   historique.
4. Export : `GET /admin/export?statut=non_categorises&format=csv|json` avec un schéma stable et
   documenté.
5. Import : dépôt de fichier, **validation stricte**, prévisualisation des changements (créations,
   modifications, conflits), application transactionnelle, rapport.
6. Pages publiques : liste des scrutins catégorisés, détail d'une catégorisation avec sa méthode, son
   auteur et son historique.
7. Authentification admin (voir décisions).
8. **Suivi des jobs dans le back-office** : reprise du fragment de progression HTMX écrit en phase 0 et
   retiré des routes publiques en phase 2. C'est ici qu'il trouve son vrai usage — suivre la passe
   heuristique ou une ingestion, derrière l'authentification.
9. **Déclenchement du job `noop` réservé aux admins** : il est conservé comme test de fumée en production
   (« le worker traite-t-il réellement quelque chose ? »), mais n'est déclenchable que depuis le
   back-office authentifié. Jamais de route publique de création de job, à aucun moment.

## Ce que les données disent, et qui commande cette phase

Mesuré le 18/08/2026 sur la base peuplée par les phases 1 et 2, avant d'arbitrer quoi que ce soit. Ces
chiffres ne sont pas décoratifs : trois d'entre eux ont changé la conception.

| Fait | Conséquence |
| --- | --- |
| 16 957 scrutins ingérés, mais **988** atteignent la participation de 50 % retenue par [methodology.md](../methodology.md#2-corpus-retenu) — dont 879 où la minorité pèse au moins 10 % des suffrages exprimés | Le travail humain porte sur **~900 scrutins**, pas 17 000. À 30 s pièce, c'est une grosse journée, pas un projet. Abaisser le seuil à 30 % le porterait à 4 318 : le seuil est donc un **paramètre en base**, et son déplacement est une décision de charge de travail |
| Dans ce corpus : 216 votes sur l'ensemble d'un texte, 542 amendements, 77 articles, 51 motions | La file de travail doit prioriser explicitement. Un vote sur l'ensemble d'un texte est catégorisable ; un amendement souvent non |
| Titres de 170 caractères en moyenne, qui **nomment le texte** (« l'ensemble du projet de loi relatif à la bioéthique ») mais jamais le contenu d'un amendement (« l'amendement n° 1193 de M. Door à l'article 16 du PLFSS 2020 ») | Réponse à D3.6 : le titre suffit à deviner le **thème**, jamais le **sens**. Le contexte le plus informatif disponible sans ingérer les textes est la **ventilation par groupe**, qui entre donc dans l'export |
| 42 organes `GP` sur les législatures 15-17, dont 3 pseudo-groupes « non inscrit » (exclus par la méthodologie) | Le seed d'ancrage fait **39 lignes**. C'est relisible ligne à ligne, donc contestable ligne à ligne |
| `scrutin_groupe` porte 186 798 lignes avec le détail pour/contre/abstentions par groupe | L'heuristique se calcule **sans jamais toucher aux 2,3 M de votes nominatifs** |

Et le cas qui justifie à lui seul de mesurer la qualité de la mesure — scrutin 17/2653, proposition de loi
de programmation énergie-climat : RN (122) et UDR (16) pour, **tout le reste contre**, de LFI à EPR en
passant par LR. La moyenne d'axe du camp « contre » est centriste par simple annulation des extrêmes. La
direction sortie est juste, la séparation est mauvaise, et un chiffre qui ne dirait pas laquelle des deux
il est serait un piège. D'où la colonne `separation` (D3.9).

> **Corrigé à l'implémentation** (lot A, F1 — [phase-3.0-feedback.md](phase-3.0-feedback.md)) : calculé
> tel qu'arbitré, ce scrutin a en réalité une séparation **élevée**, pas mauvaise — un seuil isole
> exactement RN+UDR et classe la quasi-totalité des votants. Le récit ci-dessus reste comme trace de
> l'intuition de départ ; voir le lien pour ce que la mesure dit réellement et pourquoi c'est correct.

## L'heuristique automatique

Objectif : dégrossir. **Elle mesure une position, elle ne décide pas d'un thème** (D3.7) — parce qu'elle
n'en est pas capable : un ordonnancement gauche-droite des groupes ne dit rien du fait qu'un texte parle
de santé ou d'immigration. Lui faire produire un thème l'obligerait à en inventer un.

Stratégie retenue en phase 3, `group_alignment` : position par rapport à un ordonnancement gauche-droite
des groupes parlementaires, lu depuis un **fichier de configuration versionné**
(`db/seeds/group_axis.toml`).

Cet ordonnancement n'est pas inventé : il est **dérivé de la grille des nuances politiques du ministère
de l'Intérieur** — voir
[data-sources.md](../data-sources.md#4-ministère-de-lintérieur--grille-des-nuances-politiques).
Chaque ligne du seed porte donc la nuance officielle, la version de la grille, sa date et l'URL de la
source, plus le rattachement nuance → parti → groupe parlementaire que nous établissons.

> **Corrigé à l'implémentation** (lot A, F2, résolu au lot C — [phase-3.0-feedback.md](phase-3.0-feedback.md)) : la
> grille effectivement utilisée est celle des élections **législatives de 2024** (24 nuances), pas la
> « 2026, 26 nuances » supposée ici avant vérification. Elle ne définit d'ailleurs pas de blocs : c'est
> une liste plate de 24 nuances, ordonnée implicitement de l'extrême gauche à l'extrême droite. Le
> regroupement en six blocs est notre propre lecture de cet ordre, pas une donnée publiée telle quelle
> — voir le lien pour le détail et la source primaire.

On garde un fichier plutôt qu'une ingestion directe pour trois raisons : la grille change à chaque
élection et n'est pas versionnée de façon exploitable ; elle nuance des candidat·es, pas des groupes,
la jointure est donc de toute façon notre travail ; et c'est une **décision de l'exécutif classant ses
propres opposants**, qu'on cite en la datant mais à laquelle on ne délègue pas nos conclusions. Un
désaccord argumenté se règle par une PR sur ce fichier, visible de tous.

Le passage à un ancrage dynamique (ingestion de la grille et du RNE, gestion des versions successives)
est prévu en [phase 6](phase-6-backlog-v2.md).

La seconde stratégie envisagée — `principal_axis`, axe extrait de la matrice votants × scrutins, sans
postulat de départ — **n'est pas dans cette phase** (D3.10). Elle n'a d'intérêt qu'en validation croisée,
et il n'y a rien à croiser tant qu'aucune catégorisation humaine n'existe. Elle est inscrite en
[phase 4](phase-4-partis-scores.md), étape de recette, sous condition d'un volume minimal de scrutins
relus ; la colonne `strategy` de `scrutin_axis_estimate` l'attend sans migration.

Le résultat de la mesure est affiché **dans le back-office seulement** (D3.8) : il pré-remplit le
formulaire, ordonne la file de travail et sert de contrôle croisé. Publier 17 000 positions automatiques
sur des textes parlementaires produirait exactement ce que la section Risques annonce — du plausible et
du faux, à grande échelle, sur un sujet sensible.

## Le cycle export / import

C'est le mécanisme prévu pour catégoriser en masse avec l'aide d'un LLM, sans que l'application ne
dépende d'un LLM :

```
export CSV/JSON  ──►  travail hors ligne (LLM, tableur, plusieurs relecteurs)  ──►  import validé
```

Exigences :

- schéma d'export **versionné** (colonne `schema_version`), avec l'identifiant du scrutin, son titre, sa
  date, son résultat et un extrait de contexte suffisant pour décider ;
- import qui refuse un fichier ambigu plutôt que de deviner ;
- **prévisualisation obligatoire** avant application : combien de créations, de modifications, de
  conflits avec des saisies humaines plus récentes ;
- règle de conflit explicite : une catégorisation `manual` relue n'est jamais écrasée par un import sans
  confirmation ;
- chaque ligne importée conserve sa provenance (`import`, nom du fichier, date, auteur) et, si connu, le
  modèle utilisé.

## ⚠️ À concevoir ici, pas ailleurs : la stratégie de test de l'import

C'est **la cible de test la plus rentable du projet**, et elle doit être définie avant d'écrire l'import.
Un bug dans la validation ou dans la règle de conflit ne provoque pas d'erreur visible : il détruit
silencieusement des heures de relecture humaine, qui ne sont pas régénérables comme le sont les données
ingérées.

À couvrir explicitement, avec des cas écrits avant le code :

- fichier malformé, colonnes manquantes, `schema_version` inconnue, identifiants de scrutin inexistants,
  thèmes inconnus, valeurs hors bornes → **refus propre**, jamais d'interprétation approximative ;
- doublons dans le fichier importé, et lignes contradictoires entre elles ;
- **conflit avec une catégorisation `manual` plus récente** → non écrasée sans confirmation. C'est la
  règle dont la violation coûterait le plus cher ;
- import interrompu en cours → transaction annulée entièrement, aucun état intermédiaire en base ;
- aller-retour complet : export → réimport à l'identique → **aucune modification en base**, et aucune
  ligne d'historique créée ;
- l'historique reflète exactement ce qui a changé, ni plus ni moins.

Les deux questions laissées ouvertes sont tranchées dans le plan d'exécution, section
**« Stratégie de test de l'import »** : l'aller-retour est
testé comme une propriété sur **tout** le jeu de données de test et dans les deux formats, et
l'ergonomie du back-office est vérifiée par deux tests de route plus une recette chronométrée — pas de
navigateur en CI, conformément à D2.8.

## Étapes

1. Modèle des thèmes et des axes, alimenté par un seed versionné.
2. Authentification admin et journalisation des actions.
3. Back-office de saisie : la file de travail doit être efficace au clavier, elle sera utilisée des
   centaines de fois.
4. Mesure `group_alignment` + seed de l'ordonnancement des groupes, construit à partir de la grille
   des nuances du ministère de l'Intérieur, source et date citées ligne par ligne.
5. Export, puis import avec prévisualisation.
6. Historique et pages publiques de consultation.
7. Recette : catégoriser à la main une trentaine de scrutins marquants, dont la taxe Zucman, pour valider
   que le modèle tient sur des cas réels.

## Décisions arbitrées

Toutes tranchées le 18/08/2026, avant le plan d'exécution. **Elles ne sont pas à rediscuter en cours
d'implémentation** : si l'une s'avère fausse au contact du code, le dire et proposer une révision.

| # | Question | Décision |
| --- | --- | --- |
| D3.1 | Authentification admin | **OAuth GitHub.** Liste d'autorisation de logins en variable d'environnement, session signée par cookie, ligne `admin_user` créée à la première connexion réussie. C'est ce qui permet à l'historique public de nommer un auteur, ce que methodology.md § 8 promet et qu'un compte partagé unique rendrait impossible. Aucun contournement de développement : en local aussi, on passe par une vraie application OAuth (le rappel sur `localhost` est autorisé par GitHub) |
| D3.2 | Un scrutin peut-il porter plusieurs thèmes ? | **Oui**, avec un poids par thème. La somme des poids d'un scrutin vaut **exactement 1**, vérifié par un trigger de contrainte différé (D3.13) |
| D3.3 | Pôle binaire ou position continue ? | **Position continue** dans `[-1, 1]`, avec une confiance séparée. La colonne s'appelle `position_pour` et se lit « voter *pour* ce scrutin situe à cette position sur l'axe du thème » — un scrutin n'a pas de position en soi, c'est le sens du vote qui en a une |
| D3.4 | Qui peut catégoriser ? | v1 : admins seulement. Les contributions extérieures passent par des issues. Une modération ouverte est un projet en soi |
| D3.5 | Que faire des scrutins hors thèmes ? | Un thème `autre` explicite, **sans axe** (ses deux libellés de pôle sont `NULL`), pour distinguer « non catégorisé » de « ne relève d'aucun thème ». Une catégorisation qui le porte n'a pas de `position_pour` |
| D3.6 | Contexte fourni dans l'export | Titre + type + résultat **plus la ventilation par groupe** (effectif, pour, contre, abstentions, position majoritaire de chaque groupe). Mesure à l'appui : le titre nomme le texte mais pas le contenu d'un amendement ; la ventilation par groupe est le signal le plus informatif dont on dispose sans ingérer les dossiers législatifs, ce qui reste hors périmètre (phase 6) |
| D3.7 | Statut du résultat de l'heuristique | **Une mesure, pas une catégorisation.** `scrutin_axis_estimate` (fait dérivé, calculé, daté, rattaché à une version d'ancrage) est séparée de `scrutin_label` (opinion assumée par un humain). `label_method` ne vaut donc que `manual` ou `import` : derrière toute catégorisation publiée, il y a quelqu'un. C'est la règle 2 de methodology.md § 1 appliquée au schéma, et [methodology.md](../methodology.md#5-les-trois-méthodes-de-catégorisation) § 5 est mis à jour en conséquence dans le commit qui livre la migration |
| D3.8 | La mesure est-elle publique ? | **Non en v1.** Elle vit dans le back-office. Ce qui est publié, c'est la **méthode** (page méthodologie) et, en phase 4, le **taux d'accord agrégé** avec les catégorisations humaines. Publier 17 000 positions automatiques sur des votes parlementaires, c'est publier du plausible et du faux à grande échelle |
| D3.9 | Comment dire qu'une mesure est faible ? | Trois colonnes obligatoires à côté de `position_pour` : `separation` (part des votants que l'axe classe correctement, ramenée à `[0,1]`), `couverture` (part des suffrages émis par des groupes présents dans l'ancrage) et `votants_couverts`. En dessous de 90 % de couverture, **aucune estimation n'est écrite** et une anomalie est journalisée |
| D3.10 | `principal_axis` | **Reportée en phase 4**, étape de recette, sous condition d'au moins 200 scrutins relus. La colonne `strategy` existe dès maintenant |
| D3.11 | Corpus de travail | `participation >= corpus_parametre.participation_min` (0,50 au départ, **paramètre en base**, pas une constante), chambre `assemblee`. Priorisation écrite dans la vue `scrutin_a_categoriser` : scrutins solennels et motions, puis votes sur l'ensemble d'un texte, puis articles, puis amendements ; à rang égal, les scrutins les plus disputés d'abord, puis les plus récents |
| D3.12 | `theme_axis` séparé de `theme` ? | **Fusionnés.** La relation est 1:1 (methodology.md § 4 : un thème, un axe, deux pôles). Deux colonnes `libelle_pole_negatif` / `libelle_pole_positif` sur `theme`, une jointure en moins, et la contrainte « les deux ou aucun » devient un simple `CHECK` |
| D3.13 | Types numériques | `numeric(4,3)` pour les poids, positions et confiances — jamais de flottant. Deux raisons : la somme des poids doit valoir *exactement* 1, et un aller-retour d'export doit être identique au caractère près, ce qu'un `double precision` ne garantit pas |
| D3.14 | Où s'exécute l'import ? | **Dans le backend**, en une transaction, en SQL ensembliste (`COPY` vers une table temporaire puis différence en SQL) — pas de boucle Python sur les lignes, pas de job worker. C'est une saisie d'admin, elle exige une réponse synchrone (aperçu puis application), et le volume maximal est de l'ordre de 25 000 lignes. La règle d'architecture visée interdit le traitement lourd, pas l'écriture transactionnelle d'une saisie |
| D3.15 | Granularité du refus | **Le fichier entier ou rien.** Une seule ligne invalide fait refuser l'import, avec un rapport numéroté par ligne. Pas d'import partiel : un import à moitié appliqué est un état que personne ne sait rattraper |
| D3.16 | Portée d'un import | Un scrutin **présent** dans le fichier voit son jeu de thèmes **intégralement remplacé** (sans quoi la somme des poids ne pourrait pas être garantie). Un scrutin **absent** du fichier n'est jamais touché. Cette phrase est la plus dangereuse du plan : elle est testée deux fois |
| D3.17 | Ergonomie de la file de saisie | **Sans JavaScript.** Groupe de boutons radio pour le thème et `input type=range` pour la position sont pilotables aux flèches nativement ; l'envoi rend le scrutin suivant avec `autofocus`. HTMX pour éviter le rechargement, jamais pour rendre l'interaction possible. Si la recette chronométrée montre que le compte n'y est pas, un `static/admin.js` documenté sera ajouté — après la mesure, pas avant |
| D3.18 | Format des seeds | **TOML** (`db/seeds/themes.toml`, `db/seeds/group_axis.toml`), comme `candidates.toml` en phase 2 : bibliothèque Rust de première classe, là où l'écosystème YAML de Rust est en cours d'abandon |
| D3.19 | Protection CSRF | **Vérification d'origine** (`Origin`, repli `Referer`) sur toute méthode non sûre sous `/admin`, plus un cookie de session en `SameSite=Lax`. Pas de jeton à propager dans chaque formulaire : deux mécanismes qui font le même travail, c'est un de trop à maintenir juste |
| D3.21 | Apparence du curseur de position | **Pas de remplissage de la piste, un dégradé rouge → bleu à la place.** Un `input[type=range]` peint par défaut la portion à gauche du pointeau : la barre se remplit quand on va vers la droite, ce qui dit « plus », donc « mieux ». L'axe d'un thème n'a pas de sens de progression — ses deux extrémités sont deux positions nommées symétriquement (methodology.md § 1), et un remplissage contredirait cette symétrie en silence. La piste porte donc un dégradé qui dit l'orientation gauche-droite, et le pointeau prend la couleur du dégradé à sa position. **Ces couleurs politiques ne sortent pas du back-office** : D2.11 reste entière côté public, et la palette de `10-tokens.css` reste neutre par construction. La coloration du pointeau vient d'un `static/admin.js` de cinquante lignes — première entorse à D3.17, assumée : purement cosmétique, le formulaire reste entièrement utilisable sans lui, et aucun calcul ni aucune requête n'y passe |
| D3.20 | Granularité de l'historique | **Un acte éditorial sur un scrutin = une ligne** de `label_revision`, portant l'état complet avant et après en JSONB (thèmes triés par slug, donc comparables structurellement). Contrainte `CHECK (avant <> apres)` : la base elle-même refuse d'enregistrer un changement qui n'en est pas un, ce qui fait de l'aller-retour sans effet une propriété garantie plutôt qu'un espoir |

## Plan d'exécution

Cette section est destinée à la session qui implémentera la phase. Elle est autoportante : **tout ce qui
est nécessaire est ici, dans [methodology.md](../methodology.md), dans
[data-sources.md](../data-sources.md), dans [architecture.md](../architecture.md) ou dans
[CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de conversation à retrouver. Les décisions
arbitrées ci-dessus ne sont pas à rediscuter.

Développement sur le tronc : **commits directs sur `main`**, pas de branche ni de PR. Un commit par
étape, `main` vert (lint, typage, tests) à chaque fois. Ne pas modifier les plans des autres phases —
la seule exception est écrite au commit 10 ci-dessous.

Le plan est découpé en **trois lots** qui se tiennent chacun debout : le lot A livre le modèle et la
mesure, le lot B le back-office, le lot C le cycle export/import et la publication. On peut s'arrêter
entre deux lots sans laisser le dépôt dans un état bancal, et reprendre plus tard.

**Langue** (rappel de [CLAUDE.md](../../CLAUDE.md#langue)) : identifiants en anglais, sauf les termes du
domaine parlementaire (`scrutin`, `groupe`, `organe`, `legislature`, `mandat`) et le vocabulaire
méthodologique propre au projet, déjà en français dans le schéma existant (`libelle`, `poids`,
`justification`, `confiance`, `couverture`, `separation`, `apercu`). URL et texte affiché **en
français** : `/admin/categorisation`, `/themes`, `/scrutin/…/categorisation`.

### Ce que la phase 3 ne fait pas

Pas de score et pas d'affichage d'orientation sur une fiche personne (phase 4). Pas d'appel à un LLM
depuis l'application, jamais. Pas de route publique qui crée un job. Pas d'ingestion des textes des
dossiers législatifs. Pas de `principal_axis` (D3.10). Pas de publication des estimations d'axe (D3.8).
Pas de contribution ni de modération publique (D3.4). Pas de framework JS, pas de bundler, pas de police
distante. Pas de test en navigateur (D2.8 reste en vigueur). Pas de couleur de parti.

Le backend gagne **un** appel HTTP sortant, et un seul : l'échange de jeton OAuth avec GitHub au moment
d'une connexion admin. Il n'a lieu ni pendant le rendu d'une page, ni sur une route publique. Cette
exception à « le backend ne fait pas d'appel sortant » est assumée ici et nulle part ailleurs : toute
autre tentation d'appel sortant depuis le backend est un job worker qui manque.

### Arborescence cible

```
db/
  migrations/0007_categorisation.sql   thèmes, ancrage, estimations, catégorisations, admin, imports
  seeds/themes.toml                    les 6 thèmes de methodology.md § 4, plus `autre`
  seeds/group_axis.toml                39 groupes, nuance + version de grille + date + source par ligne

worker/src/
  jobs/
    seed_themes.rs                     applique db/seeds/themes.toml (jamais de suppression)
    label_scrutins_heuristic.rs        charge l'ancrage, calcule et écrit les estimations
  axis.rs                              fonctions pures : moyennes de camp, séparation, couverture

worker/tests/
  seed_themes.rs
  label_scrutins_heuristic.rs
  axis.rs                              cas construits à la main, résultats attendus écrits à l'avance

backend/src/kyc_api/
  admin/
    __init__.py                        le routeur `/admin`, garde d'authentification en dépendance
    auth.py                            flux OAuth GitHub, session, dépendance `require_admin`
    csrf.py                            vérification d'origine sur les méthodes non sûres
    categorisation.py                  file de travail, formulaire, suppression
    imports.py                         dépôt, validation, aperçu, application, rapport
    exports.py                         CSV et JSON
    jobs.py                            liste, déclenchement (liste blanche), suivi
  labels_io.py                         schéma d'échange : lecture/écriture CSV et JSON, fonctions pures
  queries/
    themes.py  labels.py  admin.py     tout le SQL, comme en phase 2
  schemas/
    theme.py  label.py  import_.py
  static/css/45-admin.css              styles du back-office (cascade : 40 composants, 45 admin, 50 utilitaires)
  templates/
    themes.html.jinja  theme.html.jinja  scrutins.html.jinja  categorisation.html.jinja
    admin/base_admin.html.jinja  admin/dashboard.html.jinja  admin/categorisation.html.jinja
    admin/import_depot.html.jinja  admin/import_apercu.html.jinja  admin/jobs.html.jinja
    admin/job.html.jinja             ← ancien dev_job.html.jinja, déplacé
    admin/_job_status.html.jinja     ← ancien _job_status.html.jinja, déplacé
    admin/_categorisation_form.html.jinja

backend/tests/
  fixtures/imports/*.csv *.json        ~15 fichiers, un par cas de refus, plus deux fichiers valides
  test_admin_auth.py  test_admin_categorisation.py  test_admin_jobs.py
  test_labels_io.py                    schéma d'échange, fonctions pures
  test_import_validation.py            les refus
  test_import_apply.py                 conflits, transaction, aller-retour, historique
  test_export.py  test_themes_pages.py
```

### Migration `0007_categorisation.sql`

```sql
-- Phase 3 — catégorisation : thèmes et axes, ancrage gauche-droite mesuré, catégorisations
-- humaines historisées, comptes d'administration, imports.
--
-- Cette migration est immuable une fois mergée sur main (voir CLAUDE.md).

-- 1. Thèmes et axes ------------------------------------------------------------------------------
--
-- L'axe vit dans `theme` : la relation est 1:1 (methodology.md § 4), une table séparée n'ajouterait
-- qu'une jointure (D3.12). Le thème `autre` (D3.5) est le seul sans axe — ses deux libellés de pôle
-- sont NULL, et une catégorisation qui le porte n'a pas de position.
--
-- Convention de signe, à respecter dans le seed : le pôle **négatif** est celui qui se situe du côté
-- gauche de l'ancrage des nuances. Ce n'est pas un jugement, c'est ce qui rend le signe de
-- `scrutin_axis_estimate.position_pour` comparable à celui d'une catégorisation humaine — sans quoi
-- le contrôle croisé de la section « Fini quand » ne veut rien dire.

CREATE TABLE theme (
    id                   smallint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    slug                 text NOT NULL UNIQUE,
    libelle              text NOT NULL,
    description          text NOT NULL,
    libelle_pole_negatif text,
    libelle_pole_positif text,
    rang                 smallint NOT NULL,
    actif                boolean NOT NULL DEFAULT true,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT theme_axe_complet_ou_absent
        CHECK ((libelle_pole_negatif IS NULL) = (libelle_pole_positif IS NULL)),
    CONSTRAINT theme_slug_non_vide CHECK (btrim(slug) <> '')
);

-- 2. Le corpus est un paramètre, pas une constante ------------------------------------------------
--
-- methodology.md § 2 fixe 50 % « pour la v1 » en disant explicitement que le seuil pourra être
-- abaissé. Mesuré : 988 scrutins à 50 %, 4 318 à 30 %. Déplacer ce chiffre est une décision de
-- charge de travail humain, elle doit être visible et datée, pas cachée dans un WHERE.

CREATE TABLE corpus_parametre (
    id                boolean PRIMARY KEY DEFAULT true CHECK (id),
    participation_min numeric NOT NULL DEFAULT 0.50
                      CHECK (participation_min > 0 AND participation_min <= 1),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    updated_by        text
);

INSERT INTO corpus_parametre (id) VALUES (true);

-- 3. Ancrage gauche-droite des groupes ------------------------------------------------------------
--
-- Une version d'ancrage est IMMUABLE : `content_hash` permet au job de refuser de recharger la même
-- version avec un contenu différent. Changer une coordonnée impose donc de changer la version, et
-- l'explication d'une estimation calculée hier ne peut pas se mettre à mentir en silence.

CREATE TABLE group_axis (
    version        text PRIMARY KEY,
    description    text NOT NULL,
    grille_version text NOT NULL,
    grille_date    date NOT NULL,
    source_url     text NOT NULL,
    content_hash   text NOT NULL,
    is_current     boolean NOT NULL DEFAULT false,
    loaded_at      timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX group_axis_courant_idx ON group_axis (is_current) WHERE is_current;

CREATE TABLE group_axis_entry (
    axis_version text NOT NULL REFERENCES group_axis (version) ON DELETE CASCADE,
    organe_id    bigint NOT NULL REFERENCES organe (id),
    nuance_code  text NOT NULL,
    bloc         text NOT NULL,
    coordonnee   numeric(4,3) NOT NULL CHECK (coordonnee BETWEEN -1 AND 1),
    note         text,
    PRIMARY KEY (axis_version, organe_id)
);

-- 4. La mesure, séparée de l'interprétation (D3.7) -------------------------------------------------
--
-- `position_pour` se lit « voter *pour* ce scrutin situe à cette position, relativement à celles et
-- ceux qui ont voté contre ». `separation` dit à quel point l'axe sépare réellement les deux camps :
-- sans elle, le scrutin 17/2653 (RN + UDR pour, tout le reste contre) rendrait un chiffre juste et
-- muet sur sa propre fragilité.
--
-- La clé étrangère vers `group_axis` sera à relâcher le jour où `principal_axis` arrivera (D3.10) :
-- son axe n'est pas ancré sur un fichier de groupes. C'est une migration d'une ligne, et en attendant
-- l'intégrité référentielle a une valeur.

CREATE TABLE scrutin_axis_estimate (
    scrutin_id       bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    strategy         text   NOT NULL,
    axis_version     text   NOT NULL REFERENCES group_axis (version) ON DELETE CASCADE,
    position_pour    numeric(4,3) NOT NULL CHECK (position_pour BETWEEN -1 AND 1),
    separation       numeric(4,3) NOT NULL CHECK (separation BETWEEN 0 AND 1),
    couverture       numeric(4,3) NOT NULL CHECK (couverture BETWEEN 0 AND 1),
    votants_couverts integer NOT NULL CHECK (votants_couverts >= 0),
    computed_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scrutin_id, strategy, axis_version)
);

-- 5. Comptes d'administration ----------------------------------------------------------------------
--
-- Aucune adresse IP n'est conservée : le journal doit répondre à « qui a changé quoi », pas tracer
-- des personnes. `display_name` est ce qui s'affiche publiquement à côté d'une catégorisation.

CREATE TABLE admin_user (
    id           bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    github_id    bigint NOT NULL UNIQUE,
    github_login text   NOT NULL UNIQUE,
    display_name text   NOT NULL,
    actif        boolean NOT NULL DEFAULT true,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE admin_action (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    admin_user_id bigint REFERENCES admin_user (id),
    action        text NOT NULL,
    target        text,
    detail        jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX admin_action_recent_idx ON admin_action (created_at DESC);

-- 6. Imports ----------------------------------------------------------------------------------------
--
-- Le fichier déposé est conservé tel quel (`contenu`) : c'est la seule façon de rejouer un import
-- contesté, et de garantir que ce qui a été appliqué est bien ce qui a été relu. `apercu` porte le
-- plan calculé au dépôt ; il est recalculé et recomparé au moment d'appliquer.

CREATE TYPE label_import_status AS ENUM ('pending', 'applied', 'rejected');

CREATE TABLE label_import (
    id             bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    filename       text NOT NULL,
    format         text NOT NULL CHECK (format IN ('csv', 'json')),
    schema_version smallint NOT NULL,
    generateur     text,
    contenu        text NOT NULL,
    content_hash   text NOT NULL,
    status         label_import_status NOT NULL DEFAULT 'pending',
    apercu         jsonb NOT NULL,
    rapport        jsonb,
    uploaded_by    bigint NOT NULL REFERENCES admin_user (id),
    uploaded_at    timestamptz NOT NULL DEFAULT now(),
    decided_by     bigint REFERENCES admin_user (id),
    decided_at     timestamptz
);

-- 7. Les catégorisations ------------------------------------------------------------------------------
--
-- `method` ne vaut que `manual` ou `import` (D3.7) : derrière toute catégorisation, il y a quelqu'un.
-- `reviewed_at` distingue une ligne importée d'une ligne importée **puis relue** : c'est ce couple
-- (`method`, `reviewed_at`) que la règle de conflit de l'import interroge.
--
-- La justification fait au moins dix caractères. Le seuil est arbitraire, la règle ne l'est pas :
-- methodology.md § 5.b dit que le commentaire n'est pas optionnel parce que c'est lui qui sera affiché
-- en explication. « ok » n'explique rien.

CREATE TYPE label_method AS ENUM ('manual', 'import');

CREATE TABLE scrutin_label (
    scrutin_id    bigint   NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    theme_id      smallint NOT NULL REFERENCES theme (id),
    poids         numeric(4,3) NOT NULL CHECK (poids > 0 AND poids <= 1),
    position_pour numeric(4,3) CHECK (position_pour BETWEEN -1 AND 1),
    confiance     numeric(4,3) NOT NULL CHECK (confiance BETWEEN 0 AND 1),
    justification text NOT NULL CHECK (length(btrim(justification)) >= 10),
    method        label_method NOT NULL,
    author_id     bigint NOT NULL REFERENCES admin_user (id),
    import_id     bigint REFERENCES label_import (id),
    reviewed_by   bigint REFERENCES admin_user (id),
    reviewed_at   timestamptz,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scrutin_id, theme_id),
    CONSTRAINT scrutin_label_import_renseigne
        CHECK ((method = 'import') = (import_id IS NOT NULL)),
    CONSTRAINT scrutin_label_relecture_complete
        CHECK ((reviewed_by IS NULL) = (reviewed_at IS NULL))
);

CREATE INDEX scrutin_label_theme_idx ON scrutin_label (theme_id, scrutin_id);

-- 8. L'historique --------------------------------------------------------------------------------------
--
-- Une ligne = un acte éditorial sur un scrutin (D3.20), avec l'état complet avant et après. Les
-- tableaux JSONB sont triés par slug de thème à l'écriture, ce qui rend `avant <> apres` structurel :
-- la base refuse d'elle-même d'enregistrer un « changement » qui n'en est pas un. C'est cette
-- contrainte qui fait de « un aller-retour d'export ne crée aucune ligne d'historique » une propriété
-- garantie et non un espoir.

CREATE TABLE label_revision (
    id         bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    scrutin_id bigint NOT NULL REFERENCES scrutin (id) ON DELETE CASCADE,
    avant      jsonb NOT NULL,
    apres      jsonb NOT NULL,
    method     label_method NOT NULL,
    author_id  bigint NOT NULL REFERENCES admin_user (id),
    import_id  bigint REFERENCES label_import (id),
    motif      text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT label_revision_change_reel CHECK (avant <> apres)
);

CREATE INDEX label_revision_scrutin_idx ON label_revision (scrutin_id, created_at DESC);

-- 9. Cohérence d'un jeu de catégorisations --------------------------------------------------------------
--
-- Deux règles qu'aucun CHECK de ligne ne peut exprimer : la somme des poids d'un scrutin vaut
-- exactement 1, et une position n'existe que sur un thème qui a un axe. D'où un trigger de
-- contrainte DIFFÉRÉ : les écritures intermédiaires d'un remplacement (DELETE puis INSERT) sont
-- légitimement incohérentes, seul l'état à la validation compte.
--
-- PIÈGE À CONNAÎTRE, il coûte une demi-heure à qui le rencontre sans avertissement : un trigger
-- différé ne se déclenche qu'au COMMIT, et chaque test tourne dans une transaction annulée. Un test
-- qui vérifie ce refus doit forcer la vérification par `SET CONSTRAINTS ALL IMMEDIATE` dans un
-- SAVEPOINT. Écrire ce commentaire dans la fixture de test aussi.

CREATE FUNCTION scrutin_label_coherence() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    v_scrutin_id  bigint;
    v_somme       numeric;
    v_incoherents integer;
BEGIN
    IF TG_OP = 'DELETE' THEN
        v_scrutin_id := OLD.scrutin_id;
    ELSE
        v_scrutin_id := NEW.scrutin_id;
    END IF;

    SELECT sum(poids) INTO v_somme FROM scrutin_label WHERE scrutin_id = v_scrutin_id;
    IF v_somme IS NOT NULL AND v_somme <> 1 THEN
        RAISE EXCEPTION 'somme des poids du scrutin % = %, attendu exactement 1', v_scrutin_id, v_somme;
    END IF;

    SELECT count(*) INTO v_incoherents
    FROM scrutin_label sl
    JOIN theme t ON t.id = sl.theme_id
    WHERE sl.scrutin_id = v_scrutin_id
      AND (t.libelle_pole_positif IS NULL) <> (sl.position_pour IS NULL);
    IF v_incoherents > 0 THEN
        RAISE EXCEPTION
            'scrutin % : une position n''existe que sur un thème doté d''un axe, et y est obligatoire',
            v_scrutin_id;
    END IF;

    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER scrutin_label_coherence_trg
    AFTER INSERT OR UPDATE OR DELETE ON scrutin_label
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW EXECUTE FUNCTION scrutin_label_coherence();

-- 10. La file de travail ----------------------------------------------------------------------------------
--
-- Une vue et non une vue matérialisée : ~17 000 lignes filtrées, quelques millisecondes, et une vue
-- matérialisée se périmerait à chaque catégorisation saisie — exactement l'inverse de ce qu'on veut
-- d'une file de travail.
--
-- `rang_priorite` est dérivé du titre parce que c'est le seul signal disponible : `type_code` ne
-- distingue que SPO/SPS/MOC. Mesuré sur le corpus retenu : 216 votes sur l'ensemble d'un texte,
-- 77 articles, 542 amendements, 51 motions. Les titres de l'AN commencent tous par « l'ensemble
-- de… », « l'amendement n° … », « l'article … » — c'est mécanique, pas interprétatif.

CREATE VIEW scrutin_a_categoriser AS
SELECT s.id AS scrutin_id,
       s.an_uid, s.legislature, s.numero, s.date_scrutin, s.titre,
       s.type_code, s.type_libelle, s.sort_code, s.participation,
       LEAST(s.pour, s.contre)::numeric / NULLIF(s.suffrages_exprimes, 0) AS part_minoritaire,
       CASE
           WHEN s.type_code IN ('SPS', 'MOC')  THEN 1
           WHEN s.titre ILIKE 'l''ensemble%'   THEN 2
           WHEN s.titre ILIKE 'l''article%'    THEN 3
           ELSE 4
       END AS rang_priorite,
       EXISTS (SELECT 1 FROM scrutin_label sl WHERE sl.scrutin_id = s.id) AS est_categorise
FROM scrutin s
CROSS JOIN corpus_parametre c
WHERE s.chambre = 'assemblee'
  AND s.participation >= c.participation_min;
```

Le scrutin du Congrès reste hors file par le filtre `chambre` : il n'a pas de ventilation par groupe
parlementaire exploitable et n'entre dans aucun score.

> **Corrigé depuis** par `db/migrations/0008_priorite_apostrophe.sql` : les comparaisons
> `ILIKE 'l''ensemble%'` et `ILIKE 'l''article%'` écrites ci-dessus ne reconnaissent que
> l'apostrophe droite, alors que 118 titres de l'open data portent l'apostrophe typographique
> (F8, [phase-3.0-feedback.md](phase-3.0-feedback.md)). La vue utilise désormais
> `~* '^l[''’]ensemble'`. Le reste de la définition est inchangé.

### Le seed des thèmes

`db/seeds/themes.toml`, appliqué par le job `seed_themes`. Un fichier plutôt qu'un `INSERT` dans la
migration : la liste évoluera (methodology.md § 4 annonce déjà institutions, Europe, travail,
agriculture), et une migration est immuable.

```toml
# Thèmes et axes. Chaque thème a un axe à deux pôles nommés symétriquement (methodology.md § 4).
#
# Convention de signe, à ne pas inverser : le pôle négatif est celui situé du côté gauche de
# l'ancrage des nuances (db/seeds/group_axis.toml). Ce n'est pas un jugement de valeur, c'est ce
# qui rend comparables le signe d'une catégorisation humaine et celui d'une estimation automatique.
#
# `autre` n'a pas de pôle : il sert à dire « relu, ne relève d'aucun thème », ce qui n'est pas la
# même chose que « pas encore regardé ».

[[theme]]
slug = "social-fiscalite"
libelle = "Social / fiscalité"
description = "Prélèvements, prestations, retraites, salaires, protection sociale."
pole_negatif = "redistribution et protection sociale étendue"
pole_positif = "maîtrise de la dépense et de la fiscalité"
rang = 10

# … environnement, sante, education, securite, immigration, repris mot pour mot du tableau de
# methodology.md § 4 — ne pas les reformuler ici, ce sont les mêmes phrases.

[[theme]]
slug = "autre"
libelle = "Autre"
description = "Ne relève d'aucun des thèmes ci-dessus."
rang = 99
```

Le job `seed_themes { path? }` valide tout avant d'écrire (slug non vide et unique, description non
vide, les deux pôles ou aucun, rang unique), puis fait un `INSERT … ON CONFLICT (slug) DO UPDATE`.
**Il ne supprime jamais** — contrairement à `seed_candidates`, parce qu'un thème est référencé par des
catégorisations. Retirer un thème du fichier le passe à `actif = false` ; il disparaît des formulaires,
pas de l'historique.

### Le seed d'ancrage

`db/seeds/group_axis.toml` — 39 lignes, une par groupe parlementaire non-inscrit exclu, sur les
législatures 15 à 17.

> **Incomplet à l'implémentation** (lot A, F2 — [phase-3.0-feedback.md](phase-3.0-feedback.md)) : 40
> lignes réellement trouvées en base (un écart d'une unité par rapport aux 39 comptés ici), mais la
> grille des nuances 2026 elle-même n'a pas pu être vérifiée par une source primaire citable. Le fichier
> livré porte les `an_uid`/`libelle` (vérifiés) et des champs de source vides sous bandeau `TODO`, exactement
> la voie de repli prévue trois paragraphes plus bas. Le job refuse de charger le fichier tant que c'est
> le cas — voir le lien pour le détail et le travail restant.
>
> **Résolu au lot C** : la grille effectivement utilisée est celle des élections législatives de 2024
> (24 nuances, pas 26), fournie par l'utilisateur après un échec de récupération automatisée
> (Légifrance bloque les requêtes non-navigateur). Les 40 lignes portent désormais une nuance, un
> bloc et, pour les rattachements non évidents (groupes renommés, alliances électorales, groupes
> composites sans nuance propre), une note qui documente le choix. Le bandeau `TODO` est retiré, le
> job charge le fichier et calcule les 15 621 estimations sur les 16 956 scrutins du corpus — voir
> le lien pour le détail.

```toml
version = "2026-08-nuances-2026"
description = "Ancrage dérivé de la grille des nuances politiques du ministère de l'Intérieur."
grille_version = "…"          # la version telle que le ministère la nomme
grille_date = 2026-01-01      # la date de publication de la grille utilisée
source_url = "https://…"      # l'URL de la grille, pas celle d'un commentaire de presse

# Coordonnées des blocs. Ce sont les SEULS nombres que nous inventons : la grille ordonne six blocs,
# elle ne les chiffre pas. Ils sont ici pour être contestés — une PR d'une ligne suffit.
[bloc]
extreme_gauche = -1.0
gauche         = -0.6
centre         = -0.1
divers         =  0.0
droite         =  0.6
extreme_droite =  1.0

[[groupe]]
an_uid = "PO845413"           # LFI-NFP, 17e législature
libelle = "La France insoumise - Nouveau Front Populaire"
nuance = "…"                  # le code de nuance officiel
bloc = "gauche"
note = "…"                    # obligatoire si le rattachement n'est pas évident
```

Trois règles pour l'implémentation, dans cet ordre d'importance :

1. **Ne rien inventer.** Les codes de nuance, la version de la grille, sa date et son URL se recopient
   depuis la publication du ministère. Si l'environnement d'implémentation ne permet pas de les
   vérifier, livrer le fichier avec les 39 `an_uid` et `libelle` (qui, eux, se lisent en base) et
   laisser les champs de source **vides mais présents**, avec un `TODO` explicite en tête de fichier
   et le job qui échoue tant qu'ils le sont. Un fichier de source à moitié inventé est pire qu'un
   fichier incomplet : le second se voit.
2. **Les pseudo-groupes « non inscrit » (`organe.is_non_inscrit`) n'y figurent pas** — methodology.md
   § 2 : n'appartenir à aucun groupe n'est pas appartenir au groupe des sans-groupe, et aucun
   alignement de groupe n'est calculé sur ces périodes.
3. **Une version d'ancrage est immuable.** Le job calcule le hachage du contenu ; si la version existe
   déjà avec un hachage différent, il échoue en nommant les deux. Modifier une coordonnée impose donc
   de changer la chaîne `version`, ce qui laisse les estimations anciennes intactes et explicables.

### Job `label_scrutins_heuristic`

`{ "strategy"?: "group_alignment", "seed_path"?: string, "scope"?: { "legislature"?: int, "since"?: date } }`

1. Charge et valide le seed. Toute erreur fait échouer le job **avant** la moindre écriture : entrée
   dont l'`an_uid` n'existe pas en base ou n'est pas un organe `GP`, groupe non-inscrit présent, bloc
   inconnu, doublon d'`an_uid`, champ de source vide, coordonnée hors `[-1, 1]`.
2. Insère `group_axis` + `group_axis_entry` si la version est nouvelle (règle d'immuabilité ci-dessus),
   et bascule `is_current`.
3. Parcourt les scrutins du périmètre (défaut : `chambre = 'assemblee'`) en **un seul balayage** de
   `scrutin_groupe` joint à l'ancrage, trié par `scrutin_id` — 186 798 lignes, jamais la table `vote`.
   Pour chaque scrutin, avec `x_g` la coordonnée du groupe `g`, `p_g` et `c_g` ses votes pour et
   contre :

   ```
   couvert    = Σ (p_g + c_g) sur les groupes présents dans l'ancrage
   total      = Σ (p_g + c_g) sur tous les groupes du scrutin
   couverture = couvert / total

   μ+ = Σ p_g·x_g / Σ p_g          (position moyenne du camp « pour »)
   μ− = Σ c_g·x_g / Σ c_g          (position moyenne du camp « contre »)

   position_pour = borner((μ+ − μ−) / 2, −1, +1)
   ```

   Les abstentions et les non-votants n'entrent dans aucun de ces calculs (methodology.md § 3).

   `separation` : pour chaque seuil `t` pris au milieu de deux coordonnées consécutives distinctes,
   compter les votants correctement classés par « à droite de `t` = a voté comme le camp majoritaire
   de ce côté », dans les deux orientations, et garder la meilleure part `a`. Alors
   `separation = max(0, 2a − 1)`. Se lit en une phrase dans le back-office : « l'axe classe
   correctement 92 % des votants ».

4. N'écrit **aucune** estimation, et journalise une anomalie, quand : `couverture < 0,90`
   (`ancrage_insuffisant`), `couvert < 20` votants (`trop_peu_de_votants`), ou l'un des deux camps est
   vide (`scrutin_unanime` — un scrutin sans opposition ne situe personne). Ces trois cas sont des
   compteurs du run, pas des échecs.
5. Journalise dans `ingestion_run` (`source = 'label_scrutins_heuristic'`) : scrutins examinés,
   estimations écrites, les trois compteurs de refus, couverture médiane, durée. Rapporte sa
   progression comme tout job de la phase 0.

Idempotent : même seed et mêmes votes ⇒ mêmes lignes. Le calcul lui-même (moyennes, séparation,
bornage) vit dans `worker/src/axis.rs`, **fonctions pures testées seules**, avec au minimum ces cas
écrits à l'avance : polarisation parfaite gauche/contre-droite/pour, scrutin 17/2653 (deux groupes
d'extrême droite pour, tout le reste contre → position positive, séparation médiocre), scrutin où un
seul groupe se divise, groupe absent de l'ancrage, camp vide, coordonnées toutes identiques
(séparation nulle, pas de division par zéro).

### `make ingest`

Deux lignes de plus, avant `refresh_views` :

```
cargo run --release -- enqueue seed_themes
cargo run --release -- enqueue label_scrutins_heuristic
```

Rappel d'outillage : après toute modification d'une requête `sqlx::query!`/`query_as!`, régénérer avec
**`cargo sqlx prepare -- --all-targets`**, sans quoi la CI casse en mode hors ligne.

### Dépendances ajoutées

Trois, pas une de plus, toutes dans `backend/pyproject.toml` :

| Paquet | Où | Pourquoi |
| --- | --- | --- |
| `httpx` | **déplacé** du groupe `dev` vers les dépendances principales | L'échange de jeton OAuth. Il n'était jusqu'ici qu'un outil de test ; il devient du code de production, ce qui mérite d'être vu dans le diff |
| `itsdangerous` | dépendances principales | Exigé par `SessionMiddleware` de Starlette pour signer le cookie de session |
| `python-multipart` | dépendances principales | Exigé par FastAPI pour lire un formulaire `POST` et un fichier déposé. Sans lui, tout le back-office rend un 500 au premier envoi — la panne coûte dix minutes à qui ne connaît pas le message d'erreur |

Côté worker, aucune dépendance nouvelle : `toml`, `sqlx`, `serde` et `sha2` sont déjà là, et le calcul
d'axe n'a besoin d'aucune bibliothèque d'algèbre linéaire tant que `principal_axis` reste hors périmètre.

### Authentification admin (D3.1)

Quatre variables d'environnement, à ajouter à `.env.example` avec leur commentaire :

```
ADMIN_GITHUB_CLIENT_ID=      # application OAuth GitHub ; en local, rappel sur http://localhost:8000
ADMIN_GITHUB_CLIENT_SECRET=
ADMIN_GITHUB_LOGINS=         # logins autorisés, séparés par des virgules. Vide = personne n'entre
SESSION_SECRET=              # clé de signature des cookies de session. Ne PAS reprendre celle-ci en production
PUBLIC_BASE_URL=http://localhost:8000
```

Le flux, écrit à la main (~80 lignes) plutôt qu'avec une bibliothèque OAuth généraliste : une seule
autorité, un seul flux, aucune extensibilité recherchée.

1. `GET /admin/login` — tire un `state` aléatoire, le pose en session, redirige vers
   `https://github.com/login/oauth/authorize` **sans aucun `scope`** : le profil public suffit à
   connaître le login, et ne rien demander est la meilleure façon de ne rien obtenir de trop.
2. `GET /admin/auth/callback` — vérifie le `state` (sinon 400), échange le `code` contre un jeton
   (`POST https://github.com/login/oauth/access_token`, `httpx`, délai maximal 10 s), lit
   `GET https://api.github.com/user`. C'est le seul appel sortant du backend de tout le projet.
3. Login normalisé en minuscules, comparé à la liste d'autorisation. Refusé ⇒ 403 avec un message
   neutre, et une ligne `admin_action` (`action = 'login_refuse'`, login en clair dans `detail`).
   Accepté ⇒ `INSERT … ON CONFLICT (github_id) DO UPDATE` sur `admin_user`, session posée, redirection
   vers `/admin`.
4. Session : `SessionMiddleware` de Starlette (dépendance `itsdangerous`), `same_site="lax"`,
   `https_only` piloté par une variable, `max_age = 8 h`. Elle ne porte que `admin_user_id` et
   l'expiration.
5. Dépendance `require_admin`, posée **sur le routeur `/admin` entier** et non route par route : une
   route admin ajoutée plus tard est protégée par construction, sans que personne ait à y penser. Elle
   rend une redirection 303 vers `/admin/login` pour une navigation normale, et un 401 portant
   `HX-Redirect` pour une requête HTMX (un fragment ne sait pas suivre une redirection utilement).
   Elle recharge `admin_user` à chaque requête : un compte passé à `actif = false` perd la main
   immédiatement, sans attendre l'expiration de sa session.
6. Si `ADMIN_GITHUB_CLIENT_ID` est vide, `/admin/login` rend un **503** disant que l'authentification
   n'est pas configurée, et le démarrage journalise un avertissement. Pas de compte de secours, pas de
   contournement en développement, pas de drapeau : CLAUDE.md est catégorique, et un drapeau finit
   toujours par être activé « temporairement ».
7. Toutes les réponses sous `/admin` portent `X-Robots-Tag: noindex, nofollow` et `Cache-Control:
   private, no-store` — le middleware de cache HTTP de la phase 2 doit les laisser tranquilles.

**CSRF (D3.19)** : une dépendance appliquée à toute méthode non sûre sous `/admin` compare l'hôte de
`Origin` (repli `Referer`) à celui de la requête ; absence des deux ⇒ refus. Environ vingt lignes, avec
un test par branche.

`admin_action` est écrite pour : connexion, refus de connexion, déconnexion, création/modification/
suppression d'une catégorisation, dépôt d'import, application d'import, rejet d'import, création de job.

### Back-office : la file de travail

L'objectif chiffré — 30 scrutins en moins de 15 minutes — se joue entièrement ici, et la justification
obligatoire est le poste de coût dominant. L'écran est donc conçu autour d'elle.

`GET /admin/categorisation` sert **un** scrutin : le premier de `scrutin_a_categoriser` non catégorisé,
trié par `rang_priorite`, puis `part_minoritaire DESC`, puis `date_scrutin DESC`. La page montre, dans
cet ordre de lecture : titre complet, date, type, sort, compteurs, ventilation par groupe (reprise du
gabarit de la page publique de scrutin), lien vers la source AN, puis le formulaire.

Le formulaire, sans JavaScript (D3.17) :

- **thème** : un groupe de boutons radio, un par thème actif, `autofocus` sur le premier. Les flèches
  y naviguent nativement ; c'est le seul composant HTML qui donne la sélection au clavier sans code.
  Un lien « ajouter un second thème » (HTMX, avec un repli en paramètre d'URL) ajoute un bloc thème +
  poids ; tant qu'il n'y en a qu'un, le poids est masqué et vaut 1 ;
- **position** : `<input type="range" min="-1" max="1" step="0.05">`, pilotable aux flèches, encadré
  des deux libellés de pôle **en toutes lettres** — jamais un nombre nu. Désactivé si le thème
  sélectionné n'a pas d'axe. Apparence : piste sans remplissage, dégradé d'orientation, pointeau
  coloré à sa position (D3.21) ;
- **confiance** : trois boutons radio (0,4 / 0,7 / 1,0) plutôt qu'un curseur. Une confiance qu'on
  hésite à chiffrer finement n'a pas besoin de l'être ;
- **justification** : `<input type="text">` et non `<textarea>`, pour que `Entrée` valide ;
- **pré-remplissage** : si une estimation existe pour ce scrutin, la position est pré-positionnée à sa
  valeur et l'écran affiche, en toutes lettres, d'où elle vient et ce qu'elle vaut (« repère
  automatique : +0,62, l'axe classe correctement 92 % des votants, ancrage 2026-08 »). Le thème n'est
  **jamais** pré-rempli — la machine ne connaît pas les thèmes (D3.7) ;
- **envoi** : `POST`, puis redirection 303 vers `/admin/categorisation` — donc vers le scrutin suivant,
  avec le focus déjà sur le premier bouton radio. Un `Passer` (POST vers `/admin/categorisation/passer`)
  place le scrutin en fin de file pour la session en cours, sans rien écrire en base.

Chaque enregistrement écrit, **dans la même transaction** : le remplacement complet des lignes
`scrutin_label` du scrutin, une ligne `label_revision` (avant/après, thèmes triés par slug) si et
seulement si l'état a réellement changé, et une ligne `admin_action`.

> **Détail d'infrastructure découvert à l'implémentation** (lot B, F3 —
> [phase-3.0-feedback.md](phase-3.0-feedback.md)) : première écriture multi-instructions du backend,
> qui n'avait jusqu'ici que `Queryable` (pas de `.transaction()`). Voir le lien pour `WritableQueryable`/
> `get_connection` et un piège de test à connaître avant d'ordonner des écritures par horodatage.

`GET /admin/categorisation/{legislature}/{numero}` ouvre un scrutin précis, formulaire pré-rempli avec
la catégorisation existante s'il y en a une, et affiche son historique. C'est aussi ce que vise le lien
« corriger » depuis la page publique.

`POST /admin/categorisation/{legislature}/{numero}/supprimer` retire toutes les catégorisations d'un
scrutin. **Motif obligatoire**, enregistré dans `label_revision.motif` avec un `apres` vide.

### Suivi des jobs et test de fumée (livrables 8 et 9)

`admin/job.html.jinja` et `admin/_job_status.html.jinja` sont les gabarits `dev_job.html.jinja` et
`_job_status.html.jinja` laissés en place par la phase 2 : **les déplacer, pas les réécrire**, et
conserver leurs règles CSS en les rapatriant dans `45-admin.css` (avec un commentaire qui explique
d'où elles viennent).

- `GET /admin/jobs` : les 50 derniers jobs, l'état du battement de cœur du worker, et un formulaire de
  déclenchement.
- `POST /admin/jobs` : crée un job dont le type est dans une **liste blanche explicite** —
  `noop`, `label_scrutins_heuristic`, `seed_themes`, `refresh_views`. Rien d'autre en phase 3 : les
  jobs d'ingestion restent en ligne de commande, la phase 5 étendra la liste si elle en a besoin. Tout
  type hors liste ⇒ 400.
- `GET /admin/jobs/{id}` : page de suivi ; `GET /admin/fragments/jobs/{id}` : le fragment rafraîchi par
  HTMX. Les fragments admin vivent sous `/admin/` et non sous `/fragments/` — c'est une entorse assumée
  à la convention de la phase 0, en échange d'une garde d'authentification unique sur un seul préfixe.

### Routes exactes

| Route | Rendu |
| --- | --- |
| `GET /admin` | Tableau de bord : reste à catégoriser, catégorisés, imports en attente, worker |
| `GET /admin/login`, `GET /admin/auth/callback`, `POST /admin/logout` | Authentification |
| `GET /admin/categorisation` | Le prochain scrutin de la file |
| `GET,POST /admin/categorisation/{legislature}/{numero}` | Un scrutin précis |
| `POST /admin/categorisation/{legislature}/{numero}/supprimer` | Retrait, motif obligatoire |
| `POST /admin/categorisation/passer` | Reporte le scrutin en fin de file (session) |
| `GET /admin/export` | `?statut=non_categorises\|categorises\|tous&format=csv\|json&theme=&limite=` |
| `GET,POST /admin/import` | Dépôt du fichier, validation, création de l'aperçu |
| `GET /admin/import/{id}` | Aperçu détaillé |
| `POST /admin/import/{id}/appliquer` | Application transactionnelle |
| `POST /admin/import/{id}/rejeter` | Abandon, tracé |
| `GET /admin/jobs`, `POST /admin/jobs`, `GET /admin/jobs/{id}` | File de jobs |
| `GET /admin/fragments/jobs/{id}` | Fragment de progression |
| `GET /themes` | Public : les thèmes, leurs axes, leurs deux pôles, leur description |
| `GET /theme/{slug}` | Public : les scrutins catégorisés sur ce thème, paginés |
| `GET /scrutins` | Public : les scrutins catégorisés, filtrables par thème et législature |
| `GET /scrutin/{legislature}/{numero}` | Public, **enrichie** : bloc catégorisation |
| `GET /scrutin/{legislature}/{numero}/categorisation` | Public : détail, méthode, auteur, historique |

### Pages publiques

Règles d'affichage, non négociables et testées :

- une position ne s'affiche **jamais** en nombre nu : « plutôt : maîtrise de la dépense et de la
  fiscalité (position +0,6 sur l'axe *social / fiscalité*) ». Le libellé du pôle vient toujours en
  premier ;
- la méthode est visible sans cliquer : « catégorisé par *Prénom Nom*, le JJ/MM/AAAA » ou « importé du
  fichier `nom.csv` le JJ/MM/AAAA, relu par *Prénom Nom* le JJ/MM/AAAA ». Une catégorisation importée
  et non relue le dit ;
- la justification est affichée intégralement, jamais tronquée : c'est l'explication, pas une
  décoration ;
- l'historique complet est atteignable en un clic depuis la catégorisation, et il nomme l'auteur de
  chaque changement ;
- **aucune estimation automatique n'apparaît sur une page publique** (D3.8) ;
- `/themes` porte un rappel en toutes lettres : ces axes sont une construction du projet, discutable,
  et le lien vers la méthodologie est à côté.

### Le schéma d'échange, version 1

Un seul module, `labels_io.py`, écrit et lit les deux formats. Il ne touche jamais la base : il
transforme des lignes en objets Pydantic et réciproquement. C'est ce qui rend l'aller-retour testable
sans base et le reste testable sans fichier.

**JSON** — le format canonique, celui qu'on donne à un LLM :

```json
{
  "schema_version": 1,
  "generated_at": "2026-08-18T09:00:00Z",
  "filtre": { "statut": "non_categorises", "participation_min": "0.500" },
  "themes": [
    { "slug": "social-fiscalite", "libelle": "Social / fiscalité",
      "pole_negatif": "redistribution et protection sociale étendue",
      "pole_positif": "maîtrise de la dépense et de la fiscalité" }
  ],
  "scrutins": [
    {
      "scrutin_uid": "VTANR5L17V2653",
      "legislature": 17, "numero": 2653, "date": "2025-06-24",
      "titre": "l'ensemble de la proposition de loi portant programmation nationale pour l'énergie…",
      "type": "scrutin public ordinaire", "sort": "adopté",
      "pour": 140, "contre": 231, "abstentions": 47, "non_votants": 3,
      "participation": "0.720",
      "groupes": [
        { "abrege": "RN", "membres": 123, "pour": 122, "contre": 0, "abstentions": 0,
          "position_majoritaire": "pour" }
      ],
      "url_an": "https://www.assemblee-nationale.fr/dyn/17/scrutins/2653",
      "categorisation": [
        { "theme": "environnement", "poids": "1.000", "position_pour": "-0.400",
          "confiance": "0.700", "justification": "…" }
      ]
    }
  ]
}
```

Un objet `"generateur": { "outil": "…", "modele": "…" }` est facultatif à la racine ; s'il est présent,
`modele` est recopié dans `label_import.generateur` et se retrouve dans l'UI (« importé d'un travail
produit par *modèle* »).

**CSV** — une ligne par couple (scrutin, thème), séparateur `,`, encodage UTF-8, guillemets doublés :

```
schema_version,scrutin_uid,legislature,numero,date,titre,type,sort,pour,contre,abstentions,
participation,positions_groupes,url_an,theme,poids,position_pour,confiance,justification
```

`positions_groupes` encode la ventilation en une colonne lisible dans un tableur :
`RN:pour;EPR:contre;LFI-NFP:contre;…`. Un scrutin non catégorisé sort avec une seule ligne dont les
cinq dernières colonnes sont vides ; qui veut lui donner deux thèmes duplique la ligne.

Les colonnes de contexte (titre, compteurs, ventilation, URL) sont **ignorées à l'import** : seuls
`schema_version`, `scrutin_uid` et les cinq colonnes de catégorisation sont lus. Le dire dans la
documentation d'export, sinon quelqu'un finira par corriger un titre dans le fichier et s'étonnera.

> **Bug trouvé à l'implémentation** (lot C, F4 — [phase-3.0-feedback.md](phase-3.0-feedback.md)) : un
> guillemet non fermé faisait disparaître une ligne entière au lieu d'être refusée — exactement la
> classe de bug que la section « ⚠️ » ci-dessous prévient. Corrigé et testé par la fixture qui l'a
> révélé.

Les nombres sont écrits et lus comme des **chaînes à trois décimales** (`"1.000"`, `"-0.400"`), ce qui
rend l'aller-retour exact (D3.13). Les booléens n'existent pas dans ce schéma, les dates sont en
ISO 8601.

### Import : validation, aperçu, application

**Limites dures** : 5 Mo, 25 000 lignes, `schema_version` connue. Au-delà, refus immédiat.

**Validation** (D3.15 : le fichier entier ou rien). Chaque erreur porte un numéro de ligne (CSV) ou un
chemin JSON, et le rapport en liste au maximum 50 avant de dire combien il en reste :

| Refus | Message attendu |
| --- | --- |
| `schema_version` absente ou inconnue | nomme la version lue et celles acceptées |
| colonne obligatoire manquante | nomme la colonne |
| séparateur `;` détecté (export de tableur français) | le dit explicitement, plutôt qu'un « colonne manquante » incompréhensible |
| `scrutin_uid` inexistant en base | nomme l'uid |
| slug de thème inconnu ou inactif | nomme le slug et liste les slugs valides |
| poids ou confiance hors `]0, 1]` / `[0, 1]`, position hors `[-1, 1]` | nomme la valeur |
| plus de trois décimales | nomme la valeur — c'est le symptôme d'un calcul flottant en amont |
| somme des poids d'un scrutin ≠ 1 exactement | nomme le scrutin et la somme obtenue |
| couple (scrutin, thème) présent deux fois | nomme les deux lignes |
| position renseignée sur `autre`, ou absente sur un thème à axe | nomme le scrutin |
| justification de moins de dix caractères | nomme le scrutin |

**Aperçu.** Aucune écriture hors `label_import`. Le plan classe chaque scrutin du fichier en :
*création* (aucune catégorisation actuelle), *modification* (existe et diffère), *inchangé* (existe et
identique — ni écriture ni révision), *conflit* (la catégorisation actuelle est `manual`, ou porte un
`reviewed_at`, et diffère). L'écran donne les quatre compteurs, puis le détail scrutin par scrutin avec
l'avant et l'après côte à côte, conflits en premier.

**Application.** Une transaction, dans cet ordre :

1. relire le fichier depuis `label_import.contenu` — jamais depuis l'aperçu : ce qui s'applique doit
   être ce qui a été déposé ;
2. recalculer le plan et le comparer à `apercu` ; s'il a changé, **abandonner** avec « la base a changé
   depuis l'aperçu », remplacer l'aperçu par le nouveau et renvoyer l'admin dessus. C'est la garde
   contre deux admins qui travaillent en même temps ;
3. `COPY` des lignes validées vers une table temporaire, puis différence et écriture **en SQL
   ensembliste** : suppression des lignes du scrutin, insertion des nouvelles, insertion des révisions
   (D3.14). Pas de boucle Python sur les lignes ;
4. les scrutins en conflit ne sont écrits que si l'admin a coché la case « écraser les catégorisations
   relues » ; le motif de chaque révision correspondante porte cette mention ;
5. `status = 'applied'`, `rapport` (compteurs + liste des scrutins touchés), `admin_action`.

**Rappel de la règle qui coûte le plus cher si on la casse** (D3.16) : un scrutin présent dans le
fichier voit son jeu de thèmes intégralement remplacé ; un scrutin absent du fichier n'est jamais
touché. Les deux moitiés de cette phrase ont leur test.

### Stratégie de test de l'import (la section ⚠️)

**Fichiers de cas** dans `backend/tests/fixtures/imports/`, un par refus du tableau ci-dessus, plus :
CSV avec BOM UTF-8, CSV aux guillemets non fermés, JSON tronqué, fichier vide, fichier de 25 001
lignes. Un test paramétré les parcourt tous : chacun doit être refusé, **avec le message attendu** —
un refus au bon moment mais au mauvais motif est un demi-bug.

**Tests d'application**, chacun nommé d'après la règle qu'il défend :

- `test_conflit_manual_non_ecrase_sans_confirmation` — la ligne humaine est intacte, le rapport la
  compte en conflit, aucune révision n'est écrite ;
- `test_conflit_ecrase_si_confirme` — écrasée, et la révision porte le motif ;
- `test_import_interrompu_ne_laisse_rien` — une erreur injectée au milieu de l'application laisse zéro
  ligne modifiée et zéro révision ;
- `test_apercu_perime_refuse` — modification concurrente entre l'aperçu et l'application ;
- `test_scrutin_absent_du_fichier_non_touche` et
  `test_scrutin_present_voit_ses_themes_remplaces` — les deux moitiés de D3.16 ;
- `test_historique_reflete_exactement_le_changement` — comparaison du couple (`avant`, `apres`) à un
  attendu écrit à la main, pas à un `is not None`.

**Propriété d'aller-retour**, la réponse à la question laissée ouverte : elle est testée **sur tout le
jeu de données de test et dans les deux formats**, pas sur un échantillon. Le test exporte l'intégralité
des scrutins catégorisés de la fixture, réimporte le résultat, et vérifie trois choses : le plan est
« inchangé » sur toutes les lignes, `scrutin_label` est identique octet pour octet (comparaison des
lignes triées), et `count(*) FROM label_revision` n'a pas bougé. Le jeu de données doit contenir, pour
que la propriété ait un sens : un scrutin à deux thèmes de poids 0,6/0,4, un scrutin sur `autre` sans
position, une position négative, une position nulle, une justification contenant une virgule, un
guillemet et un accent, et un scrutin importé puis relu.

**Ergonomie du back-office**, l'autre question ouverte : pas de navigateur en CI (D2.8 tient). Deux
tests de route suffisent à protéger le chemin clavier — l'un vérifie que la page porte bien un groupe
de boutons radio avec `autofocus` sur le premier et un `input[type=range]` étiqueté par les deux pôles,
l'autre qu'un `POST` valide redirige vers la file et que la page servie ensuite porte un **autre**
scrutin. Le reste se mesure : la recette chronométrée des 30 scrutins est le vrai test, et son résultat
est consigné.

### Mesures à produire, pas à estimer

À exécuter sur la base réelle et à **consigner dans ce plan et dans le message du commit final** :

1. durée du job `label_scrutins_heuristic` sur les 16 957 scrutins, nombre d'estimations écrites, et
   les trois compteurs de refus (couverture, effectif, unanimité) ;
2. distribution de `separation` (déciles) : c'est la mesure qui dit si l'ancrage vaut quelque chose ;
3. `EXPLAIN (ANALYZE, BUFFERS)` sur la requête de la file de travail et sur la liste publique des
   scrutins catégorisés ;
4. p50/p95 de rendu, 20 appels, pour `/admin/categorisation`, `/scrutins`, `/theme/{slug}` et
   `/scrutin/{legislature}/{numero}` — le seuil de 100 ms de la phase 2 s'applique ;
5. **temps réel de catégorisation de 30 scrutins**, chronométré à la main, avec le nombre de scrutins
   passés ;
6. **taux d'accord** entre le signe de `position_pour` mesuré et celui saisi par un humain, sur les
   scrutins relus. À consigner même — surtout — s'il est mauvais, en disant combien de scrutins le
   sous-tendent : sur trente relectures, c'est une indication, pas une conclusion.

> **Mesuré le 19/08/2026** (lot C, commit 11), sur la base réelle peuplée par les phases 1 et 2 puis par
> `label_scrutins_heuristic` avec la grille d'ancrage résolue (F2 — [phase-3.0-feedback.md](phase-3.0-feedback.md)).
> Mesures 5 et 6 non produites : elles exigent une relecture humaine réelle, reportée à une session
> ultérieure — voir la note de suivi en tête du commit 11 ci-dessous.
>
> **1. Job `label_scrutins_heuristic`** : 2 099 ms sur 16 956 scrutins examinés (16 957 attendus par
> l'arbitrage initial, écart d'une unité déjà noté par F2) — 15 621 estimations écrites, refus
> couverture 32, refus effectif 12, refus unanimité 1 291, couverture médiane 99,3 %.
>
> **2. Distribution de `separation`** (déciles, sur les 15 621 estimations) :
>
> | D1 | D2 | D3 | D4 | D5 (médiane) | D6 | D7 | D8 | D9 |
> | --- | --- | --- | --- | --- | --- | --- | --- | --- |
> | 0,624 | 0,737 | 0,820 | 0,887 | 0,933 | 0,961 | 0,980 | 1,000 | 1,000 |
>
> Distribution tassée contre 1 : plus de 20 % des scrutins mesurés ont une séparation parfaite. Attendu
> au vu de F1 — une recherche de seuil libre récompense structurellement l'isolement d'un petit camp
> homogène à une extrémité, et le corpus retenu (participation ≥ 50 %) contient beaucoup de scrutins où
> un petit groupe vote à l'inverse de tous les autres. Une séparation élevée dit que l'axe *explique*
> le partage pour/contre sur ce scrutin, pas que le camp majoritaire est idéologiquement homogène —
> distinction déjà posée par F1, que cette distribution confirme à l'échelle du corpus plutôt que sur un
> seul cas.
>
> **3. `EXPLAIN (ANALYZE, BUFFERS)`** :
>
> - file de travail (`scrutin_a_categoriser`, requête de `get_next_to_categorize`, file vide de tout
>   `scrutin_id` exclu) : **40 à 42 ms**, `Seq Scan` sur `scrutin` (16 956 lignes) suivi d'un
>   `Nested Loop` avec `corpus_parametre`, pas d'index utilisé sur les colonnes calculées de la vue.
>   Sous les 100 ms sans index supplémentaire ; aucun ajouté, conformément à la règle phase 2 (« ne pas
>   ajouter d'index sans que la mesure le justifie »).
> - liste publique des scrutins catégorisés (`list_categorized_scrutins`, sans filtre, page 1) :
>   **16,8 ms** — pire cas mesuré, aucune catégorisation humaine n'existe encore donc `LIMIT 30` ne
>   peut jamais être satisfait tôt et le scan va jusqu'au bout. `Index Scan` sur `scrutin_date_idx`
>   (index déjà posé en phase 2) suivi d'un `Nested Loop Semi Join` avec `Index Only Scan` sur
>   `scrutin_label_pkey`. Sous les 100 ms ; à remesurer une fois des catégorisations réelles présentes,
>   où le comportement pourrait changer dans un sens comme dans l'autre selon leur répartition dans le
>   temps.
>
> **4. p50/p95 de rendu** (20 appels par route après une requête de chauffe, mesuré en mémoire via
> `httpx.ASGITransport` contre l'application réelle connectée à la base réelle — équivalent au serveur
> `uvicorn` en local, sans la latence réseau) :
>
> | Route | p50 | p95 |
> | --- | --- | --- |
> | `/admin/categorisation` | 33,3 ms | 45,9 ms |
> | `/scrutins` | 15,9 ms | 18,6 ms |
> | `/theme/environnement` | 3,8 ms | 4,7 ms |
> | `/scrutin/17/2653` | 4,5 ms | 5,6 ms |
>
> Toutes les routes tiennent largement sous les 100 ms. `/admin/categorisation` est la plus lente,
> cohérent avec le paragraphe 3 : elle exécute la requête de la file de travail à chaque affichage.
> `/scrutins` est plus lente que `/theme/{slug}` pour la même raison — elle interroge tous les thèmes
> pour son filtre en plus de la liste. Aucune de ces quatre pages ne porte de catégorisation humaine
> réelle au moment de la mesure (aucune relecture effectuée) : à remesurer une fois le back-office
> réellement utilisé, ces pages afficheront plus de contenu que leur état vide actuel.

### Ordre des commits

Un commit par ligne, `main` vert (lint, typage, tests) à chaque fois.

**Lot A — le modèle et la mesure**

1. **Migration `0007_categorisation.sql`** seule, avec ses tests de contraintes : somme des poids,
   position et axe, `avant <> apres`, cohérence `method`/`import_id`. Y compris le test qui documente
   le piège des contraintes différées.
2. **`themes.toml` + job `seed_themes`**, avec ses tests d'intégration et l'idempotence.
3. **`group_axis.toml` + `axis.rs` + job `label_scrutins_heuristic`**, tests purs d'abord, intégration
   ensuite, mesures 1 et 2 dans le message de commit.

**Lot B — le back-office**

4. **Authentification** : OAuth GitHub, session, garde CSRF, `admin_user`/`admin_action`, tableau de
   bord minimal, `X-Robots-Tag`. Les tests simulent GitHub par un transport `httpx` de test, sans
   réseau.
5. **File de travail et formulaire** de catégorisation, historique écrit à chaque acte, suppression
   avec motif.
6. **Suivi des jobs** : déplacement des deux gabarits, liste, déclenchement en liste blanche, fragment
   de progression.

**Lot C — le cycle et la publication**

7. **Export** CSV et JSON, `labels_io.py` et ses tests purs.
8. **Import : validation et aperçu**, avec tous les fichiers de cas. Aucune écriture de catégorisation
   dans ce commit — c'est ce qui permet de relire la validation sans se demander ce qu'elle applique.
9. **Import : application**, conflits, transaction, aller-retour, historique.
10. **Pages publiques** (`/themes`, `/theme/{slug}`, `/scrutins`, bloc de catégorisation et page
    d'historique sur la page scrutin), et invariants structurels étendus aux nouvelles routes.
11. **Passe finale** : recette des 30 scrutins chronométrée, mesures 3 à 6 consignées, mise à jour de
    [CLAUDE.md](../../CLAUDE.md) (variables d'environnement admin) et de ce plan si une décision a
    bougé.

[methodology.md](../methodology.md) § 5 est mis à jour **dans le commit 1**, celui de la migration :
c'est là que `label_method` cesse de pouvoir valoir `heuristic`, et un document de référence qui décrit
un mécanisme que le code contredit est exactement ce que CLAUDE.md interdit. C'est la seule modification
hors du périmètre de cette phase. Le plan de la phase 4 porte déjà, lui, l'étape qui accueille
`principal_axis` (D3.10) : elle y a été inscrite au moment de l'arbitrage, pas laissée à la mémoire.

### Vérifications avant de déclarer la phase terminée

1. `make lint`, `make typecheck`, `make test` verts ; CI verte. **Fait**, vérifié à chaque commit.
2. `make ingest` rejoué **en entier sur une base vierge** : `seed_themes` et
   `label_scrutins_heuristic` passent, et une seconde exécution ne change aucune ligne. **Fait le
   19/08/2026** sur une base `kyc_phase3_check` créée pour l'occasion, trois passages — et c'est
   cette vérification qui a trouvé F6 ([phase-3.0-feedback.md](phase-3.0-feedback.md)) : au premier
   passage, une reprise de job après incident réseau a fait calculer les dérivés sur un corpus
   incomplet, sans aucune erreur et avec un code de sortie 0. `make ingest` est découpé en cinq
   étapes depuis. Passages 2 et 3, sans incident : 15 621 estimations, empreintes identiques
   (estimations `fe768380…`, thèmes `438590e5…`), **zéro ligne réécrite** au troisième passage —
   ni un `computed_at` d'estimation, ni un slug. Seuls grandissent les deux journaux (`job`,
   `ingestion_anomaly`), par construction.
3. Le job d'ancrage **refuse** de recharger une version modifiée : vérifié à la main en changeant une
   coordonnée sans changer la version, résultat consigné. **Fait, deux fois** : une fois délibérément
   (`droite` de `0.6` à `0.65`, version inchangée → refus nommant les deux hachages, coordonnée
   restaurée ensuite), une fois accidentellement (une correction de commentaire dans ce même fichier,
   faite entre le chargement initial et ce test, a elle-même changé le hachage — le job a refusé le
   rechargement pour cette raison avant même le test délibéré, confirmant que la garde porte sur le
   fichier entier, commentaires compris, pas seulement sur les coordonnées).
4. **Recette de 30 scrutins réels**, dont la taxe Zucman, catégorisés à la main et chronométrés. Le
   modèle tient-il ? Un scrutin a-t-il exigé un thème absent de la liste ? Consigner les deux réponses.
   **Pas fait** : exige une relecture humaine réelle, voir la note de suivi du commit 11.
5. Une catégorisation saisie, corrigée, puis supprimée : l'historique public raconte exactement les
   trois actes, avec les bons auteurs et les bonnes dates.
6. Un aller-retour export → import réel sur les 30 scrutins de la recette : zéro modification, zéro
   révision.
7. Un import de conflit fabriqué à la main : refus sans confirmation, application avec confirmation, et
   le motif visible dans l'historique.
8. Chaque page admin ouverte **JavaScript désactivé** : la file, l'enregistrement, l'import et son
   aperçu fonctionnent. **Non vérifié — décision assumée du 19/08/2026** : jugé non prioritaire, à
   reprendre sur retour utilisateur. Ce que l'on sait sans navigateur : le formulaire n'exige aucun
   JavaScript par construction (D3.17). Ce que l'on ne sait pas : si un détail de rendu casse le
   parcours réel. La case reste vide plutôt que cochée à tort.
9. Navigation au clavier seul sur la file de saisie, de bout en bout, sans souris. **Non vérifié —
   même décision, même date.** La recette chronométrée (item 4) donnera la réponse en pratique : si
   la saisie au clavier coince, le temps le dira.
10. Déconnecté, chaque route `/admin` rend une redirection ou un 401 — vérifié par un test paramétré
    sur **toutes** les routes du routeur admin, pas par échantillonnage.
11. Aucune route publique ne crée de job ; aucune page publique n'affiche d'estimation automatique.
12. Les mesures sont consignées, et sous les 100 ms. **Fait** — voir la note « Mesuré le 19/08/2026 »
    ci-dessus (mesures 1 à 4 ; 5 et 6 dépendent de la recette humaine, item 4).

> **Point d'étape (19/08/2026, après la vérification 2)** : 1, 2, 3, 11 et 12 sont faits. 5, 7 et 10
> sont couverts par des tests d'intégration automatisés (`test_admin_categorisation.py`,
> `test_import_apply.py`, `test_admin_auth.py`) plutôt que reconfirmés à la main. 8 et 9 sont
> abandonnés par décision explicite (voir ci-dessus) — non prioritaires, à reprendre sur retour
> utilisateur. **Il ne reste que 4 et 6**, qui exigent une relecture humaine réelle : la recette
> chronométrée de 30 scrutins et l'aller-retour export → import sur ces 30 scrutins. Les mesures 5 et
> 6 de la section « Mesures à produire » en dépendent, et elles seules. La phase se déclare terminée
> le jour où cette recette est faite et consignée.

### Hors périmètre — ne pas ajouter

Pas de score ni d'affichage d'orientation sur une fiche personne. Pas d'appel LLM depuis l'application.
Pas de `principal_axis`. Pas d'estimation publiée. Pas d'ingestion des textes de dossiers. Pas de
contribution publique ni de modération. Pas de gestion de comptes admin dans l'interface (la liste
d'autorisation est une variable d'environnement, et elle suffit à trois relecteurs). Pas de
récupération de mot de passe, puisqu'il n'y a pas de mot de passe. Pas de navigateur en CI. Pas de
framework JS. Pas de couleur de parti.

En cas de doute sur ce qu'on a le droit d'afficher ou d'en déduire : demander, ne pas deviner.

## Fini quand

- Un admin peut catégoriser 30 scrutins en moins de 15 minutes sans quitter le clavier, **chronométré**.
- Un export retraité hors ligne se réimporte sans perte, avec prévisualisation, et l'historique montre qui
  a changé quoi.
- Un import ne peut pas écraser silencieusement une catégorisation humaine.
- Un aller-retour export → réimport à l'identique ne modifie aucune ligne et ne crée aucune révision.
- La page publique d'un scrutin affiche sa catégorisation, sa méthode, son auteur et son historique.
- La file de travail ne propose jamais un scrutin hors du corpus retenu, et le seuil qui définit ce
  corpus est une ligne en base qu'on peut lire, pas une constante enfouie dans une requête.
- La mesure `group_alignment` tourne sur l'ensemble du corpus, la distribution de sa séparation est
  documentée, et son taux d'accord avec les catégorisations humaines est mesuré et publié — même s'il est
  mauvais, surtout s'il est mauvais, et avec le nombre de relectures qui le sous-tend.

## Risques

- **Biais d'ancrage sur le relecteur.** Le vrai danger n'est plus que la mesure publie des résultats faux
  (elle ne publie rien, D3.8) : c'est qu'un curseur pré-positionné devienne la réponse par défaut, et que
  le taux d'accord mesuré ne mesure alors que sa propre influence. Trois garde-fous : le thème n'est
  jamais pré-rempli, la séparation est affichée avec la valeur, et le taux d'accord doit être publié
  **avec cette réserve écrite à côté**. Une variante honnête, si le doute persiste : catégoriser une
  dizaine de scrutins sans regarder le repère, et comparer.
- **Justifications dégénérées.** Le champ obligatoire est le poste de coût de la saisie ; sous pression,
  il se remplit de « cf. titre ». Le minimum de dix caractères n'empêche rien. Le seul remède est de
  relire un échantillon de justifications à la fin de la recette et de dire ce qu'on y a trouvé.
- **Volume de travail humain.** ~900 scrutins, soit une grosse journée de travail continu. C'est
  faisable, mais seulement si l'ergonomie tient : elle n'est pas du confort, c'est ce qui rend le projet
  possible.
- **Dérive éditoriale.** Le pouvoir de catégoriser est le pouvoir de conclure. Historique public,
  justification obligatoire et méthodologie écrite sont les seuls garde-fous.
- **L'ancrage des nuances est un objet politique.** On le cite en le datant, on ne s'y soumet pas — mais
  il reste l'entrée d'un calcul qui pré-remplit un formulaire. Que le désaccord se règle par une PR d'une
  ligne n'est pas un détail d'ergonomie : c'est la condition pour que ce choix reste discutable.
