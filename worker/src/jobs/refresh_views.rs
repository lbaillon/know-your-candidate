//! Job `refresh_views {}` — voir docs/plans/phase-2-api-ui.md. `REFRESH MATERIALIZED VIEW
//! CONCURRENTLY` exige un index unique non partiel sur la vue (posé par la migration 0005,
//! `person_apercu_pk_idx`) et ne peut pas s'exécuter dans une transaction ; `sqlx::query!` n'ouvre
//! pas de transaction implicite pour une requête simple, donc rien de spécial n'est nécessaire ici.

use serde_json::Value;

use crate::run;

use super::JobContext;

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let run_id = run::start(&ctx.pool, "refresh_views", ctx.job_id, payload.clone()).await?;

    match sqlx::query!("REFRESH MATERIALIZED VIEW CONCURRENTLY person_apercu")
        .execute(&ctx.pool)
        .await
    {
        Ok(_) => {
            run::finish_ok(&ctx.pool, run_id, serde_json::json!({}), None, None).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err.into())
        }
    }
}
