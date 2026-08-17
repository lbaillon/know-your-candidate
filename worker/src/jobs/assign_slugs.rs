//! Job `assign_slugs {}` — voir docs/plans/phase-2-api-ui.md, section « assign_slugs ». N'écrase
//! jamais un slug existant (D2.3) : c'est cette propriété qui rend les URL stables. Traite les
//! personnes sans slug courant dans l'ordre de `person.id`, pour un résultat déterministe.

use std::collections::HashSet;

use serde_json::Value;
use sqlx::PgPool;

use crate::anomaly::{self, AnomalyRecord};
use crate::run;
use crate::slug::slugify;

use super::JobContext;

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let run_id = run::start(&ctx.pool, "assign_slugs", ctx.job_id, payload.clone()).await?;

    match assign(&ctx.pool, run_id).await {
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

struct PersonSansSlug {
    id: i64,
    an_uid: Option<String>,
    wikidata_qid: Option<String>,
    prenom: Option<String>,
    nom: Option<String>,
}

/// Séparée de `run` pour permettre aux tests d'intégration de l'appeler directement avec un
/// `run_id` créé à la main — sur le modèle de `ingest_bytes` des autres jobs.
pub async fn assign(pool: &PgPool, run_id: i64) -> anyhow::Result<Value> {
    let candidates = sqlx::query_as!(
        PersonSansSlug,
        r#"
        SELECT p.id, p.an_uid, p.wikidata_qid, p.prenom, p.nom
        FROM person p
        LEFT JOIN person_slug s ON s.person_id = p.id AND s.is_current
        WHERE s.person_id IS NULL
        ORDER BY p.id
        "#
    )
    .fetch_all(pool)
    .await?;

    // Un slug non courant reste réservé (voir migration 0005) : la redirection d'un ancien lien ne
    // doit jamais finir par pointer vers quelqu'un d'autre. On charge donc tous les slugs déjà
    // attribués, pas seulement les courants.
    let mut taken: HashSet<String> = sqlx::query_scalar!("SELECT slug FROM person_slug")
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();

    let mut anomalies = Vec::new();
    let mut derives_d_identifiant = 0usize;

    for person in &candidates {
        let nom_complet = format!(
            "{} {}",
            person.prenom.as_deref().unwrap_or(""),
            person.nom.as_deref().unwrap_or(""),
        );
        let mut base = slugify(&nom_complet);

        if base.is_empty() {
            let identifiant = person
                .an_uid
                .as_deref()
                .or(person.wikidata_qid.as_deref())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "personne {} sans an_uid ni wikidata_qid : la contrainte \
                         person_a_au_moins_un_identifiant aurait dû l'empêcher",
                        person.id
                    )
                })?;
            base = identifiant.to_lowercase();
            derives_d_identifiant += 1;
            anomalies.push(AnomalyRecord {
                kind: anomaly::SLUG_DERIVE_D_IDENTIFIANT,
                subject_uid: person
                    .an_uid
                    .clone()
                    .or_else(|| person.wikidata_qid.clone()),
                detail: serde_json::json!({"person_id": person.id}),
            });
        }

        let mut slug = base.clone();
        let mut suffix = 2;
        while taken.contains(&slug) {
            slug = format!("{base}-{suffix}");
            suffix += 1;
        }

        sqlx::query!(
            "INSERT INTO person_slug (slug, person_id, is_current) VALUES ($1, $2, true)",
            slug,
            person.id,
        )
        .execute(pool)
        .await?;

        taken.insert(slug);
    }

    anomaly::record_many(pool, run_id, &anomalies).await?;

    Ok(serde_json::json!({
        "personnes_traitees": candidates.len(),
        "slugs_derives_d_identifiant": derives_d_identifiant,
    }))
}
