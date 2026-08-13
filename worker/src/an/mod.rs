//! Tout ce qui parle au format de l'open data de l'Assemblée nationale : téléchargement,
//! décompression, et parsing du JSON (qui est un XML converti mécaniquement, voir `json`).

pub mod acteur;
pub mod http;
pub mod json;
pub mod scrutin;

use std::io::Cursor;

use bytes::Bytes;

/// Ouvre l'archive `.zip` reçue, entièrement en mémoire — jamais de décompression écrite sur
/// disque (voir docs/plans/phase-1-ingestion.md, « Écriture »). Les archives font quelques
/// dizaines de mégaoctets compressés, largement dans le raisonnable pour un `Cursor` en mémoire.
pub fn open_zip(bytes: Bytes) -> anyhow::Result<zip::ZipArchive<Cursor<Bytes>>> {
    Ok(zip::ZipArchive::new(Cursor::new(bytes))?)
}
