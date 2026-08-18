# Phase 2.1 — Corrections des pages publiques

**Statut : 📝 à relire** · Dépend de : phase 2 · Bloque : phase 3

## Objectif

Corriger les dix défauts relevés à la revue de la phase 2. **Aucune fonctionnalité nouvelle** : les pages
restent ce qu'elles sont, elles doivent juste cesser d'affirmer des choses que nos données ne soutiennent
pas.

Les trois bloquants ne sont pas des bugs d'ingénierie. Ce sont **des affirmations fausses affichées sur
des personnes réelles et identifiables en contexte électoral** (F1, F2) et **une erreur d'accessibilité
bloquante** (F3) que le « Fini quand » de la [phase 2](phase-2-api-ui.md) excluait explicitement. Sur ce
projet, ce sont les défauts les plus graves possibles : ils ne cassent rien, ils mentent.

Il faut noter *quand* ils sont apparus. Tant que `candidate` était vide, les formulations de F1 et F2
étaient vraies. Elles sont devenues fausses le jour où le seed a contenu de vraies personnes — c'est
exactement ce que la phase 2 promettait de produire : « regarder les vraies données à l'écran révèle des
problèmes de modèle qu'aucune relecture de plan ne détecte ». Le plan a tenu sa promesse contre
lui-même, et **c'est aussi le plan qu'il faut corriger**, pas seulement les gabarits (voir F2).

Cette section est autoportante : **tout ce qui est nécessaire est ici, dans
[phase-2-api-ui.md](phase-2-api-ui.md), dans [methodology.md](../methodology.md), dans
[data-sources.md](../data-sources.md) ou dans [CLAUDE.md](../../CLAUDE.md)**, il n'y a pas de contexte de
conversation à retrouver. Développement sur le tronc, commits directs sur `main`, un commit par correctif
ou par groupe cohérent, `main` vert (lint, typage, tests) à chaque commit.

Chaque diagnostic ci-dessous a été **reproduit** : soit par une requête sur la base réelle, soit en
rendant la page par HTTP, soit par lecture du code quand c'est indiqué. Les causes sont établies, pas
supposées. Si l'une se révèle fausse au contact du code, le dire plutôt que de contourner.

## Ce que la revue a validé — ne pas y toucher

Le gros de la phase 2 est bon et ne doit pas être retouché :

- **l'architecture des requêtes** : tout le SQL dans `queries/`, aucune requête dans un routeur ou un
  gabarit, pages et API servies par les **mêmes** fonctions et les mêmes modèles Pydantic. C'est ce qui
  garantit qu'elles ne divergent pas, et le test qui les compare doit rester ;
- **la migration `0005`** (immuable de toute façon : toute correction passe par une nouvelle migration
  numérotée), y compris le choix « dernier groupe connu » plutôt qu'un `CURRENT_DATE` figé au
  rafraîchissement ;
- **`timeline.py`** comme fonction pure avec `today` injecté, son placement glouton en rangées et ses
  tests. Le calcul est correct sur les chevauchements réels ;
- **le cache HTTP** : `ETag` fort et `304` sur revalidation, vérifiés pour de vrai en HTTP (seul le cas
  `HEAD` manque, F9) ;
- **les mesures de performance** consignées dans le plan de la phase 2 : elles ont été réellement prises,
  elles restent la référence et ne sont pas remises en cause par ce lot ;
- **l'architecture CSS** et son contrôle de fraîcheur, y compris l'ajout du contrôle en CI que le plan
  avait manqué ;
- **le `alt=""` des photos** : la décision est juste et bien commentée, le nom de la personne étant écrit
  juste à côté. C'est le **lien** qui l'entoure qui est fautif, pas l'attribut (F3). Ne pas « corriger »
  l'`alt` ;
- **le contenu du seed** : les cinq entrées et leurs sources ont été vérifiées en ouvrant réellement les
  pages citées. Ne pas les modifier, sauf le commentaire d'en-tête (F10).

---

## Décisions arbitrées

Elles sont tranchées. La session qui implémente ne les rediscute pas ; si l'une se révèle fausse au
contact du code, elle le dit plutôt que de la contourner.

