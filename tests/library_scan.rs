use std::fs;

use cadenza::library::{BrowserEntryKind, scan_entries};

#[test]
fn filters_out_hidden_and_unsupported_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let visible_dir = temp_dir.path().join("巴赫");
    let hidden_dir = temp_dir.path().join(".cache");

    fs::create_dir_all(&visible_dir).unwrap();
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(visible_dir.join("托卡塔与赋格.ogg"), b"stub").unwrap();
    fs::write(visible_dir.join(".草稿.mp3"), b"ignore").unwrap();
    fs::write(visible_dir.join("notes.txt"), b"ignore").unwrap();
    fs::write(hidden_dir.join("ghost.ogg"), b"ignore").unwrap();

    let entries = scan_entries(temp_dir.path()).unwrap();
    let names = entries.iter().map(|entry| entry.name.as_str()).collect::<Vec<_>>();

    assert!(!names.contains(&".cache"));
    assert!(!names.contains(&".草稿.mp3"));
    assert!(!names.contains(&"notes.txt"));
}

#[test]
fn keeps_directories_and_supported_audio_files() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bach_dir = temp_dir.path().join("巴赫");

    fs::create_dir_all(&bach_dir).unwrap();
    fs::write(temp_dir.path().join("月光奏鸣曲.ogg"), b"stub").unwrap();

    let entries = scan_entries(temp_dir.path()).unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "巴赫");
    assert_eq!(entries[0].kind, BrowserEntryKind::Directory);
    assert_eq!(entries[1].name, "月光奏鸣曲.ogg");
    assert_eq!(entries[1].kind, BrowserEntryKind::AudioFile);
}
