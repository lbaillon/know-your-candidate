//! Tests d'intégration du job `ingest_scrutins` sur l'extrait de scrutins versionné (voir
//! `fixtures/README.md`). Le référentiel (`ingest_acteurs`) est ingéré d'abord, comme en
//! production (« Ordre d'exécution », docs/plans/phase-1-ingestion.md).

#![allow(clippy::unwrap_used)]

use kyc_worker::jobs::{JobContext, ingest_acteurs, ingest_scrutins};
use kyc_worker::run;
use sqlx::PgPool;

const AMO30_FIXTURE: &[u8] = include_bytes!("fixtures/amo30-extrait.zip");
const SCRUTINS_FIXTURE: &[u8] = include_bytes!("fixtures/scrutins-extrait.zip");

async fn insert_job(pool: &PgPool, job_type: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO job (type, payload) VALUES ($1, '{}'::jsonb) RETURNING id",
        job_type,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn ingest_referentiel(pool: &PgPool) {
    let job_id = insert_job(pool, "ingest_acteurs").await;
    let run_id = run::start(pool, "an_amo30", job_id, serde_json::json!({}))
        .await
        .unwrap();
    ingest_acteurs::ingest_bytes(pool, run_id, bytes::Bytes::from_static(AMO30_FIXTURE))
        .await
        .unwrap();
}

async fn run_fixture(pool: &PgPool) -> (i64, serde_json::Value) {
    let job_id = insert_job(pool, "ingest_scrutins").await;
    let ctx = JobContext {
        pool: pool.clone(),
        worker_id: "test".to_string(),
        job_id,
    };
    let run_id = run::start(pool, "an_scrutins", job_id, serde_json::json!({}))
        .await
        .unwrap();
    // La fixture mélange les législatures 15/16/17 : `legislature` n'étiquette que les compteurs
    // du run, chaque scrutin garde la sienne propre (lue dans son JSON, voir `parse_scrutin`).
    let counters = ingest_scrutins::ingest_bytes(
        &ctx,
        run_id,
        17,
        None,
        "fixture://scrutins-extrait",
        bytes::Bytes::from_static(SCRUTINS_FIXTURE),
    )
    .await
    .unwrap();
    (run_id, counters)
}

async fn checksum(pool: &PgPool, table: &str) -> String {
    let query = format!("SELECT md5(string_agg(t::text, '|' ORDER BY t::text)) FROM {table} t");
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn ingests_scrutins_groupes_votes_and_mise_au_point(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (_, counters) = run_fixture(&pool).await;

    assert_eq!(counters["scrutins"], 4);
    assert_eq!(counters["votes"], 11 + 12 + 22 + 4);

    let scrutin_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM scrutin")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(scrutin_count, 4);
}

/// Le scrutin du Congrès nomme ses blocs nominatifs au singulier (`pour`/`contre`/`abstention`) :
/// un parseur qui ne connaît que le pluriel les ingérerait à zéro vote sans erreur (voir
/// data-sources.md). Vérifié ici par le nombre de votes réellement enregistrés pour VTCGR5L16V1.
#[sqlx::test(migrations = "../db/migrations")]
async fn parses_congres_singular_blocks(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;

    let votes: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM vote v
        JOIN scrutin s ON s.id = v.scrutin_id
        WHERE s.an_uid = 'VTCGR5L16V1'
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        votes, 22,
        "les votes du Congrès ne doivent pas être ignorés"
    );

    let chambre: String = sqlx::query_scalar!(
        r#"SELECT chambre::text AS "chambre!" FROM scrutin WHERE an_uid = 'VTCGR5L16V1'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(chambre, "congres");
}

/// 5 lignes `scrutin_groupe` portant toutes `PO0` pour le même scrutin (voir migration 0003, la
/// clé est `(scrutin_id, rang)` et non `(scrutin_id, organe_id)` précisément pour ce cas).
///
/// Le total global (`groupes_fantomes` = 7) est plus élevé que les 5 lignes `PO0` : le Congrès
/// mélange des groupes sénatoriaux (`codeType = GROUPESENAT`) que `ingest_acteurs` ne retient pas
/// (D1.12, seuls `GP`/`PARPOL`/`ASSEMBLEE` le sont). Découverte faite en construisant cette
/// fixture, pas un bug — `groupe_organe_id` finit à `NULL` pour ces votant·es par le même chemin
/// de repli que `PO0`, ce qui est le comportement correct pour des sénateurs votant au Congrès.
#[sqlx::test(migrations = "../db/migrations")]
async fn ghost_group_po0_produces_several_rows_and_an_anomaly(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["groupes_fantomes"], 7);

    let ghost_rows: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM scrutin_groupe sg
        JOIN scrutin s ON s.id = sg.scrutin_id
        WHERE s.an_uid = 'VTANR5L17V501' AND sg.organe_id IS NULL
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ghost_rows, 5);

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'groupe_fantome'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 7);
}

/// VTANR5L17V1 est délibérément laissé incohérent par la fixture (voir `fixtures/README.md`) pour
/// exercer le contrôle `total nominatif = nombreVotants + nonVotants`.
#[sqlx::test(migrations = "../db/migrations")]
async fn inconsistent_counters_are_journalized_not_fatal(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["compteurs_incoherents"], 1);

    let anomaly: Option<String> = sqlx::query_scalar!(
        r#"
        SELECT subject_uid FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'compteurs_incoherents'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly.as_deref(), Some("VTANR5L17V1"));
}

/// PA720634 et PA429842 sont cités par le scrutin du Congrès mais absents du référentiel AMO30
/// (voir fixtures/README.md et data-sources.md) : le vote n'est pas perdu, une `person` réduite à
/// son `an_uid` est créée et l'anomalie journalisée.
#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_acteur_creates_a_minimal_person_instead_of_dropping_the_vote(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["acteurs_inconnus"], 2);

    for uid in ["PA720634", "PA429842"] {
        let exists: bool = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM person WHERE an_uid = $1) AS \"exists!\"",
            uid,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{uid} doit exister même sans référentiel");
    }

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'acteur_inconnu'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 2);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_fixture_is_a_no_op(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;
    let scrutin_before = checksum(&pool, "scrutin").await;
    let vote_before = checksum(&pool, "vote").await;
    let groupe_before = checksum(&pool, "scrutin_groupe").await;
    let map_before = checksum(&pool, "vote_mise_au_point").await;

    run_fixture(&pool).await;
    let scrutin_after = checksum(&pool, "scrutin").await;
    let vote_after = checksum(&pool, "vote").await;
    let groupe_after = checksum(&pool, "scrutin_groupe").await;
    let map_after = checksum(&pool, "vote_mise_au_point").await;

    assert_eq!(scrutin_before, scrutin_after, "scrutin doit être inchangé");
    assert_eq!(vote_before, vote_after, "vote doit être inchangé");
    assert_eq!(
        groupe_before, groupe_after,
        "scrutin_groupe doit être inchangé"
    );
    assert_eq!(
        map_before, map_after,
        "vote_mise_au_point doit être inchangé"
    );
}
