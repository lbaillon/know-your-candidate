//! Reconstruit en mémoire les zips d'archives de l'Assemblée nationale à partir des fixtures JSON
//! versionnées dans `fixtures/` (voir `fixtures/README.md`, D1.17 de
//! docs/plans/phase-1.1-fix.md) : le `.zip` lui-même n'est jamais commité. `CompressionMethod::Stored`
//! est utilisé volontairement — la compression n'apporte rien sur des fixtures de quelques dizaines
//! de kilo-octets.
//!
//! Ce fichier vit dans `tests/support/mod.rs` (et non `tests/support.rs`) précisément pour que cargo
//! ne le traite pas comme un binaire de test à part entière.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::io::{Cursor, Write as _};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;

fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Zippe tous les fichiers sous `tests/fixtures/<dir>`, en conservant leurs chemins relatifs à
/// `<dir>` comme noms d'entrée du zip — ajouter un cas de test, c'est ajouter un fichier, rien de
/// plus.
pub fn zip_fixture_dir(dir: &str) -> bytes::Bytes {
    let base = fixtures_root().join(dir);
    let files = collect_files(&base);
    build_zip(&base, &files)
}

/// Zippe uniquement les fichiers listés (chemins relatifs à `tests/fixtures/<dir>`) — pour les
/// tests qui doivent réingérer un sous-ensemble des fixtures (voir F10, docs/plans/phase-1.1-fix.md).
pub fn zip_fixture_files(dir: &str, relative_paths: &[&str]) -> bytes::Bytes {
    let base = fixtures_root().join(dir);
    let files: Vec<PathBuf> = relative_paths.iter().map(|p| base.join(p)).collect();
    build_zip(&base, &files)
}

fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_files_into(dir, &mut out);
    out.sort();
    out
}

fn collect_files_into(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("répertoire de fixtures lisible") {
        let entry = entry.expect("entrée de répertoire lisible");
        let path = entry.path();
        if path.is_dir() {
            collect_files_into(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn build_zip(base: &Path, files: &[PathBuf]) -> bytes::Bytes {
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut buf = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
        for path in files {
            let rel = path
                .strip_prefix(base)
                .expect("fichier sous la racine de fixtures")
                .to_str()
                .expect("chemin de fixture en UTF-8")
                .replace('\\', "/");
            let contents = std::fs::read(path).expect("fichier de fixture lisible");
            writer.start_file(rel, options).expect("entrée zip");
            writer.write_all(&contents).expect("écriture zip");
        }
        writer.finish().expect("finalisation du zip");
    }
    bytes::Bytes::from(buf)
}
