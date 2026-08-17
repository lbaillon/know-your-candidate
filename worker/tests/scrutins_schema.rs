//! Tests de contrainte de la migration 0003 (scrutins) — voir
//! docs/plans/phase-1-ingestion.md, section « Les scrutins (migration 0003) ».

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sqlx::PgPool;

async fn insert_source_document(pool: &PgPool, uid: &str) -> i64 {
    sqlx::query_scalar!(
        r#"
        INSERT INTO source_document (source, uid, url, content_hash, payload)
        VALUES ('an_scrutin', $1, 'https://example.org', 'hash', '{}'::jsonb)
        RETURNING id
        "#,
        uid,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_person(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

struct ScrutinFixture {
    id: i64,
}

async fn insert_scrutin(
    pool: &PgPool,
    an_uid: &str,
    nombre_votants: i32,
    effectif: i32,
) -> ScrutinFixture {
    let source_document_id = insert_source_document(pool, an_uid).await;
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO scrutin (an_uid, numero, legislature, date_scrutin, type_code, titre,
                              mode_publication, nombre_votants, suffrages_exprimes, pour, contre,
                              abstentions, non_votants, effectif, source_document_id)
        VALUES ($1, 1, 17, '2024-01-01', 'SPO', 'titre', 'DecompteNominatif',
                $2, $2, 0, 0, 0, 0, $3, $4)
        RETURNING id
        "#,
        an_uid,
        nombre_votants,
        effectif,
        source_document_id,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    ScrutinFixture { id }
}

#[sqlx::test(migrations = "../db/migrations")]
async fn participation_is_generated_from_votants_and_effectif(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 289, 577).await;

    let participation: Option<f64> = sqlx::query_scalar!(
        r#"SELECT participation::float8 AS "participation" FROM scrutin WHERE id = $1"#,
        scrutin.id
    )
    .fetch_one(&pool)
    .await?;

    let participation = participation.expect("participation calculée");
    assert!(
        (participation - 289.0 / 577.0).abs() < 1e-9,
        "participation = {participation}"
    );

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn participation_is_null_when_effectif_is_zero(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 0, 0).await;

    let participation: Option<f64> = sqlx::query_scalar!(
        r#"SELECT participation::float8 AS "participation" FROM scrutin WHERE id = $1"#,
        scrutin.id
    )
    .fetch_one(&pool)
    .await?;

    assert!(participation.is_none());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn vote_cause_non_vote_is_rejected_outside_non_votant(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 1, 577).await;
    let person_id = insert_person(&pool, "PA1").await;

    let result = sqlx::query!(
        r#"
        INSERT INTO vote (scrutin_id, person_id, position, cause_non_vote)
        VALUES ($1, $2, 'pour', 'PSE')
        "#,
        scrutin.id,
        person_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "cause_non_vote ne doit être acceptée que sur un non_votant"
    );

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn vote_cause_non_vote_is_accepted_on_non_votant(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 1, 577).await;
    let person_id = insert_person(&pool, "PA1").await;

    sqlx::query!(
        r#"
        INSERT INTO vote (scrutin_id, person_id, position, cause_non_vote)
        VALUES ($1, $2, 'non_votant', 'PSE')
        "#,
        scrutin.id,
        person_id,
    )
    .execute(&pool)
    .await?;

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn mise_au_point_rejects_unknown_position(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 1, 577).await;
    let person_id = insert_person(&pool, "PA1").await;
    let source_document_id = insert_source_document(&pool, "VTANR5L17V1-map").await;

    let result = sqlx::query!(
        r#"
        INSERT INTO vote_mise_au_point (scrutin_id, person_id, position_declaree, source_document_id)
        VALUES ($1, $2, 'ne_sait_pas', $3)
        "#,
        scrutin.id,
        person_id,
        source_document_id,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err());

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn mise_au_point_accepts_the_five_known_positions(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V1", 1, 577).await;
    let person_id = insert_person(&pool, "PA1").await;
    let source_document_id = insert_source_document(&pool, "VTANR5L17V1-map").await;

    for position in [
        "pour",
        "contre",
        "abstention",
        "non_votant",
        "non_votant_volontaire",
    ] {
        sqlx::query!(
            r#"
            INSERT INTO vote_mise_au_point (scrutin_id, person_id, position_declaree, source_document_id)
            VALUES ($1, $2, $3, $4)
            "#,
            scrutin.id,
            person_id,
            position,
            source_document_id,
        )
        .execute(&pool)
        .await?;
    }

    Ok(())
}

/// 14 scrutins de la 17e publient leurs groupes sous l'identifiant fantôme `PO0` : plusieurs
/// lignes de `scrutin_groupe` portent alors le même `organe_id` NULL pour un même scrutin. La clé
/// est donc `(scrutin_id, rang)`, pas `(scrutin_id, organe_id)`.
#[sqlx::test(migrations = "../db/migrations")]
async fn scrutin_groupe_allows_several_ghost_organe_rows(pool: PgPool) -> sqlx::Result<()> {
    let scrutin = insert_scrutin(&pool, "VTANR5L17V501", 1, 577).await;

    for rang in 0..3 {
        sqlx::query!(
            r#"
            INSERT INTO scrutin_groupe (scrutin_id, rang, organe_id, nombre_membres, pour,
                                         contre, abstentions, non_votants)
            VALUES ($1, $2, NULL, 10, 0, 0, 0, 0)
            "#,
            scrutin.id,
            rang,
        )
        .execute(&pool)
        .await?;
    }

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM scrutin_groupe WHERE scrutin_id = $1",
        scrutin.id
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(count, 3);

    Ok(())
}
