//! Écriture dans `ingestion_anomaly` — vocabulaire fermé des `kind`, en constantes plutôt qu'en
//! chaînes disséminées (voir docs/plans/phase-1-ingestion.md, migration 0004). Une anomalie se
//! journalise, elle n'interrompt jamais une ingestion : c'est elle qui remplace la file de
//! réconciliation manuelle prévue initialement par le plan.

use serde_json::Value;
use sqlx::PgPool;

pub const ACTEUR_INCONNU: &str = "acteur_inconnu";
pub const MANDAT_INCLUS: &str = "mandat_inclus";
pub const MANDAT_CHARNIERE: &str = "mandat_charniere";
pub const MANDAT_CHEVAUCHEMENT: &str = "mandat_chevauchement";
pub const GROUPE_FANTOME: &str = "groupe_fantome";
pub const CAUSE_NON_VOTE_INCONNUE: &str = "cause_non_vote_inconnue";
pub const BLOC_NOMINATIF_INCONNU: &str = "bloc_nominatif_inconnu";
pub const COMPTEURS_INCOHERENTS: &str = "compteurs_incoherents";
#[allow(dead_code)] // utilisé par enrich_wikidata
pub const PHOTO_SANS_LICENCE: &str = "photo_sans_licence";
#[allow(dead_code)] // utilisé par enrich_wikidata
pub const WIKIDATA_QID_AMBIGU: &str = "wikidata_qid_ambigu";
pub const SLUG_DERIVE_D_IDENTIFIANT: &str = "slug_derive_d_identifiant";
pub const ANCRAGE_INSUFFISANT: &str = "ancrage_insuffisant";
pub const TROP_PEU_DE_VOTANTS: &str = "trop_peu_de_votants";
pub const SCRUTIN_UNANIME: &str = "scrutin_unanime";

pub struct AnomalyRecord {
    pub kind: &'static str,
    pub subject_uid: Option<String>,
    pub detail: Value,
}

const BATCH_SIZE: usize = 1000;

/// Écrit un lot d'anomalies pour un `ingestion_run`. Un lot vide n'émet aucune requête.
pub async fn record_many(
    pool: &PgPool,
    ingestion_run_id: i64,
    records: &[AnomalyRecord],
) -> sqlx::Result<()> {
    for batch in records.chunks(BATCH_SIZE) {
        let run_ids: Vec<i64> = batch.iter().map(|_| ingestion_run_id).collect();
        let kinds: Vec<&str> = batch.iter().map(|r| r.kind).collect();
        let subjects: Vec<Option<&str>> = batch.iter().map(|r| r.subject_uid.as_deref()).collect();
        let details: Vec<Value> = batch.iter().map(|r| r.detail.clone()).collect();

        sqlx::query!(
            r#"
            INSERT INTO ingestion_anomaly (ingestion_run_id, kind, subject_uid, detail)
            SELECT * FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::jsonb[])
            "#,
            &run_ids,
            &kinds as _,
            &subjects as _,
            &details,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}
