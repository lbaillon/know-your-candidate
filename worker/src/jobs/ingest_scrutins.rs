//! Job `ingest_scrutins { legislature, since?, force_refetch? }` — voir
//! docs/plans/phase-1-ingestion.md. Ordre d'exécution : après `ingest_acteurs`, qui porte le
//! référentiel des personnes et des groupes ; si un vote cite un acteur inconnu, une `person`
//! réduite à son `an_uid` est créée ici, on ne perd pas un vote parce que le référentiel est en
//! retard.

use std::collections::{HashMap, HashSet};
use std::io::Read as _;

use chrono::NaiveDate;
use serde_json::Value;
use sqlx::PgPool;

use crate::an::document::{RawDocument, archive_documents, fetch_latest_document_ids};
use crate::an::http::{AnClient, sha256_hex};
use crate::an::open_zip;
use crate::an::scrutin::{self, ParsedGroupeVentilation, ParsedScrutin};
use crate::anomaly::{self, AnomalyRecord};
use crate::run;

use super::JobContext;
use super::queue;

const BASE_URL: &str = "https://data.assemblee-nationale.fr/static/openData/repository/";
/// Rapporter la progression tous les ~500 scrutins (voir plan, « Job ingest_scrutins »).
const PROGRESS_EVERY: usize = 500;
/// Nombre de lignes par lot `UNNEST` — largement sous la limite de paramètres bind de Postgres
/// (65535) même pour la table `vote`, la plus large en colonnes.
const INSERT_BATCH: usize = 2000;
/// Les trois seules causes de non-vote connues (voir data-sources.md) : président de séance,
/// président de l'Assemblée, membre du Gouvernement. Un code inédit est une anomalie, pas une
/// erreur d'ingestion.
const KNOWN_CAUSES_NON_VOTE: [&str; 3] = ["PSE", "PAN", "MG"];

