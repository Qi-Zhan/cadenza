use std::path::PathBuf;

use crate::catalog::CatalogEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPlan {
    pub composer: String,
    pub title: String,
    pub source_page: String,
    pub target_path: PathBuf,
}

pub fn plan_downloads(entries: &[CatalogEntry], root: PathBuf) -> Vec<DownloadPlan> {
    entries
        .iter()
        .map(|entry| DownloadPlan {
            composer: entry.composer.clone(),
            title: entry.title.clone(),
            source_page: entry.source_page.clone(),
            target_path: root.join(&entry.target_path),
        })
        .collect()
}
