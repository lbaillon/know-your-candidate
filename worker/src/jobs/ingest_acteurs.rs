//! Job `ingest_acteurs { force_refetch? }` — voir docs/plans/phase-1-ingestion.md. Pas de
//! paramètre de législature : AMO30 est global (D. spike). Ordre : organes, puis acteurs, puis
//! mandats normalisés.

use std::collections::HashMap;
use std::io::Read as _;

use serde_json::Value;
use sqlx::PgPool;

use crate::an::acteur::{self, MandatGroupe, NormalizationEvent, ParsedActeur, ParsedOrgane};
use crate::an::document::{RawDocument, archive_documents};
use crate::an::http::{AnClient, sha256_hex};
use crate::an::open_zip;
use crate::run;

use super::JobContext;

const AMO30_URL: &str = "https://data.assemblee-nationale.fr/static/openData/repository/17/amo/tous_acteurs_mandats_organes_xi_legislature/AMO30_tous_acteurs_tous_mandats_tous_organes_historique.json.zip";
const RETAINED_CODE_TYPES: [&str; 3] = ["GP", "PARPOL", "ASSEMBLEE"];
const BATCH_SIZE: usize = 1000;

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let force_refetch = payload
        .get("force_refetch")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let run_id = run::start(&ctx.pool, "an_amo30").await?;

    match ingest(&ctx.pool, force_refetch).await {
        Ok(counters) => {
            run::finish_ok(&ctx.pool, run_id, counters).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

async fn ingest(pool: &PgPool, force_refetch: bool) -> anyhow::Result<Value> {
    let client = AnClient::new(cache_dir())?;
    let archive = client.fetch_zip(AMO30_URL, force_refetch).await?;

    tracing::info!(
        content_hash = %archive.content_hash,
        from_cache = archive.from_cache,
        "AMO30 téléchargée"
    );

    let bytes = archive.bytes.clone();
    let (organe_docs, acteur_docs) =
        tokio::task::spawn_blocking(move || read_amo30_docs(bytes)).await??;

    archive_documents(pool, "an_organe", AMO30_URL, &organe_docs).await?;
    archive_documents(pool, "an_acteur", AMO30_URL, &acteur_docs).await?;

    let organes: Vec<ParsedOrgane> = organe_docs
        .iter()
        .filter_map(|doc| acteur::parse_organe(&doc.payload))
        .filter(|o| RETAINED_CODE_TYPES.contains(&o.code_type.as_str()))
        .collect();
    upsert_organes(pool, &organes).await?;
    let organe_ids = fetch_organe_ids(pool).await?;

    let acteurs: Vec<ParsedActeur> = acteur_docs
        .iter()
        .filter_map(|doc| acteur::parse_acteur(&doc.payload))
        .collect();
    upsert_persons(pool, &acteurs).await?;
    let person_ids = fetch_person_ids(pool).await?;

    let mandat_stats = upsert_mandats(pool, &acteurs, &organe_ids, &person_ids).await?;

    Ok(serde_json::json!({
        "organes": organes.len(),
        "personnes": acteurs.len(),
        "mandats_geres": mandat_stats.geres,
        "mandats_ignores_organe_inconnu": mandat_stats.organe_inconnu,
        "mandats_ignores_date_debut_manquante": mandat_stats.date_debut_manquante,
        "mandats_ignores_date_fin_avant_debut": mandat_stats.date_fin_avant_debut,
        "normalisation_inclusions": mandat_stats.inclusions,
        "normalisation_charnieres": mandat_stats.charnieres,
        "normalisation_chevauchements": mandat_stats.chevauchements,
        "content_hash": archive.content_hash,
    }))
}

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var("AN_CACHE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| Some(std::path::PathBuf::from(".cache")))
}

/// Décompression et lecture brute de l'archive — travail CPU, exécuté hors de l'exécuteur async
/// pour ne pas le bloquer le temps de parcourir ~14 000 fichiers.
fn read_amo30_docs(bytes: bytes::Bytes) -> anyhow::Result<(Vec<RawDocument>, Vec<RawDocument>)> {
    let mut zip = open_zip(bytes)?;
    let mut organes = Vec::new();
    let mut acteurs = Vec::new();

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        let name = file.name().to_string();
        let category = if name.starts_with("json/organe/") {
            "organe"
        } else if name.starts_with("json/acteur/") {
            "acteur"
        } else {
            continue;
        };

        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        let payload: Value = serde_json::from_slice(&buf)?;
        let uid = name
            .rsplit('/')
            .next()
            .unwrap_or(&name)
            .trim_end_matches(".json")
            .to_string();
        let content_hash = sha256_hex(&buf);

        let doc = RawDocument {
            uid,
            payload,
            content_hash,
        };
        if category == "organe" {
            organes.push(doc);
        } else {
            acteurs.push(doc);
        }
    }

    Ok((organes, acteurs))
}

async fn upsert_organes(pool: &PgPool, organes: &[ParsedOrgane]) -> sqlx::Result<()> {
    for batch in organes.chunks(BATCH_SIZE) {
        let an_uids: Vec<&str> = batch.iter().map(|o| o.an_uid.as_str()).collect();
        let code_types: Vec<&str> = batch.iter().map(|o| o.code_type.as_str()).collect();
        let libelles: Vec<&str> = batch.iter().map(|o| o.libelle.as_str()).collect();
        let libelles_abrege: Vec<Option<&str>> =
            batch.iter().map(|o| o.libelle_abrege.as_deref()).collect();
        let libelles_abrev: Vec<Option<&str>> =
            batch.iter().map(|o| o.libelle_abrev.as_deref()).collect();
        let legislatures: Vec<Option<i16>> = batch.iter().map(|o| o.legislature).collect();
        let debuts: Vec<Option<chrono::NaiveDate>> = batch.iter().map(|o| o.date_debut).collect();
        let fins: Vec<Option<chrono::NaiveDate>> = batch.iter().map(|o| o.date_fin).collect();
        let non_inscrits: Vec<bool> = batch
            .iter()
            .map(|o| o.code_type == "GP" && o.libelle_abrev.as_deref() == Some("NI"))
            .collect();

        sqlx::query!(
            r#"
            INSERT INTO organe (an_uid, code_type, libelle, libelle_abrege, libelle_abrev,
                                 legislature, period, is_non_inscrit)
            SELECT an_uid, code_type, libelle, libelle_abrege, libelle_abrev, legislature,
                   daterange(debut, CASE WHEN fin IS NULL THEN NULL ELSE fin + 1 END, '[)'),
                   is_non_inscrit
            FROM UNNEST(
                $1::text[], $2::text[], $3::text[], $4::text[], $5::text[],
                $6::smallint[], $7::date[], $8::date[], $9::bool[]
            ) AS t(an_uid, code_type, libelle, libelle_abrege, libelle_abrev, legislature,
                   debut, fin, is_non_inscrit)
            ON CONFLICT (an_uid) DO UPDATE SET
                code_type = EXCLUDED.code_type,
                libelle = EXCLUDED.libelle,
                libelle_abrege = EXCLUDED.libelle_abrege,
                libelle_abrev = EXCLUDED.libelle_abrev,
                legislature = EXCLUDED.legislature,
                period = EXCLUDED.period,
                is_non_inscrit = EXCLUDED.is_non_inscrit,
                updated_at = now()
            WHERE organe.code_type IS DISTINCT FROM EXCLUDED.code_type
               OR organe.libelle IS DISTINCT FROM EXCLUDED.libelle
               OR organe.libelle_abrege IS DISTINCT FROM EXCLUDED.libelle_abrege
               OR organe.libelle_abrev IS DISTINCT FROM EXCLUDED.libelle_abrev
               OR organe.legislature IS DISTINCT FROM EXCLUDED.legislature
               OR organe.period IS DISTINCT FROM EXCLUDED.period
               OR organe.is_non_inscrit IS DISTINCT FROM EXCLUDED.is_non_inscrit
            "#,
            &an_uids as _,
            &code_types as _,
            &libelles as _,
            &libelles_abrege as _,
            &libelles_abrev as _,
            &legislatures as _,
            &debuts as _,
            &fins as _,
            &non_inscrits,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

struct OrganeInfo {
    id: i64,
    is_non_inscrit: bool,
}

async fn fetch_organe_ids(pool: &PgPool) -> sqlx::Result<HashMap<String, OrganeInfo>> {
    let rows = sqlx::query!("SELECT an_uid, id, is_non_inscrit FROM organe")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.an_uid,
                OrganeInfo {
                    id: r.id,
                    is_non_inscrit: r.is_non_inscrit,
                },
            )
        })
        .collect())
}

