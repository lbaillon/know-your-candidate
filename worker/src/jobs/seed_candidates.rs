//! Job `seed_candidates { path?: string }` — voir docs/plans/phase-2-api-ui.md, section « Le seed
//! des candidat·es ». Applique `db/seeds/candidates.toml` : c'est un choix éditorial versionné, pas
//! une ingestion distante — un fichier invalide fait échouer le job **avant toute écriture**,
//! deviner n'a aucun sens pour une donnée qu'on tient à la main.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::Value;
use sqlx::{PgConnection, PgPool};

use crate::run;

use super::JobContext;

/// Résolu à la compilation, relatif à ce crate : indépendant du répertoire depuis lequel le
/// binaire est lancé (`cd worker && cargo run`, comme `make ingest`, ou une autre racine en
/// production).
const DEFAULT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../db/seeds/candidates.toml");

const KNOWN_STATUTS: [&str; 3] = ["declare", "pressenti", "retire"];

// `deny_unknown_fields` sur les deux structures (F4, docs/plans/phase-2.1-fix.md) : une clé
// inconnue — `[[candidats]]` au lieu de `[[candidate]]`, la faute la plus naturelle du monde en
// français — n'est jamais une intention, toujours une faute de frappe. Sans ce drapeau, serde
// l'ignorait silencieusement et `SeedFile::candidates` restait vide via son `#[serde(default)]`,
// ce qui vidait `candidate` sans la moindre erreur.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    #[serde(default, rename = "candidate")]
    candidates: Vec<RawEntry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEntry {
    an_uid: Option<String>,
    wikidata_qid: Option<String>,
    prenom: Option<String>,
    nom: Option<String>,
    statut: String,
    source_url: String,
    source_date: toml::value::Date,
    note: Option<String>,
}

enum Identifiant {
    AnUid(String),
    Wikidata {
        qid: String,
        prenom: String,
        nom: String,
    },
}

