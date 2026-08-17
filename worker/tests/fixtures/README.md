# Fixtures d'ingestion

Extraits d'archives réelles de l'Assemblée nationale, pas des données inventées (voir
[docs/plans/phase-1-ingestion.md](../../../docs/plans/phase-1-ingestion.md), « Fixtures et stratégie
de test »). Budget : moins de 300 Ko au total.

## Contenu

- `scrutins-extrait.zip` — quatre scrutins réels, chacun tronqué à quelques votants (voir « Ce qui a
  changé » ci-dessous), couvrant les cas qui cassent :

  | Fichier | Cas couvert |
  | --- | --- |
  | `VTANR5L15V3415.json` | scrutin ordinaire, plusieurs groupes, mise au point réellement remplie, vote par délégation |
  | `VTANR5L17V501.json` | groupe fantôme `PO0` (plusieurs lignes pour un même scrutin) |
  | `VTCGR5L16V1.json` | scrutin du Congrès : blocs nominatifs au **singulier** (`pour`/`contre`/`abstention`/`nonVotant`), et les 2 acteurs absents du référentiel (`PA720634`, `PA429842`, voir data-sources.md) — ces deux-là votent dans des groupes **sénatoriaux** (`codeType = GROUPESENAT`), une découverte faite en construisant cette fixture : le Congrès mélange des groupes que `ingest_acteurs` ne retient pas (D1.12), qui tombent donc aussi dans le chemin `groupe_fantome` alors qu'ils ne sont pas littéralement `PO0` — comportement correct, pas un bug, mais qui fait que `groupes_fantomes` compte 7 sur cette fixture (5 vrais `PO0` + 2 groupes sénatoriaux), pas 5 |
  | `VTANR5L17V1.json` | compteurs incohérents (déclaré 197+10=207, nominatif volontairement différent après trim) |

- `amo30-extrait.zip` — les organes (`GP`/`PARPOL`/`ASSEMBLEE`) et acteurs cités par les quatre
  scrutins ci-dessus, plus deux acteurs choisis pour exercer `normalize_gp_mandats` :
  - `PA332747` — mandats de groupe qui **s'incluent** (`mandat_inclus`) ;
  - `PA2511` — mandats de groupe qui partagent une **charnière** (`mandat_charniere`).

  Les acteurs et organes sont copiés **tels quels** depuis AMO30 (mandats complets, non tronqués) :
  seuls les votants des scrutins ont été réduits, pas le référentiel.

## Comment les refabriquer

`build_fixtures.py`, dans ce répertoire, télécharge les quatre archives sources et reconstruit les
deux zips ci-dessus. Il documente en code chaque décision de troncature. Pour le rejouer :

```bash
cd /tmp && mkdir fixtures-build && cd fixtures-build
curl -sSL -o scrutins15.zip -A "know-your-candidate/0.1" \
  "https://data.assemblee-nationale.fr/static/openData/repository/15/loi/scrutins/Scrutins_XV.json.zip"
curl -sSL -o scrutins16.zip -A "know-your-candidate/0.1" \
  "https://data.assemblee-nationale.fr/static/openData/repository/16/loi/scrutins/Scrutins.json.zip"
curl -sSL -o scrutins17.zip -A "know-your-candidate/0.1" \
  "https://data.assemblee-nationale.fr/static/openData/repository/17/loi/scrutins/Scrutins.json.zip"
curl -sSL -o amo30.zip -A "know-your-candidate/0.1" \
  "https://data.assemblee-nationale.fr/static/openData/repository/17/amo/tous_acteurs_mandats_organes_xi_legislature/AMO30_tous_acteurs_tous_mandats_tous_organes_historique.json.zip"
python3 /chemin/vers/worker/tests/fixtures/build_fixtures.py
```

Le script écrit `scrutins-extrait.zip` et `amo30-extrait.zip` dans le répertoire courant ; il ne
reste qu'à les recopier ici.

## Ce qui a changé par rapport au brut

Le référentiel (organes, acteurs, mandats) est **intact** — seuls les scrutins sont tronqués, pour
tenir dans le budget de taille (un acteur AMO30 pèse ~30 Ko en moyenne, tout son historique de
mandats inclus ; les garder tous pour les ~1 000 acteurs cités par ces quatre scrutins réels, dont
902 pour le seul Congrès, aurait dépassé 40 Mo) :

- chaque scrutin ne garde que 3 à 5 groupes sur son total réel, et au plus 2 à 3 votants par bloc
  nominatif, en priorisant les votants qui illustrent le cas testé (délégation, cause de non-vote,
  acteur absent du référentiel) ;
- `syntheseVote` (nombre de votants, décompte) est **recalculé** pour rester cohérent avec les
  votants effectivement conservés — sauf sur `VTANR5L17V1`, volontairement laissé tel quel pour
  déclencher l'anomalie `compteurs_incoherents` (sinon les quatre fixtures la déclencheraient toutes,
  par simple effet de troncature, ce qui n'est pas le cas dans les données réelles) ;
- les champs de groupe (`organeRef`, `nombreMembresGroupe`, `decompteVoix`, `positionMajoritaire`)
  ne sont **pas** modifiés : ce sont les compteurs réels de l'Assemblée pour ce groupe, même si tous
  ses votants n'ont pas été conservés dans le fichier.

Les chiffres attendus par les tests d'intégration sur ces fixtures ne sont donc **pas** ceux de
l'ingestion complète (voir data-sources.md pour les vrais chiffres) : c'est le comportement du code
sur chaque cas qui est vérifié, pas un volume.
