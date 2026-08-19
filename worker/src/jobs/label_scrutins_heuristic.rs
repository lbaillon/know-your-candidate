//! Job `label_scrutins_heuristic { strategy?, seed_path?, scope? }` — voir
//! docs/plans/phase-3-categorisation.md, section « Job `label_scrutins_heuristic` ». Charge
//! l'ancrage gauche-droite (`db/seeds/group_axis.toml`), puis balaie `scrutin_groupe` en un seul
//! passage pour calculer `scrutin_axis_estimate` (D3.7 : une mesure, jamais une catégorisation,
//! jamais un thème).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::an::http::sha256_hex;
use crate::anomaly::{self, AnomalyRecord};
use crate::axis::{self, GroupeVote, Refus};
use crate::run;

use super::JobContext;

const DEFAULT_SEED_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../db/seeds/group_axis.toml");
const STRATEGY: &str = "group_alignment";
const RETAINED_LEGISLATURES: [i16; 3] = [15, 16, 17];
const BATCH_SIZE: usize = 1000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    version: String,
    #[serde(default)]
    description: String,
    grille_version: String,
    #[serde(default)]
    grille_date: Option<toml::value::Date>,
    source_url: String,
    #[serde(default, rename = "bloc")]
    blocs: HashMap<String, f64>,
    #[serde(default, rename = "groupe")]
    groupes: Vec<RawGroupe>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGroupe {
    an_uid: String,
    #[serde(default)]
    libelle: String,
    nuance: String,
    bloc: String,
    #[serde(default)]
    note: Option<String>,
}

/// Une entrée validée syntaxiquement, avant toute résolution contre `organe`.
struct ValidatedGroupe {
    an_uid: String,
    nuance: String,
    bloc: String,
    note: Option<String>,
    coordonnee: f64,
}

struct ValidatedSeed {
    version: String,
    description: String,
    grille_version: String,
    grille_date: chrono::NaiveDate,
    source_url: String,
    groupes: Vec<ValidatedGroupe>,
}

/// Une entrée validée puis résolue contre `organe` : ce que `load_axis` écrit réellement.
struct ResolvedGroupe<'a> {
    organe_id: i64,
    nuance: &'a str,
    bloc: &'a str,
    note: Option<&'a str>,
    coordonnee: f64,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub legislature: Option<i16>,
    pub since: Option<chrono::NaiveDate>,
}

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let seed_path = payload
        .get("seed_path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_SEED_PATH.to_string());
    let strategy = payload
        .get("strategy")
        .and_then(Value::as_str)
        .unwrap_or(STRATEGY)
        .to_string();
    let scope: Scope = match payload.get("scope") {
        Some(value) => serde_json::from_value(value.clone())?,
        None => Scope::default(),
    };

    let run_id = run::start(
        &ctx.pool,
        "label_scrutins_heuristic",
        ctx.job_id,
        payload.clone(),
    )
    .await?;

    match label(&ctx.pool, run_id, &strategy, &seed_path, &scope).await {
        Ok(counters) => {
            run::finish_ok(&ctx.pool, run_id, counters, None, None).await?;
            Ok(())
        }
        Err(err) => {
            run::finish_err(&ctx.pool, run_id, &err.to_string()).await?;
            Err(err)
        }
    }
}

/// Séparée de `run` pour permettre aux tests d'intégration de l'appeler directement.
pub async fn label(
    pool: &PgPool,
    run_id: i64,
    strategy: &str,
    seed_path: &str,
    scope: &Scope,
) -> anyhow::Result<Value> {
    if strategy != STRATEGY {
        anyhow::bail!(
            "stratégie inconnue : {strategy} (seule « {STRATEGY} » existe en phase 3 — \
             `principal_axis` est reportée en phase 4, D3.10)"
        );
    }

    let raw = std::fs::read_to_string(seed_path)
        .map_err(|err| anyhow::anyhow!("lecture de {seed_path} impossible : {err}"))?;
    let seed = parse_and_validate_syntax(&raw)?;
    let content_hash = sha256_hex(raw.as_bytes());

    // Toute résolution se fait avant la moindre écriture (job spec, étape 1) : an_uid inconnu,
    // pas un organe GP, hors 15-17, ou non-inscrit font tous échouer ici, avant `load_axis`.
    let mut resolved = Vec::with_capacity(seed.groupes.len());
    for groupe in &seed.groupes {
        let organe_id = resolve_organe_id(pool, &groupe.an_uid).await?;
        resolved.push(ResolvedGroupe {
            organe_id,
            nuance: &groupe.nuance,
            bloc: &groupe.bloc,
            note: groupe.note.as_deref(),
            coordonnee: groupe.coordonnee,
        });
    }

    let axis_version = load_axis(pool, &seed, &content_hash, &resolved).await?;

    let coordonnees: HashMap<i64, f64> = resolved
        .iter()
        .map(|g| (g.organe_id, g.coordonnee))
        .collect();

    compute_estimates(pool, run_id, &axis_version, scope, &coordonnees).await
}