| # | Question | Décision |
| --- | --- | --- |
| D2.12 | Comment nommer l'annuaire, qui contient des non-député·es ? | **Renommer la route en `/personnes`**, cohérente avec `/personne/{slug}`. **Aucune redirection depuis `/deputes`** : le site n'a jamais été déployé (la phase 5 n'a pas eu lieu), aucun lien public n'existe, une redirection serait une machinerie sans usage. Voir F1 |
| D2.13 | Que peut-on affirmer d'une personne sans mandat ni vote ? | **Rien sur le monde, tout sur nos données.** Le gabarit ne dit jamais « n'a jamais siégé » mais « nous n'avons aucun mandat pour cette personne », en nommant la couverture du corpus. Nos mandats commencent à la XIe législature et nos votes à la 15e : une absence n'est pas une preuve d'absence. Voir F2 |
| D2.14 | Comment corriger le lien de photo sans nom accessible ? | **Supprimer le lien**, garder l'image nue. Le nom cliquable est immédiatement à côté et mène au même endroit ; un second lien vers la même cible n'apporte rien et doit être annoncé. Pas de `aria-hidden` ni de `tabindex="-1"`, qui masqueraient le symptôme en gardant un lien inutile dans le DOM. Voir F3 |
| D2.15 | Comment rendre la recherche insensible aux accents ? | **Colonne normalisée dans `person_apercu`** (`kyc_unaccent(lower(...))`) et index trigramme dessus, dans une migration `0006` qui recrée la vue. Pas de `unaccent()` appliqué à la volée des deux côtés de la comparaison : ce serait non indexable, donc un balayage complet déguisé en correctif. Voir F5 et F6 |
| D2.16 | Que signifie `source_date` ? | **La date de l'acte** (déclaration, désignation), pas celle de l'article qui le rapporte. C'est l'acte que la fiche affiche ; l'article n'est que la preuve. Le commentaire d'en-tête du seed dit aujourd'hui le contraire et c'est lui qui a tort. Voir F10 |

---

## Bloquants

### F1 — L'annuaire affirme que des non-député·es sont député·es

**Fichiers** : `backend/src/kyc_api/templates/directory.html.jinja:3,6,8`,
`backend/src/kyc_api/routers/pages.py:28-29`, `backend/src/kyc_api/routers/fragments.py:28-29`,
`backend/src/kyc_api/templates/base.html.jinja:16`

**Symptôme, reproduit** : la page porte `<title>Annuaire des député·es</title>`, un `<h1>Annuaire des
personnes ingérées</h1>` qui le contredit, et une phrase d'introduction « Toute personne dont les données
ont été ingérées depuis l'Assemblée nationale ». Or :

```
curl -s "http://127.0.0.1:8000/deputes?q=arthaud" | grep person-card__name
  → <a class="person-card__name" href="/personne/nathalie-arthaud">Nathalie Arthaud</a>
```

Nathalie Arthaud a été **créée par `seed_candidates` depuis un QID Wikidata**, n'a jamais été élue, et
n'a jamais été ingérée depuis l'Assemblée nationale. Trois éléments de la page affirment donc d'elle
quelque chose de faux : l'URL, le `<title>` et l'introduction. Le `<title>` est le plus grave — c'est lui
qui part dans l'onglet, le favori, le partage et l'indexation.

C'est un manquement direct à [methodology.md](../methodology.md) § 1 (« on n'affiche que des faits
vérifiables et sourcés ») sur une personne réelle en contexte électoral.

**Correctif** :

1. route `/deputes` → **`/personnes`**, et `/fragments/deputes` → **`/fragments/personnes`** (D2.12) ;
2. `<title>` : « Annuaire des personnes — Know Your Candidate » ;
3. `<h1>` : « Annuaire des personnes » — le `<title>` et le `<h1>` doivent enfin dire la même chose ;
4. introduction, qui doit couvrir les deux provenances réelles :

   > Toutes les personnes présentes dans nos données : celles issues du référentiel de l'Assemblée
   > nationale, et les candidat·es à la présidentielle 2027 que nous suivons, qu'elles aient siégé ou
   > non. Voir [la méthodologie](/methodologie).

5. lien de navigation de `base.html.jinja` : `href="/personnes"`, libellé « Annuaire » (inchangé) ;
6. `action="/personnes"` du formulaire de recherche, et les `hx-get` correspondants ;
7. mettre à jour le tableau des livrables de [phase-2-api-ui.md](phase-2-api-ui.md) et la décision D2.2
   **dans le même commit** (CLAUDE.md, règle 3) : le plan écrit `/deputes` partout.

