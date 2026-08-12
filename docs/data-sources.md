# Sources de données

Toutes les sources doivent être **ouvertes, officielles ou vérifiables, et citées dans l'UI**. Une donnée
sans source affichable n'entre pas en base.

> Les URLs ci-dessous ont été relevées en août 2026 et doivent être revalidées au moment de
> l'implémentation (voir le spike de la [phase 1](plans/phase-1-ingestion.md)) : l'Assemblée nationale
> réorganise régulièrement ses chemins d'archives.

## 1. Assemblée nationale — open data officiel (source principale)

Portail : <https://data.assemblee-nationale.fr> · Licence Ouverte (Etalab) · formats XML, JSON, parfois CSV,
distribués en archives `.zip`.

### Scrutins (les votes)

Page de référence : <https://data.assemblee-nationale.fr/travaux-parlementaires/votes>

```
https://data.assemblee-nationale.fr/static/openData/repository/{legislature}/loi/scrutins/Scrutins.json.zip
https://data.assemblee-nationale.fr/static/openData/repository/{legislature}/loi/scrutins/Scrutins.xml.zip
```

`{legislature}` vaut `17` pour la législature en cours ; les archives des législatures 15 et 16 sont
disponibles. Chaque archive fournit un fichier par scrutin, contenant le titre, la date, le type
(solennel, ordinaire, motion, déclaration du Gouvernement), les compteurs (votants, pour, contre,
abstentions) et **la position de vote de chaque député, groupe par groupe**.

Un MD5 est publié à côté de chaque archive : à utiliser pour éviter de retraiter une archive inchangée.

Points d'attention connus :

- la position de vote est structurée par groupe puis par votant, avec des cas particuliers
  (`misesAuPoint` — corrections de vote déclarées après coup) qu'il faut décider de suivre ou d'ignorer ;
- les délégations de vote existent : un vote peut être émis pour le compte d'un autre député ;
- « non-votant » recouvre des situations très différentes (présidence de séance, mission, absence) et ne
  doit jamais être lu comme une opinion.

### Acteurs, mandats, organes (AMO) — qui siège, dans quel groupe, à quelles dates

Page de référence : <https://data.assemblee-nationale.fr/acteurs/deputes-en-exercice>

```
.../{legislature}/amo/deputes_actifs_mandats_actifs_organes/AMO10_deputes_actifs_mandats_actifs_organes.json.zip
.../{legislature}/amo/acteurs_mandats_organes_divises/AMO50_acteurs_mandats_organes_divises.json.zip
```

C'est la source des **appartenances aux groupes parlementaires avec dates de début et de fin**, donc la
brique qui permet de reconstruire l'historique « PS de 2015 à 2018, puis LFI ». À vérifier lors du spike :
l'existence et le nom exact du jeu de données *historique* (tous acteurs / tous mandats / tous organes),
nécessaire pour couvrir les dix dernières années et pas seulement la législature en cours.

Distinction importante : un **groupe parlementaire** n'est pas un **parti**. On peut être encarté dans un
parti sans siéger dans le groupe correspondant, et les groupes se renomment ou se recomposent. Le modèle
doit porter les deux notions.

## 2. Wikidata — identité, partis hors Assemblée, photos

Endpoint SPARQL : <https://query.wikidata.org/sparql> · Licence CC0.

Sert à :

- rattacher une personne à un identifiant stable (`QID`) et faire le lien avec d'autres bases ;
- récupérer les **adhésions à un parti** (`P102`) avec leurs qualificatifs de date début/fin — utile pour
  les partis qui ne sont pas des groupes parlementaires ;
- récupérer la **photo** (`P18`), hébergée sur Wikimedia Commons.

Wikidata est collaboratif, donc faillible : les données servent d'appoint et de complément, jamais de
contradiction à l'open data officiel. En cas de désaccord, l'AN gagne.

## 3. Wikimedia Commons — photos des candidat·es

