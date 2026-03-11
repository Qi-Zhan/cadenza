use std::path::Path;

use anyhow::Context;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct CatalogEntry {
    pub composer: String,
    pub title: String,
    pub source_page: String,
    pub target_path: String,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    tracks: Vec<CatalogEntry>,
}

pub fn load_catalog<P>(path: P) -> anyhow::Result<Vec<CatalogEntry>>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let catalog: CatalogFile =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(catalog.tracks)
}
