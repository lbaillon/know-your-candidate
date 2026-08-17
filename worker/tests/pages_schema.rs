//! Tests de contrainte de la migration 0005 (pages) — voir docs/plans/phase-2-api-ui.md,
//! section « Migration 0005_pages.sql ».

#![allow(clippy::unwrap_used, clippy::expect_used)]

use sqlx::PgPool;

async fn insert_person_an(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn insert_person_wikidata(pool: &PgPool, qid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (wikidata_qid) VALUES ($1) RETURNING id",
        qid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// La personne créée par le seed des candidat·es (D2.1) n'a jamais siégé : ni `an_uid` ni
/// `wikidata_qid` seul ne suffit à en faire une identité rejetable, mais l'absence des deux doit
/// l'être.
#[sqlx::test(migrations = "../db/migrations")]
async fn person_without_any_identifier_is_rejected(pool: PgPool) -> sqlx::Result<()> {
    let result = sqlx::query!("INSERT INTO person (an_uid, wikidata_qid) VALUES (NULL, NULL)")
        .execute(&pool)
        .await;

    assert!(
        result.is_err(),
        "une personne sans an_uid ni wikidata_qid doit être rejetée"
    );

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn person_with_only_wikidata_qid_is_accepted(pool: PgPool) -> sqlx::Result<()> {
    insert_person_wikidata(&pool, "Q1").await;
    Ok(())
}

/// `source_url`/`source_date` sont `NOT NULL` (D2.1) : la règle « pas de statut sans source »
/// appartient au schéma, sur le modèle de `person_photo.licence`.
#[sqlx::test(migrations = "../db/migrations")]
async fn candidate_without_source_is_rejected(pool: PgPool) -> sqlx::Result<()> {
    let person_id = insert_person_an(&pool, "PA1").await;

    let result = sqlx::query!(
        "INSERT INTO candidate (person_id, statut, source_url, source_date) \
         VALUES ($1, 'declare', NULL, '2026-01-01')",
        person_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un statut de candidat·e sans source_url doit être rejeté"
    );

    Ok(())
}

#[sqlx::test(migrations = "../db/migrations")]
async fn candidate_with_blank_source_url_is_rejected(pool: PgPool) -> sqlx::Result<()> {
    let person_id = insert_person_an(&pool, "PA1").await;

    let result = sqlx::query!(
        "INSERT INTO candidate (person_id, statut, source_url, source_date) \
         VALUES ($1, 'declare', '   ', '2026-01-01')",
        person_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un source_url réduit à des espaces doit être rejeté (candidate_source_non_vide)"
    );

    Ok(())
}

/// `person_slug_courant_idx` (index unique partiel sur `is_current`) est ce qui garantit qu'une
/// personne n'a jamais deux slugs à la fois « courants » — sans quoi deux URL serviraient la même
/// fiche sans qu'aucune ne soit la référence.
#[sqlx::test(migrations = "../db/migrations")]
async fn a_person_cannot_have_two_current_slugs(pool: PgPool) -> sqlx::Result<()> {
    let person_id = insert_person_an(&pool, "PA1").await;

    sqlx::query!(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ('jean-dupont', $1, true)",
        person_id,
    )
    .execute(&pool)
    .await?;

    let result = sqlx::query!(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ('j-dupont', $1, true)",
        person_id,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "deux slugs courants pour la même personne doivent être rejetés"
    );

    Ok(())
}

/// Un ancien slug (`is_current = false`) reste réservé : sinon la redirection 301 finirait par
/// pointer vers quelqu'un d'autre (voir `assign_slugs` dans le plan d'exécution).
#[sqlx::test(migrations = "../db/migrations")]
async fn two_persons_cannot_share_the_same_slug(pool: PgPool) -> sqlx::Result<()> {
    let person_a = insert_person_an(&pool, "PA1").await;
    let person_b = insert_person_an(&pool, "PA2").await;

    sqlx::query!(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ('jean-dupont', $1, false)",
        person_a,
    )
    .execute(&pool)
    .await?;

    let result = sqlx::query!(
        "INSERT INTO person_slug (slug, person_id, is_current) VALUES ('jean-dupont', $1, true)",
        person_b,
    )
    .execute(&pool)
    .await;

    assert!(
        result.is_err(),
        "un slug déjà pris, même par un ancien slug, doit être rejeté (clé primaire)"
    );

    Ok(())
}

/// Le `coalesce(v.votes_total, 0)` de `person_apercu` est le genre de détail qu'on croit acquis :
/// sans lui une personne sans aucun vote afficherait NULL plutôt que zéro.
#[sqlx::test(migrations = "../db/migrations")]
async fn person_apercu_counts_zero_votes_for_a_person_without_any_vote(
    pool: PgPool,
) -> sqlx::Result<()> {
    let person_id = insert_person_an(&pool, "PA1").await;

    // REFRESH ... CONCURRENTLY ne peut pas s'exécuter dans une transaction (voir plan
    // d'exécution, « Vues matérialisées et tests ») ; chaque test `sqlx::test` tourne pourtant
    // hors transaction explicite ici (`sqlx::test` fournit une base éphémère dédiée), donc un
    // simple REFRESH suffit.
    sqlx::query!("REFRESH MATERIALIZED VIEW person_apercu")
        .execute(&pool)
        .await?;

    let votes_total: i64 = sqlx::query_scalar!(
        r#"SELECT votes_total AS "votes_total!" FROM person_apercu WHERE person_id = $1"#,
        person_id,
    )
    .fetch_one(&pool)
    .await?;

    assert_eq!(votes_total, 0);

    Ok(())
}