/// La 14e législature n'a pas de détail nominatif sur la moitié de ses scrutins (D1.7) : elle
/// n'est pas ingérable en l'état et reste hors périmètre v1.
fn scrutin_url(legislature: i16) -> anyhow::Result<String> {
    let path = match legislature {
        17 => "17/loi/scrutins/Scrutins.json.zip",
        16 => "16/loi/scrutins/Scrutins.json.zip",
        15 => "15/loi/scrutins/Scrutins_XV.json.zip",
        other => anyhow::bail!(
            "législature {other} non ingérable : seules 15, 16 et 17 publient un détail nominatif complet (D1.7)"
        ),
    };
    Ok(format!("{BASE_URL}{path}"))
}

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let legislature = payload
        .get("legislature")
        .and_then(Value::as_i64)
        .ok_or_else(|| anyhow::anyhow!("paramètre requis manquant : legislature"))?
        as i16;
    let since = payload
        .get("since")
        .and_then(Value::as_str)
        .map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d"))
        .transpose()?;
    let force_refetch = payload
        .get("force_refetch")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let run_id = run::start(&ctx.pool, "an_scrutins", ctx.job_id, payload.clone()).await?;

    match ingest(ctx, run_id, legislature, since, force_refetch).await {
        Ok((counters, url, content_hash)) => {
            run::finish_ok(&ctx.pool, run_id, counters, &url, Some(&content_hash)).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

struct IngestStats {
    scrutins: usize,
    votes: usize,
    mises_au_point: usize,
    acteurs_inconnus: usize,
    groupes_fantomes: usize,
    blocs_nominatifs_inconnus: usize,
    compteurs_incoherents: usize,
}

async fn ingest(
    ctx: &JobContext,
    run_id: i64,
    legislature: i16,
    since: Option<NaiveDate>,
    force_refetch: bool,
) -> anyhow::Result<(Value, String, String)> {
    let url = scrutin_url(legislature)?;
    let client = AnClient::new(cache_dir())?;
    let archive = client.fetch_zip(&url, force_refetch).await?;

    tracing::info!(
        legislature,
        content_hash = %archive.content_hash,
        from_cache = archive.from_cache,
        "archive des scrutins téléchargée"
    );

    let counters =
        ingest_bytes(ctx, run_id, legislature, since, &url, archive.bytes.clone()).await?;
    Ok((counters, url, archive.content_hash))
}

/// Traitement pur d'une archive de scrutins déjà en mémoire — séparé de `ingest` pour permettre
/// aux tests d'intégration de rejouer un extrait versionné sans réseau (voir
/// `worker/tests/fixtures/README.md`). `url` ne sert qu'à l'archivage du brut (`source_document`).
pub async fn ingest_bytes(
    ctx: &JobContext,
    run_id: i64,
    legislature: i16,
    since: Option<NaiveDate>,
    url: &str,
    bytes: bytes::Bytes,
) -> anyhow::Result<Value> {
    let pool = &ctx.pool;
    let docs = tokio::task::spawn_blocking(move || read_scrutin_docs(bytes, since)).await??;

    archive_documents(pool, "an_scrutin", url, docs.iter().map(|(raw, _)| raw)).await?;
    let document_ids = fetch_latest_document_ids(pool, "an_scrutin").await?;

    let mut person_ids = fetch_person_ids(pool).await?;
    let unknown_uids = resolve_unknown_acteurs(pool, &docs, &mut person_ids).await?;
    let acteurs_inconnus = unknown_uids.len();
    let acteur_anomalies: Vec<AnomalyRecord> = unknown_uids
        .into_iter()
        .map(|uid| AnomalyRecord {
            kind: anomaly::ACTEUR_INCONNU,
            subject_uid: Some(uid),
            detail: serde_json::json!({}),
        })
        .collect();
    anomaly::record_many(pool, run_id, &acteur_anomalies).await?;
    let organe_ids = fetch_organe_ids(pool).await?;

    let mut stats = IngestStats {
        scrutins: docs.len(),
        votes: 0,
        mises_au_point: 0,
        acteurs_inconnus,
        groupes_fantomes: 0,
        blocs_nominatifs_inconnus: 0,
        compteurs_incoherents: 0,
    };

    let total = docs.len();
    for (processed, chunk) in docs.chunks(PROGRESS_EVERY).enumerate() {
        write_chunk(
            pool,
            run_id,
            chunk,
            &document_ids,
            &person_ids,
            &organe_ids,
            &mut stats,
        )
        .await?;

        let done = (processed * PROGRESS_EVERY + chunk.len()).min(total);
        if total > 0 {
            queue::set_progress(
                ctx,
                done as f32 / total as f32,
                &format!("{done}/{total} scrutins"),
            )
            .await?;
        }
    }

    Ok(serde_json::json!({
        "legislature": legislature,
        "scrutins": stats.scrutins,
        "votes": stats.votes,
        "mises_au_point": stats.mises_au_point,
        "acteurs_inconnus": stats.acteurs_inconnus,
        "groupes_fantomes": stats.groupes_fantomes,
        "blocs_nominatifs_inconnus": stats.blocs_nominatifs_inconnus,
        "compteurs_incoherents": stats.compteurs_incoherents,
    }))
}

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("AN_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| Some(std::path::PathBuf::from(".cache")))
}

/// Décompression et parsing — travail CPU, hors de l'exécuteur async (voir `ingest_acteurs`, même
/// raison). Le filtre `since` s'applique après parsing : c'est `dateScrutin` qui décide, pas le
/// nom de fichier.
fn read_scrutin_docs(
    bytes: bytes::Bytes,
    since: Option<NaiveDate>,
) -> anyhow::Result<Vec<(RawDocument, ParsedScrutin)>> {
    let mut zip = open_zip(bytes)?;
    let mut out = Vec::new();

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_string();
        if !name.starts_with("json/") || !name.ends_with(".json") {
            continue;
        }

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let payload: Value = serde_json::from_slice(&buf)?;
        let Some(parsed) = scrutin::parse_scrutin(&payload) else {
            continue;
        };
        if let Some(since) = since
            && parsed.date_scrutin < since
        {
            continue;
        }

        let content_hash = sha256_hex(&buf);
        out.push((
            RawDocument {
                uid: parsed.an_uid.clone(),
                payload,
                content_hash,
            },
            parsed,
        ));
    }

    Ok(out)
}

async fn fetch_person_ids(pool: &PgPool) -> sqlx::Result<HashMap<String, i64>> {
    let rows = sqlx::query!("SELECT an_uid, id FROM person")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.an_uid, r.id)).collect())
}

async fn fetch_organe_ids(pool: &PgPool) -> sqlx::Result<HashMap<String, i64>> {
    let rows = sqlx::query!("SELECT an_uid, id FROM organe")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.an_uid, r.id)).collect())
}