struct ValidEntry {
    identifiant: Identifiant,
    statut: String,
    source_url: String,
    source_date: chrono::NaiveDate,
    note: Option<String>,
}

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_PATH.to_string());
    // F4 : vider `candidate` reste possible, mais devient un geste explicite plutôt que l'effet
    // de bord d'un fichier vide ou mal nommé.
    let allow_empty = payload
        .get("allow_empty")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let run_id = run::start(&ctx.pool, "seed_candidates", ctx.job_id, payload.clone()).await?;

    match seed(&ctx.pool, &path, allow_empty).await {
        Ok(counters) => {
            run::finish_ok(&ctx.pool, run_id, counters, None, None).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

/// Séparée de `run` pour permettre aux tests d'intégration de l'appeler directement sur un chemin
/// de fixture — sur le modèle de `ingest_bytes` des autres jobs. Pas de `run_id` : ce job n'écrit
/// aucune anomalie, ses seuls comptes rendus sont les compteurs renvoyés (journalisés par
/// l'appelant via `run::finish_ok`).
///
/// Tout — résolution, upsert, suppression — vit dans une seule transaction (F7,
/// docs/plans/phase-2.1-fix.md) : sans elle, un `an_uid` inconnu en cinquième entrée laissait en
/// base les personnes déjà créées par les quatre premières.
pub async fn seed(pool: &PgPool, path: &str, allow_empty: bool) -> anyhow::Result<Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("lecture de {path} impossible : {err}"))?;
    let entries = parse_and_validate(&raw)?;

    let mut tx = pool.begin().await?;

    if entries.is_empty() && !allow_empty {
        // F4 : un retrait massif ne doit jamais passer inaperçu dans les journaux, y compris
        // quand on le refuse — `candidats_retires` apparaît donc dans le message d'erreur comme
        // dans le succès.
        let candidats_retires: i64 =
            sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM candidate"#)
                .fetch_one(&mut *tx)
                .await?;
        anyhow::bail!(
            "aucune entrée [[candidate]] dans {path} : refus de vider `candidate` \
             (candidats_retires si confirmé : {candidats_retires}).\n\
             Si le retrait de toutes les candidatures est voulu, relancer avec le payload \
             {{\"allow_empty\": true}}."
        );
    }

    let existing_before: HashSet<i64> = sqlx::query_scalar!("SELECT person_id FROM candidate")
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();

    let mut person_ids = Vec::with_capacity(entries.len());
    let mut personnes_creees = 0usize;
    for entry in &entries {
        person_ids.push(resolve_person(&mut tx, &entry.identifiant, &mut personnes_creees).await?);
    }

    // F8 : deux identifiants différents peuvent désigner la même personne (an_uid et
    // wikidata_qid) — la plupart des député·es portent les deux. Refuser explicitement plutôt
    // que de laisser PostgreSQL rejeter l'upsert avec « command cannot affect row a second
    // time », qui ne dit pas ce qui s'est passé.
    reject_duplicate_persons(&entries, &person_ids)?;

    let statuts: Vec<&str> = entries.iter().map(|e| e.statut.as_str()).collect();
    let source_urls: Vec<&str> = entries.iter().map(|e| e.source_url.as_str()).collect();
    let source_dates: Vec<chrono::NaiveDate> = entries.iter().map(|e| e.source_date).collect();
    let notes: Vec<Option<&str>> = entries.iter().map(|e| e.note.as_deref()).collect();

    let result = sqlx::query!(
        r#"
        INSERT INTO candidate (person_id, statut, source_url, source_date, note)
        SELECT person_id, statut::candidate_statut, source_url, source_date, note
        FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::date[], $5::text[])
            AS t(person_id, statut, source_url, source_date, note)
        ON CONFLICT (person_id) DO UPDATE SET
            statut      = EXCLUDED.statut,
            source_url  = EXCLUDED.source_url,
            source_date = EXCLUDED.source_date,
            note        = EXCLUDED.note,
            updated_at  = now()
        WHERE candidate.statut IS DISTINCT FROM EXCLUDED.statut
           OR candidate.source_url IS DISTINCT FROM EXCLUDED.source_url
           OR candidate.source_date IS DISTINCT FROM EXCLUDED.source_date
           OR candidate.note IS DISTINCT FROM EXCLUDED.note
        "#,
        &person_ids,
        &statuts as _,
        &source_urls as _,
        &source_dates,
        &notes as _,
    )
    .execute(&mut *tx)
    .await?;

    let candidats_crees = person_ids
        .iter()
        .filter(|id| !existing_before.contains(id))
        .count();
    let candidats_mis_a_jour = (result.rows_affected() as usize).saturating_sub(candidats_crees);

    // Le seed est la source de vérité de `candidate` (voir plan) : toute personne qui n'est plus
    // dans le fichier n'est plus candidate. Seule suppression autorisée de la phase 2 — elle ne
    // touche ni `person`, ni un vote, ni une donnée ingérée, seulement l'index éditorial.
    let retires = sqlx::query_scalar!(
        "DELETE FROM candidate WHERE person_id <> ALL($1::bigint[]) RETURNING person_id",
        &person_ids,
    )
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(serde_json::json!({
        "entrees_lues": entries.len(),
        "candidats_crees": candidats_crees,
        "candidats_mis_a_jour": candidats_mis_a_jour,
        "candidats_retires": retires.len(),
        "personnes_creees": personnes_creees,
    }))
}

/// Nomme les deux identifiants en cause plutôt que de laisser PostgreSQL échouer sur une
/// contrainte : c'est tout l'intérêt de vérifier ici plutôt que de laisser filer jusqu'à
/// l'upsert (F8).
fn reject_duplicate_persons(entries: &[ValidEntry], person_ids: &[i64]) -> anyhow::Result<()> {
    let mut seen: HashMap<i64, &Identifiant> = HashMap::new();
    for (entry, person_id) in entries.iter().zip(person_ids) {
        if let Some(previous) = seen.insert(*person_id, &entry.identifiant) {
            anyhow::bail!(
                "les identifiants {} et {} désignent la même personne (person_id {person_id}) : \
                 entrées en conflit dans le fichier",
                identifiant_repr(previous),
                identifiant_repr(&entry.identifiant),
            );
        }
    }
    Ok(())
}