**Vérification** : un test qui insère une personne **sans `an_uid`** (donc hors référentiel AN), lui donne
un slug, rafraîchit `person_apercu`, et vérifie qu'elle apparaît bien dans `/personnes` **et** qu'aucun
des mots « député » ou « ingérées depuis l'Assemblée » n'apparaît dans le `<title>` ni dans le `<h1>` de
la page. Vérifier aussi qu'aucun `/deputes` ne subsiste dans le dépôt (`grep -rn "/deputes" backend/ docs/`).

### F2 — « n'a jamais siégé à l'Assemblée nationale » affirme sur le monde, pas sur nos données

**Fichiers** : `backend/src/kyc_api/templates/person.html.jinja:43-46`,
`backend/src/kyc_api/templates/macros/timeline.html.jinja:3-5`,
`backend/src/kyc_api/routers/pages.py:67`, et
[phase-2-api-ui.md](phase-2-api-ui.md), section « Points de conception »

**Symptôme, reproduit** : la fiche affiche « Cette personne n'a jamais siégé à l'Assemblée nationale »
dès que `has_ever_sat` est faux, et la frise affiche la même chose dès qu'elle n'a aucun segment. Or la
phrase est une affirmation **sur le monde**, que nos données ne peuvent pas soutenir :

```sql
SELECT (SELECT count(*) FROM person WHERE an_uid IS NOT NULL) AS persons_an,
       (SELECT count(DISTINCT person_id) FROM mandat)          AS avec_mandat;
  → persons_an = 3119 | avec_mandat = 2122
```

**997 personnes issues du référentiel de l'Assemblée n'ont aucun mandat en base** et reçoivent donc cette
phrase. Le cas est visible dans le seed lui-même : Bruno Retailleau (`PA2538`) a zéro mandat en base et sa
fiche affirme qu'il n'a jamais siégé, alors que Wikidata lui enregistre une fonction en 1994-1997 —
c'est-à-dire **juste avant la fenêtre de notre référentiel**, qui commence à la XIe législature
([data-sources.md](../data-sources.md), AMO30). Une liste de mandats vide ne prouve donc rien : elle dit
seulement que nous n'avons rien.

Le même raisonnement vaut pour une personne créée par le seed : son absence du référentiel signifie
qu'elle n'a pas siégé **depuis la XIe législature**, pas qu'elle n'a jamais siégé.

**Correctif** — remplacer les deux formulations, en nommant à chaque fois la couverture réelle :

- frise vide (`macros/timeline.html.jinja`) :

  > Nous n'avons aucun mandat parlementaire pour cette personne. Notre référentiel couvre les
  > législatures XI et suivantes : un mandat antérieur n'y figurerait pas.

- votes, quand la personne n'a aucun mandat de député·e (`person.html.jinja`) :

  > Nous n'avons aucun vote personnel pour cette personne. Notre corpus couvre les législatures 15 à 17
  > (depuis 2017) ; un vote antérieur n'y figurerait pas.

Les deux branches de l'état vide des votes (`not has_ever_sat` et `not recent_votes`) disent désormais la
même chose au fond — **les fusionner** : la distinction `has_ever_sat` ne portait que la formulation
fautive et n'a plus de raison d'être. Supprimer le calcul de `has_ever_sat` dans `routers/pages.py` s'il
ne sert plus qu'à cela.

**Corriger aussi le plan** : [phase-2-api-ui.md](phase-2-api-ui.md), « Points de conception », donne la
phrase fautive en exemple d'état vide réussi. C'est de là qu'elle vient. La remplacer par la nouvelle
formulation et ajouter une ligne disant pourquoi — sinon la prochaine phase la réintroduira de bonne foi.

**Vérification** : un test qui crée une personne **avec** `an_uid`, sans mandat ni vote, et vérifie que sa
fiche ne contient nulle part la chaîne « jamais siégé », et qu'elle contient bien la mention de la
couverture du corpus. Un second test sur une personne **sans** `an_uid` : même exigence.

### F3 — Chaque carte de personne porte un lien sans nom accessible

**Fichier** : `backend/src/kyc_api/templates/macros/person_card.html.jinja:4-15`

**Symptôme, reproduit** sur `/` par extraction du DOM rendu : **5 liens sans nom accessible**, un par
carte — et 30 par page sur l'annuaire, qui en affiche 30. Le lien enveloppe soit une image en `alt=""`,
soit un placeholder en `aria-hidden="true"` :