/// Un vote qui cite un `acteurRef` absent du référentiel ne doit pas faire perdre le vote (voir
/// « Ordre d'exécution » du plan) : une `person` réduite à son `an_uid` est créée,
/// `ON CONFLICT DO NOTHING` pour ne jamais écraser une fiche déjà enrichie par `ingest_acteurs`.
async fn resolve_unknown_acteurs(
    pool: &PgPool,
    docs: &[(RawDocument, ParsedScrutin)],
    person_ids: &mut HashMap<String, i64>,
) -> anyhow::Result<Vec<String>> {
    let mut missing: HashSet<&str> = HashSet::new();
    for (_, scrutin) in docs {
        for groupe in &scrutin.groupes {
            for vote in &groupe.votes {
                if !person_ids.contains_key(&vote.acteur_uid) {
                    missing.insert(&vote.acteur_uid);
                }
            }
        }
        for map in &scrutin.mise_au_point {
            if !person_ids.contains_key(&map.acteur_uid) {
                missing.insert(&map.acteur_uid);
            }
        }
    }

    if missing.is_empty() {
        return Ok(Vec::new());
    }

    let uids: Vec<&str> = missing.into_iter().collect();
    for batch in uids.chunks(INSERT_BATCH) {
        sqlx::query!(
            r#"
            INSERT INTO person (an_uid)
            SELECT * FROM UNNEST($1::text[])
            ON CONFLICT (an_uid) DO NOTHING
            "#,
            batch as _,
        )
        .execute(pool)
        .await?;
    }

    for batch in uids.chunks(INSERT_BATCH) {
        let rows = sqlx::query!(
            "SELECT an_uid, id FROM person WHERE an_uid = ANY($1::text[])",
            batch as _,
        )
        .fetch_all(pool)
        .await?;
        for row in rows {
            person_ids.insert(row.an_uid, row.id);
        }
    }

    Ok(uids.into_iter().map(String::from).collect())
}

/// Le mandat de siège (`ASSEMBLEE`) ne sert qu'à distinguer un titulaire d'un suppléant (D. spike) ;
/// c'est le mandat de **groupe** qui comble un `organeRef` fantôme. Choisit le mandat dont la date
/// de début est la plus tardive quand plusieurs couvrent la date (plan, étape `ingest_scrutins`) —
/// règle inchangée par F8, seulement passée d'une requête par vote fantôme à une requête pour tout
/// le lot courant (voir docs/plans/phase-1.1-fix.md, F8). Un couple sans ligne dans le résultat vaut
/// « aucun mandat » : absent de la `HashMap` rendue, jamais une entrée à `None`.
async fn resolve_groupes_from_mandats(
    pool: &PgPool,
    keys: &HashSet<(i64, NaiveDate)>,
) -> sqlx::Result<HashMap<(i64, NaiveDate), i64>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let person_ids: Vec<i64> = keys.iter().map(|(person_id, _)| *person_id).collect();
    let dates: Vec<NaiveDate> = keys.iter().map(|(_, date)| *date).collect();

    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT ON (t.person_id, t.d) t.person_id AS "person_id!", t.d AS "d!", m.organe_id
        FROM UNNEST($1::bigint[], $2::date[]) AS t(person_id, d)
        JOIN mandat m ON m.person_id = t.person_id AND m.type_organe = 'GP' AND m.period @> t.d
        ORDER BY t.person_id, t.d, lower(m.period) DESC
        "#,
        &person_ids,
        &dates,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ((r.person_id, r.d), r.organe_id))
        .collect())
}

/// Couples `(person_id, date_scrutin)` dont le groupe est à reconstituer depuis les mandats pour ce
/// lot : une ligne de groupe fantôme, par vote. Collecté en amont de la boucle d'écriture pour ne
/// résoudre qu'en une seule requête (F8).
fn mandat_lookup_keys(
    chunk: &[(RawDocument, ParsedScrutin)],
    person_ids: &HashMap<String, i64>,
    organe_ids: &HashMap<String, i64>,
) -> HashSet<(i64, NaiveDate)> {
    let mut keys = HashSet::new();
    for (_, parsed) in chunk {
        for groupe in &parsed.groupes {
            if !is_groupe_fantome(groupe, organe_ids) {
                continue;
            }
            for vote in &groupe.votes {
                if let Some(&person_id) = person_ids.get(&vote.acteur_uid) {
                    keys.insert((person_id, parsed.date_scrutin));
                }
            }
        }
    }
    keys
}

fn is_groupe_fantome(groupe: &ParsedGroupeVentilation, organe_ids: &HashMap<String, i64>) -> bool {
    groupe
        .organe_uid
        .as_deref()
        .is_none_or(|u| u == "PO0" || !organe_ids.contains_key(u))
}

struct ScrutinRow<'a> {
    an_uid: &'a str,
    numero: i32,
    legislature: i16,
    date_scrutin: NaiveDate,
    chambre: &'static str,
    organe_id: Option<i64>,
    session_ref: Option<&'a str>,
    seance_ref: Option<&'a str>,
    type_code: &'a str,
    type_libelle: Option<&'a str>,
    type_majorite: Option<&'a str>,
    sort_code: Option<&'a str>,
    titre: &'a str,
    demandeur: Option<&'a str>,
    dossier_ref: Option<&'a str>,
    mode_publication: &'a str,
    lieu_vote: Option<&'a str>,
    nombre_votants: i32,
    suffrages_exprimes: i32,
    pour: i32,
    contre: i32,
    abstentions: i32,
    non_votants: i32,
    non_votants_volontaires: i32,
    effectif: i32,
    source_document_id: i64,
}

