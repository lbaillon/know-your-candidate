//! Job `seed_candidates { path?: string }` — voir docs/plans/phase-2-api-ui.md, section « Le seed
//! des candidat·es ». Applique `db/seeds/candidates.toml` : c'est un choix éditorial versionné, pas
//! une ingestion distante — un fichier invalide fait échouer le job **avant toute écriture**,
//! deviner n'a aucun sens pour une donnée qu'on tient à la main.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::run;

use super::JobContext;

/// Résolu à la compilation, relatif à ce crate : indépendant du répertoire depuis lequel le
/// binaire est lancé (`cd worker && cargo run`, comme `make ingest`, ou une autre racine en
/// production).
const DEFAULT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../db/seeds/candidates.toml");

const KNOWN_STATUTS: [&str; 3] = ["declare", "pressenti", "retire"];

#[derive(Deserialize)]
struct SeedFile {
    #[serde(default, rename = "candidate")]
    candidates: Vec<RawEntry>,
}

#[derive(Deserialize)]
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

    let run_id = run::start(&ctx.pool, "seed_candidates", ctx.job_id, payload.clone()).await?;

    match seed(&ctx.pool, &path).await {
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
pub async fn seed(pool: &PgPool, path: &str) -> anyhow::Result<Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("lecture de {path} impossible : {err}"))?;
    let entries = parse_and_validate(&raw)?;

    let existing_before: HashSet<i64> = sqlx::query_scalar!("SELECT person_id FROM candidate")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();

    let mut person_ids = Vec::with_capacity(entries.len());
    let mut personnes_creees = 0usize;
    for entry in &entries {
        person_ids.push(resolve_person(pool, &entry.identifiant, &mut personnes_creees).await?);
    }

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
    .execute(pool)
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
    .fetch_all(pool)
    .await?;

    Ok(serde_json::json!({
        "entrees_lues": entries.len(),
        "candidats_crees": candidats_crees,
        "candidats_mis_a_jour": candidats_mis_a_jour,
        "candidats_retires": retires.len(),
        "personnes_creees": personnes_creees,
    }))
}

async fn resolve_person(
    pool: &PgPool,
    identifiant: &Identifiant,
    personnes_creees: &mut usize,
) -> anyhow::Result<i64> {
    match identifiant {
        Identifiant::AnUid(an_uid) => {
            sqlx::query_scalar!("SELECT id FROM person WHERE an_uid = $1", an_uid)
                .fetch_optional(pool)
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
                    .fetch_optional(pool)
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
            .fetch_one(pool)
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
