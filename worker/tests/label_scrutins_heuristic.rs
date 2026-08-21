//! Tests d'intégration du job `label_scrutins_heuristic` — voir
//! docs/plans/phase-3-categorisation.md, section « Job `label_scrutins_heuristic` ». Les cas des
//! fonctions pures (moyennes, séparation, bornage) sont couverts par `tests/axis.rs` ; ici, ce qui
//! ne se teste qu'avec une vraie base : résolution des `an_uid` contre `organe`, immuabilité d'une
//! version d'ancrage, écriture par lots, filtrage par périmètre, journalisation des refus.
//!
//! Chaque test ici écrit son propre fichier TOML éphémère, complet, inventé pour l'occasion —
//! indépendant du seed réel (`db/seeds/group_axis.toml`), qui porte la vraie grille des nuances
//! (voir phase-3.0-feedback.md, F2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::label_scrutins_heuristic::{self, Scope};
use sqlx::PgPool;
use tempfile::NamedTempFile;

/// Une vraie ligne `ingestion_run` (elle-même rattachée à une vraie ligne `job`) :
/// `label_scrutins_heuristic::label` y journalise ses anomalies (clé étrangère
/// `ingestion_anomaly.ingestion_run_id`, elle-même rattachée par `job_id`), un entier arbitraire
/// ne suffit pas.
async fn start_run(pool: &PgPool) -> i64 {
    let job_id = sqlx::query_scalar!(
        "INSERT INTO job (type) VALUES ('label_scrutins_heuristic') RETURNING id"
    )
    .fetch_one(pool)
    .await
    .unwrap();
    kyc_worker::run::start(
        pool,
        "label_scrutins_heuristic",
        job_id,
        serde_json::json!({}),
    )
    .await
    .unwrap()
}

fn write_toml(contents: &str) -> NamedTempFile {
    use std::io::Write as _;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file
}

/// Deux groupes, un de chaque bloc — de quoi couvrir un scrutin bipolaire simple. `version` et
/// `an_uid_gauche`/`an_uid_droite` sont paramétrés pour permettre aux tests d'immuabilité et de
/// résolution de faire varier un seul élément à la fois.
fn seed_toml(version: &str, an_uid_gauche: &str, an_uid_droite: &str) -> String {
    format!(
        r#"
        version = "{version}"
        description = "Ancrage de test"
        grille_version = "test-1"
        grille_date = 2026-01-01
        source_url = "https://example.org/grille"

        [bloc]
        gauche = -0.5
        droite = 0.5

        [[groupe]]
        an_uid = "{an_uid_gauche}"
        libelle = "Groupe gauche"
        nuance = "GA"
        bloc = "gauche"

        [[groupe]]
        an_uid = "{an_uid_droite}"
        libelle = "Groupe droite"
        nuance = "DR"
        bloc = "droite"
        "#
    )
}

async fn insert_organe(
    pool: &PgPool,
    an_uid: &str,
    code_type: &str,
    legislature: Option<i16>,
    is_non_inscrit: bool,
) -> i64 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO organe (an_uid, code_type, libelle, legislature, is_non_inscrit)
        VALUES ($1, $2, $1, $3, $4)
        RETURNING id
        "#,
        an_uid,
        code_type,
        legislature,
        is_non_inscrit,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_gp(pool: &PgPool, an_uid: &str, legislature: i16) -> i64 {
    insert_organe(pool, an_uid, "GP", Some(legislature), false).await
}

