use std::time::Duration;

use sqlx::PgPool;

use super::queue;

/// Job factice de la phase 0 : attend quelques secondes en publiant sa progression, pour prouver
/// que la chaîne backend → job → worker → base fonctionne de bout en bout.
pub async fn run(pool: &PgPool, job_id: i64, payload: &serde_json::Value) -> anyhow::Result<()> {
    let seconds = payload
        .get("seconds")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(3)
        .clamp(1, 30);

    for step in 1..=seconds {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let progress = step as f32 / seconds as f32;
        queue::set_progress(pool, job_id, progress, &format!("étape {step}/{seconds}")).await?;
    }

    Ok(())
}