fn identifiant_repr(identifiant: &Identifiant) -> String {
    match identifiant {
        Identifiant::AnUid(an_uid) => format!("an_uid={an_uid}"),
        Identifiant::Wikidata { qid, .. } => format!("wikidata_qid={qid}"),
    }
}

async fn resolve_person(
    conn: &mut PgConnection,
    identifiant: &Identifiant,
    personnes_creees: &mut usize,
) -> anyhow::Result<i64> {
    match identifiant {
        Identifiant::AnUid(an_uid) => {
            sqlx::query_scalar!("SELECT id FROM person WHERE an_uid = $1", an_uid)
                .fetch_optional(&mut *conn)
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "an_uid {an_uid} absent du référentiel : AMO30 couvre toutes les \
                         législatures depuis la XIe, une absence signale une faute de frappe"
                    )
                })
        }
        Identifiant::Wikidata { qid, prenom, nom } => {
            if let Some(id) =
                sqlx::query_scalar!("SELECT id FROM person WHERE wikidata_qid = $1", qid)
                    .fetch_optional(&mut *conn)
                    .await?
            {
                return Ok(id);
            }
            let id = sqlx::query_scalar!(
                "INSERT INTO person (wikidata_qid, prenom, nom) VALUES ($1, $2, $3) RETURNING id",
                qid,
                prenom,
                nom,
            )
            .fetch_one(&mut *conn)
            .await?;
            *personnes_creees += 1;
            Ok(id)
        }
    }
}

/// Tout est validé avant que `seed` n'écrive quoi que ce soit : statut connu, source non vide,
/// exactement un des deux identifiants, `prenom`/`nom` obligatoires pour une entrée `wikidata_qid`,
/// aucun doublon d'identifiant.
fn parse_and_validate(raw: &str) -> anyhow::Result<Vec<ValidEntry>> {
    let file: SeedFile =
        toml::from_str(raw).map_err(|err| anyhow::anyhow!("fichier de seed invalide : {err}"))?;

    let mut seen_an_uids = HashSet::new();
    let mut seen_qids = HashSet::new();
    let mut out = Vec::with_capacity(file.candidates.len());

    for (index, entry) in file.candidates.into_iter().enumerate() {
        let position = index + 1;

        if !KNOWN_STATUTS.contains(&entry.statut.as_str()) {
            anyhow::bail!("entrée {position} : statut inconnu {:?}", entry.statut);
        }
        if entry.source_url.trim().is_empty() {
            anyhow::bail!("entrée {position} : source_url vide");
        }

        let identifiant = match (entry.an_uid, entry.wikidata_qid) {
            (Some(an_uid), None) => {
                if !seen_an_uids.insert(an_uid.clone()) {
                    anyhow::bail!("an_uid en double dans le fichier : {an_uid}");
                }
                Identifiant::AnUid(an_uid)
            }
            (None, Some(qid)) => {
                if !seen_qids.insert(qid.clone()) {
                    anyhow::bail!("wikidata_qid en double dans le fichier : {qid}");
                }
                let prenom = entry
                    .prenom
                    .filter(|p| !p.trim().is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!("entrée {position} (wikidata_qid={qid}) : prenom requis")
                    })?;
                let nom = entry.nom.filter(|n| !n.trim().is_empty()).ok_or_else(|| {
                    anyhow::anyhow!("entrée {position} (wikidata_qid={qid}) : nom requis")
                })?;
                Identifiant::Wikidata { qid, prenom, nom }
            }
            (Some(_), Some(_)) => anyhow::bail!(
                "entrée {position} : an_uid et wikidata_qid sont mutuellement exclusifs"
            ),
            (None, None) => anyhow::bail!("entrée {position} : ni an_uid ni wikidata_qid"),
        };

        let source_date = chrono::NaiveDate::from_ymd_opt(
            i32::from(entry.source_date.year),
            u32::from(entry.source_date.month),
            u32::from(entry.source_date.day),
        )
        .ok_or_else(|| anyhow::anyhow!("entrée {position} : source_date invalide"))?;

        out.push(ValidEntry {
            identifiant,
            statut: entry.statut,
            source_url: entry.source_url,
            source_date,
            note: entry.note,
        });
    }

    Ok(out)
}