async fn insert_scrutin(pool: &PgPool, an_uid: &str, legislature: i16) -> i64 {
    let source_document_id = sqlx::query_scalar!(
        r#"
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        "#,
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query_scalar!(
        r#"
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, chambre, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, 1, $2, '2024-01-01', 'assemblee', 'SPO', 'titre', 'DecompteNominatif',
                0, 0, 0, 0, 0, 0, 577, $3)
        RETURNING id
        "#,
        an_uid,
        legislature,
        source_document_id,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_scrutin_groupe(
    pool: &PgPool,
    scrutin_id: i64,
    rang: i16,
    organe_id: Option<i64>,
    pour: i32,
    contre: i32,
) {
    sqlx::query!(
        r#"
        INSERT INTO scrutin_groupe
            (scrutin_id, rang, organe_id, nombre_membres, pour, contre, abstentions, non_votants)
        VALUES ($1, $2, $3, $4, $5, $6, 0, 0)
        "#,
        scrutin_id,
        rang,
        organe_id,
        pour + contre,
        pour,
        contre,
    )
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../db/migrations")]
async fn happy_path_writes_an_estimate_for_a_well_covered_scrutin(pool: PgPool) {
    let gauche = insert_gp(&pool, "PO_G", 17).await;
    let droite = insert_gp(&pool, "PO_D", 17).await;
    let scrutin_id = insert_scrutin(&pool, "SC1", 17).await;
    insert_scrutin_groupe(&pool, scrutin_id, 1, Some(gauche), 0, 100).await;
    insert_scrutin_groupe(&pool, scrutin_id, 2, Some(droite), 100, 0).await;

    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    let counters = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    assert_eq!(counters["estimations_ecrites"], 1);
    assert_eq!(counters["refus_couverture"], 0);
    assert_eq!(counters["refus_effectif"], 0);
    assert_eq!(counters["refus_unanime"], 0);

    let row = sqlx::query!(
        r#"SELECT position_pour::float8 AS "position_pour!", separation::float8 AS "separation!",
                  couverture::float8 AS "couverture!", votants_couverts, axis_version
           FROM scrutin_axis_estimate WHERE scrutin_id = $1"#,
        scrutin_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.axis_version, "v1");
    assert_eq!(row.votants_couverts, 200);

    let is_current: bool =
        sqlx::query_scalar!("SELECT is_current FROM group_axis WHERE version = 'v1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(is_current);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_seed_and_votes_is_idempotent(pool: PgPool) {
    let gauche = insert_gp(&pool, "PO_G", 17).await;
    let droite = insert_gp(&pool, "PO_D", 17).await;
    let scrutin_id = insert_scrutin(&pool, "SC1", 17).await;
    insert_scrutin_groupe(&pool, scrutin_id, 1, Some(gauche), 0, 100).await;
    insert_scrutin_groupe(&pool, scrutin_id, 2, Some(droite), 100, 0).await;

    let contents = seed_toml("v1", "PO_G", "PO_D");
    let first = write_toml(&contents);
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        first.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();
    let computed_at_first: chrono::DateTime<chrono::Utc> = sqlx::query_scalar!(
        "SELECT computed_at FROM scrutin_axis_estimate WHERE scrutin_id = $1",
        scrutin_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let second = write_toml(&contents);
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        second.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();
    let computed_at_second: chrono::DateTime<chrono::Utc> = sqlx::query_scalar!(
        "SELECT computed_at FROM scrutin_axis_estimate WHERE scrutin_id = $1",
        scrutin_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(
        computed_at_first, computed_at_second,
        "même seed, mêmes votes : aucune écriture au second passage"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_unknown_an_uid_fails_before_any_write(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    // PO_D n'existe pas en base.
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM group_axis"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_gp_organe_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_organe(&pool, "PO_D", "PARPOL", Some(17), false).await;
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_inscrit_organe_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_organe(&pool, "PO_D", "GP", Some(17), true).await;
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_legislature_outside_15_17_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 14).await;
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_duplicate_an_uid_in_the_seed_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_G"));

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_unknown_bloc_reference_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 17).await;
    let file = write_toml(
        r#"
        version = "v1"
        description = "d"
        grille_version = "test-1"
        grille_date = 2026-01-01
        source_url = "https://example.org/grille"

        [bloc]
        gauche = -0.5

        [[groupe]]
        an_uid = "PO_G"
        nuance = "GA"
        bloc = "gauche"

        [[groupe]]
        an_uid = "PO_D"
        nuance = "DR"
        bloc = "droite-inconnu"
        "#,
    );

    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_unknown_strategy_is_rejected(pool: PgPool) {
    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "principal_axis",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reloading_the_same_version_with_different_content_is_rejected(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 17).await;
    insert_gp(&pool, "PO_E", 17).await;

    let first = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        first.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    // Même version, groupe de droite différent : le contenu a changé.
    let second = write_toml(&seed_toml("v1", "PO_G", "PO_E"));
    let result = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        second.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await;
    assert!(result.is_err());

    let entry_count: i64 = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM group_axis_entry WHERE axis_version = 'v1'"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(entry_count, 2, "le contenu original n'a pas dû être touché");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn reloading_the_same_version_with_identical_content_is_a_no_op(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 17).await;

    let contents = seed_toml("v1", "PO_G", "PO_D");
    let first = write_toml(&contents);
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        first.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    let second = write_toml(&contents);
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        second.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    let axis_count: i64 = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM group_axis"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(axis_count, 1);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn loading_a_second_version_flips_is_current(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 17).await;
    insert_gp(&pool, "PO_E", 17).await;

    let first = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        first.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    let second = write_toml(&seed_toml("v2", "PO_G", "PO_E"));
    label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        second.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    let current: Vec<String> =
        sqlx::query_scalar!("SELECT version FROM group_axis WHERE is_current")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(current, vec!["v2".to_string()]);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn scope_legislature_filters_the_scan(pool: PgPool) {
    let gauche = insert_gp(&pool, "PO_G", 17).await;
    let droite = insert_gp(&pool, "PO_D", 17).await;
    let scrutin_17 = insert_scrutin(&pool, "SC17", 17).await;
    insert_scrutin_groupe(&pool, scrutin_17, 1, Some(gauche), 0, 100).await;
    insert_scrutin_groupe(&pool, scrutin_17, 2, Some(droite), 100, 0).await;

    let gauche16 = insert_gp(&pool, "PO_G16", 16).await;
    let droite16 = insert_gp(&pool, "PO_D16", 16).await;
    let scrutin_16 = insert_scrutin(&pool, "SC16", 16).await;
    insert_scrutin_groupe(&pool, scrutin_16, 1, Some(gauche16), 0, 100).await;
    insert_scrutin_groupe(&pool, scrutin_16, 2, Some(droite16), 100, 0).await;

    let file = write_toml(
        r#"
        version = "v1"
        description = "d"
        grille_version = "test-1"
        grille_date = 2026-01-01
        source_url = "https://example.org/grille"

        [bloc]
        gauche = -0.5
        droite = 0.5

        [[groupe]]
        an_uid = "PO_G"
        nuance = "GA"
        bloc = "gauche"
        [[groupe]]
        an_uid = "PO_D"
        nuance = "DR"
        bloc = "droite"
        [[groupe]]
        an_uid = "PO_G16"
        nuance = "GA"
        bloc = "gauche"
        [[groupe]]
        an_uid = "PO_D16"
        nuance = "DR"
        bloc = "droite"
        "#,
    );

    let counters = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope {
            legislature: Some(17),
            since: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(counters["scrutins_examines"], 1);
    let estimate_scrutin_id: i64 =
        sqlx::query_scalar!("SELECT scrutin_id FROM scrutin_axis_estimate")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(estimate_scrutin_id, scrutin_17);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_low_coverage_scrutin_is_refused_and_journalized(pool: PgPool) {
    let gauche = insert_gp(&pool, "PO_G", 17).await;
    let droite = insert_gp(&pool, "PO_D", 17).await;
    let scrutin_id = insert_scrutin(&pool, "SC1", 17).await;
    // Groupes couverts : 20 votants. Groupe hors ancrage (organe_id NULL) : 1000 votants. La
    // couverture tombe largement sous 90 %.
    insert_scrutin_groupe(&pool, scrutin_id, 1, Some(gauche), 0, 10).await;
    insert_scrutin_groupe(&pool, scrutin_id, 2, Some(droite), 10, 0).await;
    insert_scrutin_groupe(&pool, scrutin_id, 3, None, 500, 500).await;

    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    let counters = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    assert_eq!(counters["estimations_ecrites"], 0);
    assert_eq!(counters["refus_couverture"], 1);

    let anomaly_kind: String = sqlx::query_scalar!(
        "SELECT kind FROM ingestion_anomaly WHERE subject_uid = $1",
        scrutin_id.to_string(),
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(anomaly_kind, "ancrage_insuffisant");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_unanimous_scrutin_is_refused_and_journalized(pool: PgPool) {
    let gauche = insert_gp(&pool, "PO_G", 17).await;
    let droite = insert_gp(&pool, "PO_D", 17).await;
    let scrutin_id = insert_scrutin(&pool, "SC1", 17).await;
    insert_scrutin_groupe(&pool, scrutin_id, 1, Some(gauche), 30, 0).await;
    insert_scrutin_groupe(&pool, scrutin_id, 2, Some(droite), 30, 0).await;

    let file = write_toml(&seed_toml("v1", "PO_G", "PO_D"));
    let counters = label_scrutins_heuristic::label(
        &pool,
        start_run(&pool).await,
        "group_alignment",
        file.path().to_str().unwrap(),
        &Scope::default(),
    )
    .await
    .unwrap();

    assert_eq!(counters["estimations_ecrites"], 0);
    assert_eq!(counters["refus_unanime"], 1);
}

/// Revenir à une version d'ancrage déjà chargée — un retour en arrière de grille, que ce job
/// autorise explicitement puisqu'un rechargement à contenu identique est idempotent.
///
/// Ce test échoue sur un unique `UPDATE group_axis SET is_current = (version = $1)` : l'index
/// unique partiel `group_axis_courant_idx` n'est pas DEFERRABLE, donc Postgres le vérifie ligne à
/// ligne dans l'ordre physique de balayage. Ici la ligne à ACTIVER (`v1`, insérée en premier) est
/// balayée avant celle à DÉSACTIVER (`v2`) : les deux sont `is_current` en même temps l'espace
/// d'une ligne, et la contrainte rejette. Même piège que dans `recompute_scores` (phase 4.1), à la
/// différence près que le déclencheur n'est pas le nombre de lignes mais leur ordre physique — il
/// suffit de deux.
#[sqlx::test(migrations = "../db/migrations")]
async fn returning_to_an_earlier_version_flips_is_current_back(pool: PgPool) {
    insert_gp(&pool, "PO_G", 17).await;
    insert_gp(&pool, "PO_D", 17).await;
    insert_gp(&pool, "PO_E", 17).await;

    for (version, droite) in [("v1", "PO_D"), ("v2", "PO_E"), ("v1", "PO_D")] {
        let file = write_toml(&seed_toml(version, "PO_G", droite));
        label_scrutins_heuristic::label(
            &pool,
            start_run(&pool).await,
            "group_alignment",
            file.path().to_str().unwrap(),
            &Scope::default(),
        )
        .await
        .unwrap();
    }

    let current: Vec<String> =
        sqlx::query_scalar!("SELECT version FROM group_axis WHERE is_current")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(current, vec!["v1".to_string()]);
}
