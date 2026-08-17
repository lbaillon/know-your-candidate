//! Job `enrich_wikidata { person_uids? }` — voir docs/plans/phase-1-ingestion.md. Une seule
//! requête SPARQL, jointure exacte sur `P4123` (suffixe numérique de l'`an_uid`). L'AN gagne
//! toujours : en cas de QID ambigu pour un même `P4123`, rien n'est écrit et l'anomalie est
//! journalisée (D. spike). La licence des photos se lit ensuite sur Commons, par lots de 50.

use std::collections::HashMap;
use std::time::Duration;

use percent_encoding::percent_decode_str;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::an::http::USER_AGENT;
use crate::anomaly::{self, AnomalyRecord};
use crate::run;

use super::JobContext;

const SPARQL_ENDPOINT: &str = "https://query.wikidata.org/sparql";
const SPARQL_QUERY: &str = r#"
SELECT ?p ?an ?img WHERE {
  ?p wdt:P4123 ?an .
  OPTIONAL { ?p wdt:P18 ?img }
}
"#;
const COMMONS_API: &str = "https://commons.wikimedia.org/w/api.php";
const COMMONS_BATCH: usize = 50;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const UPDATE_BATCH: usize = 1000;

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let person_uids: Option<Vec<String>> = payload.get("person_uids").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
    });

    let run_id = run::start(&ctx.pool, "wikidata", ctx.job_id, payload.clone()).await?;

    match ingest(&ctx.pool, run_id, person_uids.as_deref()).await {
        Ok(counters) => {
            run::finish_ok(&ctx.pool, run_id, counters, SPARQL_ENDPOINT, None).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

struct WikidataHit {
    qid: String,
    an_uid: String,
    /// URL Commons `Special:FilePath/...` du fichier `P18`, si renseigné.
    image_url: Option<String>,
}

async fn ingest(
    pool: &PgPool,
    run_id: i64,
    person_uids_filter: Option<&[String]>,
) -> anyhow::Result<Value> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;

    let hits = fetch_wikidata(&client).await?;
    tracing::info!(count = hits.len(), "résultats SPARQL reçus");

    // Regroupement par an_uid : un P4123 ambigu (plusieurs QID) ne doit rien écrire.
    let mut by_an_uid: HashMap<String, Vec<&WikidataHit>> = HashMap::new();
    for hit in &hits {
        by_an_uid.entry(hit.an_uid.clone()).or_default().push(hit);
    }

    let person_filter: Option<std::collections::HashSet<&str>> =
        person_uids_filter.map(|uids| uids.iter().map(String::as_str).collect());

    let mut anomalies: Vec<AnomalyRecord> = Vec::new();
    let mut resolved: Vec<(&str, &str)> = Vec::new(); // (an_uid, qid)
    let mut photo_candidates: Vec<(&str, &str)> = Vec::new(); // (an_uid, image_url)

    for (an_uid, group) in &by_an_uid {
        if let Some(filter) = &person_filter
            && !filter.contains(an_uid.as_str())
        {
            continue;
        }

        let distinct_qids: std::collections::HashSet<&str> =
            group.iter().map(|h| h.qid.as_str()).collect();
        if distinct_qids.len() > 1 {
            anomalies.push(AnomalyRecord {
                kind: anomaly::WIKIDATA_QID_AMBIGU,
                subject_uid: Some(an_uid.clone()),
                detail: serde_json::json!({"qids": distinct_qids}),
            });
            continue;
        }

        let hit = group[0];
        resolved.push((an_uid.as_str(), hit.qid.as_str()));
        if let Some(image_url) = &hit.image_url {
            photo_candidates.push((an_uid.as_str(), image_url.as_str()));
        }
    }

    let updated_qids = update_wikidata_qids(pool, &resolved).await?;

    let photo_stats = enrich_photos(&client, pool, run_id, &photo_candidates).await?;

    anomaly::record_many(pool, run_id, &anomalies).await?;

    Ok(serde_json::json!({
        "resultats_sparql": hits.len(),
        "personnes_uniques": by_an_uid.len(),
        "qid_ambigus": anomalies.len(),
        "qid_mis_a_jour": updated_qids,
        "photos_candidates": photo_candidates.len(),
        "photos_enregistrees": photo_stats.enregistrees,
        "photos_sans_licence": photo_stats.sans_licence,
    }))
}

#[derive(Deserialize)]
struct SparqlResponse {
    results: SparqlResults,
}
#[derive(Deserialize)]
struct SparqlResults {
    bindings: Vec<HashMap<String, SparqlValue>>,
}
#[derive(Deserialize)]
struct SparqlValue {
    value: String,
}

async fn fetch_wikidata(client: &reqwest::Client) -> anyhow::Result<Vec<WikidataHit>> {
    let response = client
        .get(SPARQL_ENDPOINT)
        .query(&[("query", SPARQL_QUERY), ("format", "json")])
        .header(reqwest::header::ACCEPT, "application/sparql-results+json")
        .send()
        .await?
        .error_for_status()?;
    let body = response.text().await?;
    let parsed: SparqlResponse = serde_json::from_str(&body)?;

    let mut hits = Vec::new();
    for row in parsed.results.bindings {
        let Some(p) = row.get("p") else { continue };
        let Some(an) = row.get("an") else { continue };
        let qid = p.value.rsplit('/').next().unwrap_or(&p.value).to_string();
        let an_uid = format!("PA{}", an.value);
        let image_url = row.get("img").map(|v| v.value.clone());
        hits.push(WikidataHit {
            qid,
            an_uid,
            image_url,
        });
    }
    Ok(hits)
}

async fn update_wikidata_qids(pool: &PgPool, resolved: &[(&str, &str)]) -> sqlx::Result<u64> {
    let mut total = 0u64;
    for batch in resolved.chunks(UPDATE_BATCH) {
        let an_uids: Vec<&str> = batch.iter().map(|(a, _)| *a).collect();
        let qids: Vec<&str> = batch.iter().map(|(_, q)| *q).collect();

        let result = sqlx::query!(
            r#"
            UPDATE person p
            SET wikidata_qid = t.qid, updated_at = now()
            FROM UNNEST($1::text[], $2::text[]) AS t(an_uid, qid)
            WHERE p.an_uid = t.an_uid AND p.wikidata_qid IS DISTINCT FROM t.qid
            "#,
            &an_uids as _,
            &qids as _,
        )
        .execute(pool)
        .await?;
        total += result.rows_affected();
    }
    Ok(total)
}

/// Titre de fichier Commons extrait d'une URL `Special:FilePath/<fichier encodé>`.
fn commons_title_from_image_url(image_url: &str) -> Option<String> {
    let filename = image_url.rsplit('/').next()?;
    let decoded = percent_decode_str(filename).decode_utf8().ok()?;
    Some(format!("File:{decoded}"))
}

#[derive(Default)]
struct PhotoStats {
    enregistrees: usize,
    sans_licence: usize,
}

#[derive(Deserialize)]
struct CommonsResponse {
    query: Option<CommonsQuery>,
}
#[derive(Deserialize)]
struct CommonsQuery {
    pages: HashMap<String, CommonsPage>,
}
#[derive(Deserialize)]
struct CommonsPage {
    title: String,
    imageinfo: Option<Vec<CommonsImageInfo>>,
}
#[derive(Deserialize)]
struct CommonsImageInfo {
    url: String,
    extmetadata: Option<HashMap<String, CommonsMetaValue>>,
}
#[derive(Deserialize)]
struct CommonsMetaValue {
    value: Value,
}

struct PhotoRow {
    person_id: i64,
    url: String,
    commons_file: String,
    licence: String,
    licence_url: Option<String>,
    auteur: Option<String>,
}

async fn enrich_photos(
    client: &reqwest::Client,
    pool: &PgPool,
    run_id: i64,
    candidates: &[(&str, &str)],
) -> anyhow::Result<PhotoStats> {
    let mut stats = PhotoStats::default();
    if candidates.is_empty() {
        return Ok(stats);
    }

    let person_ids = fetch_person_ids(pool, candidates.iter().map(|(u, _)| *u)).await?;

    let mut rows: Vec<PhotoRow> = Vec::new();
    let mut anomalies: Vec<AnomalyRecord> = Vec::new();

    // Chaque titre Commons est associé à son an_uid pour ré-attribuer la réponse groupée.
    let mut title_to_an_uid: HashMap<String, &str> = HashMap::new();
    let mut titles: Vec<String> = Vec::new();
    for (an_uid, image_url) in candidates {
        let Some(title) = commons_title_from_image_url(image_url) else {
            continue;
        };
        title_to_an_uid.insert(title.clone(), an_uid);
        titles.push(title);
    }

    for batch in titles.chunks(COMMONS_BATCH) {
        let joined = batch.join("|");
        let response = client
            .get(COMMONS_API)
            .query(&[
                ("action", "query"),
                ("prop", "imageinfo"),
                ("iiprop", "url|extmetadata"),
                ("titles", joined.as_str()),
                ("format", "json"),
            ])
            .send()
            .await?
            .error_for_status()?;
        let body = response.text().await?;
        let parsed: CommonsResponse = serde_json::from_str(&body)?;

        let Some(query) = parsed.query else { continue };
        for page in query.pages.into_values() {
            let Some(&an_uid) = title_to_an_uid.get(&page.title) else {
                continue;
            };
            let Some(&person_id) = person_ids.get(an_uid) else {
                continue;
            };
            let Some(info) = page.imageinfo.and_then(|v| v.into_iter().next()) else {
                stats.sans_licence += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::PHOTO_SANS_LICENCE,
                    subject_uid: Some(an_uid.to_string()),
                    detail: serde_json::json!({"raison": "pas d'imageinfo"}),
                });
                continue;
            };
            let Some(meta) = &info.extmetadata else {
                stats.sans_licence += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::PHOTO_SANS_LICENCE,
                    subject_uid: Some(an_uid.to_string()),
                    detail: serde_json::json!({"raison": "pas d'extmetadata"}),
                });
                continue;
            };
            let licence = meta
                .get("LicenseShortName")
                .and_then(|v| v.value.as_str())
                .map(str::to_string);
            let Some(licence) = licence else {
                stats.sans_licence += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::PHOTO_SANS_LICENCE,
                    subject_uid: Some(an_uid.to_string()),
                    detail: serde_json::json!({"raison": "pas de LicenseShortName"}),
                });
                continue;
            };

            let licence_url = meta
                .get("LicenseUrl")
                .and_then(|v| v.value.as_str())
                .map(str::to_string);
            let auteur = meta
                .get("Artist")
                .and_then(|v| v.value.as_str())
                .map(str::to_string);

            rows.push(PhotoRow {
                person_id,
                url: info.url,
                commons_file: page.title.clone(),
                licence,
                licence_url,
                auteur,
            });
        }
    }

    stats.enregistrees = rows.len();
    upsert_photos(pool, &rows).await?;
    anomaly::record_many(pool, run_id, &anomalies).await?;

    Ok(stats)
}

