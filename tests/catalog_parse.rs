use cadenza::catalog::load_catalog;

#[test]
fn parses_catalog_entries() {
    let entries = load_catalog("catalog/classics.toml").unwrap();

    assert!(entries.len() >= 8);
    assert!(entries.iter().any(|entry| entry.composer == "Bach"));
}
