//! Job `recompute_scores { scope? }` — voir docs/plans/phase-4-partis-scores.md, section « Job
//! `recompute_scores` ». `scope` n'existe que pour le débogage côté payload : **le calcul est
//! toujours complet et repart de zéro** (architecture.md § 6, « même corpus + mêmes règles = mêmes
//! scores »), il n'est jamais lu ici.
//!
//! Écrit `score_contribution`, `person_theme_score`, `groupe_theme_score` et `mandat_theme_score`
//! pour un nouveau `score_run`, puis bascule `is_current`. Ne supprime jamais un run antérieur
//! (D4.6) : c'est ce qui permet d'expliquer qu'un chiffre a bougé.
//!
//! Le score d'un groupe (et son score restreint à un mandat) se calcule à partir de
//! `scrutin_groupe` — déjà agrégé par (scrutin, groupe) à l'ingestion — plutôt qu'en rebalayant
//! chaque vote individuel : `apport` et `poids` ne dépendent que du (scrutin, thème), jamais de la
//! personne qui vote, donc la moyenne pondérée d'un groupe se reconstruit exactement à partir des
//! comptes agrégés « pour »/« contre ». Une même valeur d'apport répétée `n` fois pèse, dans une
//! moyenne pondérée, comme une seule contribution de poids `n × poids_unitaire` — c'est le principe
//! qui rend cette reconstruction correcte et évite de balayer les votes individuels pour les scores
//! de groupe. `score_contribution` (la preuve, l'explication cliquable) reste strictement
//! personnelle : ni `groupe_theme_score` ni `mandat_theme_score` n'en ont besoin, ils n'affichent
//! qu'un chiffre et une cohésion, jamais une liste de scrutins nommés (D4 : « le score personnel et
//! le score de groupe ne sont jamais fusionnés »).

use std::collections::HashMap;

use serde_json::Value;
use sqlx::PgPool;

use crate::run;
use crate::scoring::{self, Contribution, Position, ScrutinVotesGroupe};

use super::JobContext;

