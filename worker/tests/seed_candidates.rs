//! Tests d'intégration du job `seed_candidates` — voir docs/plans/phase-2-api-ui.md, section
//! « Le seed des candidat·es ». Chaque test écrit son propre fichier TOML éphémère
//! (`tempfile::NamedTempFile`) : ce sont de petits fichiers texte inventés pour l'occasion, pas des
//! archives binaires versionnées comme les fixtures AN.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use kyc_worker::jobs::seed_candidates;
use sqlx::PgPool;
use tempfile::NamedTempFile;

fn write_toml(contents: &str) -> NamedTempFile {
    use std::io::Write as _;
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file
}

async fn insert_person_an(pool: &PgPool, an_uid: &str) -> i64 {
    sqlx::query_scalar!(
        "INSERT INTO person (an_uid) VALUES ($1) RETURNING id",
        an_uid
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_candidate_identified_by_an_uid(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        note = "test"
        "#,
    );

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false)
        .await
        .unwrap();

    assert_eq!(counters["entrees_lues"], 1);
    assert_eq!(counters["candidats_crees"], 1);
    assert_eq!(counters["personnes_creees"], 0);

    let row = sqlx::query!(
        "SELECT statut::text AS \"statut!\", source_url, note FROM candidate WHERE person_id = $1",
        person_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.statut, "declare");
    assert_eq!(row.source_url, "https://example.org/annonce");
    assert_eq!(row.note.as_deref(), Some("test"));
}

/// Le cas qui casse (D2.1) : une personne jamais élue à l'Assemblée, identifiée par son seul
/// `wikidata_qid`, créée par le job plutôt que refusée.
#[sqlx::test(migrations = "../db/migrations")]
async fn seeds_a_candidate_identified_by_wikidata_qid_and_creates_the_person(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        wikidata_qid = "Q1"
        prenom = "Ada"
        nom = "Exemple"
        statut = "pressenti"
        source_url = "https://example.org/annonce"
        source_date = 2026-01-15
        "#,
    );

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false)
        .await
        .unwrap();

    assert_eq!(counters["personnes_creees"], 1);
    assert_eq!(counters["candidats_crees"], 1);

    let person = sqlx::query!("SELECT prenom, nom, an_uid FROM person WHERE wikidata_qid = 'Q1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(person.prenom.as_deref(), Some("Ada"));
    assert_eq!(person.nom.as_deref(), Some("Exemple"));
    assert_eq!(person.an_uid, None, "jamais élue à l'Assemblée");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_an_uid_fails_without_writing_anything(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA_INCONNU"
        statut = "declare"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn unknown_statut_is_rejected_before_any_write(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "ne_sait_pas"
        source_url = "https://example.org/annonce"
        source_date = 2026-05-03
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../db/migrations")]
async fn duplicate_identifier_in_the_file_is_rejected(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01

        [[candidate]]
        an_uid = "PA1"
        statut = "retire"
        source_url = "https://example.org/2"
        source_date = 2026-02-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());
}

#[sqlx::test(migrations = "../db/migrations")]
async fn wikidata_entry_without_a_name_is_rejected(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        wikidata_qid = "Q1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM person")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "aucune personne ne doit être créée par une entrée invalide"
    );
}

#[sqlx::test(migrations = "../db/migrations")]
async fn blank_source_url_is_rejected(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "   "
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());
}

/// Rejouer le même fichier ne doit ni recréer ni faire bouger `updated_at` — idempotence, comme le
/// reste de l'ingestion (CLAUDE.md).
#[sqlx::test(migrations = "../db/migrations")]
async fn rerunning_the_same_file_is_a_no_op(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );

    seed_candidates::seed(&pool, file.path().to_str().unwrap(), false)
        .await
        .unwrap();
    let updated_at_before: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar!("SELECT updated_at FROM candidate")
            .fetch_one(&pool)
            .await
            .unwrap();

    let counters = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false)
        .await
        .unwrap();
    let updated_at_after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar!("SELECT updated_at FROM candidate")
            .fetch_one(&pool)
            .await
            .unwrap();

    assert_eq!(counters["candidats_crees"], 0);
    assert_eq!(counters["candidats_mis_a_jour"], 0);
    assert_eq!(updated_at_before, updated_at_after);
}

/// Le seed est la source de vérité de `candidate` (voir plan) : une personne retirée du fichier
/// n'est plus candidate après un nouveau passage. Vider la table reste possible, mais devient un
/// geste explicite (F4) : sans `allow_empty`, un fichier vide échoue plutôt — voir
/// `an_empty_file_is_rejected_without_allow_empty` et
/// `an_empty_file_with_allow_empty_retires_everyone` ci-dessous.
#[sqlx::test(migrations = "../db/migrations")]
async fn removing_an_entry_retires_the_candidate(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    insert_person_an(&pool, "PA2").await;
    let with_entry = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01

        [[candidate]]
        an_uid = "PA2"
        statut = "declare"
        source_url = "https://example.org/2"
        source_date = 2026-01-01
        "#,
    );
    seed_candidates::seed(&pool, with_entry.path().to_str().unwrap(), false)
        .await
        .unwrap();

    // Un second fichier retire PA2 sans vider la table : ce n'est pas le cas `allow_empty`.
    let one_left = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    let counters = seed_candidates::seed(&pool, one_left.path().to_str().unwrap(), false)
        .await
        .unwrap();

    assert_eq!(counters["candidats_retires"], 1);
    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