```html
<a class="person-card__photo-link" href="/personne/nathalie-arthaud">
  <span class="person-card__photo person-card__photo--placeholder" aria-hidden="true"></span>
</a>
```

Un lecteur d'écran annonce « lien » sans aucune destination, trente fois de suite. C'est un échec WCAG
2.4.4 (*Link Purpose*) et 4.1.2 (*Name, Role, Value*), donc une « erreur d'accessibilité bloquante » au
sens du « Fini quand » de la phase 2.

`test_html_invariants.py` ne l'a pas vu parce qu'il vérifie que tout `<img>` porte un `alt` — jamais qu'un
lien porte un nom. **C'est la brèche qu'il faut refermer, pas seulement le symptôme.**

**Correctif** :

1. supprimer le `<a class="person-card__photo-link">` et garder l'image (ou le placeholder) nue (D2.14).
   Le `alt=""` reste, avec son commentaire : il est correct ;
2. ajuster le CSS `.person-card__photo-link` de `40-components.css`, devenu sans objet ;
3. **ajouter l'invariant manquant** à `test_html_invariants.py`, paramétré sur toutes les routes comme les
   autres : tout `<a href>` doit avoir un nom accessible, c'est-à-dire du texte non vide, ou un
   descendant `<img>` à `alt` non vide, ou un `aria-label`, `aria-labelledby` ou `title`.

**Vérification** : le nouvel invariant échoue sur le gabarit actuel et passe après correction — le
vérifier dans cet ordre, sinon rien ne prouve que le test teste quelque chose.

---

## Importants

### F4 — Une faute de frappe dans le seed vide silencieusement la table `candidate`

**Fichier** : `worker/src/jobs/seed_candidates.rs:23-27,131-137`

**Symptôme** : `SeedFile` ne porte pas `deny_unknown_fields` et son champ est `#[serde(default)]`. Écrire
`[[candidats]]` au lieu de `[[candidate]]` — un pluriel, la faute la plus naturelle du monde en
français — donne donc **zéro entrée sans la moindre erreur**, puis :

```sql
DELETE FROM candidate WHERE person_id <> ALL(ARRAY[]::bigint[]);
  → DELETE 5   -- vérifié sur la base réelle, dans une transaction annulée
```

La table est vidée, et le job **rend un succès** avec `entrees_lues: 0`. C'est la destruction silencieuse
d'une donnée éditoriale non régénérable : exactement la classe de défaut que le plan de la
[phase 3](phase-3-categorisation.md) désigne comme la plus coûteuse du projet, arrivée une phase plus tôt
que prévu.

**Correctif** :

1. `#[serde(deny_unknown_fields)]` sur `SeedFile` **et** sur `RawEntry` — une clé inconnue est une faute
   de frappe, jamais une intention ;
2. refuser un fichier sans aucune entrée :

   ```
   aucune entrée [[candidate]] dans <chemin> : refus de vider `candidate`.
   Si le retrait de toutes les candidatures est voulu, relancer avec le payload {"allow_empty": true}.
   ```

   Le drapeau `allow_empty` est lu depuis le payload du job, à côté de `path`. Vider la table reste
   possible, mais devient un geste explicite ;
3. le compte rendu du job doit faire apparaître `candidats_retires` dans le message d'erreur comme dans le
   succès — un retrait massif ne doit jamais passer inaperçu dans les journaux.

**Vérification** : trois tests d'intégration — un fichier avec `[[candidats]]` échoue sans écrire ; un
fichier vide échoue sans écrire ; le même fichier vide avec `allow_empty: true` vide bien la table. Le
premier doit vérifier explicitement que les lignes préexistantes **sont toujours là** après l'échec.

### F5 — La recherche est sensible aux accents

**Fichiers** : `backend/src/kyc_api/queries/persons.py:79-85` (`_DIRECTORY_WHERE`) et
`backend/src/kyc_api/queries/candidates.py:17-22`

**Symptôme, reproduit** :

```sql
SELECT count(*) FROM person_apercu
WHERE (coalesce(prenom,'') || ' ' || coalesce(nom,'')) ILIKE '%melenchon%';  → 0
                                                        ILIKE '%mélenchon%'; → 1
```

Sur un site français, la majorité des visiteurs tapent sans accent. La recherche est donc en défaut sur
tous les noms accentués — et c'est l'annuaire, seul moyen de parcourir les 3 120 personnes, qui en pâtit
le plus. Touche l'accueil, l'annuaire et le paramètre `?q=` de l'API.

