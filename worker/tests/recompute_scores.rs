//! Tests d'intégration du job `recompute_scores` — voir docs/plans/phase-4-partis-scores.md,
//! section « Stratégie de test », niveau 2 : idempotence, respect des deux seuils, restriction par
//! période de mandat, exclusion des non-inscrits, bascule de `is_current`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::NaiveDate;
use kyc_worker::jobs::recompute_scores;
use sqlx::PgPool;

async fn insert_theme(pool: &PgPool, slug: &str) -> i16 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO theme (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description', 'pôle négatif', 'pôle positif', 1)
        RETURNING id
        "#,
        slug,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_person(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid, nom, prenom) VALUES ($1, 'Dupont', 'Jean') RETURNING id",
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_organe(pool: &PgPool, an_uid: &str, is_non_inscrit: bool) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO organe (an_uid, code_type, libelle, is_non_inscrit) VALUES ($1, 'GP', 'Groupe de test', $2) RETURNING id",
        an_uid,
        is_non_inscrit,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_mandat(
    pool: &PgPool,
    an_uid: &str,
    person_id: i64,
    organe_id: i64,
    debut: NaiveDate,
    fin_exclusive: Option<NaiveDate>,
) -> i64 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO mandat (an_uid, person_id, organe_id, type_organe, period)
        VALUES ($1, $2, $3, 'GP', daterange($4, $5, '[)'))
        RETURNING id
        "#,
        an_uid,
        person_id,
        organe_id,
        debut,
        fin_exclusive,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_scrutin(pool: &PgPool, an_uid: &str, numero: i32, date_scrutin: NaiveDate) -> i64 {
    let source_document_id = sqlx::query_scalar!(
        r#"
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', $1, '{}'::jsonb)
        RETURNING id
        "#,
        an_uid,
    )
    .fetch_one(pool)
    .await
    .unwrap();

    sqlx::query_scalar!(
        r#"
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, $2, 17, $3, 'SPO', 'titre', 'DecompteNominatif', 0, 0, 0, 0, 0, 0, 577, $4)
        RETURNING id
        "#,
        an_uid,
        numero,
        date_scrutin,
        source_document_id,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_admin_user(pool: &PgPool) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO admin_user (github_id, github_login, display_name) VALUES (1, 'alice', 'alice') RETURNING id"
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
async fn insert_scrutin_label(
    pool: &PgPool,
    scrutin_id: i64,
    theme_id: i16,
    position_pour: f64,
    poids: f64,
    confiance: f64,
    author_id: i64,
) {
    sqlx::query!(
        r#"
        INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
        VALUES ($1, $2, $3::float8::numeric, $4::float8::numeric, $5::float8::numeric,
                'justification suffisamment longue', 'manual', $6)
        "#,
        scrutin_id,
        theme_id,
        poids,
        position_pour,
        confiance,
        author_id,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_vote(
    pool: &PgPool,
    scrutin_id: i64,
    person_id: i64,
    position: &str,
    groupe_organe_id: Option<i64>,
) {
    sqlx::query!(
        r#"
        INSERT INTO vote (scrutin_id, person_id, position, groupe_organe_id)
        VALUES ($1, $2, $3::text::vote_position, $4)
        "#,
        scrutin_id,
        person_id,
        position,
        groupe_organe_id,
    )
    .execute(pool)
    .await
    .unwrap();
}

#[allow(clippy::too_many_arguments)]
async fn insert_scrutin_groupe(
    pool: &PgPool,
    scrutin_id: i64,
    rang: i16,
    organe_id: i64,
    pour: i32,
    contre: i32,
) {
    sqlx::query!(
        r#"
        INSERT INTO scrutin_groupe (scrutin_id, rang, organe_id, nombre_membres, pour, contre, abstentions, non_votants)
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

async fn insert_group_axis(pool: &PgPool, version: &str) {
    sqlx::query!(
        r#"INSERT INTO group_axis (version, description, grille_version, grille_date, source_url, content_hash, is_current)
           VALUES ($1, 'd', 'g1', '2026-01-01', 'https://example.org', $1, true)"#,
        version,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_estimate(pool: &PgPool, scrutin_id: i64, version: &str, bipolarite: f64) {
    sqlx::query!(
        r#"
        INSERT INTO scrutin_axis_estimate
            (scrutin_id, strategy, axis_version, position_pour, separation, couverture, votants_couverts, bipolarite)
        VALUES ($1, 'group_alignment', $2, 0.000, 0.900, 0.950, 400, $3::float8::numeric)
        "#,
        scrutin_id,
        version,
        bipolarite,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn set_parametres(pool: &PgPool, contributions_min: i16, scrutins_min_par_theme: i16) {
    sqlx::query!(
        "UPDATE score_parametre SET contributions_min = $1, scrutins_min_par_theme = $2",
        contributions_min,
        scrutins_min_par_theme,
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Prépare `n` scrutins catégorisés sur `theme_id`, tous avec un axe exploitable (bipolarité
/// nulle), et fait voter `person_id` « pour » sur chacun. Renvoie les ids de scrutin, pour que le
/// test puisse ajouter d'autres votes ou groupes dessus.
async fn seed_categorized_scrutins(
    pool: &PgPool,
    theme_id: i16,
    author_id: i64,
    axis_version: &str,
    count: i32,
    date_base: NaiveDate,
) -> Vec<i64> {
    let mut ids = Vec::with_capacity(count as usize);
    for i in 0..count {
        let an_uid = format!("SC-{theme_id}-{i}");
        let scrutin_id = insert_scrutin(
            pool,
            &an_uid,
            i,
            date_base + chrono::Duration::days(i64::from(i)),
        )
        .await;
        insert_scrutin_label(pool, scrutin_id, theme_id, 0.600, 1.000, 1.000, author_id).await;
        insert_estimate(pool, scrutin_id, axis_version, 0.0).await;
        ids.push(scrutin_id);
    }
    ids
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_person_below_contributions_min_gets_no_score_row(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_id = insert_person(&pool, "PA1").await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 5, 1).await;

    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        4, // en dessous de contributions_min = 5
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    for scrutin_id in &scrutins {
        insert_vote(&pool, *scrutin_id, person_id, "pour", None).await;
    }

    recompute_scores::recompute(&pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM person_theme_score WHERE person_id = $1",
        person_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "4 contributions < seuil 5 : aucune ligne de score"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_person_at_exactly_contributions_min_gets_a_score_row(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_id = insert_person(&pool, "PA1").await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 5, 1).await;

    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        5, // exactement contributions_min
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    for scrutin_id in &scrutins {
        insert_vote(&pool, *scrutin_id, person_id, "pour", None).await;
    }

    recompute_scores::recompute(&pool).await.unwrap();

    let row = sqlx::query!(
        "SELECT contributions FROM person_theme_score WHERE person_id = $1 AND theme_id = $2",
        person_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.contributions, 5);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_theme_below_scrutins_min_produces_no_row_for_anyone(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_id = insert_person(&pool, "PA1").await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 1, 10).await; // scrutins_min_par_theme = 10

    // Seulement 3 scrutins catégorisés sur ce thème : sous le seuil de 10.
    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        3,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    for scrutin_id in &scrutins {
        insert_vote(&pool, *scrutin_id, person_id, "pour", None).await;
    }

    let counters = recompute_scores::recompute(&pool).await.unwrap();
    assert_eq!(counters["themes_eligibles"], 0);

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM person_theme_score")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "thème sous le seuil : aucune ligne, pour personne"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_non_inscrit_organe_gets_no_groupe_score(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let organe_id = insert_organe(&pool, "PO-NI", true).await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 1, 1).await;

    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        1,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    insert_scrutin_groupe(&pool, scrutins[0], 1, organe_id, 10, 5).await;

    recompute_scores::recompute(&pool).await.unwrap();

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM groupe_theme_score WHERE organe_id = $1",
        organe_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "un organe non-inscrit n'appartient au groupe de personne : aucun score de groupe"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_mandat_score_only_counts_scrutins_inside_its_period(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_id = insert_person(&pool, "PA1").await;
    let organe_id = insert_organe(&pool, "PO1", false).await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 1, 1).await;

    // Un mandat de janvier à février 2024 (borne haute exclusive).
    let mandat_id = insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_id,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        Some(NaiveDate::from_ymd_opt(2024, 2, 1).unwrap()),
    )
    .await;

    // Un scrutin dans la période du mandat.
    let inside = insert_scrutin(
        &pool,
        "SC-IN",
        1,
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
    )
    .await;
    insert_scrutin_label(&pool, inside, theme_id, 0.600, 1.000, 1.000, author_id).await;
    insert_estimate(&pool, inside, "v1", 0.0).await;
    insert_scrutin_groupe(&pool, inside, 1, organe_id, 10, 0).await;

    // Un scrutin hors période (mars 2024, après la fin du mandat).
    let outside = insert_scrutin(
        &pool,
        "SC-OUT",
        2,
        NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
    )
    .await;
    insert_scrutin_label(&pool, outside, theme_id, -0.600, 1.000, 1.000, author_id).await;
    insert_estimate(&pool, outside, "v1", 0.0).await;
    insert_scrutin_groupe(&pool, outside, 1, organe_id, 0, 10).await;

    recompute_scores::recompute(&pool).await.unwrap();

    let row = sqlx::query!(
        "SELECT contributions FROM mandat_theme_score WHERE mandat_id = $1 AND theme_id = $2",
        mandat_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row.contributions, 10,
        "seul le scrutin de janvier (10 votants pour) doit compter, pas celui de mars"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn exactly_two_runs_flip_is_current_to_the_latest(pool: PgPool) {
    insert_group_axis(&pool, "v1").await;

    recompute_scores::recompute(&pool).await.unwrap();
    let first_current: i64 =
        sqlx::query_scalar!("SELECT id AS \"id!\" FROM score_run WHERE is_current")
            .fetch_one(&pool)
            .await
            .unwrap();

    recompute_scores::recompute(&pool).await.unwrap();
    let second_current: i64 =
        sqlx::query_scalar!("SELECT id AS \"id!\" FROM score_run WHERE is_current")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert!(second_current > first_current);

    let total_current: i64 =
        sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM score_run WHERE is_current")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(total_current, 1, "au plus un run courant à la fois");

    let total_runs: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM score_run")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        total_runs, 2,
        "les runs précédents ne sont jamais supprimés (D4.6)"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn two_consecutive_runs_produce_identical_scores(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_a = insert_person(&pool, "PA1").await;
    let person_b = insert_person(&pool, "PA2").await;
    let organe_id = insert_organe(&pool, "PO1", false).await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 2, 1).await;

    let mandat_id = insert_mandat(
        &pool,
        "MDT1",
        person_a,
        organe_id,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        None,
    )
    .await;

    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        3,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    for (i, scrutin_id) in scrutins.iter().enumerate() {
        insert_vote(&pool, *scrutin_id, person_a, "pour", Some(organe_id)).await;
        let position_b = if i % 2 == 0 { "pour" } else { "contre" };
        insert_vote(&pool, *scrutin_id, person_b, position_b, None).await;
        insert_scrutin_groupe(&pool, *scrutin_id, 1, organe_id, 8, 2).await;
    }

    recompute_scores::recompute(&pool).await.unwrap();
    let fingerprint_1 = fingerprint(&pool).await;

    recompute_scores::recompute(&pool).await.unwrap();
    let fingerprint_2 = fingerprint(&pool).await;

    assert_eq!(
        fingerprint_1, fingerprint_2,
        "même corpus, mêmes règles : deux runs consécutifs doivent produire des scores identiques"
    );

    // Sanity check : le run le plus récent a bien produit des lignes pour la personne, le groupe
    // et le mandat — sinon l'empreinte identique ne prouverait rien.
    let person_rows: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM person_theme_score ptc
         JOIN score_run sr ON sr.id = ptc.run_id AND sr.is_current
         WHERE ptc.person_id = $1",
        person_a,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(person_rows, 1);

    let mandat_rows: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM mandat_theme_score mts
         JOIN score_run sr ON sr.id = mts.run_id AND sr.is_current
         WHERE mts.mandat_id = $1",
        mandat_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(mandat_rows, 1);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn the_materialized_views_only_reflect_the_current_run(pool: PgPool) {
    let theme_id = insert_theme(&pool, "theme-a").await;
    let author_id = insert_admin_user(&pool).await;
    let person_id = insert_person(&pool, "PA1").await;
    let organe_id = insert_organe(&pool, "PO1", false).await;
    insert_group_axis(&pool, "v1").await;
    set_parametres(&pool, 1, 1).await;

    let mandat_id = insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_id,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
        None,
    )
    .await;

    let scrutins = seed_categorized_scrutins(
        &pool,
        theme_id,
        author_id,
        "v1",
        1,
        NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
    )
    .await;
    insert_vote(&pool, scrutins[0], person_id, "pour", Some(organe_id)).await;
    insert_scrutin_groupe(&pool, scrutins[0], 1, organe_id, 10, 0).await;

    recompute_scores::recompute(&pool).await.unwrap();

    let person_view_score: f64 = sqlx::query_scalar!(
        "SELECT score::float8 AS \"score!\" FROM person_theme_score_courant WHERE person_id = $1 AND theme_id = $2",
        person_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((person_view_score - 0.6).abs() < 1e-9);

    let mandat_view_score: f64 = sqlx::query_scalar!(
        "SELECT score::float8 AS \"score!\" FROM mandat_theme_score_courant WHERE mandat_id = $1 AND theme_id = $2",
        mandat_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((mandat_view_score - 0.6).abs() < 1e-9);

    // Un second run avec un score différent doit remplacer, pas s'ajouter à, la vue.
    sqlx::query!(
        "UPDATE scrutin_label SET position_pour = -0.600 WHERE scrutin_id = $1 AND theme_id = $2",
        scrutins[0],
        theme_id,
    )
    .execute(&pool)
    .await
    .unwrap();
    recompute_scores::recompute(&pool).await.unwrap();

    let refreshed_score: f64 = sqlx::query_scalar!(
        "SELECT score::float8 AS \"score!\" FROM person_theme_score_courant WHERE person_id = $1 AND theme_id = $2",
        person_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((refreshed_score - (-0.6)).abs() < 1e-9);

    let total_rows: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM person_theme_score_courant WHERE person_id = $1 AND theme_id = $2",
        person_id,
        theme_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        total_rows, 1,
        "la vue ne porte qu'une ligne par (personne, thème) : la précédente est remplacée"
    );
}

/// Empreinte du run courant sur les trois tables de score, indépendante de l'ordre des lignes ou du
/// `run_id` lui-même (deux runs différents doivent pouvoir produire la même empreinte).
async fn fingerprint(pool: &PgPool) -> Vec<String> {
    let mut rows: Vec<String> = sqlx::query_scalar!(
        r#"
        SELECT format('person:%s:%s:%s:%s:%s:%s', person_id, theme_id, score, incertitude, contributions, abstentions)
        FROM person_theme_score ptc
        JOIN score_run sr ON sr.id = ptc.run_id AND sr.is_current
        UNION ALL
        SELECT format('groupe:%s:%s:%s:%s:%s:%s', organe_id, theme_id, score, cohesion, contributions, membres)
        FROM groupe_theme_score gts
        JOIN score_run sr ON sr.id = gts.run_id AND sr.is_current
        UNION ALL
        SELECT format('mandat:%s:%s:%s:%s:%s', mandat_id, theme_id, score, cohesion, contributions)
        FROM mandat_theme_score mts
        JOIN score_run sr ON sr.id = mts.run_id AND sr.is_current
        "#
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|r| r.unwrap_or_default())
    .collect();
    rows.sort();
    rows
}