async fn upsert_persons(pool: &PgPool, acteurs: &[ParsedActeur]) -> sqlx::Result<()> {
    for batch in acteurs.chunks(BATCH_SIZE) {
        let an_uids: Vec<&str> = batch.iter().map(|a| a.an_uid.as_str()).collect();
        let civilites: Vec<Option<&str>> = batch.iter().map(|a| a.civilite.as_deref()).collect();
        let prenoms: Vec<Option<&str>> = batch.iter().map(|a| a.prenom.as_deref()).collect();
        let noms: Vec<Option<&str>> = batch.iter().map(|a| a.nom.as_deref()).collect();
        let dates_naissance: Vec<Option<chrono::NaiveDate>> =
            batch.iter().map(|a| a.date_naissance).collect();
        let villes_naissance: Vec<Option<&str>> =
            batch.iter().map(|a| a.ville_naissance.as_deref()).collect();
        let departements_naissance: Vec<Option<&str>> = batch
            .iter()
            .map(|a| a.departement_naissance.as_deref())
            .collect();
        let pays_naissance: Vec<Option<&str>> =
            batch.iter().map(|a| a.pays_naissance.as_deref()).collect();
        let dates_deces: Vec<Option<chrono::NaiveDate>> =
            batch.iter().map(|a| a.date_deces).collect();
        let uris_hatvp: Vec<Option<&str>> = batch.iter().map(|a| a.uri_hatvp.as_deref()).collect();

        sqlx::query!(
            r#"
            INSERT INTO person (an_uid, civilite, prenom, nom, date_naissance, ville_naissance,
                                 departement_naissance, pays_naissance, date_deces, uri_hatvp)
            SELECT * FROM UNNEST(
                $1::text[], $2::text[], $3::text[], $4::text[], $5::date[], $6::text[],
                $7::text[], $8::text[], $9::date[], $10::text[]
            )
            ON CONFLICT (an_uid) DO UPDATE SET
                civilite = EXCLUDED.civilite,
                prenom = EXCLUDED.prenom,
                nom = EXCLUDED.nom,
                date_naissance = EXCLUDED.date_naissance,
                ville_naissance = EXCLUDED.ville_naissance,
                departement_naissance = EXCLUDED.departement_naissance,
                pays_naissance = EXCLUDED.pays_naissance,
                date_deces = EXCLUDED.date_deces,
                uri_hatvp = EXCLUDED.uri_hatvp,
                updated_at = now()
            WHERE person.civilite IS DISTINCT FROM EXCLUDED.civilite
               OR person.prenom IS DISTINCT FROM EXCLUDED.prenom
               OR person.nom IS DISTINCT FROM EXCLUDED.nom
               OR person.date_naissance IS DISTINCT FROM EXCLUDED.date_naissance
               OR person.ville_naissance IS DISTINCT FROM EXCLUDED.ville_naissance
               OR person.departement_naissance IS DISTINCT FROM EXCLUDED.departement_naissance
               OR person.pays_naissance IS DISTINCT FROM EXCLUDED.pays_naissance
               OR person.date_deces IS DISTINCT FROM EXCLUDED.date_deces
               OR person.uri_hatvp IS DISTINCT FROM EXCLUDED.uri_hatvp
            "#,
            &an_uids as _,
            &civilites as _,
            &prenoms as _,
            &noms as _,
            &dates_naissance as _,
            &villes_naissance as _,
            &departements_naissance as _,
            &pays_naissance as _,
            &dates_deces as _,
            &uris_hatvp as _,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

async fn fetch_person_ids(pool: &PgPool) -> sqlx::Result<HashMap<String, i64>> {
    let rows = sqlx::query!("SELECT an_uid, id FROM person")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.an_uid, r.id)).collect())
}