async fn resolve_organe_id(pool: &PgPool, an_uid: &str) -> anyhow::Result<i64> {
    let row = sqlx::query!(
        "SELECT id, code_type, legislature, is_non_inscrit FROM organe WHERE an_uid = $1",
        an_uid,
    )
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or_else(|| anyhow::anyhow!("groupe {an_uid} : an_uid inconnu en base"))?;

    if row.code_type != "GP" {
        anyhow::bail!(
            "groupe {an_uid} : n'est pas un organe GP (code_type = {})",
            row.code_type
        );
    }
    let legislature = row
        .legislature
        .ok_or_else(|| anyhow::anyhow!("groupe {an_uid} : organe GP sans législature"))?;
    if !RETAINED_LEGISLATURES.contains(&legislature) {
        anyhow::bail!("groupe {an_uid} : législature {legislature} hors du périmètre 15-17");
    }
    if row.is_non_inscrit {
        anyhow::bail!(
            "groupe {an_uid} : organe non-inscrit, exclu de l'ancrage (methodology.md § 2)"
        );
    }

    Ok(row.id)
}

/// Insère `group_axis` + `group_axis_entry` si cette version est nouvelle, refuse si elle existe
/// déjà avec un contenu différent (immuabilité, job spec règle 3), et bascule `is_current` sur
/// cette version dans tous les cas.
async fn load_axis(
    pool: &PgPool,
    seed: &ValidatedSeed,
    content_hash: &str,
    resolved: &[ResolvedGroupe<'_>],
) -> anyhow::Result<String> {
    let mut tx = pool.begin().await?;

    let existing_hash: Option<String> = sqlx::query_scalar!(
        "SELECT content_hash FROM group_axis WHERE version = $1",
        seed.version,
    )
    .fetch_optional(&mut *tx)
    .await?;

    match existing_hash {
        Some(hash) if hash != content_hash => {
            anyhow::bail!(
                "version d'ancrage « {} » déjà chargée avec un contenu différent \
                 (hash existant {hash}, nouveau {content_hash}) : une coordonnée modifiée exige \
                 une nouvelle chaîne `version`",
                seed.version,
            );
        }
        Some(_) => {} // Contenu identique : rien à réinsérer, idempotent.
        None => {
            sqlx::query!(
                r#"
                INSERT INTO group_axis
                    (version, description, grille_version, grille_date, source_url, content_hash, is_current)
                VALUES ($1, $2, $3, $4, $5, $6, false)
                "#,
                seed.version,
                seed.description,
                seed.grille_version,
                seed.grille_date,
                seed.source_url,
                content_hash,
            )
            .execute(&mut *tx)
            .await?;

            let organe_ids: Vec<i64> = resolved.iter().map(|g| g.organe_id).collect();
            let nuances: Vec<&str> = resolved.iter().map(|g| g.nuance).collect();
            let blocs: Vec<&str> = resolved.iter().map(|g| g.bloc).collect();
            let coordonnees: Vec<f64> = resolved.iter().map(|g| g.coordonnee).collect();
            let notes: Vec<Option<&str>> = resolved.iter().map(|g| g.note).collect();

            sqlx::query!(
                r#"
                INSERT INTO group_axis_entry
                    (axis_version, organe_id, nuance_code, bloc, coordonnee, note)
                SELECT $1, organe_id, nuance_code, bloc, coordonnee::numeric, note
                FROM UNNEST($2::bigint[], $3::text[], $4::text[], $5::float8[], $6::text[])
                    AS t(organe_id, nuance_code, bloc, coordonnee, note)
                "#,
                seed.version,
                &organe_ids,
                &nuances as _,
                &blocs as _,
                &coordonnees,
                &notes as _,
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query!(
        "UPDATE group_axis SET is_current = (version = $1)",
        seed.version,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(seed.version.clone())
}

struct EstimateBatch {
    scrutin_ids: Vec<i64>,
    positions: Vec<f64>,
    separations: Vec<f64>,
    couvertures: Vec<f64>,
    votants: Vec<i32>,
}

impl EstimateBatch {
    fn new() -> Self {
        Self {
            scrutin_ids: Vec::new(),
            positions: Vec::new(),
            separations: Vec::new(),
            couvertures: Vec::new(),
            votants: Vec::new(),
        }
    }

    fn push(&mut self, scrutin_id: i64, estimation: axis::Estimation) {
        self.scrutin_ids.push(scrutin_id);
        self.positions.push(estimation.position_pour);
        self.separations.push(estimation.separation);
        self.couvertures.push(estimation.couverture);
        self.votants.push(estimation.votants_couverts as i32);
    }

    fn is_empty(&self) -> bool {
        self.scrutin_ids.is_empty()
    }

    /// Idempotent (job spec) : même seed et mêmes votes donnent la même ligne, `IS DISTINCT FROM`
    /// évite une écriture — et donc un `computed_at` qui bougerait — pour une rediffusion sans
    /// changement.
    async fn flush(&mut self, pool: &PgPool, axis_version: &str) -> anyhow::Result<()> {
        if self.is_empty() {
            return Ok(());
        }
        let strategies: Vec<&str> = self.scrutin_ids.iter().map(|_| STRATEGY).collect();
        let axis_versions: Vec<&str> = self.scrutin_ids.iter().map(|_| axis_version).collect();
        sqlx::query!(
            r#"
            INSERT INTO scrutin_axis_estimate
                (scrutin_id, strategy, axis_version, position_pour, separation, couverture, votants_couverts)
            SELECT scrutin_id, strategy, axis_version, position_pour::numeric, separation::numeric,
                   couverture::numeric, votants_couverts
            FROM UNNEST($1::bigint[], $2::text[], $3::text[], $4::float8[], $5::float8[], $6::float8[], $7::integer[])
                AS t(scrutin_id, strategy, axis_version, position_pour, separation, couverture, votants_couverts)
            ON CONFLICT (scrutin_id, strategy, axis_version) DO UPDATE SET
                position_pour    = EXCLUDED.position_pour,
                separation       = EXCLUDED.separation,
                couverture       = EXCLUDED.couverture,
                votants_couverts = EXCLUDED.votants_couverts,
                computed_at      = now()
            WHERE scrutin_axis_estimate.position_pour    IS DISTINCT FROM EXCLUDED.position_pour
               OR scrutin_axis_estimate.separation       IS DISTINCT FROM EXCLUDED.separation
               OR scrutin_axis_estimate.couverture       IS DISTINCT FROM EXCLUDED.couverture
               OR scrutin_axis_estimate.votants_couverts IS DISTINCT FROM EXCLUDED.votants_couverts
            "#,
            &self.scrutin_ids,
            &strategies as _,
            &axis_versions as _,
            &self.positions,
            &self.separations,
            &self.couvertures,
            &self.votants,
        )
        .execute(pool)
        .await?;
        self.scrutin_ids.clear();
        self.positions.clear();
        self.separations.clear();
        self.couvertures.clear();
        self.votants.clear();
        Ok(())
    }
}

struct ScrutinGroupeRow {
    scrutin_id: i64,
    organe_id: Option<i64>,
    pour: i32,
    contre: i32,
}

/// Balaie `scrutin_groupe` en un seul passage, trié par `scrutin_id` (jamais `vote`, job spec
/// étape 3). Calcule chaque estimation en mémoire (`axis::estimate`, fonctions pures) et écrit par
/// lots de `BATCH_SIZE`.
async fn compute_estimates(
    pool: &PgPool,
    run_id: i64,
    axis_version: &str,
    scope: &Scope,
    coordonnees: &HashMap<i64, f64>,
) -> anyhow::Result<Value> {
    let started = std::time::Instant::now();

    let rows = sqlx::query_as!(
        ScrutinGroupeRow,
        r#"
        SELECT sg.scrutin_id, sg.organe_id, sg.pour, sg.contre
        FROM scrutin_groupe sg
        JOIN scrutin s ON s.id = sg.scrutin_id
        WHERE s.chambre = 'assemblee'
          AND ($1::smallint IS NULL OR s.legislature = $1)
          AND ($2::date IS NULL OR s.date_scrutin >= $2)
        ORDER BY sg.scrutin_id
        "#,
        scope.legislature,
        scope.since,
    )
    .fetch_all(pool)
    .await?;

    let mut examined = 0i64;
    let mut written = 0i64;
    let mut refus_couverture = 0i64;
    let mut refus_effectif = 0i64;
    let mut refus_unanime = 0i64;
    let mut couvertures: Vec<f64> = Vec::new();

    let mut batch = EstimateBatch::new();
    let mut anomalies: Vec<AnomalyRecord> = Vec::new();

    let process_group = |scrutin_id: i64,
                         groupes: &[GroupeVote],
                         examined: &mut i64,
                         written: &mut i64,
                         refus_couverture: &mut i64,
                         refus_effectif: &mut i64,
                         refus_unanime: &mut i64,
                         couvertures: &mut Vec<f64>,
                         batch: &mut EstimateBatch,
                         anomalies: &mut Vec<AnomalyRecord>| {
        *examined += 1;
        match axis::estimate(groupes) {
            Ok(estimation) => {
                *written += 1;
                couvertures.push(estimation.couverture);
                batch.push(scrutin_id, estimation);
            }
            Err(Refus::AncrageInsuffisant) => {
                *refus_couverture += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::ANCRAGE_INSUFFISANT,
                    subject_uid: Some(scrutin_id.to_string()),
                    detail: serde_json::json!({}),
                });
            }
            Err(Refus::TropPeuDeVotants) => {
                *refus_effectif += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::TROP_PEU_DE_VOTANTS,
                    subject_uid: Some(scrutin_id.to_string()),
                    detail: serde_json::json!({}),
                });
            }
            Err(Refus::ScrutinUnanime) => {
                *refus_unanime += 1;
                anomalies.push(AnomalyRecord {
                    kind: anomaly::SCRUTIN_UNANIME,
                    subject_uid: Some(scrutin_id.to_string()),
                    detail: serde_json::json!({}),
                });
            }
        }
    };

    let mut current_scrutin_id: Option<i64> = None;
    let mut current_groupes: Vec<GroupeVote> = Vec::new();

    for row in rows {
        if current_scrutin_id != Some(row.scrutin_id) {
            if let Some(scrutin_id) = current_scrutin_id {
                process_group(
                    scrutin_id,
                    &current_groupes,
                    &mut examined,
                    &mut written,
                    &mut refus_couverture,
                    &mut refus_effectif,
                    &mut refus_unanime,
                    &mut couvertures,
                    &mut batch,
                    &mut anomalies,
                );
                if batch.scrutin_ids.len() >= BATCH_SIZE {
                    batch.flush(pool, axis_version).await?;
                }
                if anomalies.len() >= BATCH_SIZE {
                    anomaly::record_many(pool, run_id, &anomalies).await?;
                    anomalies.clear();
                }
            }
            current_scrutin_id = Some(row.scrutin_id);
            current_groupes.clear();
        }

        let coordonnee = row.organe_id.and_then(|id| coordonnees.get(&id).copied());
        current_groupes.push(GroupeVote {
            pour: row.pour as i64,
            contre: row.contre as i64,
            coordonnee,
        });
    }
    if let Some(scrutin_id) = current_scrutin_id {
        process_group(
            scrutin_id,
            &current_groupes,
            &mut examined,
            &mut written,
            &mut refus_couverture,
            &mut refus_effectif,
            &mut refus_unanime,
            &mut couvertures,
            &mut batch,
            &mut anomalies,
        );
    }

    batch.flush(pool, axis_version).await?;
    if !anomalies.is_empty() {
        anomaly::record_many(pool, run_id, &anomalies).await?;
    }

    let couverture_mediane = median(&mut couvertures);

    Ok(serde_json::json!({
        "scrutins_examines": examined,
        "estimations_ecrites": written,
        "refus_couverture": refus_couverture,
        "refus_effectif": refus_effectif,
        "refus_unanime": refus_unanime,
        "couverture_mediane": couverture_mediane,
        "duree_ms": started.elapsed().as_millis() as i64,
    }))
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        Some((values[mid - 1] + values[mid]) / 2.0)
    } else {
        Some(values[mid])
    }
}