Les images référencées par `P18` sont sur Commons, avec une licence par fichier (souvent CC-BY-SA, parfois
domaine public). Conséquences :

- il faut stocker et **afficher l'auteur et la licence** de chaque photo ;
- on stocke une URL et les métadonnées, pas nécessairement le binaire ;
- une photo sans licence exploitable = pas de photo, on affiche un placeholder.

## 4. Ministère de l'Intérieur — grille des nuances politiques

Le ministère de l'Intérieur publie, à chaque élection, une **grille des nuances politiques** servant à
agréger les résultats. Version 2026 : 26 nuances, regroupées en 6 blocs (extrême gauche, gauche, divers,
centre, droite, extrême droite). La nuance est attribuée par les préfectures, **indépendamment de
l'étiquette revendiquée par le candidat**. Les résultats nuancés sont publiés sur data.gouv.fr et
interieur.gouv.fr, et le Répertoire national des élus (RNE) porte la nuance des élu·es.

C'est la seule source **officielle, publiée et datée** qui positionne les formations politiques sur un axe
gauche-droite. Elle est donc un excellent point d'ancrage pour l'heuristique de la
[phase 3](plans/phase-3-categorisation.md) — bien meilleur qu'un classement que nous inventerions.

Trois réserves à afficher avec la donnée, sans quoi on ferait passer un acte administratif pour une
vérité :

1. **C'est une décision de l'exécutif**, qui classe notamment ses propres opposants. Le classement de LFI
   en « extrême gauche » pour les municipales de 2026, contesté devant le Conseil d'État, en est
   l'illustration : la grille est un objet politique, pas une mesure.
2. **Elle évolue d'une élection à l'autre.** Toute utilisation doit donc mentionner la version et la date
   de la grille employée. Une nuance n'est pas une propriété stable d'un parti.
3. **Elle nuance des candidat·es et des listes, pas des groupes parlementaires.** Le rattachement
   nuance → parti → groupe à l'Assemblée est une jointure que nous établissons, et qui doit être
   documentée comme telle.

Usage retenu : la grille alimente le seed d'ancrage gauche-droite, **chaque ligne citant sa source et sa
date**, et reste modifiable par PR argumentée. On cite, on ne délègue pas.

## 5. Sources secondaires envisagées (hors périmètre v1)

| Source | Apport | Réserve |
| --- | --- | --- |
| NosDéputés.fr (Regards Citoyens) | API pratique, données déjà normalisées, synthèses d'activité | Intermédiaire supplémentaire ; à utiliser pour recouper, pas comme source primaire |
| data.gouv.fr | Miroirs et jeux dérivés | Fraîcheur variable |
| Sénat (open data) | Couvre les candidat·es passé·es par le Sénat | Format et modèle différents, coût non négligeable |
| Parlement européen | Couvre les candidat·es eurodéputé·es | Autre modèle de vote, autre échelle politique |
| HATVP (déclarations d'intérêts) | Conflits d'intérêts | Interprétation délicate, à traiter avec les mêmes garde-fous que la v2 |

## 6. Règles d'ingestion

1. **Archiver le brut avant de parser.** Le payload d'origine va en `source_document` (JSONB) avec son URL
   et sa date de récupération. Quand le format change, on rejoue le parsing sans retélécharger.
2. **Idempotence par clé naturelle.** L'`uid` de l'AN est la clé. Rejouer une ingestion met à jour, ne
   duplique pas, ne supprime pas.
3. **Additivité.** On peut lancer une ingestion sur une seule législature, une période, ou un seuil de
   participation plus bas, sans toucher à ce qui existe déjà.
4. **Traçabilité.** Chaque `ingestion_run` note la source, l'URL, le hash de l'archive, les compteurs et
   les erreurs. Une fiche candidat doit pouvoir remonter jusqu'au fichier qui a produit la donnée.
5. **Politesse.** Un `User-Agent` identifiant le projet avec son URL, pas de parallélisme agressif, respect
   des MD5 pour ne pas retélécharger inutilement. Ce sont des serveurs publics.
