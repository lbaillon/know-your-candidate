//! Bibliothèque du worker : le binaire (`main.rs`) n'est qu'un point d'entrée fin, pour que les
//! tests d'intégration de `tests/` puissent appeler directement les modules (notamment `jobs::queue`).

pub mod config;
pub mod db;
pub mod heartbeat;
pub mod jobs;
pub mod shutdown;
