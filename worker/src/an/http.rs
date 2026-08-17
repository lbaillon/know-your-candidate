//! Client HTTP poli vers l'open data de l'Assemblée nationale — voir
//! docs/plans/phase-1-ingestion.md, section « Client HTTP ». Une requête à la fois (le worker
//! n'ingère jamais deux archives en parallèle), `User-Agent` identifiant le projet,
//! `If-None-Match` sur l'`ETag` pour éviter un retéléchargement inutile, cache disque en
//! développement.

use std::path::PathBuf;
use std::time::Duration;

use sha2::{Digest, Sha256};

/// Un serveur public qui ne répond plus ne doit jamais bloquer un job indéfiniment.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

pub const USER_AGENT: &str =
    "know-your-candidate/0.1 (+https://github.com/lbaillon/know-your-candidate)";

/// Ce que le worker a reçu pour une archive : le contenu, le hash **que nous calculons** (seul
/// hash qui fait foi, voir data-sources.md — le MD5 publié par l'AN ne correspond pas toujours à
/// l'archive servie), et l'ETag pour un prochain appel conditionnel.
pub struct FetchedArchive {
    pub bytes: bytes::Bytes,
    pub content_hash: String,
    pub etag: Option<String>,
    /// `true` si le contenu vient du cache disque (304 Not Modified ou pas de reconnexion tentée).
    pub from_cache: bool,
}

/// Répertoire de cache des archives téléchargées, actif **si et seulement si** `AN_CACHE_DIR` est
/// renseignée (D1.18, docs/plans/phase-1.1-fix.md, F6) : `None` sinon, y compris en production, où
/// il n'y a pas de disque persistant à en attendre entre deux déploiements et où écrire ~300 Mo
/// d'archives dans le répertoire courant du worker n'a rien d'anodin. Factorisée ici — les deux
/// jobs qui construisaient chacun leur propre chemin la partagent désormais.
pub fn cache_dir() -> Option<PathBuf> {
    std::env::var("AN_CACHE_DIR").ok().map(PathBuf::from)
}

pub struct AnClient {
    http: reqwest::Client,
    /// `None` : cache désactivé (voir `cache_dir()`).
    cache_dir: Option<PathBuf>,
}

impl AnClient {
    pub fn new(cache_dir: Option<PathBuf>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;
        Ok(Self { http, cache_dir })
    }

    fn cache_paths(&self, url: &str) -> Option<(PathBuf, PathBuf)> {
        let dir = self.cache_dir.as_ref()?;
        let name = cache_key(url);
        Some((
            dir.join(format!("{name}.body")),
            dir.join(format!("{name}.etag")),
        ))
    }

    /// Télécharge une archive `.zip`, en flux, sans jamais écrire de contenu décompressé sur
    /// disque : seul le corps compressé (tel que reçu) va au cache. `force_refetch` ignore le
    /// cache et l'ETag connu — utilisé quand un job le demande explicitement.
    pub async fn fetch_zip(
        &self,
        url: &str,
        force_refetch: bool,
    ) -> anyhow::Result<FetchedArchive> {
        let cache = self.cache_paths(url);

        let cached_etag = if force_refetch {
            None
        } else {
            match &cache {
                Some((_, etag_path)) => tokio::fs::read_to_string(etag_path).await.ok(),
                None => None,
            }
        };

        let mut request = self.http.get(url);
        if let Some(etag) = &cached_etag {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }

        let response = request.send().await?.error_for_status()?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let (body_path, _) = cache
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("304 reçu sans cache local pour {url}"))?;
            let bytes = tokio::fs::read(body_path).await?;
            let content_hash = sha256_hex(&bytes);
            return Ok(FetchedArchive {
                bytes: bytes::Bytes::from(bytes),
                content_hash,
                etag: cached_etag,
                from_cache: true,
            });
        }

        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = response.bytes().await?;
        let content_hash = sha256_hex(&bytes);

        if let Some((body_path, etag_path)) = &cache {
            // F9 (docs/plans/phase-1.1-fix.md) : le répertoire parent de `body_path` est déjà le
            // chemin qui prouve la présence du cache, pas besoin de retraverser `self.cache_dir`
            // via un `.expect()` — `cache_paths` garantit que `body_path` a toujours un parent.
            if let Some(parent) = body_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(body_path, &bytes).await?;
            if let Some(etag) = &etag {
                tokio::fs::write(etag_path, etag).await?;
            }
        }

        Ok(FetchedArchive {
            bytes,
            content_hash,
            etag,
            from_cache: false,
        })
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn cache_key(url: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    to_hex(&hasher.finalize())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    to_hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_hex_is_stable_and_deterministic() {
        let a = sha256_hex(b"contenu");
        let b = sha256_hex(b"contenu");
        let c = sha256_hex(b"autre contenu");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn cache_key_is_stable_and_differs_by_url() {
        let a = cache_key("https://example.org/a.zip");
        let b = cache_key("https://example.org/a.zip");
        let c = cache_key("https://example.org/b.zip");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