/// Valide la syntaxe et l'auto-cohérence du fichier (job spec étape 1) : champs de source non
/// vides, blocs connus, coordonnées dans `[-1, 1]`, an_uid non vide et sans doublon. Ne touche pas
/// à la base — la résolution des an_uid contre `organe` se fait séparément, dans `label`.
fn parse_and_validate_syntax(raw: &str) -> anyhow::Result<ValidatedSeed> {
    let file: SeedFile =
        toml::from_str(raw).map_err(|err| anyhow::anyhow!("fichier de seed invalide : {err}"))?;

    if file.version.trim().is_empty() {
        anyhow::bail!("`version` est vide : voir le bandeau TODO en tête de ce fichier");
    }
    if file.grille_version.trim().is_empty() {
        anyhow::bail!("`grille_version` est vide : voir le bandeau TODO en tête de ce fichier");
    }
    let grille_date = file.grille_date.ok_or_else(|| {
        anyhow::anyhow!("`grille_date` est absente : voir le bandeau TODO en tête de ce fichier")
    })?;
    let grille_date = chrono::NaiveDate::from_ymd_opt(
        i32::from(grille_date.year),
        u32::from(grille_date.month),
        u32::from(grille_date.day),
    )
    .ok_or_else(|| anyhow::anyhow!("`grille_date` invalide"))?;
    if file.source_url.trim().is_empty() {
        anyhow::bail!("`source_url` est vide : voir le bandeau TODO en tête de ce fichier");
    }
    if file.groupes.is_empty() {
        anyhow::bail!("aucune entrée [[groupe]] dans le fichier");
    }

    for (nom, coordonnee) in &file.blocs {
        if !(-1.0..=1.0).contains(coordonnee) {
            anyhow::bail!("bloc {nom} : coordonnée {coordonnee} hors de [-1, 1]");
        }
    }

    let mut seen_an_uids = std::collections::HashSet::new();
    let mut groupes = Vec::with_capacity(file.groupes.len());
    for (index, groupe) in file.groupes.into_iter().enumerate() {
        let position = index + 1;
        if groupe.an_uid.trim().is_empty() {
            anyhow::bail!("entrée {position} : an_uid vide");
        }
        if !seen_an_uids.insert(groupe.an_uid.clone()) {
            anyhow::bail!("an_uid en double dans le fichier : {}", groupe.an_uid);
        }
        if groupe.nuance.trim().is_empty() {
            anyhow::bail!(
                "groupe {} : `nuance` est vide : voir le bandeau TODO en tête de ce fichier",
                groupe.an_uid
            );
        }
        if groupe.bloc.trim().is_empty() {
            anyhow::bail!(
                "groupe {} : `bloc` est vide : voir le bandeau TODO en tête de ce fichier",
                groupe.an_uid
            );
        }
        let coordonnee = *file.blocs.get(&groupe.bloc).ok_or_else(|| {
            anyhow::anyhow!(
                "groupe {} : bloc « {} » inconnu de la table [bloc]",
                groupe.an_uid,
                groupe.bloc
            )
        })?;
        let _ = &groupe.libelle; // Non utilisé au-delà de la lisibilité du fichier de seed.

        groupes.push(ValidatedGroupe {
            an_uid: groupe.an_uid,
            nuance: groupe.nuance,
            bloc: groupe.bloc,
            note: groupe.note,
            coordonnee,
        });
    }

    Ok(ValidatedSeed {
        version: file.version,
        description: file.description,
        grille_version: file.grille_version,
        grille_date,
        source_url: file.source_url,
        groupes,
    })
}
