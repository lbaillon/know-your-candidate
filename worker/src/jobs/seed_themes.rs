//! Job `seed_themes { path?: string }` — voir docs/plans/phase-3-categorisation.md, section « Le
//! seed des thèmes ». Applique `db/seeds/themes.toml` : un choix éditorial versionné, pas une
//! ingestion distante — un fichier invalide fait échouer le job **avant toute écriture**, sur le
//! modèle de `seed_candidates` (docs/plans/phase-2-api-ui.md).

use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;

use crate::run;

use super::JobContext;

const DEFAULT_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../db/seeds/themes.toml");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedFile {
    #[serde(default, rename = "theme")]
    themes: Vec<RawTheme>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTheme {
    slug: String,
    libelle: String,
    description: String,
    pole_negatif: Option<String>,
    pole_positif: Option<String>,
    rang: i16,
}

struct ValidTheme {
    slug: String,
    libelle: String,
    description: String,
    pole_negatif: Option<String>,
    pole_positif: Option<String>,
    rang: i16,
}

pub async fn run(ctx: &JobContext, payload: &Value) -> anyhow::Result<()> {
    let path = payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| DEFAULT_PATH.to_string());

    let run_id = run::start(&ctx.pool, "seed_themes", ctx.job_id, payload.clone()).await?;

    match seed(&ctx.pool, &path).await {
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

/// Séparée de `run` pour permettre aux tests d'intégration de l'appeler directement sur un chemin
/// de fixture. Tout — upsert et désactivation — vit dans une seule transaction : un slug invalide
/// en dixième entrée ne doit laisser en base ni les neuf premières, ni une désactivation partielle.
pub async fn seed(pool: &PgPool, path: &str) -> anyhow::Result<Value> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("lecture de {path} impossible : {err}"))?;
    let themes = parse_and_validate(&raw)?;

    let mut tx = pool.begin().await?;

    let slugs: Vec<&str> = themes.iter().map(|t| t.slug.as_str()).collect();
    let libelles: Vec<&str> = themes.iter().map(|t| t.libelle.as_str()).collect();
    let descriptions: Vec<&str> = themes.iter().map(|t| t.description.as_str()).collect();
    let poles_negatifs: Vec<Option<&str>> =
        themes.iter().map(|t| t.pole_negatif.as_deref()).collect();
    let poles_positifs: Vec<Option<&str>> =
        themes.iter().map(|t| t.pole_positif.as_deref()).collect();
    let rangs: Vec<i16> = themes.iter().map(|t| t.rang).collect();

    let existing_before: std::collections::HashSet<String> =
        sqlx::query_scalar!("SELECT slug FROM theme")
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect();

    let result = sqlx::query!(
        r#"
        INSERT INTO theme (slug, libelle, description, libelle_pole_negatif, libelle_pole_positif, rang, actif)
        SELECT slug, libelle, description, pole_negatif, pole_positif, rang, true
        FROM UNNEST($1::text[], $2::text[], $3::text[], $4::text[], $5::text[], $6::smallint[])
            AS t(slug, libelle, description, pole_negatif, pole_positif, rang)
        ON CONFLICT (slug) DO UPDATE SET
            libelle              = EXCLUDED.libelle,
            description          = EXCLUDED.description,
            libelle_pole_negatif = EXCLUDED.libelle_pole_negatif,
            libelle_pole_positif = EXCLUDED.libelle_pole_positif,
            rang                 = EXCLUDED.rang,
            actif                = true,
            updated_at           = now()
        WHERE theme.libelle              IS DISTINCT FROM EXCLUDED.libelle
           OR theme.description          IS DISTINCT FROM EXCLUDED.description
           OR theme.libelle_pole_negatif IS DISTINCT FROM EXCLUDED.libelle_pole_negatif
           OR theme.libelle_pole_positif IS DISTINCT FROM EXCLUDED.libelle_pole_positif
           OR theme.rang                 IS DISTINCT FROM EXCLUDED.rang
           OR theme.actif                IS DISTINCT FROM true
        "#,
        &slugs as _,
        &libelles as _,
        &descriptions as _,
        &poles_negatifs as _,
        &poles_positifs as _,
        &rangs,
    )
    .execute(&mut *tx)
    .await?;

    let themes_crees = slugs
        .iter()
        .filter(|slug| !existing_before.contains(**slug))
        .count();
    let themes_mis_a_jour = (result.rows_affected() as usize).saturating_sub(themes_crees);

    // Jamais de suppression (contrairement à `seed_candidates`) : un thème est référencé par des
    // catégorisations. Retirer un thème du fichier le désactive, il disparaît des formulaires mais
    // reste dans l'historique.
    let desactives = sqlx::query_scalar!(
        r#"
        UPDATE theme SET actif = false, updated_at = now()
        WHERE slug <> ALL($1::text[]) AND actif = true
        RETURNING slug
        "#,
        &slugs as _,
    )
    .fetch_all(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(serde_json::json!({
        "entrees_lues": themes.len(),
        "themes_crees": themes_crees,
        "themes_mis_a_jour": themes_mis_a_jour,
        "themes_desactives": desactives.len(),
    }))
}

/// Tout est validé avant que `seed` n'écrive quoi que ce soit : slug non vide et unique,
/// description et libellé non vides, les deux pôles ensemble ou aucun, rang unique.
fn parse_and_validate(raw: &str) -> anyhow::Result<Vec<ValidTheme>> {
    let file: SeedFile =
        toml::from_str(raw).map_err(|err| anyhow::anyhow!("fichier de seed invalide : {err}"))?;

    let mut seen_slugs = std::collections::HashSet::new();
    let mut seen_rangs = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(file.themes.len());

    for (index, theme) in file.themes.into_iter().enumerate() {
        let position = index + 1;

        if theme.slug.trim().is_empty() {
            anyhow::bail!("entrée {position} : slug vide");
        }
        if !seen_slugs.insert(theme.slug.clone()) {
            anyhow::bail!("slug en double dans le fichier : {}", theme.slug);
        }
        if theme.libelle.trim().is_empty() {
            anyhow::bail!("entrée {position} (slug={}) : libelle vide", theme.slug);
        }
        if theme.description.trim().is_empty() {
            anyhow::bail!("entrée {position} (slug={}) : description vide", theme.slug);
        }
        if !seen_rangs.insert(theme.rang) {
            anyhow::bail!(
                "entrée {position} (slug={}) : rang {} déjà utilisé par un autre thème",
                theme.slug,
                theme.rang
            );
        }

        let pole_negatif = theme.pole_negatif.filter(|p| !p.trim().is_empty());
        let pole_positif = theme.pole_positif.filter(|p| !p.trim().is_empty());
        if pole_negatif.is_some() != pole_positif.is_some() {
            anyhow::bail!(
                "entrée {position} (slug={}) : un axe a les deux pôles ou aucun",
                theme.slug
            );
        }

        out.push(ValidTheme {
            slug: theme.slug,
            libelle: theme.libelle,
            description: theme.description,
            pole_negatif,
            pole_positif,
            rang: theme.rang,
        });
    }

    Ok(out)
}