**Correctif** — migration `0006_recherche.sql`, avec F6 (D2.15) :

```sql
CREATE EXTENSION IF NOT EXISTS unaccent;

-- `unaccent()` est STABLE et non IMMUTABLE (son dictionnaire peut être rechargé), donc inutilisable
-- telle quelle dans un index. Fixer le dictionnaire explicitement lève l'ambiguïté et rend
-- l'enveloppe déclarable IMMUTABLE : c'est la forme recommandée par la documentation PostgreSQL.
CREATE FUNCTION kyc_unaccent(text) RETURNS text
    AS $$ SELECT public.unaccent('public.unaccent'::regdictionary, $1) $$
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE;
```

Puis **recréer `person_apercu`** (PostgreSQL ne connaît pas `CREATE OR REPLACE MATERIALIZED VIEW`) avec
une colonne normalisée en plus :

```sql
DROP MATERIALIZED VIEW person_apercu;
CREATE MATERIALIZED VIEW person_apercu AS
SELECT  -- … définition de la migration 0005, recopiée telle quelle …
       kyc_unaccent(lower(coalesce(p.prenom, '') || ' ' || coalesce(p.nom, ''))) AS recherche
FROM person p
-- … le reste à l'identique …
```

**Recopier la définition de `0005` mot pour mot** et n'ajouter que cette colonne : ce n'est pas
l'occasion de l'améliorer, et toute divergence involontaire serait invisible.

Recréer **les quatre index** (une `DROP MATERIALIZED VIEW` emporte les siens) :

```sql
CREATE UNIQUE INDEX person_apercu_pk_idx   ON person_apercu (person_id);
CREATE UNIQUE INDEX person_apercu_slug_idx ON person_apercu (slug);
CREATE INDEX person_apercu_votes_idx       ON person_apercu (votes_total DESC);
CREATE INDEX person_apercu_recherche_idx   ON person_apercu USING gin (recherche gin_trgm_ops);
```

L'index unique sur `person_id` n'est pas décoratif : `REFRESH MATERIALIZED VIEW CONCURRENTLY` l'exige, et
`refresh_views` cesserait de fonctionner sans lui.

Côté requêtes, les deux filtres deviennent, à l'identique dans `persons.py` et `candidates.py` :

```sql
$1::text IS NULL OR pa.recherche LIKE '%' || kyc_unaccent(lower($1)) || '%'
```

`LIKE` et non `ILIKE` : la colonne est déjà en minuscules et sans accent, la casse est traitée à
l'écriture plutôt qu'à chaque lecture.

**Note d'exécution** : recréer la vue la repeuple immédiatement (`WITH DATA` par défaut), ce qui recalcule
les agrégats sur 2,3 M de votes. Compter quelques secondes à quelques dizaines de secondes sur la base
réelle — c'est attendu, pas un blocage.

**Vérification** : un test qui insère une personne nommée « Mélenchon » et vérifie que `q=melenchon`,
`q=MELENCHON` et `q=mélenchon` la trouvent tous les trois, sur l'accueil, sur l'annuaire et sur l'API.

### F6 — Les deux index trigrammes ne servent à rien

**Fichiers** : `db/migrations/0002_referentiel.sql:38`, `db/migrations/0005_pages.sql:54-58`

**Symptôme, reproduit** : `person_nom_trgm_idx` et `person_recherche_trgm_idx` sont posés sur `person`,
alors que **toutes** les recherches interrogent `person_apercu`. Le planificateur le confirme :

```
EXPLAIN … person_apercu … ILIKE '%dupont%'
  → Seq Scan on person_apercu pa
```

Sans conséquence à 3 120 lignes, mais deux index morts coûtent à chaque écriture, et surtout **le
commentaire de la migration `0005` affirme que l'index sert la recherche** : il est faux, et c'est le
genre d'affirmation qu'on croit sur parole six mois plus tard.

**Correctif** : dans la même migration `0006`, `DROP INDEX person_recherche_trgm_idx;` et
`DROP INDEX person_nom_trgm_idx;`. L'index utile est celui posé par F5, là où la requête regarde. Si un
besoin de recherche directe sur `person` apparaît un jour, il se reposera à ce moment-là, avec sa requête.

**Vérification** : `EXPLAIN` sur la recherche de l'annuaire montre désormais un parcours d'index
(`Bitmap Index Scan on person_apercu_recherche_idx`), et plus un `Seq Scan`. Le consigner dans le message
de commit.

