//! Tests d'intégration du job `ingest_scrutins` sur les fixtures JSON versionnées (voir
//! `fixtures/README.md`). Le référentiel (`ingest_acteurs`) est ingéré d'abord, comme en
//! production (« Ordre d'exécution », docs/plans/phase-1-ingestion.md). Le zip lu par le job est
//! reconstruit en mémoire par `support::zip_fixture_dir` / `zip_fixture_files` : le contenu des
//! fixtures est inventé (D1.17, docs/plans/phase-1.1-fix.md), donc les compteurs ci-dessous sont
//! propres à cette fixture, pas ceux du corpus réel.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use chrono::NaiveDate;
use kyc_worker::jobs::{JobContext, ingest_acteurs, ingest_scrutins};
use kyc_worker::run;
use sqlx::PgPool;

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
    ingest_acteurs::ingest_bytes(pool, run_id, support::zip_fixture_dir("amo30"))
        .await
        .unwrap();
}

async fn run_fixture_since(pool: &PgPool, since: Option<NaiveDate>) -> (i64, serde_json::Value) {
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
        since,
        "fixture://scrutins",
        support::zip_fixture_dir("scrutins"),
    )
    .await
    .unwrap();
    (run_id, counters)
}

async fn run_fixture(pool: &PgPool) -> (i64, serde_json::Value) {
    run_fixture_since(pool, None).await
}

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
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

    assert_eq!(counters["scrutins"], 5);
    assert_eq!(counters["votes"], 10 + 3 + 1 + 4 + 1);
    assert_eq!(counters["mises_au_point"], 2);

    let scrutin_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM scrutin")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(scrutin_count, 5);
}

/// Le scrutin du Congrès nomme ses blocs nominatifs au singulier (`pour`/`contre`/`abstention`) :
/// un parseur qui ne connaît que le pluriel les ingérerait à zéro vote sans erreur (voir
/// data-sources.md). Vérifié ici par le nombre de votes réellement enregistrés pour
/// VTCGR5L16V900001, et par le groupe d'un type non retenu (`GROUPESENAT`) qui l'accompagne.
#[sqlx::test(migrations = "../db/migrations")]
async fn parses_congres_singular_blocks_and_untracked_organe_type(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, _) = run_fixture(&pool).await;

    let votes: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM vote v
        JOIN scrutin s ON s.id = v.scrutin_id
        WHERE s.an_uid = 'VTCGR5L16V900001'
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        votes, 4,
        "les votes du Congrès ne doivent pas être ignorés"
    );

    let chambre: String = sqlx::query_scalar!(
        r#"SELECT chambre::text AS "chambre!" FROM scrutin WHERE an_uid = 'VTCGR5L16V900001'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(chambre, "congres");

    // Le groupe PO900007 (GROUPESENAT) n'est pas d'un type retenu par ingest_acteurs (D1.12) :
    // il tombe dans le même chemin que PO0, groupe_organe_id NULL.
    let ghost_row: bool = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM scrutin_groupe sg
            JOIN scrutin s ON s.id = sg.scrutin_id
            WHERE s.an_uid = 'VTCGR5L16V900001' AND sg.rang = 1 AND sg.organe_id IS NULL
        ) AS "exists!"
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(ghost_row);

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'groupe_fantome' AND subject_uid = 'VTCGR5L16V900001'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 1);
}

/// 3 lignes `scrutin_groupe` portant toutes `PO0` pour le même scrutin (voir migration 0003, la
/// clé est `(scrutin_id, rang)` et non `(scrutin_id, organe_id)` précisément pour ce cas).
#[sqlx::test(migrations = "../db/migrations")]
async fn ghost_group_po0_produces_several_rows_and_an_anomaly(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    // 3 lignes PO0 (V900002) + 1 groupe GROUPESENAT (V900004, voir le test dédié) = 4.
    assert_eq!(counters["groupes_fantomes"], 4);

    let ghost_rows: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM scrutin_groupe sg
        JOIN scrutin s ON s.id = sg.scrutin_id
        WHERE s.an_uid = 'VTANR5L17V900002' AND sg.organe_id IS NULL
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ghost_rows, 3);

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
    assert_eq!(anomaly_count, 4);
}

/// `PA900020` (sans mandat de groupe) et `PA900021` (avec un mandat de groupe couvrant la date)
/// votent tous deux sur une ligne fantôme de VTANR5L17V900002 — voir fixtures/README.md.
/// **Attendu à corriger par F4** (docs/plans/phase-1.1-fix.md) : `groupe_from_mandat` vaut encore
/// `true` inconditionnellement aujourd'hui, y compris quand aucun mandat n'a été trouvé pour
/// `PA900020`. Ne pas retoucher cette note en dehors du commit F4.
#[sqlx::test(migrations = "../db/migrations")]
async fn groupe_from_mandat_reflects_whether_a_mandat_was_actually_found(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;

    let (groupe_organe_id, groupe_from_mandat): (Option<i64>, bool) = sqlx::query_as(
        r#"
        SELECT v.groupe_organe_id, v.groupe_from_mandat
        FROM vote v
        JOIN person p ON p.id = v.person_id
        JOIN scrutin s ON s.id = v.scrutin_id
        WHERE p.an_uid = 'PA900020' AND s.an_uid = 'VTANR5L17V900002'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        groupe_organe_id, None,
        "aucun mandat de groupe : pas de résolution possible"
    );
    assert!(groupe_from_mandat, "bug F4 : encore vrai inconditionnellement avant le correctif");

    let (groupe_organe_id, groupe_from_mandat): (Option<i64>, bool) = sqlx::query_as(
        r#"
        SELECT v.groupe_organe_id, v.groupe_from_mandat
        FROM vote v
        JOIN person p ON p.id = v.person_id
        JOIN scrutin s ON s.id = v.scrutin_id
        WHERE p.an_uid = 'PA900021' AND s.an_uid = 'VTANR5L17V900002'
        "#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let alpha_id: i64 = sqlx::query_scalar!("SELECT id FROM organe WHERE an_uid = 'PO900001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        groupe_organe_id,
        Some(alpha_id),
        "un mandat de groupe couvrant la date doit être résolu"
    );
    assert!(groupe_from_mandat);
}