struct GroupeRow {
    scrutin_an_uid: String,
    rang: i16,
    organe_id: Option<i64>,
    nombre_membres: i32,
    position_majoritaire: Option<String>,
    pour: i32,
    contre: i32,
    abstentions: i32,
    non_votants: i32,
}

struct VoteRow {
    scrutin_an_uid: String,
    person_id: i64,
    position: &'static str,
    cause_non_vote: Option<String>,
    groupe_organe_id: Option<i64>,
    groupe_from_mandat: bool,
    mandat_an_uid: Option<String>,
    par_delegation: bool,
}

struct MiseAuPointRow {
    scrutin_an_uid: String,
    person_id: i64,
    position_declaree: &'static str,
    source_document_id: i64,
}

#[allow(clippy::too_many_arguments)]
async fn write_chunk(
    pool: &PgPool,
    run_id: i64,
    chunk: &[(RawDocument, ParsedScrutin)],
    document_ids: &HashMap<String, i64>,
    person_ids: &HashMap<String, i64>,
    organe_ids: &HashMap<String, i64>,
    stats: &mut IngestStats,
) -> anyhow::Result<()> {
    let mut scrutin_rows = Vec::with_capacity(chunk.len());
    let mut groupe_rows = Vec::new();
    let mut vote_rows = Vec::new();
    let mut mise_au_point_rows = Vec::new();
    let mut anomalies: Vec<AnomalyRecord> = Vec::new();

    // Résolu en une seule requête pour tout le lot (F8), plutôt qu'une requête par vote de groupe
    // fantôme dans la boucle d'écriture ci-dessous.
    let resolved_from_mandat =
        resolve_groupes_from_mandats(pool, &mandat_lookup_keys(chunk, person_ids, organe_ids))
            .await?;

    for (_, parsed) in chunk {
        let Some(&source_document_id) = document_ids.get(&parsed.an_uid) else {
            anyhow::bail!(
                "document source manquant pour {} après archivage",
                parsed.an_uid
            );
        };

        let total_nominatif = parsed.total_nominatif();
        if total_nominatif != parsed.nombre_votants + parsed.non_votants {
            stats.compteurs_incoherents += 1;
            anomalies.push(AnomalyRecord {
                kind: anomaly::COMPTEURS_INCOHERENTS,
                subject_uid: Some(parsed.an_uid.clone()),
                detail: serde_json::json!({
                    "total_nominatif": total_nominatif,
                    "nombre_votants": parsed.nombre_votants,
                    "non_votants": parsed.non_votants,
                }),
            });
        }

        scrutin_rows.push(ScrutinRow {
            an_uid: &parsed.an_uid,
            numero: parsed.numero,
            legislature: parsed.legislature,
            date_scrutin: parsed.date_scrutin,
            chambre: parsed.chambre.as_db_str(),
            organe_id: parsed
                .organe_uid
                .as_deref()
                .and_then(|u| organe_ids.get(u))
                .copied(),
            session_ref: parsed.session_ref.as_deref(),
            seance_ref: parsed.seance_ref.as_deref(),
            type_code: &parsed.type_code,
            type_libelle: parsed.type_libelle.as_deref(),
            type_majorite: parsed.type_majorite.as_deref(),
            sort_code: parsed.sort_code.as_deref(),
            titre: &parsed.titre,
            demandeur: parsed.demandeur.as_deref(),
            dossier_ref: parsed.dossier_ref.as_deref(),
            mode_publication: &parsed.mode_publication,
            lieu_vote: parsed.lieu_vote.as_deref(),
            nombre_votants: parsed.nombre_votants,
            suffrages_exprimes: parsed.suffrages_exprimes,
            pour: parsed.pour,
            contre: parsed.contre,
            abstentions: parsed.abstentions,
            non_votants: parsed.non_votants,
            non_votants_volontaires: parsed.non_votants_volontaires,
            effectif: parsed.effectif(),
            source_document_id,
        });

        for (rang, groupe) in parsed.groupes.iter().enumerate() {
            if !groupe.blocs_inconnus.is_empty() {
                stats.blocs_nominatifs_inconnus += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::BLOC_NOMINATIF_INCONNU,
                    subject_uid: Some(parsed.an_uid.clone()),
                    detail: serde_json::json!({"blocs": groupe.blocs_inconnus}),
                });
            }

            let is_fantome = is_groupe_fantome(groupe, organe_ids);
            if is_fantome {
                stats.groupes_fantomes += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::GROUPE_FANTOME,
                    subject_uid: Some(parsed.an_uid.clone()),
                    detail: serde_json::json!({"organe_uid": groupe.organe_uid, "rang": rang}),
                });
            }
            let groupe_organe_id = groupe
                .organe_uid
                .as_deref()
                .and_then(|u| organe_ids.get(u))
                .copied();

            groupe_rows.push(GroupeRow {
                scrutin_an_uid: parsed.an_uid.clone(),
                rang: rang as i16,
                organe_id: groupe_organe_id,
                nombre_membres: groupe.nombre_membres,
                position_majoritaire: groupe.position_majoritaire.clone(),
                pour: groupe.pour,
                contre: groupe.contre,
                abstentions: groupe.abstentions,
                non_votants: groupe.non_votants,
            });

            for vote in &groupe.votes {
                let Some(&person_id) = person_ids.get(&vote.acteur_uid) else {
                    anyhow::bail!("personne inconnue après résolution : {}", vote.acteur_uid);
                };

                let (resolved_organe_id, groupe_from_mandat) = if is_fantome {
                    // F4 : `groupe_from_mandat` ne vaut `true` que si un mandat a effectivement
                    // été trouvé — un couple absent de `resolved_from_mandat` vaut « aucun
                    // mandat », pas une reconstitution qui aurait abouti.
                    let mandat_organe_id = resolved_from_mandat
                        .get(&(person_id, parsed.date_scrutin))
                        .copied();
                    (mandat_organe_id, mandat_organe_id.is_some())
                } else {
                    (groupe_organe_id, false)
                };

                if let Some(cause) = &vote.cause_non_vote
                    && !KNOWN_CAUSES_NON_VOTE.contains(&cause.as_str())
                {
                    anomalies.push(AnomalyRecord {
                        kind: anomaly::CAUSE_NON_VOTE_INCONNUE,
                        subject_uid: Some(parsed.an_uid.clone()),
                        detail: serde_json::json!({
                            "acteur_uid": vote.acteur_uid,
                            "cause": cause,
                        }),
                    });
                }

                vote_rows.push(VoteRow {
                    scrutin_an_uid: parsed.an_uid.clone(),
                    person_id,
                    position: vote.position.as_db_str(),
                    cause_non_vote: vote.cause_non_vote.clone(),
                    groupe_organe_id: resolved_organe_id,
                    groupe_from_mandat,
                    mandat_an_uid: vote.mandat_uid.clone(),
                    par_delegation: vote.par_delegation,
                });
            }
        }

        for map in &parsed.mise_au_point {
            let Some(&person_id) = person_ids.get(&map.acteur_uid) else {
                anyhow::bail!("personne inconnue après résolution : {}", map.acteur_uid);
            };
            mise_au_point_rows.push(MiseAuPointRow {
                scrutin_an_uid: parsed.an_uid.clone(),
                person_id,
                position_declaree: map.position_declaree,
                source_document_id,
            });
        }
    }

    stats.votes += vote_rows.len();
    stats.mises_au_point += mise_au_point_rows.len();

    upsert_scrutins(pool, &scrutin_rows).await?;
    let scrutin_ids = fetch_scrutin_ids(
        pool,
        &scrutin_rows.iter().map(|s| s.an_uid).collect::<Vec<_>>(),
    )
    .await?;

    upsert_groupes(pool, &groupe_rows, &scrutin_ids).await?;
    upsert_votes(pool, &vote_rows, &scrutin_ids).await?;
    upsert_mise_au_point(pool, &mise_au_point_rows, &scrutin_ids).await?;
    anomaly::record_many(pool, run_id, &anomalies).await?;

    Ok(())
}