async fn fetch_person_ids<'a>(
    pool: &PgPool,
    an_uids: impl Iterator<Item = &'a str>,
) -> sqlx::Result<HashMap<String, i64>> {
    let uids: Vec<&str> = an_uids.collect();
    let mut out = HashMap::new();
    for batch in uids.chunks(UPDATE_BATCH) {
        let rows = sqlx::query!(
            "SELECT an_uid, id FROM person WHERE an_uid = ANY($1::text[])",
            batch as _,
        )
        .fetch_all(pool)
        .await?;
        out.extend(rows.into_iter().map(|r| (r.an_uid, r.id)));
    }
    Ok(out)
}

/// Pas de photo sans licence affichable (D1.13, contrainte NOT NULL sur `licence`) : les lignes
/// arrivées ici ont déjà toutes une licence exploitable.
async fn upsert_photos(pool: &PgPool, rows: &[PhotoRow]) -> sqlx::Result<()> {
    for batch in rows.chunks(UPDATE_BATCH) {
        let person_ids: Vec<i64> = batch.iter().map(|r| r.person_id).collect();
        let urls: Vec<&str> = batch.iter().map(|r| r.url.as_str()).collect();
        let commons_files: Vec<&str> = batch.iter().map(|r| r.commons_file.as_str()).collect();
        let licences: Vec<&str> = batch.iter().map(|r| r.licence.as_str()).collect();
        let licence_urls: Vec<Option<&str>> =
            batch.iter().map(|r| r.licence_url.as_deref()).collect();
        let auteurs: Vec<Option<&str>> = batch.iter().map(|r| r.auteur.as_deref()).collect();

        sqlx::query!(
            r#"
            INSERT INTO person_photo (person_id, url, commons_file, licence, licence_url, auteur)
            SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::text[], $5::text[], $6::text[])
            ON CONFLICT (person_id) DO UPDATE SET
                url = EXCLUDED.url,
                commons_file = EXCLUDED.commons_file,
                licence = EXCLUDED.licence,
                licence_url = EXCLUDED.licence_url,
                auteur = EXCLUDED.auteur,
                fetched_at = now()
            WHERE person_photo.url IS DISTINCT FROM EXCLUDED.url
               OR person_photo.commons_file IS DISTINCT FROM EXCLUDED.commons_file
               OR person_photo.licence IS DISTINCT FROM EXCLUDED.licence
               OR person_photo.licence_url IS DISTINCT FROM EXCLUDED.licence_url
               OR person_photo.auteur IS DISTINCT FROM EXCLUDED.auteur
            "#,
            &person_ids,
            &urls as _,
            &commons_files as _,
            &licences as _,
            &licence_urls as _,
            &auteurs as _,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}
