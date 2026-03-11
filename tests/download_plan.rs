use std::path::PathBuf;

use cadenza::catalog::load_catalog;
use cadenza::download::plan_downloads;

#[test]
fn builds_download_plan_for_catalog() {
    let entries = load_catalog("catalog/classics.toml").unwrap();
    let plans = plan_downloads(&entries, PathBuf::from("music"));

    assert_eq!(plans.len(), entries.len());
    assert!(plans[0].target_path.starts_with("music"));
    assert!(plans[0].target_path.extension().is_some());
}