/// F4 : une faute de frappe (`[[candidats]]` au pluriel) ou un fichier vraiment vide donnent tous
/// deux zéro entrée lue — le job doit refuser de vider `candidate` sans confirmation explicite,
/// et ne rien écrire en échouant.
#[sqlx::test(migrations = "../db/migrations")]
async fn an_empty_file_is_rejected_without_allow_empty(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    sqlx::query!(
        "INSERT INTO candidate (person_id, statut, source_url, source_date) \
         VALUES ($1, 'declare', 'https://example.org/1', '2026-01-01')",
        person_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let empty = write_toml("");
    let result = seed_candidates::seed(&pool, empty.path().to_str().unwrap(), false).await;

    assert!(result.is_err());
    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "les lignes préexistantes doivent rester en place");
}

/// F4 : le même fichier vide, avec `allow_empty: true`, vide bien la table — retirer tout le
/// monde reste possible, mais devient un geste explicite.
#[sqlx::test(migrations = "../db/migrations")]
async fn an_empty_file_with_allow_empty_retires_everyone(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    sqlx::query!(
        "INSERT INTO candidate (person_id, statut, source_url, source_date) \
         VALUES ($1, 'declare', 'https://example.org/1', '2026-01-01')",
        person_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let empty = write_toml("");
    let counters = seed_candidates::seed(&pool, empty.path().to_str().unwrap(), true)
        .await
        .unwrap();

    assert_eq!(counters["candidats_retires"], 1);
    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// Une faute de frappe plausible (`[[candidats]]`) ne doit jamais être avalée en silence : c'est
/// exactement le défaut qui a vidé `candidate` sans erreur avant F4.
#[sqlx::test(migrations = "../db/migrations")]
async fn a_misspelled_table_key_is_rejected_without_writing_anything(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    sqlx::query!(
        "INSERT INTO candidate (person_id, statut, source_url, source_date) \
         VALUES ($1, 'declare', 'https://example.org/1', '2026-01-01')",
        person_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let file = write_toml(
        r#"
        [[candidats]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;

    assert!(result.is_err());
    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "les lignes préexistantes doivent rester en place");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn a_changed_field_updates_the_row_and_is_counted_as_a_change(pool: PgPool) {
    insert_person_an(&pool, "PA1").await;
    let declare = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    seed_candidates::seed(&pool, declare.path().to_str().unwrap(), false)
        .await
        .unwrap();

    let retire = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "retire"
        source_url = "https://example.org/1"
        source_date = 2026-01-01
        "#,
    );
    let counters = seed_candidates::seed(&pool, retire.path().to_str().unwrap(), false)
        .await
        .unwrap();

    assert_eq!(counters["candidats_crees"], 0);
    assert_eq!(counters["candidats_mis_a_jour"], 1);
    let statut = sqlx::query_scalar!("SELECT statut::text AS \"statut!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(statut, "retire");
}

#[sqlx::test(migrations = "../db/migrations")]
async fn missing_file_fails_cleanly(pool: PgPool) {
    let result = seed_candidates::seed(&pool, "/does/not/exist.toml", false).await;
    assert!(result.is_err());
}

/// F7 : tout — résolution, upsert, suppression — vit désormais dans une seule transaction. Une
/// entrée invalide en fin de fichier ne doit donc laisser en base aucune des personnes Wikidata
/// créées par les entrées précédentes.
#[sqlx::test(migrations = "../db/migrations")]
async fn a_failure_after_creating_a_wikidata_person_leaves_nothing_behind(pool: PgPool) {
    let file = write_toml(
        r#"
        [[candidate]]
        wikidata_qid = "Q1"
        prenom = "Ada"
        nom = "Exemple"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01

        [[candidate]]
        an_uid = "PA_INCONNU"
        statut = "declare"
        source_url = "https://example.org/2"
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;
    assert!(result.is_err());

    let count: i64 =
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM person WHERE wikidata_qid = 'Q1'"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

/// F8 : la plupart des député·es portent à la fois un `an_uid` et un `wikidata_qid` — deux entrées
/// qui résolvent la même personne par ces deux chemins doivent échouer avec un message qui nomme
/// les deux identifiants, pas avec l'erreur PostgreSQL brute de l'upsert.
#[sqlx::test(migrations = "../db/migrations")]
async fn two_identifiers_for_the_same_person_are_rejected_with_a_clear_message(pool: PgPool) {
    let person_id = insert_person_an(&pool, "PA1").await;
    sqlx::query!(
        "UPDATE person SET wikidata_qid = 'Q1' WHERE id = $1",
        person_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let file = write_toml(
        r#"
        [[candidate]]
        an_uid = "PA1"
        statut = "declare"
        source_url = "https://example.org/1"
        source_date = 2026-01-01

        [[candidate]]
        wikidata_qid = "Q1"
        prenom = "Jean"
        nom = "Dupont"
        statut = "declare"
        source_url = "https://example.org/2"
        source_date = 2026-01-01
        "#,
    );

    let result = seed_candidates::seed(&pool, file.path().to_str().unwrap(), false).await;

    let err = result.unwrap_err().to_string();
    assert!(err.contains("an_uid=PA1"), "message reçu : {err}");
    assert!(err.contains("wikidata_qid=Q1"), "message reçu : {err}");

    let count: i64 = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM candidate")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