async fn upsert_scrutins(pool: &PgPool, rows: &[ScrutinRow<'_>]) -> sqlx::Result<()> {
    for batch in rows.chunks(INSERT_BATCH) {
        let an_uids: Vec<&str> = batch.iter().map(|r| r.an_uid).collect();
        let numeros: Vec<i32> = batch.iter().map(|r| r.numero).collect();
        let legislatures: Vec<i16> = batch.iter().map(|r| r.legislature).collect();
        let dates: Vec<NaiveDate> = batch.iter().map(|r| r.date_scrutin).collect();
        let chambres: Vec<&str> = batch.iter().map(|r| r.chambre).collect();
        let organe_ids: Vec<Option<i64>> = batch.iter().map(|r| r.organe_id).collect();
        let session_refs: Vec<Option<&str>> = batch.iter().map(|r| r.session_ref).collect();
        let seance_refs: Vec<Option<&str>> = batch.iter().map(|r| r.seance_ref).collect();
        let type_codes: Vec<&str> = batch.iter().map(|r| r.type_code).collect();
        let type_libelles: Vec<Option<&str>> = batch.iter().map(|r| r.type_libelle).collect();
        let type_majorites: Vec<Option<&str>> = batch.iter().map(|r| r.type_majorite).collect();
        let sort_codes: Vec<Option<&str>> = batch.iter().map(|r| r.sort_code).collect();
        let titres: Vec<&str> = batch.iter().map(|r| r.titre).collect();
        let demandeurs: Vec<Option<&str>> = batch.iter().map(|r| r.demandeur).collect();
        let dossier_refs: Vec<Option<&str>> = batch.iter().map(|r| r.dossier_ref).collect();
        let modes: Vec<&str> = batch.iter().map(|r| r.mode_publication).collect();
        let lieux: Vec<Option<&str>> = batch.iter().map(|r| r.lieu_vote).collect();
        let nombre_votants: Vec<i32> = batch.iter().map(|r| r.nombre_votants).collect();
        let suffrages: Vec<i32> = batch.iter().map(|r| r.suffrages_exprimes).collect();
        let pours: Vec<i32> = batch.iter().map(|r| r.pour).collect();
        let contres: Vec<i32> = batch.iter().map(|r| r.contre).collect();
        let abstentions: Vec<i32> = batch.iter().map(|r| r.abstentions).collect();
        let non_votants: Vec<i32> = batch.iter().map(|r| r.non_votants).collect();
        let non_votants_vol: Vec<i32> = batch.iter().map(|r| r.non_votants_volontaires).collect();
        let effectifs: Vec<i32> = batch.iter().map(|r| r.effectif).collect();
        let source_document_ids: Vec<i64> = batch.iter().map(|r| r.source_document_id).collect();

        sqlx::query!(
            r#"
            INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, organe_id,
                                  session_ref, seance_ref, type_code, type_libelle, type_majorite,
                                  sort_code, titre, demandeur, dossier_ref, mode_publication,
                                  lieu_vote, nombre_votants, suffrages_exprimes, pour, contre,
                                  abstentions, non_votants, non_votants_volontaires, effectif,
                                  source_document_id)
            SELECT an_uid, numero, legislature, date_scrutin, chambre::chambre, organe_id,
                   session_ref, seance_ref, type_code, type_libelle, type_majorite, sort_code,
                   titre, demandeur, dossier_ref, mode_publication, lieu_vote, nombre_votants,
                   suffrages_exprimes, pour, contre, abstentions, non_votants,
                   non_votants_volontaires, effectif, source_document_id
            FROM UNNEST(
                $1::text[], $2::int[], $3::smallint[], $4::date[], $5::text[], $6::bigint[],
                $7::text[], $8::text[], $9::text[], $10::text[], $11::text[], $12::text[],
                $13::text[], $14::text[], $15::text[], $16::text[], $17::text[], $18::int[],
                $19::int[], $20::int[], $21::int[], $22::int[], $23::int[], $24::int[],
                $25::int[], $26::bigint[]
            ) AS t(an_uid, numero, legislature, date_scrutin, chambre, organe_id, session_ref,
                   seance_ref, type_code, type_libelle, type_majorite, sort_code, titre,
                   demandeur, dossier_ref, mode_publication, lieu_vote, nombre_votants,
                   suffrages_exprimes, pour, contre, abstentions, non_votants,
                   non_votants_volontaires, effectif, source_document_id)
            ON CONFLICT (an_uid) DO UPDATE SET
                numero = EXCLUDED.numero,
                legislature = EXCLUDED.legislature,
                date_scrutin = EXCLUDED.date_scrutin,
                chambre = EXCLUDED.chambre,
                organe_id = EXCLUDED.organe_id,
                session_ref = EXCLUDED.session_ref,
                seance_ref = EXCLUDED.seance_ref,
                type_code = EXCLUDED.type_code,
                type_libelle = EXCLUDED.type_libelle,
                type_majorite = EXCLUDED.type_majorite,
                sort_code = EXCLUDED.sort_code,
                titre = EXCLUDED.titre,
                demandeur = EXCLUDED.demandeur,
                dossier_ref = EXCLUDED.dossier_ref,
                mode_publication = EXCLUDED.mode_publication,
                lieu_vote = EXCLUDED.lieu_vote,
                nombre_votants = EXCLUDED.nombre_votants,
                suffrages_exprimes = EXCLUDED.suffrages_exprimes,
                pour = EXCLUDED.pour,
                contre = EXCLUDED.contre,
                abstentions = EXCLUDED.abstentions,
                non_votants = EXCLUDED.non_votants,
                non_votants_volontaires = EXCLUDED.non_votants_volontaires,
                effectif = EXCLUDED.effectif,
                source_document_id = EXCLUDED.source_document_id,
                updated_at = now()
            WHERE scrutin.numero IS DISTINCT FROM EXCLUDED.numero
               OR scrutin.legislature IS DISTINCT FROM EXCLUDED.legislature
               OR scrutin.date_scrutin IS DISTINCT FROM EXCLUDED.date_scrutin
               OR scrutin.chambre IS DISTINCT FROM EXCLUDED.chambre
               OR scrutin.organe_id IS DISTINCT FROM EXCLUDED.organe_id
               OR scrutin.session_ref IS DISTINCT FROM EXCLUDED.session_ref
               OR scrutin.seance_ref IS DISTINCT FROM EXCLUDED.seance_ref
               OR scrutin.type_code IS DISTINCT FROM EXCLUDED.type_code
               OR scrutin.type_libelle IS DISTINCT FROM EXCLUDED.type_libelle
               OR scrutin.type_majorite IS DISTINCT FROM EXCLUDED.type_majorite
               OR scrutin.sort_code IS DISTINCT FROM EXCLUDED.sort_code
               OR scrutin.titre IS DISTINCT FROM EXCLUDED.titre
               OR scrutin.demandeur IS DISTINCT FROM EXCLUDED.demandeur
               OR scrutin.dossier_ref IS DISTINCT FROM EXCLUDED.dossier_ref
               OR scrutin.mode_publication IS DISTINCT FROM EXCLUDED.mode_publication
               OR scrutin.lieu_vote IS DISTINCT FROM EXCLUDED.lieu_vote
               OR scrutin.nombre_votants IS DISTINCT FROM EXCLUDED.nombre_votants
               OR scrutin.suffrages_exprimes IS DISTINCT FROM EXCLUDED.suffrages_exprimes
               OR scrutin.pour IS DISTINCT FROM EXCLUDED.pour
               OR scrutin.contre IS DISTINCT FROM EXCLUDED.contre
               OR scrutin.abstentions IS DISTINCT FROM EXCLUDED.abstentions
               OR scrutin.non_votants IS DISTINCT FROM EXCLUDED.non_votants
               OR scrutin.non_votants_volontaires IS DISTINCT FROM EXCLUDED.non_votants_volontaires
               OR scrutin.effectif IS DISTINCT FROM EXCLUDED.effectif
               OR scrutin.source_document_id IS DISTINCT FROM EXCLUDED.source_document_id
            "#,
            &an_uids as _,
            &numeros,
            &legislatures,
            &dates,
            &chambres as _,
            &organe_ids as _,
            &session_refs as _,
            &seance_refs as _,
            &type_codes as _,
            &type_libelles as _,
            &type_majorites as _,
            &sort_codes as _,
            &titres as _,
            &demandeurs as _,
            &dossier_refs as _,
            &modes as _,
            &lieux as _,
            &nombre_votants,
            &suffrages,
            &pours,
            &contres,
            &abstentions,
            &non_votants,
            &non_votants_vol,
            &effectifs,
            &source_document_ids,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn fetch_scrutin_ids(pool: &PgPool, an_uids: &[&str]) -> sqlx::Result<HashMap<String, i64>> {
    let mut out = HashMap::new();
    for batch in an_uids.chunks(INSERT_BATCH) {
        let rows = sqlx::query!(
            "SELECT an_uid, id FROM scrutin WHERE an_uid = ANY($1::text[])",
            batch as _,
        )
        .fetch_all(pool)
        .await?;
        out.extend(rows.into_iter().map(|r| (r.an_uid, r.id)));
    }
    Ok(out)
}

async fn upsert_groupes(
    pool: &PgPool,
    rows: &[GroupeRow],
    scrutin_ids: &HashMap<String, i64>,
) -> anyhow::Result<()> {
    for batch in rows.chunks(INSERT_BATCH) {
        let scrutin_id: Vec<i64> = batch
            .iter()
            .map(|r| scrutin_ids[&r.scrutin_an_uid])
            .collect();
        let rangs: Vec<i16> = batch.iter().map(|r| r.rang).collect();
        let organe_id: Vec<Option<i64>> = batch.iter().map(|r| r.organe_id).collect();
        let nombre_membres: Vec<i32> = batch.iter().map(|r| r.nombre_membres).collect();
        let position_majoritaire: Vec<Option<&str>> = batch
            .iter()
            .map(|r| r.position_majoritaire.as_deref())
            .collect();
        let pours: Vec<i32> = batch.iter().map(|r| r.pour).collect();
        let contres: Vec<i32> = batch.iter().map(|r| r.contre).collect();
        let abstentions: Vec<i32> = batch.iter().map(|r| r.abstentions).collect();
        let non_votants: Vec<i32> = batch.iter().map(|r| r.non_votants).collect();

        sqlx::query!(
            r#"
            INSERT INTO scrutin_groupe (scrutin_id, rang, organe_id, nombre_membres,
                                         position_majoritaire, pour, contre, abstentions,
                                         non_votants)
            SELECT * FROM UNNEST(
                $1::bigint[], $2::smallint[], $3::bigint[], $4::int[], $5::text[], $6::int[],
                $7::int[], $8::int[], $9::int[]
            )
            ON CONFLICT (scrutin_id, rang) DO UPDATE SET
                organe_id = EXCLUDED.organe_id,
                nombre_membres = EXCLUDED.nombre_membres,
                position_majoritaire = EXCLUDED.position_majoritaire,
                pour = EXCLUDED.pour,
                contre = EXCLUDED.contre,
                abstentions = EXCLUDED.abstentions,
                non_votants = EXCLUDED.non_votants
            "#,
            &scrutin_id,
            &rangs,
            &organe_id as _,
            &nombre_membres,
            &position_majoritaire as _,
            &pours,
            &contres,
            &abstentions,
            &non_votants,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn upsert_votes(
    pool: &PgPool,
    rows: &[VoteRow],
    scrutin_ids: &HashMap<String, i64>,
) -> anyhow::Result<()> {
    for batch in rows.chunks(INSERT_BATCH) {
        let scrutin_id: Vec<i64> = batch
            .iter()
            .map(|r| scrutin_ids[&r.scrutin_an_uid])
            .collect();
        let person_id: Vec<i64> = batch.iter().map(|r| r.person_id).collect();
        let position: Vec<&str> = batch.iter().map(|r| r.position).collect();
        let cause_non_vote: Vec<Option<&str>> =
            batch.iter().map(|r| r.cause_non_vote.as_deref()).collect();
        let groupe_organe_id: Vec<Option<i64>> = batch.iter().map(|r| r.groupe_organe_id).collect();
        let groupe_from_mandat: Vec<bool> = batch.iter().map(|r| r.groupe_from_mandat).collect();
        let mandat_an_uid: Vec<Option<&str>> =
            batch.iter().map(|r| r.mandat_an_uid.as_deref()).collect();
        let par_delegation: Vec<bool> = batch.iter().map(|r| r.par_delegation).collect();

        sqlx::query!(
            r#"
            INSERT INTO vote (scrutin_id, person_id, position, cause_non_vote, groupe_organe_id,
                               groupe_from_mandat, mandat_an_uid, par_delegation)
            SELECT scrutin_id, person_id, position::vote_position, cause_non_vote,
                   groupe_organe_id, groupe_from_mandat, mandat_an_uid, par_delegation
            FROM UNNEST(
                $1::bigint[], $2::bigint[], $3::text[], $4::text[], $5::bigint[], $6::bool[],
                $7::text[], $8::bool[]
            ) AS t(scrutin_id, person_id, position, cause_non_vote, groupe_organe_id,
                   groupe_from_mandat, mandat_an_uid, par_delegation)
            ON CONFLICT (scrutin_id, person_id) DO UPDATE SET
                position = EXCLUDED.position,
                cause_non_vote = EXCLUDED.cause_non_vote,
                groupe_organe_id = EXCLUDED.groupe_organe_id,
                groupe_from_mandat = EXCLUDED.groupe_from_mandat,
                mandat_an_uid = EXCLUDED.mandat_an_uid,
                par_delegation = EXCLUDED.par_delegation
            "#,
            &scrutin_id,
            &person_id,
            &position as _,
            &cause_non_vote as _,
            &groupe_organe_id as _,
            &groupe_from_mandat,
            &mandat_an_uid as _,
            &par_delegation,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn upsert_mise_au_point(
    pool: &PgPool,
    rows: &[MiseAuPointRow],
    scrutin_ids: &HashMap<String, i64>,
) -> anyhow::Result<()> {
    for batch in rows.chunks(INSERT_BATCH) {
        let scrutin_id: Vec<i64> = batch
            .iter()
            .map(|r| scrutin_ids[&r.scrutin_an_uid])
            .collect();
        let person_id: Vec<i64> = batch.iter().map(|r| r.person_id).collect();
        let position_declaree: Vec<&str> = batch.iter().map(|r| r.position_declaree).collect();
        let source_document_id: Vec<i64> = batch.iter().map(|r| r.source_document_id).collect();

        sqlx::query!(
            r#"
            INSERT INTO vote_mise_au_point (scrutin_id, person_id, position_declaree,
                                             source_document_id)
            SELECT * FROM UNNEST($1::bigint[], $2::bigint[], $3::text[], $4::bigint[])
            ON CONFLICT (scrutin_id, person_id, position_declaree) DO UPDATE SET
                source_document_id = EXCLUDED.source_document_id
            "#,
            &scrutin_id,
            &person_id,
            &position_declaree as _,
            &source_document_id,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}
