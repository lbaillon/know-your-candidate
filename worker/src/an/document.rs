//! Archivage du brut avant tout parsing — voir docs/plans/phase-1-ingestion.md, « Règles
//! d'ingestion » : un document par scrutin et par acteur, dans `source_document`, avant toute
//! interprétation. Partagé entre `ingest_acteurs` et `ingest_scrutins`.

use std::collections::HashMap;

use serde_json::Value;
use sqlx::PgPool;

pub struct RawDocument {
    pub uid: String,
    pub payload: Value,
    pub content_hash: String,
}

const BATCH_SIZE: usize = 1000;

/// `ON CONFLICT ... DO NOTHING` : la clé unique porte le hash (voir migration 0002), un payload
/// identique à ce qui est déjà archivé n'ajoute donc jamais de ligne.
pub async fn archive_documents(
    pool: &PgPool,
    source: &str,
    url: &str,
    docs: &[RawDocument],
) -> sqlx::Result<()> {
    for batch in docs.chunks(BATCH_SIZE) {
        let sources: Vec<&str> = batch.iter().map(|_| source).collect();
        let uids: Vec<&str> = batch.iter().map(|d| d.uid.as_str()).collect();
        let urls: Vec<&str> = batch.iter().map(|_| url).collect();
        let hashes: Vec<&str> = batch.iter().map(|d| d.content_hash.as_str()).collect();
        let payloads: Vec<Value> = batch.iter().map(|d| d.payload.clone()).collect();

        sqlx::query!(
            r#"
            INSERT INTO source_document (source, uid, url, content_hash, payload)
            SELECT * FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::jsonb[])
            ON CONFLICT (source, uid, content_hash) DO NOTHING
            "#,
            &sources as _,
            &uids as _,
            &urls as _,
            &hashes as _,
            &payloads,
        )
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Renvoie l'id `source_document` **le plus récent** pour chaque `uid` de la source donnée — un
/// même `uid` peut porter plusieurs lignes (une par `content_hash` observé), et c'est toujours la
/// plus récente qui doit être référencée par `scrutin.source_document_id` /
/// `vote_mise_au_point.source_document_id`.
pub async fn fetch_latest_document_ids(
    pool: &PgPool,
    source: &str,
) -> sqlx::Result<HashMap<String, i64>> {
    let rows = sqlx::query!(
        r#"
        SELECT DISTINCT ON (uid) uid, id
        FROM source_document
        WHERE source = $1
        ORDER BY uid, fetched_at DESC
        "#,
        source,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.uid, r.id)).collect())
}