const BATCH_SIZE: usize = 2000;
const STRATEGY: &str = "group_alignment";

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let ingestion_run_id =
        run::start(&ctx.pool, "recompute_scores", ctx.job_id, payload.clone()).await?;

    match recompute(&ctx.pool).await {
        Ok(counters) => {
            run::finish_ok(&ctx.pool, ingestion_run_id, counters, None, None).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, ingestion_run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

struct Parametres {
    contributions_min: i16,
    scrutins_min_par_theme: i16,
    formula_version: i16,
}

async fn read_parametres(pool: &PgPool) -> anyhow::Result<Parametres> {
    let row = sqlx::query!(
        r#"SELECT contributions_min, scrutins_min_par_theme, formula_version FROM score_parametre WHERE id"#
    )
    .fetch_one(pool)
    .await?;
    Ok(Parametres {
        contributions_min: row.contributions_min,
        scrutins_min_par_theme: row.scrutins_min_par_theme,
        formula_version: row.formula_version,
    })
}

async fn start_score_run(pool: &PgPool, p: &Parametres) -> anyhow::Result<i64> {
    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO score_run (formula_version, contributions_min, scrutins_min_par_theme)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
        p.formula_version,
        p.contributions_min,
        p.scrutins_min_par_theme,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Thèmes éligibles (D4.7) : au moins `scrutins_min_par_theme` scrutins catégorisés avec une
/// position. Les autres ne produisent aucune ligne, pour personne — ils ne sont pas calculés puis
/// masqués à l'affichage, ils n'existent pas dans le run.
async fn eligible_theme_ids(
    pool: &PgPool,
    scrutins_min_par_theme: i16,
) -> anyhow::Result<Vec<i16>> {
    let ids = sqlx::query_scalar!(
        r#"
        SELECT t.id
        FROM theme t
        WHERE t.libelle_pole_positif IS NOT NULL
          AND (
              SELECT count(*) FROM scrutin_label sl
              WHERE sl.theme_id = t.id AND sl.position_pour IS NOT NULL
          ) >= $1::bigint
        ORDER BY t.id
        "#,
        i64::from(scrutins_min_par_theme),
    )
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

/// Séparée de `run` pour permettre aux tests d'intégration de l'appeler directement — même modèle
/// que `label_scrutins_heuristic::label`.
pub async fn recompute(pool: &PgPool) -> anyhow::Result<Value> {
    let started = std::time::Instant::now();
    let parametres = read_parametres(pool).await?;
    let score_run_id = start_score_run(pool, &parametres).await?;

    let axis_version: Option<String> =
        sqlx::query_scalar!(r#"SELECT version FROM group_axis WHERE is_current"#)
            .fetch_optional(pool)
            .await?;

    let eligible_themes = eligible_theme_ids(pool, parametres.scrutins_min_par_theme).await?;

    let (
        contributions_written,
        person_scores_written,
        groupe_scores_written,
        mandat_scores_written,
    ) = match axis_version.as_deref() {
        Some(axis_version) if !eligible_themes.is_empty() => {
            let (contributions_written, person_scores_written) = compute_person_scores(
                pool,
                score_run_id,
                &eligible_themes,
                axis_version,
                parametres.contributions_min,
            )
            .await?;
            let groupe_scores_written =
                compute_groupe_scores(pool, score_run_id, &eligible_themes, axis_version).await?;
            let mandat_scores_written =
                compute_mandat_scores(pool, score_run_id, &eligible_themes, axis_version).await?;
            (
                contributions_written,
                person_scores_written,
                groupe_scores_written,
                mandat_scores_written,
            )
        }
        _ => (0i64, 0i64, 0i64, 0i64),
    };

    // Bascule atomique, sur le modèle de `group_axis` (label_scrutins_heuristic) : un seul UPDATE
    // met à false toute autre ligne et à true celle-ci, sans jamais violer l'index partiel
    // « au plus un run courant ».
    sqlx::query!(
        r#"UPDATE score_run SET is_current = (id = $1)"#,
        score_run_id
    )
    .execute(pool)
    .await?;

    // Migration 0010 : les vues de lecture ne portent que le run courant. `CONCURRENTLY` exige
    // l'index unique posé par cette même migration et ne s'exécute pas dans une transaction —
    // aucune n'est ouverte ici, comme le reste de ce job (voir refresh_views.rs).
    sqlx::query!(r#"REFRESH MATERIALIZED VIEW CONCURRENTLY person_theme_score_courant"#)
        .execute(pool)
        .await?;
    sqlx::query!(r#"REFRESH MATERIALIZED VIEW CONCURRENTLY mandat_theme_score_courant"#)
        .execute(pool)
        .await?;

    // `score_run` ne grossit que d'une ligne par recalcul : autovacuum n'a souvent pas de raison
    // de la ré-analyser avant longtemps, et des statistiques périmées sur une table aussi petite
    // ont fait sous-estimer sa cardinalité à ce point que le planificateur déclenchait la
    // compilation JIT sur des lectures qui ne touchent que quelques lignes (mesuré : ~280 ms au
    // lieu de ~0,3 ms sur `queries/scores.py::get_person_orientations`). `ANALYZE` explicite ici,
    // pas laissé à autovacuum, sur les tables que les pages lisent à chaque requête.
    sqlx::query!(
        r#"ANALYZE score_run, person_theme_score_courant, mandat_theme_score_courant,
                    score_contribution, groupe_theme_score"#
    )
    .execute(pool)
    .await?;

    let counters = serde_json::json!({
        "themes_eligibles": eligible_themes.len(),
        // Le backend lit cette liste pour savoir quels thèmes afficher (« données insuffisantes »
        // vs. absent) sans redéfinir le seuil D4.7 côté Python : l'éligibilité gelée par CE run,
        // pas recalculée contre la valeur *actuelle* de score_parametre qui a pu changer depuis.
        "themes_eligibles_ids": eligible_themes,
        "score_contribution_ecrites": contributions_written,
        "person_theme_score_ecrites": person_scores_written,
        "groupe_theme_score_ecrites": groupe_scores_written,
        "mandat_theme_score_ecrites": mandat_scores_written,
        "duree_ms": started.elapsed().as_millis() as i64,
    });

    sqlx::query!(
        r#"UPDATE score_run SET finished_at = now(), counters = $2 WHERE id = $1"#,
        score_run_id,
        counters,
    )
    .execute(pool)
    .await?;

    Ok(counters)
}

fn parse_position(raw: &str) -> anyhow::Result<Position> {
    match raw {
        "pour" => Ok(Position::Pour),
        "contre" => Ok(Position::Contre),
        "abstention" => Ok(Position::Abstention),
        other => anyhow::bail!("position inattendue dans le calcul de score : {other}"),
    }
}

// --- Scores personnels ---------------------------------------------------------------------------

struct VoteRow {
    person_id: i64,
    theme_id: i16,
    scrutin_id: i64,
    position: String,
    position_pour: f64,
    poids_theme: f64,
    confiance: f64,
    bipolarite: f64,
    reviewed: bool,
}

struct ContributionBatch {
    run_id: i64,
    person_ids: Vec<i64>,
    theme_ids: Vec<i16>,
    scrutin_ids: Vec<i64>,
    positions: Vec<String>,
    apports: Vec<Option<f64>>,
    poids: Vec<f64>,
}

impl ContributionBatch {
    fn new(run_id: i64) -> Self {
        Self {
            run_id,
            person_ids: Vec::new(),
            theme_ids: Vec::new(),
            scrutin_ids: Vec::new(),
            positions: Vec::new(),
            apports: Vec::new(),
            poids: Vec::new(),
        }
    }

    fn push(
        &mut self,
        person_id: i64,
        theme_id: i16,
        scrutin_id: i64,
        position: &str,
        apport: Option<f64>,
        poids: f64,
    ) {
        self.person_ids.push(person_id);
        self.theme_ids.push(theme_id);
        self.scrutin_ids.push(scrutin_id);
        self.positions.push(position.to_string());
        self.apports.push(apport);
        self.poids.push(poids);
    }

    fn len(&self) -> usize {
        self.person_ids.len()
    }

    async fn flush(&mut self, pool: &PgPool) -> anyhow::Result<()> {
        if self.person_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            r#"
            INSERT INTO score_contribution (run_id, person_id, theme_id, scrutin_id, position, apport, poids)
            SELECT $1, person_id, theme_id, scrutin_id, position::vote_position, apport::numeric, poids::numeric
            FROM UNNEST($2::bigint[], $3::smallint[], $4::bigint[], $5::text[], $6::float8[], $7::float8[])
                AS t(person_id, theme_id, scrutin_id, position, apport, poids)
            "#,
            self.run_id,
            &self.person_ids,
            &self.theme_ids,
            &self.scrutin_ids,
            &self.positions,
            &self.apports as &[Option<f64>],
            &self.poids,
        )
        .execute(pool)
        .await?;
        self.person_ids.clear();
        self.theme_ids.clear();
        self.scrutin_ids.clear();
        self.positions.clear();
        self.apports.clear();
        self.poids.clear();
        Ok(())
    }
}

struct PersonScoreBatch {
    run_id: i64,
    person_ids: Vec<i64>,
    theme_ids: Vec<i16>,
    scores: Vec<f64>,
    incertitudes: Vec<f64>,
    contributions: Vec<i32>,
    abstentions: Vec<i32>,
    relues: Vec<i32>,
}

impl PersonScoreBatch {
    fn new(run_id: i64) -> Self {
        Self {
            run_id,
            person_ids: Vec::new(),
            theme_ids: Vec::new(),
            scores: Vec::new(),
            incertitudes: Vec::new(),
            contributions: Vec::new(),
            abstentions: Vec::new(),
            relues: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        person_id: i64,
        theme_id: i16,
        score: f64,
        incertitude: f64,
        contributions: i32,
        abstentions: i32,
        relues: i32,
    ) {
        self.person_ids.push(person_id);
        self.theme_ids.push(theme_id);
        self.scores.push(score);
        self.incertitudes.push(incertitude);
        self.contributions.push(contributions);
        self.abstentions.push(abstentions);
        self.relues.push(relues);
    }

    fn len(&self) -> usize {
        self.person_ids.len()
    }

    async fn flush(&mut self, pool: &PgPool) -> anyhow::Result<()> {
        if self.person_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            r#"
            INSERT INTO person_theme_score
                (run_id, person_id, theme_id, score, incertitude, contributions, abstentions, relues)
            SELECT $1, person_id, theme_id, score::numeric, incertitude::numeric, contributions, abstentions, relues
            FROM UNNEST($2::bigint[], $3::smallint[], $4::float8[], $5::float8[], $6::integer[], $7::integer[], $8::integer[])
                AS t(person_id, theme_id, score, incertitude, contributions, abstentions, relues)
            "#,
            self.run_id,
            &self.person_ids,
            &self.theme_ids,
            &self.scores,
            &self.incertitudes,
            &self.contributions,
            &self.abstentions,
            &self.relues,
        )
        .execute(pool)
        .await?;
        self.person_ids.clear();
        self.theme_ids.clear();
        self.scores.clear();
        self.incertitudes.clear();
        self.contributions.clear();
        self.abstentions.clear();
        self.relues.clear();
        Ok(())
    }
}

/// Traite un groupe de lignes (une personne, un thème, tous ses scrutins) : écrit toutes les
/// contributions dans `contribution_batch` (le seuil ne s'applique jamais à l'écriture de la
/// preuve, D4.2 protège l'affichage d'un score, pas la trace), et pousse une ligne dans
/// `score_batch` seulement si le nombre de contributions qui pèsent atteint `contributions_min`.
/// Rend `(nombre de contributions écrites, une ligne de score a-t-elle été écrite ?)`.
fn process_person_theme_group(
    group: &[VoteRow],
    contribution_batch: &mut ContributionBatch,
    score_batch: &mut PersonScoreBatch,
    contributions_min: i16,
) -> anyhow::Result<(i64, bool)> {
    if group.is_empty() {
        return Ok((0, false));
    }
    let person_id = group[0].person_id;
    let theme_id = group[0].theme_id;

    let mut scoring_contributions = Vec::with_capacity(group.len());
    for r in group {
        let position = parse_position(&r.position)?;
        let apport = scoring::apport(position, r.position_pour);
        let poids = if position == Position::Abstention {
            0.0
        } else {
            scoring::poids(r.poids_theme, r.confiance, r.bipolarite)
        };
        scoring_contributions.push(Contribution { apport, poids });
        contribution_batch.push(
            person_id,
            theme_id,
            r.scrutin_id,
            &r.position,
            apport,
            poids,
        );
    }

    let mut wrote_score = false;
    if let Some(aggregate) = scoring::aggregate(&scoring_contributions)
        && aggregate.contributions >= i64::from(contributions_min)
    {
        let relues = group.iter().filter(|r| r.reviewed).count() as i32;
        score_batch.push(
            person_id,
            theme_id,
            aggregate.score,
            aggregate.incertitude,
            aggregate.contributions as i32,
            aggregate.abstentions as i32,
            relues,
        );
        wrote_score = true;
    }

    Ok((group.len() as i64, wrote_score))
}

/// Balaie les votes des scrutins catégorisés sur un thème éligible (non-votants déjà exclus par la
/// requête, methodology.md § 3), écrit `score_contribution` pour chaque vote qui pèse ou qui
/// s'abstient, puis agrège en `person_theme_score` en écartant les couples sous
/// `contributions_min` (D4.2).
async fn compute_person_scores(
    pool: &PgPool,
    run_id: i64,
    eligible_themes: &[i16],
    axis_version: &str,
    contributions_min: i16,
) -> anyhow::Result<(i64, i64)> {
    let rows = sqlx::query_as!(
        VoteRow,
        r#"
        SELECT v.person_id, sl.theme_id, sl.scrutin_id,
               v.position::text AS "position!",
               sl.position_pour::float8 AS "position_pour!",
               sl.poids::float8 AS "poids_theme!",
               sl.confiance::float8 AS "confiance!",
               sae.bipolarite::float8 AS "bipolarite!",
               (sl.reviewed_at IS NOT NULL) AS "reviewed!"
        FROM scrutin_label sl
        JOIN scrutin_axis_estimate sae
            ON sae.scrutin_id = sl.scrutin_id AND sae.strategy = $3
           AND sae.axis_version = $1 AND sae.bipolarite IS NOT NULL
        JOIN vote v ON v.scrutin_id = sl.scrutin_id AND v.position <> 'non_votant'
        WHERE sl.theme_id = ANY($2::smallint[]) AND sl.position_pour IS NOT NULL
        ORDER BY v.person_id, sl.theme_id, sl.scrutin_id
        "#,
        axis_version,
        eligible_themes,
        STRATEGY,
    )
    .fetch_all(pool)
    .await?;

    let mut contribution_batch = ContributionBatch::new(run_id);
    let mut score_batch = PersonScoreBatch::new(run_id);
    let mut contributions_written = 0i64;
    let mut scores_written = 0i64;

    let mut current_key: Option<(i64, i16)> = None;
    let mut group: Vec<VoteRow> = Vec::new();

    for row in rows {
        let key = (row.person_id, row.theme_id);
        if current_key != Some(key) && !group.is_empty() {
            let (written, scored) = process_person_theme_group(
                &group,
                &mut contribution_batch,
                &mut score_batch,
                contributions_min,
            )?;
            contributions_written += written;
            if scored {
                scores_written += 1;
            }
            group.clear();
            if contribution_batch.len() >= BATCH_SIZE {
                contribution_batch.flush(pool).await?;
            }
            if score_batch.len() >= BATCH_SIZE {
                score_batch.flush(pool).await?;
            }
        }
        current_key = Some(key);
        group.push(row);
    }
    if !group.is_empty() {
        let (written, scored) = process_person_theme_group(
            &group,
            &mut contribution_batch,
            &mut score_batch,
            contributions_min,
        )?;
        contributions_written += written;
        if scored {
            scores_written += 1;
        }
    }

    contribution_batch.flush(pool).await?;
    score_batch.flush(pool).await?;

    Ok((contributions_written, scores_written))
}

// --- Scores de groupe ------------------------------------------------------------------------------

struct GroupeVoteRow {
    organe_id: i64,
    theme_id: i16,
    pour: i64,
    contre: i64,
    position_pour: f64,
    poids_theme: f64,
    confiance: f64,
    bipolarite: f64,
}

struct GroupeScoreBatch {
    run_id: i64,
    organe_ids: Vec<i64>,
    theme_ids: Vec<i16>,
    scores: Vec<f64>,
    cohesions: Vec<f64>,
    contributions: Vec<i32>,
    membres: Vec<i32>,
}

impl GroupeScoreBatch {
    fn new(run_id: i64) -> Self {
        Self {
            run_id,
            organe_ids: Vec::new(),
            theme_ids: Vec::new(),
            scores: Vec::new(),
            cohesions: Vec::new(),
            contributions: Vec::new(),
            membres: Vec::new(),
        }
    }

    fn push(
        &mut self,
        organe_id: i64,
        theme_id: i16,
        score: f64,
        cohesion: f64,
        contributions: i32,
        membres: i32,
    ) {
        self.organe_ids.push(organe_id);
        self.theme_ids.push(theme_id);
        self.scores.push(score);
        self.cohesions.push(cohesion);
        self.contributions.push(contributions);
        self.membres.push(membres);
    }

    fn len(&self) -> usize {
        self.organe_ids.len()
    }

    async fn flush(&mut self, pool: &PgPool) -> anyhow::Result<()> {
        if self.organe_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            r#"
            INSERT INTO groupe_theme_score (run_id, organe_id, theme_id, score, cohesion, contributions, membres)
            SELECT $1, organe_id, theme_id, score::numeric, cohesion::numeric, contributions, membres
            FROM UNNEST($2::bigint[], $3::smallint[], $4::float8[], $5::float8[], $6::integer[], $7::integer[])
                AS t(organe_id, theme_id, score, cohesion, contributions, membres)
            "#,
            self.run_id,
            &self.organe_ids,
            &self.theme_ids,
            &self.scores,
            &self.cohesions,
            &self.contributions,
            &self.membres,
        )
        .execute(pool)
        .await?;
        self.organe_ids.clear();
        self.theme_ids.clear();
        self.scores.clear();
        self.cohesions.clear();
        self.contributions.clear();
        self.membres.clear();
        Ok(())
    }
}

/// Nombre de personnes ayant un jour détenu un mandat `GP` dans chaque organe, tous mandats
/// confondus — la taille du groupe affichée à côté de son score et de sa cohésion. Indépendant du
/// thème : la même valeur vaut pour chaque ligne d'un même `organe_id`.
async fn membres_par_organe(pool: &PgPool) -> anyhow::Result<HashMap<i64, i32>> {
    struct Row {
        organe_id: i64,
        membres: Option<i64>,
    }
    let rows = sqlx::query_as!(
        Row,
        r#"
        SELECT organe_id, count(DISTINCT person_id) AS membres
        FROM mandat
        WHERE type_organe = 'GP'
        GROUP BY organe_id
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.organe_id, r.membres.unwrap_or(0) as i32))
        .collect())
}

/// Reconstruit le score et la cohésion d'un groupe sur un thème à partir des comptes agrégés
/// « pour »/« contre » de `scrutin_groupe` (voir le commentaire d'en-tête du module). `None` quand
/// aucun scrutin ne pèse — pas de ligne écrite, sur le même principe qu'un `person_theme_score`
/// manquant.
fn process_groupe_theme_group(
    group: &[GroupeVoteRow],
    membres_lookup: &HashMap<i64, i32>,
) -> Option<(i64, i16, f64, f64, i32, i32)> {
    if group.is_empty() {
        return None;
    }
    let organe_id = group[0].organe_id;
    let theme_id = group[0].theme_id;

    let mut scoring_contributions = Vec::with_capacity(group.len() * 2);
    let mut cohesion_input = Vec::with_capacity(group.len());
    let mut contributions_count = 0i64;

    for r in group {
        cohesion_input.push(ScrutinVotesGroupe {
            pour: r.pour,
            contre: r.contre,
        });

        let poids = scoring::poids(r.poids_theme, r.confiance, r.bipolarite);
        if poids > 0.0 {
            if r.pour > 0 {
                scoring_contributions.push(Contribution {
                    apport: Some(r.position_pour),
                    poids: poids * r.pour as f64,
                });
                contributions_count += r.pour;
            }
            if r.contre > 0 {
                scoring_contributions.push(Contribution {
                    apport: Some(-r.position_pour),
                    poids: poids * r.contre as f64,
                });
                contributions_count += r.contre;
            }
        }
    }

    let aggregate = scoring::aggregate(&scoring_contributions)?;
    let cohesion = scoring::cohesion(&cohesion_input)?;
    let membres = membres_lookup.get(&organe_id).copied().unwrap_or(0);

    Some((
        organe_id,
        theme_id,
        aggregate.score,
        cohesion,
        contributions_count as i32,
        membres,
    ))
}

/// Score de groupe sur toute son existence (D4.4, D4.12) : le groupe est déjà scopé à une
/// législature dans le schéma (un `organe_id` par instance de groupe), donc « toute son existence »
/// est simplement l'ensemble des votes rapportés sous cet `organe_id` par `scrutin_groupe`, sans
/// filtre de période supplémentaire.
async fn compute_groupe_scores(
    pool: &PgPool,
    run_id: i64,
    eligible_themes: &[i16],
    axis_version: &str,
) -> anyhow::Result<i64> {
    let membres_lookup = membres_par_organe(pool).await?;

    let rows = sqlx::query_as!(
        GroupeVoteRow,
        r#"
        WITH sg_agg AS (
            SELECT scrutin_id, organe_id, sum(pour) AS pour, sum(contre) AS contre
            FROM scrutin_groupe
            WHERE organe_id IS NOT NULL
            GROUP BY scrutin_id, organe_id
        )
        SELECT sg.organe_id AS "organe_id!", sl.theme_id,
               sg.pour AS "pour!", sg.contre AS "contre!",
               sl.position_pour::float8 AS "position_pour!",
               sl.poids::float8 AS "poids_theme!",
               sl.confiance::float8 AS "confiance!",
               sae.bipolarite::float8 AS "bipolarite!"
        FROM sg_agg sg
        JOIN organe o ON o.id = sg.organe_id AND NOT o.is_non_inscrit
        JOIN scrutin_label sl ON sl.scrutin_id = sg.scrutin_id AND sl.theme_id = ANY($2::smallint[])
                              AND sl.position_pour IS NOT NULL
        JOIN scrutin_axis_estimate sae
            ON sae.scrutin_id = sg.scrutin_id AND sae.strategy = $3
           AND sae.axis_version = $1 AND sae.bipolarite IS NOT NULL
        ORDER BY sg.organe_id, sl.theme_id, sg.scrutin_id
        "#,
        axis_version,
        eligible_themes,
        STRATEGY,
    )
    .fetch_all(pool)
    .await?;

    let mut batch = GroupeScoreBatch::new(run_id);
    let mut written = 0i64;

    let mut current_key: Option<(i64, i16)> = None;
    let mut group: Vec<GroupeVoteRow> = Vec::new();

    for row in rows {
        let key = (row.organe_id, row.theme_id);
        if current_key != Some(key) && !group.is_empty() {
            if let Some((organe_id, theme_id, score, cohesion, contributions, membres)) =
                process_groupe_theme_group(&group, &membres_lookup)
            {
                batch.push(organe_id, theme_id, score, cohesion, contributions, membres);
                written += 1;
            }
            group.clear();
            if batch.len() >= BATCH_SIZE {
                batch.flush(pool).await?;
            }
        }
        current_key = Some(key);
        group.push(row);
    }
    if !group.is_empty()
        && let Some((organe_id, theme_id, score, cohesion, contributions, membres)) =
            process_groupe_theme_group(&group, &membres_lookup)
    {
        batch.push(organe_id, theme_id, score, cohesion, contributions, membres);
        written += 1;
    }

    batch.flush(pool).await?;
    Ok(written)
}

// --- Scores restreints à un mandat (D4.12) --------------------------------------------------------

struct MandatVoteRow {
    mandat_id: i64,
    theme_id: i16,
    pour: i64,
    contre: i64,
    position_pour: f64,
    poids_theme: f64,
    confiance: f64,
    bipolarite: f64,
}

struct MandatScoreBatch {
    run_id: i64,
    mandat_ids: Vec<i64>,
    theme_ids: Vec<i16>,
    scores: Vec<f64>,
    cohesions: Vec<f64>,
    contributions: Vec<i32>,
}

impl MandatScoreBatch {
    fn new(run_id: i64) -> Self {
        Self {
            run_id,
            mandat_ids: Vec::new(),
            theme_ids: Vec::new(),
            scores: Vec::new(),
            cohesions: Vec::new(),
            contributions: Vec::new(),
        }
    }

    fn push(
        &mut self,
        mandat_id: i64,
        theme_id: i16,
        score: f64,
        cohesion: f64,
        contributions: i32,
    ) {
        self.mandat_ids.push(mandat_id);
        self.theme_ids.push(theme_id);
        self.scores.push(score);
        self.cohesions.push(cohesion);
        self.contributions.push(contributions);
    }

    fn len(&self) -> usize {
        self.mandat_ids.len()
    }

    async fn flush(&mut self, pool: &PgPool) -> anyhow::Result<()> {
        if self.mandat_ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            r#"
            INSERT INTO mandat_theme_score (run_id, mandat_id, theme_id, score, cohesion, contributions)
            SELECT $1, mandat_id, theme_id, score::numeric, cohesion::numeric, contributions
            FROM UNNEST($2::bigint[], $3::smallint[], $4::float8[], $5::float8[], $6::integer[])
                AS t(mandat_id, theme_id, score, cohesion, contributions)
            "#,
            self.run_id,
            &self.mandat_ids,
            &self.theme_ids,
            &self.scores,
            &self.cohesions,
            &self.contributions,
        )
        .execute(pool)
        .await?;
        self.mandat_ids.clear();
        self.theme_ids.clear();
        self.scores.clear();
        self.cohesions.clear();
        self.contributions.clear();
        Ok(())
    }
}

fn process_mandat_theme_group(group: &[MandatVoteRow]) -> Option<(i64, i16, f64, f64, i32)> {
    if group.is_empty() {
        return None;
    }
    let mandat_id = group[0].mandat_id;
    let theme_id = group[0].theme_id;

    let mut scoring_contributions = Vec::with_capacity(group.len() * 2);
    let mut cohesion_input = Vec::with_capacity(group.len());
    let mut contributions_count = 0i64;

    for r in group {
        cohesion_input.push(ScrutinVotesGroupe {
            pour: r.pour,
            contre: r.contre,
        });

        let poids = scoring::poids(r.poids_theme, r.confiance, r.bipolarite);
        if poids > 0.0 {
            if r.pour > 0 {
                scoring_contributions.push(Contribution {
                    apport: Some(r.position_pour),
                    poids: poids * r.pour as f64,
                });
                contributions_count += r.pour;
            }
            if r.contre > 0 {
                scoring_contributions.push(Contribution {
                    apport: Some(-r.position_pour),
                    poids: poids * r.contre as f64,
                });
                contributions_count += r.contre;
            }
        }
    }

    let aggregate = scoring::aggregate(&scoring_contributions)?;
    let cohesion = scoring::cohesion(&cohesion_input)?;

    Some((
        mandat_id,
        theme_id,
        aggregate.score,
        cohesion,
        contributions_count as i32,
    ))
}

/// Score d'un groupe restreint à la période d'un mandat précis (D4.12) : les mêmes votes du groupe
/// que `compute_groupe_scores`, mais filtrés par `mandat.period @> scrutin.date_scrutin` — « tout
/// l'intérêt des `daterange` ».
async fn compute_mandat_scores(
    pool: &PgPool,
    run_id: i64,
    eligible_themes: &[i16],
    axis_version: &str,
) -> anyhow::Result<i64> {
    let rows = sqlx::query_as!(
        MandatVoteRow,
        r#"
        WITH sg_agg AS (
            SELECT scrutin_id, organe_id, sum(pour) AS pour, sum(contre) AS contre
            FROM scrutin_groupe
            WHERE organe_id IS NOT NULL
            GROUP BY scrutin_id, organe_id
        )
        SELECT m.id AS "mandat_id!", sl.theme_id,
               sg.pour AS "pour!", sg.contre AS "contre!",
               sl.position_pour::float8 AS "position_pour!",
               sl.poids::float8 AS "poids_theme!",
               sl.confiance::float8 AS "confiance!",
               sae.bipolarite::float8 AS "bipolarite!"
        FROM mandat m
        JOIN organe o ON o.id = m.organe_id AND NOT o.is_non_inscrit
        JOIN sg_agg sg ON sg.organe_id = m.organe_id
        JOIN scrutin s ON s.id = sg.scrutin_id AND m.period @> s.date_scrutin
        JOIN scrutin_label sl ON sl.scrutin_id = sg.scrutin_id AND sl.theme_id = ANY($2::smallint[])
                              AND sl.position_pour IS NOT NULL
        JOIN scrutin_axis_estimate sae
            ON sae.scrutin_id = sg.scrutin_id AND sae.strategy = $3
           AND sae.axis_version = $1 AND sae.bipolarite IS NOT NULL
        WHERE m.type_organe = 'GP'
        ORDER BY m.id, sl.theme_id, sg.scrutin_id
        "#,
        axis_version,
        eligible_themes,
        STRATEGY,
    )
    .fetch_all(pool)
    .await?;

    let mut batch = MandatScoreBatch::new(run_id);
    let mut written = 0i64;

    let mut current_key: Option<(i64, i16)> = None;
    let mut group: Vec<MandatVoteRow> = Vec::new();

    for row in rows {
        let key = (row.mandat_id, row.theme_id);
        if current_key != Some(key) && !group.is_empty() {
            if let Some((mandat_id, theme_id, score, cohesion, contributions)) =
                process_mandat_theme_group(&group)
            {
                batch.push(mandat_id, theme_id, score, cohesion, contributions);
                written += 1;
            }
            group.clear();
            if batch.len() >= BATCH_SIZE {
                batch.flush(pool).await?;
            }
        }
        current_key = Some(key);
        group.push(row);
    }
    if !group.is_empty()
        && let Some((mandat_id, theme_id, score, cohesion, contributions)) =
            process_mandat_theme_group(&group)
    {
        batch.push(mandat_id, theme_id, score, cohesion, contributions);
        written += 1;
    }

    batch.flush(pool).await?;
    Ok(written)
}