/// VTANR5L17V900003 est délibérément laissé incohérent par la fixture (voir `fixtures/README.md`)
/// pour exercer le contrôle `total nominatif = nombreVotants + nonVotants`.
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
    assert_eq!(anomaly.as_deref(), Some("VTANR5L17V900003"));
}

/// PA900099, PA900023 et PA900024 sont cités par les scrutins mais absents du référentiel AMO30
/// (voir fixtures/README.md) : le vote n'est pas perdu, une `person` réduite à son `an_uid` est
/// créée et l'anomalie journalisée.
#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_acteur_creates_a_minimal_person_instead_of_dropping_the_vote(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["acteurs_inconnus"], 3);

    for uid in ["PA900099", "PA900023", "PA900024"] {
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
    assert_eq!(anomaly_count, 3);
}

/// Cause de non-vote hors des trois connues (`PSE`, `PAN`, `MG`) : journalisée, pas fatale.
#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_non_vote_cause_is_journalized(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, _) = run_fixture(&pool).await;

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'cause_non_vote_inconnue'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 1);
}

/// `blocMystere` (V900001) est un nom de bloc hors des huit graphies connues.
#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_nominatif_block_is_journalized(pool: PgPool) {
    ingest_referentiel(&pool).await;
    let (run_id, counters) = run_fixture(&pool).await;

    assert_eq!(counters["blocs_nominatifs_inconnus"], 1);

    let anomaly_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!" FROM ingestion_anomaly
        WHERE ingestion_run_id = $1 AND kind = 'bloc_nominatif_inconnu'
        "#,
        run_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_count, 1);
}

/// La mise au point de VTANR5L17V900001 mélange de vraies entrées et des blocs `[null, null]`
/// (artefact de la conversion XML) qui ne doivent produire aucune ligne.
#[sqlx::test(migrations = "../db/migrations")]
async fn mise_au_point_ignores_null_placeholder_blocks(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;

    let rows: i64 = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
        FROM vote_mise_au_point m
        JOIN scrutin s ON s.id = m.scrutin_id
        WHERE s.an_uid = 'VTANR5L17V900001'
        "#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rows, 2);
}

/// Le filtre `since` s'applique après parsing (sur `dateScrutin`, pas sur le nom de fichier) :
/// seuls les scrutins postérieurs entrent, et ceux déjà en base restent intacts.
#[sqlx::test(migrations = "../db/migrations")]
async fn since_filters_by_date_scrutin_without_touching_the_rest(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;

    let (run_id, counters) = run_fixture_since(&pool, Some(date("2025-03-01"))).await;
    assert_eq!(counters["scrutins"], 3);
    assert_eq!(counters["votes"], 10 + 3 + 1);

    let scrutin_count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM scrutin")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        scrutin_count, 5,
        "les scrutins déjà en base avant `since` ne doivent pas disparaître"
    );

    for uid in ["VTCGR5L16V900001", "VTANR5L15V900001"] {
        let exists: bool = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM scrutin WHERE an_uid = $1) AS \"exists!\"",
            uid,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(exists, "{uid} est antérieur à `since` mais était déjà en base");
    }

    let _ = run_id;
}

/// Vérification n° 5 de la phase 1 : ingérer une seule législature sur une base portant déjà les
/// trois ne modifie rien d'autre.
#[sqlx::test(migrations = "../db/migrations")]
async fn ingesting_a_single_legislature_does_not_touch_the_others(pool: PgPool) {
    ingest_referentiel(&pool).await;
    run_fixture(&pool).await;

    let scrutin_before = checksum(&pool, "scrutin").await;
    let vote_before = checksum(&pool, "vote").await;
    let groupe_before = checksum(&pool, "scrutin_groupe").await;

    let job_id = insert_job(&pool, "ingest_scrutins").await;
    let ctx = JobContext {
        pool: pool.clone(),
        worker_id: "test".to_string(),
        job_id,
    };
    let run_id = run::start(&pool, "an_scrutins", job_id, serde_json::json!({}))
        .await
        .unwrap();
    let seizieme_seulement =
        support::zip_fixture_files("scrutins", &["json/VTCGR5L16V900001.json"]);
    let counters = ingest_scrutins::ingest_bytes(
        &ctx,
        run_id,
        16,
        None,
        "fixture://scrutins-16-seule",
        seizieme_seulement,
    )
    .await
    .unwrap();
    assert_eq!(counters["scrutins"], 1);

    let scrutin_after = checksum(&pool, "scrutin").await;
    let vote_after = checksum(&pool, "vote").await;
    let groupe_after = checksum(&pool, "scrutin_groupe").await;

    assert_eq!(scrutin_before, scrutin_after);
    assert_eq!(vote_before, vote_after);
    assert_eq!(groupe_before, groupe_after);
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
