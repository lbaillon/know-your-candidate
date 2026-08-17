//! Job `enrich_wikidata { person_uids? }` — voir docs/plans/phase-1-ingestion.md. Une seule
//! requête SPARQL, jointure exacte sur `P4123` (suffixe numérique de l'`an_uid`). L'AN gagne
//! toujours : en cas de QID ambigu pour un même `P4123` **ou** un `P4123` ambigu pour un même QID,
//! rien n'est écrit côté de l'ambiguïté et l'anomalie est journalisée (D. spike, F2a,
//! docs/plans/phase-1.1-fix.md). La licence des photos se lit ensuite sur Commons, par lots de 50.

use std::collections::{HashMap, HashSet};
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
            run::finish_ok(&ctx.pool, run_id, counters, Some(SPARQL_ENDPOINT), None).await?;
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

/// Un couple `(an_uid, qid)` retenu après résolution des ambiguïtés — voir `resolve_wikidata_hits`.
struct ResolvedHit<'a> {
    an_uid: &'a str,
    qid: &'a str,
    image_url: Option<&'a str>,
}

struct WikidataResolution<'a> {
    resolved: Vec<ResolvedHit<'a>>,
    ambiguities: Vec<AnomalyRecord>,
}

/// Fonction pure : groupe les résultats SPARQL par `an_uid` **et** par `qid`, dans les deux sens
/// (F2a, docs/plans/phase-1.1-fix.md). L'AN gagne toujours, Wikidata ne tranche jamais un conflit
/// à notre place :
///
/// - un `an_uid` revendiqué par plusieurs `qid` (le cas déjà couvert avant F2a) : rien n'est écrit
///   pour cet `an_uid` ;
/// - un `qid` revendiqué par plusieurs `an_uid` (le cas miroir, manqué avant F2a — c'est lui qui
///   fait violer l'unicité de `person.wikidata_qid`) : rien n'est écrit pour aucun des `an_uid`
///   concernés.
///
/// Dans les deux cas, l'anomalie journalisée précise de quel côté vient l'ambiguïté.
fn resolve_wikidata_hits(hits: &[WikidataHit]) -> WikidataResolution<'_> {
    let mut by_an_uid: HashMap<&str, Vec<&WikidataHit>> = HashMap::new();
    let mut an_uids_by_qid: HashMap<&str, HashSet<&str>> = HashMap::new();
    for hit in hits {
        by_an_uid.entry(hit.an_uid.as_str()).or_default().push(hit);
        an_uids_by_qid
            .entry(hit.qid.as_str())
            .or_default()
            .insert(hit.an_uid.as_str());
    }

    let mut resolved = Vec::new();
    let mut ambiguities = Vec::new();

    // Ordre déterministe : indispensable pour que les tests (et les runs successifs) obtiennent
    // toujours le même ordre d'anomalies.
    let mut an_uids: Vec<&&str> = by_an_uid.keys().collect();
    an_uids.sort_unstable();

    for an_uid in an_uids {
        let group = &by_an_uid[an_uid];
        let distinct_qids: HashSet<&str> = group.iter().map(|h| h.qid.as_str()).collect();
        if distinct_qids.len() > 1 {
            let mut qids: Vec<&str> = distinct_qids.into_iter().collect();
            qids.sort_unstable();
            ambiguities.push(AnomalyRecord {
                kind: anomaly::WIKIDATA_QID_AMBIGU,
                subject_uid: Some(an_uid.to_string()),
                detail: serde_json::json!({"cote": "an_uid", "an_uid": an_uid, "qids": qids}),
            });
            continue;
        }

        let hit = group[0];
        let an_uids_claiming_qid = &an_uids_by_qid[hit.qid.as_str()];
        if an_uids_claiming_qid.len() > 1 {
            let mut autres: Vec<&str> = an_uids_claiming_qid.iter().copied().collect();
            autres.sort_unstable();
            ambiguities.push(AnomalyRecord {
                kind: anomaly::WIKIDATA_QID_AMBIGU,
                subject_uid: Some(an_uid.to_string()),
                detail: serde_json::json!({"cote": "qid", "qid": hit.qid, "an_uids": autres}),
            });
            continue;
        }

        resolved.push(ResolvedHit {
            an_uid,
            qid: hit.qid.as_str(),
            image_url: hit.image_url.as_deref(),
        });
    }

    WikidataResolution {
        resolved,
        ambiguities,
    }
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

    let person_filter: Option<HashSet<&str>> =
        person_uids_filter.map(|uids| uids.iter().map(String::as_str).collect());

    let personnes_uniques: HashSet<&str> = hits.iter().map(|h| h.an_uid.as_str()).collect();
    let resolution = resolve_wikidata_hits(&hits);

    let mut resolved: Vec<(&str, &str)> = Vec::new();
    let mut photo_candidates: Vec<(&str, &str)> = Vec::new();
    for hit in &resolution.resolved {
        if let Some(filter) = &person_filter
            && !filter.contains(hit.an_uid)
        {
            continue;
        }
        resolved.push((hit.an_uid, hit.qid));
        if let Some(image_url) = hit.image_url {
            photo_candidates.push((hit.an_uid, image_url));
        }
    }

    let updated_qids = update_wikidata_qids(pool, &resolved).await?;

    let photo_stats = enrich_photos(&client, pool, run_id, &photo_candidates).await?;

    anomaly::record_many(pool, run_id, &resolution.ambiguities).await?;

    Ok(serde_json::json!({
        "resultats_sparql": hits.len(),
        "personnes_uniques": personnes_uniques.len(),
        "qid_ambigus": resolution.ambiguities.len(),
        "qid_mis_a_jour": updated_qids,
        "photos_candidates": photo_candidates.len(),
        "photos_enregistrees": photo_stats.enregistrees,
        "photos_sans_licence": photo_stats.sans_licence,
        "photos_hors_perimetre": photo_stats.hors_perimetre,
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

pub async fn update_wikidata_qids(pool: &PgPool, resolved: &[(&str, &str)]) -> sqlx::Result<u64> {
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

/// Titre de fichier Commons extrait d'une URL `Special:FilePath/<fichier encodé>`. Normalise les
/// soulignés en espaces **après** le percent-décodage (F2b, docs/plans/phase-1.1-fix.md) : l'API
/// Commons rend `page.title` avec des espaces (`File:Nom Du Fichier.jpg`), alors que l'URL `P18`
/// les porte en soulignés (`Special:FilePath/Nom_Du_Fichier.jpg`) — sans cette normalisation, la
/// jointure sur le titre échoue et la photo est perdue sans qu'aucune anomalie ne le signale.
fn commons_title_from_image_url(image_url: &str) -> Option<String> {
    let filename = image_url.rsplit('/').next()?;
    let decoded = percent_decode_str(filename).decode_utf8().ok()?;
    Some(format!("File:{}", normalize_commons_title(&decoded)))
}

/// Même normalisation appliquée à `page.title` côté lecture, par symétrie défensive (F2b) : la
/// correspondance ne doit dépendre d'aucun des deux côtés pour porter un souligné au lieu d'un
/// espace.
fn normalize_commons_title(title: &str) -> String {
    title.replace('_', " ")
}

#[derive(Default)]
struct PhotoStats {
    enregistrees: usize,
    sans_licence: usize,
    /// `P4123` couvre tous les député·es depuis toujours, pas seulement les législatures 15 à 17 :
    /// un candidat photo dont l'`an_uid` n'a pas de ligne `person` n'est pas une perte de notre
    /// corpus, mesurée séparément de `sans_licence` pour ne pas la faire passer pour une anomalie
    /// de données (trouvé au contact de l'exécution réelle du job, F2c, docs/plans/phase-1.1-fix.md).
    hors_perimetre: usize,
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

pub struct PhotoRow {
    pub person_id: i64,
    pub url: String,
    pub commons_file: String,
    pub licence: String,
    pub licence_url: Option<String>,
    pub auteur: Option<String>,
}

struct LicenceInfo {
    licence: String,
    licence_url: Option<String>,
    auteur: Option<String>,
}

/// Fonction pure : lit `LicenseShortName`/`LicenseUrl`/`Artist` dans l'`extmetadata` d'une réponse
/// Commons. `None` couvre les quatre cas « pas de licence exploitable » : `extmetadata` absent,
/// `LicenseShortName` absent, d'un type autre qu'une chaîne, ou vide.
///
/// Le filtre sur le vide est ce qui donne son sens à la contrainte `NOT NULL` de `person_photo`
/// (D1.13, « une photo sans licence exploitable = pas de photo ») : une chaîne vide la satisfait
/// tout en affichant une photo sans licence lisible, ce qui est précisément ce qu'on s'interdit.
/// Jamais observé sur Commons — 0 `photo_sans_licence` sur les 1 599 photos mesurées le
/// 17 août 2026 — mais une garantie qui ne tient qu'à l'amabilité de la source n'en est pas une.
fn read_licence(extmetadata: Option<&HashMap<String, CommonsMetaValue>>) -> Option<LicenceInfo> {
    let meta = extmetadata?;
    let licence = meta
        .get("LicenseShortName")
        .and_then(|v| v.value.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)?;
    let licence_url = meta
        .get("LicenseUrl")
        .and_then(|v| v.value.as_str())
        .map(str::to_string);
    let auteur = meta
        .get("Artist")
        .and_then(|v| v.value.as_str())
        .map(str::to_string);
    Some(LicenceInfo {
        licence,
        licence_url,
        auteur,
    })
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
    // `attendues` (F2b) : tout an_uid dont un titre a pu être construit, pour détecter en fin de
    // lot celles que la réponse Commons n'a jamais couvertes (retirées, renommées, requête
    // partiellement vide...) plutôt que de les laisser disparaître sans anomalie.
    //
    // Filtré sur `person_ids` **avant** toute requête Commons : `P4123` couvre tous les
    // député·es depuis la XIe législature, pas seulement notre corpus L15-L17 (trouvé en
    // exécutant réellement F2c) — interroger Commons pour des personnes qu'on ne suit pas
    // gaspillerait des appels, et surtout ferait retomber leur non-résolution dans le même
    // `continue` silencieux que F2b vise justement à éliminer.
    let mut title_to_an_uid: HashMap<String, &str> = HashMap::new();
    let mut titles: Vec<String> = Vec::new();
    let mut attendues: HashSet<&str> = HashSet::new();
    for (an_uid, image_url) in candidates {
        if !person_ids.contains_key(*an_uid) {
            stats.hors_perimetre += 1;
            continue;
        }
        let Some(title) = commons_title_from_image_url(image_url) else {
            stats.sans_licence += 1;
            anomalies.push(AnomalyRecord {
                kind: anomaly::PHOTO_SANS_LICENCE,
                subject_uid: Some(an_uid.to_string()),
                detail: serde_json::json!({"raison": "URL Commons sans nom de fichier exploitable"}),
            });
            continue;
        };
        title_to_an_uid.insert(title.clone(), an_uid);
        titles.push(title);
        attendues.insert(an_uid);
    }

    let mut recues: HashSet<&str> = HashSet::new();

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
            let Some(&an_uid) = title_to_an_uid.get(&normalize_commons_title(&page.title)) else {
                continue;
            };
            recues.insert(an_uid);
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
            let Some(licence_info) = read_licence(info.extmetadata.as_ref()) else {
                stats.sans_licence += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::PHOTO_SANS_LICENCE,
                    subject_uid: Some(an_uid.to_string()),
                    detail: serde_json::json!({"raison": "pas de licence exploitable dans extmetadata"}),
                });
                continue;
            };

            rows.push(PhotoRow {
                person_id,
                url: info.url,
                commons_file: page.title.clone(),
                licence: licence_info.licence,
                licence_url: licence_info.licence_url,
                auteur: licence_info.auteur,
            });
        }
    }

    // F2b : un an_uid attendu mais jamais retrouvé dans une réponse Commons est une perte muette
    // tant qu'elle n'est pas journalisée — le titre a pu être retiré, renommé, ou la requête a pu
    // omettre certaines entrées du lot.
    for &an_uid in &attendues {
        if !recues.contains(an_uid) {
            stats.sans_licence += 1;
            anomalies.push(AnomalyRecord {
                kind: anomaly::PHOTO_SANS_LICENCE,
                subject_uid: Some(an_uid.to_string()),
                detail: serde_json::json!({"raison": "titre Commons absent de la réponse"}),
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
            r#"SELECT an_uid AS "an_uid!", id FROM person WHERE an_uid = ANY($1::text[])"#,
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
pub async fn upsert_photos(pool: &PgPool, rows: &[PhotoRow]) -> sqlx::Result<()> {
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn hit(an_uid: &str, qid: &str, image_url: Option<&str>) -> WikidataHit {
        WikidataHit {
            qid: qid.to_string(),
            an_uid: an_uid.to_string(),
            image_url: image_url.map(String::from),
        }
    }

    // --- resolve_wikidata_hits ---------------------------------------------------------------

    #[test]
    fn resolve_cas_nominal() {
        let hits = vec![hit("PA1", "Q1", Some("https://x/Special:FilePath/A.jpg"))];
        let result = resolve_wikidata_hits(&hits);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].an_uid, "PA1");
        assert_eq!(result.resolved[0].qid, "Q1");
        assert!(result.ambiguities.is_empty());
    }

    #[test]
    fn resolve_deux_qid_pour_un_meme_an_uid() {
        let hits = vec![hit("PA1", "Q1", None), hit("PA1", "Q2", None)];
        let result = resolve_wikidata_hits(&hits);
        assert!(result.resolved.is_empty());
        assert_eq!(result.ambiguities.len(), 1);
        assert_eq!(result.ambiguities[0].kind, anomaly::WIKIDATA_QID_AMBIGU);
        assert_eq!(
            result.ambiguities[0].detail["cote"].as_str(),
            Some("an_uid")
        );
    }

    /// F2a : le cas miroir manqué avant le correctif — un même QID revendiqué par deux `P4123`
    /// (donc deux `an_uid`) produit deux lignes `resolved` visant deux personnes différentes, ce
    /// qui violerait l'unicité de `person.wikidata_qid` si on les laissait passer.
    #[test]
    fn resolve_deux_an_uid_pour_un_meme_qid() {
        let hits = vec![hit("PA1", "Q1", None), hit("PA2", "Q1", None)];
        let result = resolve_wikidata_hits(&hits);
        assert!(
            result.resolved.is_empty(),
            "aucun des deux côtés n'est écrit"
        );
        assert_eq!(result.ambiguities.len(), 2);
        assert!(
            result
                .ambiguities
                .iter()
                .all(|a| a.detail["cote"].as_str() == Some("qid"))
        );
        let subjects: HashSet<&str> = result
            .ambiguities
            .iter()
            .filter_map(|a| a.subject_uid.as_deref())
            .collect();
        assert_eq!(subjects, HashSet::from(["PA1", "PA2"]));
    }

    #[test]
    fn resolve_hit_sans_p18_est_retenu_sans_candidat_photo() {
        let hits = vec![hit("PA1", "Q1", None)];
        let result = resolve_wikidata_hits(&hits);
        assert_eq!(result.resolved.len(), 1);
        assert_eq!(result.resolved[0].image_url, None);
    }

    // --- commons_title_from_image_url --------------------------------------------------------

    #[test]
    fn commons_title_url_percent_encodee() {
        let url = "https://commons.wikimedia.org/wiki/Special:FilePath/Jean%20Dupont.jpg";
        assert_eq!(
            commons_title_from_image_url(url).as_deref(),
            Some("File:Jean Dupont.jpg")
        );
    }

    /// F2b : le bug corrigé — l'URL `P18` porte des soulignés, `page.title` rend des espaces.
    #[test]
    fn commons_title_url_a_soulignes_est_normalisee_en_espaces() {
        let url = "https://commons.wikimedia.org/wiki/Special:FilePath/Jean_Dupont.jpg";
        assert_eq!(
            commons_title_from_image_url(url).as_deref(),
            Some("File:Jean Dupont.jpg")
        );
    }

    /// `rsplit('/').next()` rend toujours `Some`, y compris une chaîne vide (segment final vide
    /// sur une URL qui se termine par `/`) : la seule façon dont la fonction rend `None` est un
    /// pourcentage-encodage invalide, pas une URL « sans nom de fichier » au sens littéral.
    #[test]
    fn commons_title_url_percent_encodage_invalide() {
        assert_eq!(
            commons_title_from_image_url("https://x/Special:FilePath/%FF%FE"),
            None
        );
    }

    #[test]
    fn commons_title_url_segment_final_vide() {
        assert_eq!(
            commons_title_from_image_url("https://x/Special:FilePath/"),
            Some("File:".to_string())
        );
    }

    // --- read_licence --------------------------------------------------------------------------

    fn meta_value(s: &str) -> CommonsMetaValue {
        CommonsMetaValue {
            value: Value::String(s.to_string()),
        }
    }

    #[test]
    fn read_licence_shortname_present() {
        let mut meta = HashMap::new();
        meta.insert("LicenseShortName".to_string(), meta_value("CC BY-SA 4.0"));
        meta.insert(
            "LicenseUrl".to_string(),
            meta_value("https://creativecommons.org/licenses/by-sa/4.0/"),
        );
        meta.insert("Artist".to_string(), meta_value("Jean Dupont"));

        let info = read_licence(Some(&meta)).expect("licence exploitable");
        assert_eq!(info.licence, "CC BY-SA 4.0");
        assert_eq!(
            info.licence_url.as_deref(),
            Some("https://creativecommons.org/licenses/by-sa/4.0/")
        );
        assert_eq!(info.auteur.as_deref(), Some("Jean Dupont"));
    }

    #[test]
    fn read_licence_shortname_absent() {
        let mut meta = HashMap::new();
        meta.insert("Artist".to_string(), meta_value("Jean Dupont"));
        assert!(read_licence(Some(&meta)).is_none());
    }

    #[test]
    fn read_licence_extmetadata_absent() {
        assert!(read_licence(None).is_none());
    }

    /// Une `LicenseShortName` vide satisferait le `NOT NULL` de `person_photo` tout en affichant
    /// une photo sans licence lisible — exactement ce que D1.13 interdit. Elle doit donc valoir
    /// « pas de licence exploitable », au même titre qu'une clé absente.
    #[test]
    fn read_licence_shortname_vide_ou_blanche() {
        for vide in ["", "   "] {
            let mut meta = HashMap::new();
            meta.insert("LicenseShortName".to_string(), meta_value(vide));
            assert!(
                read_licence(Some(&meta)).is_none(),
                "{vide:?} n'est pas une licence exploitable"
            );
        }
    }

    /// `extmetadata` est du JSON libre : `LicenseShortName` peut porter autre chose qu'une chaîne.
    #[test]
    fn read_licence_shortname_pas_une_chaine() {
        let mut meta = HashMap::new();
        meta.insert(
            "LicenseShortName".to_string(),
            CommonsMetaValue { value: Value::Null },
        );
        assert!(read_licence(Some(&meta)).is_none());
    }
}