---

## Mineurs

### F7 — `seed_candidates` n'est pas transactionnel

**Fichier** : `worker/src/jobs/seed_candidates.rs:80-152`

**Symptôme** : le module affirme en tête qu'« un fichier invalide fait échouer le job **avant toute
écriture** ». C'est vrai de la validation du fichier, faux de la résolution : `resolve_person` insère les
personnes une par une, donc un `an_uid` inconnu en cinquième entrée laisse en base les personnes créées
par les entrées précédentes. Ces personnes apparaissent ensuite dans l'annuaire, sans mandat ni vote.
L'upsert et la suppression finale ne sont pas non plus dans la même unité.

Sévérité faible parce que c'est auto-réparant : au rejeu, `resolve_person` retrouve la personne par son
QID plutôt que d'en créer une seconde. Mais la promesse écrite n'est pas tenue.

**Correctif** : ouvrir une transaction en tête de `seed`, faire passer résolution, upsert et suppression
dessus, et ne committer qu'à la fin. Les signatures de `resolve_person` prennent alors un
`&mut PgConnection` plutôt qu'un `&PgPool`.

**Vérification** : un test avec un fichier dont la dernière entrée porte un `an_uid` inconnu et dont une
entrée précédente est une personne Wikidata inédite — après l'échec, `SELECT count(*) FROM person WHERE
wikidata_qid = …` doit rendre 0.

### F8 — Deux identifiants désignant la même personne font échouer le job sur une erreur brute

**Fichier** : `worker/src/jobs/seed_candidates.rs:193-260` (`parse_and_validate`)

**Symptôme** : la validation refuse deux `an_uid` identiques et deux QID identiques, mais pas une entrée
par `an_uid` et une autre par le `wikidata_qid` **de la même personne** — le cas est réaliste, la plupart
des député·es portant les deux identifiants (Mélenchon : `PA2150` et `Q5829`). Les deux entrées résolvent
alors le même `person_id`, et PostgreSQL rejette l'`INSERT … ON CONFLICT DO UPDATE` avec
`command cannot affect row a second time`. Le job échoue — ce qui est le bon comportement — mais avec un
message qui ne dit pas ce qui s'est passé, là où toute la validation vise justement le refus explicite.

**Correctif** : après résolution et **avant** l'upsert, vérifier l'unicité des `person_id` résolus et
échouer en nommant les deux identifiants en cause. Ce contrôle vit naturellement dans la transaction de
F7.

**Vérification** : un test avec les deux entrées contradictoires, qui attend un message citant les deux
identifiants et une table inchangée.

### F9 — `HEAD` ne renvoie ni `ETag` ni `Cache-Control`

**Fichier** : `backend/src/kyc_api/http_cache.py:44`

**Symptôme** : le middleware sort immédiatement sur `request.method != "GET"`. Or Starlette route
automatiquement `HEAD` vers les routes déclarées en `GET` : une requête `HEAD /` renvoie donc `200` sans
en-tête de cache, alors que HTTP exige qu'un `HEAD` porte les mêmes en-têtes que le `GET` correspondant.
Sans conséquence pour un navigateur, mais un cache intermédiaire ou une sonde peuvent s'y fier — et la
phase 5 en met un devant.

**Correctif** : accepter `GET` **et** `HEAD`.

**Vérification** : un test qui compare l'`ETag` d'un `HEAD` et d'un `GET` sur la même route et les exige
identiques. Vérifier au passage que le corps d'un `HEAD` reste vide.

### F10 — `source_date` a deux sens

**Fichiers** : `db/seeds/candidates.toml:3-4`, et la ligne `PA2538`

**Symptôme** : l'en-tête du seed définit `source_date` comme « la date à laquelle cette source a été
publiée ». Quatre entrées sur cinq respectent cette définition parce que l'acte et l'article tombent le
même jour. La cinquième ne le peut pas : Bruno Retailleau a été désigné le 19 avril 2026 et le communiqué
du parti est daté du 20 — la ligne porte `2026-04-19`, c'est-à-dire la date de l'acte, contre sa propre
documentation.

Sur un projet dont la règle est qu'une date affichée doit être exacte, un champ à deux sens est un défaut
en soi : c'est celui que la fiche affiche.

**Correctif** (D2.16) : réécrire le commentaire d'en-tête —

> `source_date` : la date de **l'acte** (déclaration, désignation), pas celle de l'article qui le
> rapporte. C'est l'acte que la fiche affiche ; l'article n'en est que la preuve. Quand les deux
> diffèrent, le dire dans `note`.

Ne changer **aucune valeur** : les cinq sont déjà justes sous cette définition. Vérifier que le libellé
affiché sur la fiche et dans l'API dit bien « déclaration du … » ou « désignation du … » et non « source
publiée le … » ; l'ajuster si besoin.

**Vérification** : lecture. Pas de test — c'est une définition, pas un comportement.

---

## Ordre des commits

Chaque commit laisse `main` vert (lint, typage, tests).

1. **F3** — suppression du lien de photo **et** ajout de l'invariant « tout lien a un nom accessible ».
   À faire en premier : l'invariant garde toutes les routes que les commits suivants vont toucher.
2. **F1** — renommage `/deputes` → `/personnes`, titres, introduction, navigation, formulaire, et mise à
   jour du plan de la phase 2 dans le même commit.
3. **F2** — reformulation des deux états vides, fusion des branches, suppression de `has_ever_sat` s'il
   devient inutile, et correction de la section « Points de conception » du plan de la phase 2.
4. **F5 + F6** — migration `0006`, requêtes de recherche, suppression des index morts. Les trois se
   tiennent, les séparer ferait passer `main` par un état où l'index existe sans son usage.
5. **F4 + F7 + F8** — durcissement de `seed_candidates` : `deny_unknown_fields`, refus du fichier vide,
   transaction, unicité des `person_id` résolus. Même fichier, même sujet.
6. **F9 + F10** — `HEAD` dans le middleware, définition de `source_date`.
7. **Passe finale** — relancer `make ingest` sur une base vierge, reprendre les mesures des routes
   touchées (l'annuaire change de requête de recherche), et consigner les chiffres dans le message de
   commit et dans le plan de la phase 2, comme l'a fait `3a33ec0`.

## Vérifications avant de déclarer la phase 2.1 terminée

1. `make lint`, `make typecheck`, `make test` verts en local, **CI verte sur `main`** — vérifiée sur
   GitHub Actions, pas déduite du local.
2. `grep -rn "/deputes" backend/ docs/` ne rend plus rien hors de ce plan et de l'historique.
3. `grep -rn "jamais siégé" backend/ docs/` ne rend plus rien hors de ce plan : la phrase a disparu des
   gabarits **et** du plan de la phase 2.
4. Le nouvel invariant de nom accessible **échoue** si on remet le lien de photo — vérifié une fois, puis
   le lien retiré pour de bon.
5. `q=melenchon` sans accent trouve Jean-Luc Mélenchon sur l'accueil, sur l'annuaire et sur l'API, et
   `EXPLAIN` montre un parcours d'index.
6. `make ingest` rejoué en entier sur une base vierge : les trois jobs de la phase 2 passent, la
   migration `0006` se rejoue sans erreur sur une base déjà peuplée, et `refresh_views` fonctionne
   toujours (c'est le test de l'index unique recréé).
7. Un fichier de seed avec `[[candidats]]` fait **échouer** le job sans toucher la table.
8. Les fiches des cinq candidat·es du seed sont ouvertes une par une et relues : aucune phrase n'y affirme
   quoi que ce soit qui ne soit pas dans nos données. **Consigner cette relecture dans le message du
   commit final** — c'est la vérification qui compte le plus dans ce lot, et elle ne s'automatise pas.
9. Aucune route ne crée de job ; aucun échafaudage de développement n'est réapparu.

## Note d'outillage

F4, F7 et F8 touchent des requêtes `sqlx::query!` de `seed_candidates.rs`, et F5 change la définition de
`person_apercu`, donc les types de toute requête qui la lit. Après **chaque** modification, régénérer les
métadonnées avec `cargo sqlx prepare -- --all-targets` — sans `--all-targets`, les requêtes des tests
d'intégration perdent leurs entrées `.sqlx` et la CI casse en mode hors ligne.

Côté Python, la migration `0006` est rejouée par la fixture de session de `backend/tests/conftest.py`,
qui repart d'un schéma vide : rien à faire de particulier. Rappel du piège déjà documenté en phase 2 —
les tests rafraîchissent `person_apercu` **sans** `CONCURRENTLY`, qui ne peut pas s'exécuter dans une
transaction.