#[derive(Default)]
struct MandatStats {
    geres: usize,
    organe_inconnu: usize,
    date_debut_manquante: usize,
    date_fin_avant_debut: usize,
    inclusions: usize,
    charnieres: usize,
    chevauchements: usize,
}

struct MandatARetenir {
    an_uid: String,
    person_id: i64,
    organe_id: i64,
    type_organe: String,
    debut: chrono::NaiveDate,
    fin: Option<chrono::NaiveDate>,
    qualite: Option<String>,
    legislature: Option<i16>,
}

async fn upsert_mandats(
    pool: &PgPool,
    acteurs: &[ParsedActeur],
    organe_ids: &HashMap<String, OrganeInfo>,
    person_ids: &HashMap<String, i64>,
) -> anyhow::Result<MandatStats> {
    let mut stats = MandatStats::default();
    let mut a_inserer: Vec<MandatARetenir> = Vec::new();

    for acteur in acteurs {
        let Some(&person_id) = person_ids.get(&acteur.an_uid) else {
            continue;
        };

        let mut gp_mandats: Vec<MandatGroupe> = Vec::new();
        let mut gp_details: HashMap<String, &crate::an::acteur::ParsedMandat> = HashMap::new();
        let mut autres: Vec<&crate::an::acteur::ParsedMandat> = Vec::new();

        for mandat in &acteur.mandats {
            let Some(type_organe) = &mandat.type_organe else {
                continue;
            };
            if !RETAINED_CODE_TYPES.contains(&type_organe.as_str()) {
                continue;
            }
            let Some(organe_uid) = &mandat.organe_uid else {
                stats.organe_inconnu += 1;
                continue;
            };
            let Some(organe_info) = organe_ids.get(organe_uid) else {
                stats.organe_inconnu += 1;
                continue;
            };
            let Some(debut) = mandat.date_debut else {
                stats.date_debut_manquante += 1;
                continue;
            };
            if let Some(fin) = mandat.date_fin
                && fin < debut
            {
                stats.date_fin_avant_debut += 1;
                continue;
            }

            if type_organe == "GP" {
                gp_mandats.push(MandatGroupe {
                    an_uid: mandat.an_uid.clone(),
                    organe_uid: organe_uid.clone(),
                    debut,
                    fin: mandat.date_fin,
                    is_non_inscrit: organe_info.is_non_inscrit,
                });
                gp_details.insert(mandat.an_uid.clone(), mandat);
            } else {
                autres.push(mandat);
            }
        }

        let normalized = acteur::normalize_gp_mandats(gp_mandats);
        for event in &normalized.events {
            match event {
                NormalizationEvent::Inclus { .. } => stats.inclusions += 1,
                NormalizationEvent::Charniere { .. } => stats.charnieres += 1,
                NormalizationEvent::Chevauchement { .. } => stats.chevauchements += 1,
            }
        }

        for retenu in normalized.retained {
            let Some(organe_id) = organe_ids.get(&retenu.organe_uid).map(|o| o.id) else {
                continue;
            };
            let detail = gp_details.get(&retenu.an_uid);
            a_inserer.push(MandatARetenir {
                an_uid: retenu.an_uid,
                person_id,
                organe_id,
                type_organe: "GP".to_string(),
                debut: retenu.debut,
                fin: retenu.fin,
                qualite: detail.and_then(|d| d.qualite.clone()),
                legislature: detail.and_then(|d| d.legislature),
            });
        }

        for mandat in autres {
            let organe_id = organe_ids[mandat.organe_uid.as_ref().expect("filtré ci-dessus")].id;
            let debut = mandat.date_debut.expect("filtré ci-dessus");
            a_inserer.push(MandatARetenir {
                an_uid: mandat.an_uid.clone(),
                person_id,
                organe_id,
                type_organe: mandat.type_organe.clone().expect("filtré ci-dessus"),
                debut,
                fin: mandat.date_fin,
                qualite: mandat.qualite.clone(),
                legislature: mandat.legislature,
            });
        }
    }

    stats.geres = a_inserer.len();

    for batch in a_inserer.chunks(BATCH_SIZE) {
        let an_uids: Vec<&str> = batch.iter().map(|m| m.an_uid.as_str()).collect();
        let person_id_list: Vec<i64> = batch.iter().map(|m| m.person_id).collect();
        let organe_id_list: Vec<i64> = batch.iter().map(|m| m.organe_id).collect();
        let type_organes: Vec<&str> = batch.iter().map(|m| m.type_organe.as_str()).collect();
        let debuts: Vec<chrono::NaiveDate> = batch.iter().map(|m| m.debut).collect();
        let fins: Vec<Option<chrono::NaiveDate>> = batch.iter().map(|m| m.fin).collect();
        let qualites: Vec<Option<&str>> = batch.iter().map(|m| m.qualite.as_deref()).collect();
        let legislatures: Vec<Option<i16>> = batch.iter().map(|m| m.legislature).collect();

        sqlx::query!(
            r#"
            INSERT INTO mandat (an_uid, person_id, organe_id, type_organe, period, qualite, legislature)
            SELECT an_uid, person_id, organe_id, type_organe,
                   daterange(debut, fin + 1, '[)'),
                   qualite, legislature
            FROM UNNEST(
                $1::text[], $2::bigint[], $3::bigint[], $4::text[], $5::date[], $6::date[],
                $7::text[], $8::smallint[]
            ) AS t(an_uid, person_id, organe_id, type_organe, debut, fin, qualite, legislature)
            ON CONFLICT (an_uid) DO UPDATE SET
                person_id = EXCLUDED.person_id,
                organe_id = EXCLUDED.organe_id,
                type_organe = EXCLUDED.type_organe,
                period = EXCLUDED.period,
                qualite = EXCLUDED.qualite,
                legislature = EXCLUDED.legislature,
                updated_at = now()
            WHERE mandat.person_id IS DISTINCT FROM EXCLUDED.person_id
               OR mandat.organe_id IS DISTINCT FROM EXCLUDED.organe_id
               OR mandat.type_organe IS DISTINCT FROM EXCLUDED.type_organe
               OR mandat.period IS DISTINCT FROM EXCLUDED.period
               OR mandat.qualite IS DISTINCT FROM EXCLUDED.qualite
               OR mandat.legislature IS DISTINCT FROM EXCLUDED.legislature
            "#,
            &an_uids as _,
            &person_id_list,
            &organe_id_list,
            &type_organes as _,
            &debuts,
            &fins as _,
            &qualites as _,
            &legislatures as _,
        )
        .execute(pool)
        .await?;
    }

    Ok(stats)
}
