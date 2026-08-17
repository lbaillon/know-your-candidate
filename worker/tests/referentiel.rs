//! Tests de contrainte de la migration 0002 (référentiel) — voir docs/plans/phase-1-ingestion.md,
//! section « Le référentiel (migration 0002) ». La contrainte `EXCLUDE` sur `mandat` porte sur
//! (personne, organe), pas sur (personne, type) : elle doit rejeter tout chevauchement entre deux
//! mandats `GP` d'une même personne dans le même organe, mais laisser passer un chevauchement entre
//! deux organes différents ou entre deux mandats qui ne sont pas de type `GP`. La normalisation des
//! chevauchements réels (inclusions, charnières) est un problème applicatif du job `ingest_acteurs`,
//! pas de cette migration.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sqlx::PgPool;

async fn insert_person(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_organe(pool: &PgPool, an_uid: &str, code_type: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO organe (an_uid, code_type, libelle) VALUES ($1, $2, $3) RETURNING id",
        an_uid,
        code_type,
        format!("Libellé {an_uid}"),
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
    type_organe: &str,
    debut: &str,
    fin: &str,
) -> sqlx::Result<()> {
    let debut = debut.parse::<sqlx::types::chrono::NaiveDate>().unwrap();
    let fin = fin.parse::<sqlx::types::chrono::NaiveDate>().unwrap();
    sqlx::query!(
        r#"
        INSERT INTO mandat (an_uid, person_id, organe_id, type_organe, period)
        VALUES ($1, $2, $3, $4, daterange($5, $6, '[)'))
        "#,
        an_uid,
        person_id,
        organe_id,
        type_organe,
        debut,
        fin,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn overlapping_gp_mandats_of_same_person_and_organe_are_rejected(
    pool: PgPool,
) -> sqlx::Result<()> {
    let person_id = insert_person(&pool, "PA1").await;
    let organe_id = insert_organe(&pool, "PO1", "GP").await;

    insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_id,
        "GP",
        "2020-01-01",
        "2021-01-01",
    )
    .await?;

    let result = insert_mandat(
        &pool,
        "MDT2",
        person_id,
        organe_id,
        "GP",
        "2020-06-01",
        "2020-08-01",
    )
    .await;

    assert!(
        result.is_err(),
        "un mandat GP inclus dans un autre du même organe doit être rejeté par la contrainte"
    );

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn non_overlapping_gp_mandats_of_same_person_and_organe_are_accepted(
    pool: PgPool,
) -> sqlx::Result<()> {
    let person_id = insert_person(&pool, "PA1").await;
    let organe_id = insert_organe(&pool, "PO1", "GP").await;

    insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_id,
        "GP",
        "2020-01-01",
        "2021-01-01",
    )
    .await?;
    insert_mandat(
        &pool,
        "MDT2",
        person_id,
        organe_id,
        "GP",
        "2021-01-01",
        "2022-01-01",
    )
    .await?;

    Ok(())
}

/// La contrainte est posée sur (personne, organe) : un chevauchement entre deux organes
/// différents (le seul cas mesuré par le spike, hors corpus) doit rester acceptable en base — la
/// normalisation de ce cas résiduel est journalisée par le job, pas empêchée par le schéma.
#[sqlx::test(migrations = "../db/migrations")]
async fn overlapping_gp_mandats_of_different_organes_are_accepted(
    pool: PgPool,
) -> sqlx::Result<()> {
    let person_id = insert_person(&pool, "PA1").await;
    let organe_a = insert_organe(&pool, "PO1", "GP").await;
    let organe_b = insert_organe(&pool, "PO2", "GP").await;

    insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_a,
        "GP",
        "2020-01-01",
        "2021-01-01",
    )
    .await?;
    insert_mandat(
        &pool,
        "MDT2",
        person_id,
        organe_b,
        "GP",
        "2020-06-01",
        "2020-08-01",
    )
    .await?;

    Ok(())
}

/// La contrainte ne porte que sur `type_organe = 'GP'` : un mandat `PARPOL` qui chevauche un autre
/// mandat `PARPOL` du même organe n'est pas contraint (D1.12 : seuls les mandats GP entrent dans la
/// contrainte d'appartenance à un groupe).
#[sqlx::test(migrations = "../db/migrations")]
async fn overlapping_non_gp_mandats_are_accepted(pool: PgPool) -> sqlx::Result<()> {
    let person_id = insert_person(&pool, "PA1").await;
    let organe_id = insert_organe(&pool, "PO1", "PARPOL").await;

    insert_mandat(
        &pool,
        "MDT1",
        person_id,
        organe_id,
        "PARPOL",
        "2020-01-01",
        "2021-01-01",
    )
    .await?;
    insert_mandat(
        &pool,
        "MDT2",
        person_id,
        organe_id,
        "PARPOL",
        "2020-06-01",
        "2020-08-01",
    )
    .await?;

    Ok(())
}

/// `person_photo.licence` est `NOT NULL` : la contrainte porte la règle « pas de photo sans
/// licence affichable » (D1.13), pas le code applicatif.
#[sqlx::test(migrations = "../db/migrations")]
async fn person_photo_requires_a_licence(pool: PgPool) -> sqlx::Result<()> {
    let person_id = insert_person(&pool, "PA1").await;

    let result = sqlx::query!(
        "INSERT INTO person_photo (person_id, url, licence) VALUES ($1, $2, NULL)",
        person_id,
        "https://example.org/photo.jpg",
    )
    .execute(&pool)
    .await;

    assert!(result.is_err(), "une photo sans licence doit être rejetée");

    Ok(())
}
