//! Test d'intégration du job `refresh_views` — voir docs/plans/phase-2-api-ui.md.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::{JobContext, refresh_views};
use sqlx::PgPool;

#[sqlx::test(migrations = "../db/migrations")]
async fn refreshes_person_apercu_after_a_new_vote(pool: PgPool) {
    let person_id = sqlx::query_scalar!("INSERT INTO person (an_uid) VALUES ('PA1') RETURNING id")
        .fetch_one(&pool)
        .await
        .unwrap();

    let job_id: i64 = sqlx::query_scalar!(
        "INSERT INTO job (type, payload) VALUES ('refresh_views', '{}'::jsonb) RETURNING id"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ctx = JobContext {
        pool: pool.clone(),
        worker_id: "test".to_string(),
        job_id,
    };

    refresh_views::run(&ctx, &serde_json::json!({}))
        .await
        .unwrap();

    // `person_apercu` a été créée par la migration (donc déjà rafraîchie une première fois à
    // l'instant zéro) : ce test vérifie surtout que `REFRESH ... CONCURRENTLY` s'exécute sans
    // erreur en dehors d'une transaction explicite, pas un changement de contenu.
    let votes_total: i64 = sqlx::query_scalar!(
        r#"SELECT votes_total AS "votes_total!" FROM person_apercu WHERE person_id = $1"#,
        person_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(votes_total, 0);

    let run_status = sqlx::query_scalar!(
        "SELECT status FROM ingestion_run WHERE source = 'refresh_views' ORDER BY id DESC LIMIT 1"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(run_status, "done");
}
