//! Tests de contrainte de la migration 0007 (catégorisation) — voir
//! docs/plans/phase-3-categorisation.md, section « Migration `0007_categorisation.sql` ».
//!
//! PIÈGE À CONNAÎTRE (voir le commentaire de la migration, section 9) : le trigger de cohérence de
//! `scrutin_label` (somme des poids, position/axe) est DEFERRABLE INITIALLY DEFERRED — il ne se
//! déclenche qu'au COMMIT de la transaction qui l'a provoqué, jamais avant. `#[sqlx::test]` fournit
//! ici un vrai `PgPool` sur une base éphémère réelle (pas une transaction enveloppante annulée à la
//! fin du test) : un `.execute(&pool)` isolé est donc son propre COMMIT immédiat, et le déclenche
//! normalement. Pour observer l'état *intermédiaire* volontairement incohérent d'un remplacement
//! (DELETE puis INSERT) sans aller jusqu'au commit réel, il faut forcer la vérification avec
//! `SET CONSTRAINTS ALL IMMEDIATE` à l'intérieur d'un SAVEPOINT — voir
//! `deferred_constraint_can_be_forced_immediately_with_a_savepoint` ci-dessous.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sqlx::PgPool;

async fn insert_admin_user(pool: &PgPool, github_id: i64, login: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO admin_user (github_id, github_login, display_name) VALUES ($1, $2, $2) RETURNING id",
        github_id,
        login,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_theme(pool: &PgPool, slug: &str, with_axis: bool) -> i16 {
    let (pole_neg, pole_pos) = if with_axis {
        (Some("pôle négatif"), Some("pôle positif"))
    } else {
        (None, None)
    };
    sqlx::query_scalar!(
        r#"
        INSERT INTO theme (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang)
        VALUES ($1, $1, 'description', $2, $3, 1)
        RETURNING id
        "#,
        slug,
        pole_neg,
        pole_pos,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_scrutin(pool: &PgPool, an_uid: &str) -> i64 {
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
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, 1, 17, '2024-01-01', 'SPO', 'titre', 'DecompteNominatif', 0, 0, 0, 0, 0, 0, 577, $2)
        RETURNING id
        "#,
        an_uid,
        source_document_id,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

struct LabelArgs {
    scrutin_id: i64,
    theme_id: i16,
    poids: f64,
    position_pour: Option<f64>,
    author_id: i64,
}

/// `$3::float8::numeric` (et non `$3::numeric`) : un cast direct vers `numeric` forcerait sqlx à
/// décrire le paramètre comme NUMERIC, qui exige la fonctionnalité cargo `bigdecimal` (non ajoutée,
/// voir docs/plans/phase-3-categorisation.md, section « Dépendances ajoutées » : aucune dépendance
/// nouvelle côté worker). En décrivant le paramètre comme `float8`, sqlx le mappe simplement sur
/// `f64`, et le second cast explicite convertit vers `numeric` côté SQL au moment de l'écriture.
async fn insert_label(pool: &PgPool, args: LabelArgs) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO scrutin_label
            (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
        VALUES ($1, $2, $3::float8::numeric, $4::float8::numeric, 0.700,
                'justification suffisamment longue', 'manual', $5)
        "#,
        args.scrutin_id,
        args.theme_id,
        args.poids,
        args.position_pour,
        args.author_id,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn weights_summing_to_exactly_one_across_two_rows_are_accepted(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_a = insert_theme(&pool, "theme-a", true).await;
    let theme_b = insert_theme(&pool, "theme-b", true).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 0.600, 0.200, 0.700, 'justification suffisamment longue', 'manual', $3)"#,
        scrutin_id, theme_a, author_id,
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 0.400, -0.100, 0.700, 'justification suffisamment longue', 'manual', $3)"#,
        scrutin_id, theme_b, author_id,
    )
    .execute(&mut *tx)
    .await
    .unwrap();

    // Au commit seulement : le trigger différé accepte une somme exactement égale à 1.
    tx.commit().await.unwrap();
}

#[sqlx::test(migrations = "../db/migrations")]
async fn weights_not_summing_to_one_are_rejected_at_commit(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = insert_label(
        &pool,
        LabelArgs {
            scrutin_id,
            theme_id,
            poids: 0.600,
            position_pour: Some(0.200),
            author_id,
        },
    )
    .await;

    assert!(
        result.is_err(),
        "un seul INSERT via le pool est son propre commit : la somme 0.6 ≠ 1 doit être rejetée"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_position_on_a_theme_without_an_axis_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "autre", false).await;

    let result = insert_label(
        &pool,
        LabelArgs {
            scrutin_id,
            theme_id,
            poids: 1.000,
            position_pour: Some(0.200),
            author_id,
        },
    )
    .await;

    assert!(
        result.is_err(),
        "« autre » n'a pas d'axe, il ne peut pas porter de position"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_missing_position_on_a_themed_axis_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = insert_label(
        &pool,
        LabelArgs {
            scrutin_id,
            theme_id,
            poids: 1.000,
            position_pour: None,
            author_id,
        },
    )
    .await;

    assert!(
        result.is_err(),
        "un thème doté d'un axe exige une position, elle n'est jamais optionnelle"
    );
}

/// Démontre le piège documenté dans la migration (section 9) : sans forcer la vérification, l'état
/// intermédiaire d'un remplacement en deux temps (poids = 0.6 seul, donc incohérent) reste invisible
/// tant que la transaction n'a pas commité. `SET CONSTRAINTS ALL IMMEDIATE` dans un SAVEPOINT permet
/// de l'observer sans pour autant valider la transaction entière.
#[sqlx::test(migrations = "../db/migrations")]
async fn deferred_constraint_can_be_forced_immediately_with_a_savepoint(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 0.600, 0.200, 0.700, 'justification suffisamment longue', 'manual', $3)"#,
        scrutin_id, theme_id, author_id,
    )
    .execute(&mut *tx)
    .await
    .expect("l'INSERT seul réussit : le trigger différé ne s'est pas encore déclenché");

    sqlx::query("SAVEPOINT verification_immediate")
        .execute(&mut *tx)
        .await
        .unwrap();
    let forced = sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await;
    assert!(
        forced.is_err(),
        "forcer la vérification révèle l'incohérence (somme = 0.6) sans attendre le commit réel"
    );
    sqlx::query("ROLLBACK TO SAVEPOINT verification_immediate")
        .execute(&mut *tx)
        .await
        .unwrap();

    // La transaction reste utilisable après le rollback au savepoint : on peut la compléter pour
    // la rendre cohérente, preuve que le piège n'empêche pas de continuer à travailler dedans.
    let theme_b = insert_theme(&pool, "theme-b", true).await;
    sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 0.400, -0.100, 0.700, 'justification suffisamment longue', 'manual', $3)"#,
        scrutin_id, theme_b, author_id,
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test(migrations = "../db/migrations")]
async fn an_import_method_without_an_import_id_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 1.000, 0.200, 0.700, 'justification suffisamment longue', 'import', $3)"#,
        scrutin_id, theme_id, author_id,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "method = import exige import_id");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_review_with_only_reviewed_at_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = sqlx::query!(
        r#"INSERT INTO scrutin_label
               (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id, reviewed_at)
           VALUES ($1, $2, 1.000, 0.200, 0.700, 'justification suffisamment longue', 'manual', $3, now())"#,
        scrutin_id, theme_id, author_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "reviewed_by et reviewed_at doivent être renseignés ensemble ou pas du tout"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_revision_where_avant_equals_apres_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;

    let result = sqlx::query!(
        r#"INSERT INTO label_revision (scrutin_id, avant, apres, method, author_id)
           VALUES ($1, '[]'::jsonb, '[]'::jsonb, 'manual', $2)"#,
        scrutin_id,
        author_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un « changement » identique avant/après ne doit jamais s'écrire dans l'historique"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn only_one_current_group_axis_version_is_allowed(pool: PgPool) {
    sqlx::query!(
        r#"INSERT INTO group_axis (version, description, grille_version, grille_date, source_url, content_hash, is_current)
           VALUES ('v1', 'd', 'g1', '2026-01-01', 'https://example.org', 'hash1', true)"#
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = sqlx::query!(
        r#"INSERT INTO group_axis (version, description, grille_version, grille_date, source_url, content_hash, is_current)
           VALUES ('v2', 'd', 'g2', '2026-02-01', 'https://example.org', 'hash2', true)"#
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "au plus une version d'ancrage courante à la fois"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_justification_shorter_than_ten_characters_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = sqlx::query!(
        r#"INSERT INTO scrutin_label (scrutin_id, theme_id, poids, position_pour, confiance, justification, method, author_id)
           VALUES ($1, $2, 1.000, 0.200, 0.700, 'trop', 'manual', $3)"#,
        scrutin_id, theme_id, author_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "« trop » fait moins de dix caractères, doit être rejeté"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_zero_weight_is_rejected(pool: PgPool) {
    let author_id = insert_admin_user(&pool, 1, "alice").await;
    let scrutin_id = insert_scrutin(&pool, "SC1").await;
    let theme_id = insert_theme(&pool, "theme-a", true).await;

    let result = insert_label(
        &pool,
        LabelArgs {
            scrutin_id,
            theme_id,
            poids: 0.000,
            position_pour: Some(0.200),
            author_id,
        },
    )
    .await;

    assert!(
        result.is_err(),
        "un poids nul n'a pas de sens pour une catégorisation retenue"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_theme_with_only_one_pole_is_rejected_at_the_schema_level(pool: PgPool) {
    let result = sqlx::query!(
        r#"INSERT INTO theme (slug, libelle, description, libelle_pole_negatif, rang)
           VALUES ('boiteux', 'Boiteux', 'description', 'seul', 1)"#
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un axe a les deux pôles ou aucun, la contrainte doit le refuser même hors du job seed_themes"
    );
}
