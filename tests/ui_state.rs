use std::fs;

use cadenza::app::{AppState, BrowserAction};

#[test]
fn selection_moves_with_navigation_commands() {
    let temp_dir = tempfile::tempdir().unwrap();

    fs::create_dir_all(temp_dir.path().join("巴赫")).unwrap();
    fs::write(temp_dir.path().join("月光奏鸣曲.ogg"), b"stub").unwrap();

    let mut app = AppState::new(temp_dir.path()).unwrap();

    app.move_down();
    assert_eq!(app.selected_index(), 1);

    app.move_up();
    assert_eq!(app.selected_index(), 0);
}

#[test]
fn selection_stays_bounded() {
    let temp_dir = tempfile::tempdir().unwrap();

    fs::write(temp_dir.path().join("月光奏鸣曲.ogg"), b"stub").unwrap();
    let mut app = AppState::new(temp_dir.path()).unwrap();

    app.move_up();
    assert_eq!(app.selected_index(), 0);

    app.move_down();
    assert_eq!(app.selected_index(), 0);
}

#[test]
fn startup_expands_the_first_level_only() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bach_dir = temp_dir.path().join("巴赫");
    let organ_dir = bach_dir.join("管风琴");

    fs::create_dir_all(&organ_dir).unwrap();
    fs::write(bach_dir.join("托卡塔.ogg"), b"stub").unwrap();
    fs::write(organ_dir.join("赋格.ogg"), b"stub").unwrap();

    let app = AppState::new(temp_dir.path()).unwrap();
    let names = app
        .entries()
        .iter()
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["巴赫", "管风琴", "托卡塔.ogg"]);
    assert!(app.entries()[0].expanded);
    assert!(!app.entries()[1].expanded);
    assert_eq!(app.entries()[1].depth, 1);
}

#[test]
fn enter_toggles_directory_expansion() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bach_dir = temp_dir.path().join("巴赫");
    let organ_dir = bach_dir.join("管风琴");

    fs::create_dir_all(&organ_dir).unwrap();
    fs::write(organ_dir.join("赋格.ogg"), b"stub").unwrap();

    let mut app = AppState::new(temp_dir.path()).unwrap();

    assert_eq!(app.activate_selected().unwrap(), BrowserAction::ToggledDirectory);
    assert_eq!(
        app.entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["巴赫"]
    );

    assert_eq!(app.activate_selected().unwrap(), BrowserAction::ToggledDirectory);
    assert_eq!(
        app.entries()
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["巴赫", "管风琴"]
    );
}

#[test]
fn enter_plays_audio_files() {
    let temp_dir = tempfile::tempdir().unwrap();

    fs::create_dir_all(temp_dir.path().join("巴赫")).unwrap();
    fs::write(temp_dir.path().join("月光奏鸣曲.ogg"), b"stub").unwrap();

    let mut app = AppState::new(temp_dir.path()).unwrap();
    app.move_down();
    assert_eq!(
        app.activate_selected().unwrap(),
        BrowserAction::PlayFile(temp_dir.path().join("月光奏鸣曲.ogg"))
    );
}

#[test]
fn left_on_child_moves_selection_to_parent() {
    let temp_dir = tempfile::tempdir().unwrap();
    let bach_dir = temp_dir.path().join("巴赫");
    let organ_dir = bach_dir.join("管风琴");

    fs::create_dir_all(&organ_dir).unwrap();
    fs::write(organ_dir.join("赋格.ogg"), b"stub").unwrap();

    let mut app = AppState::new(temp_dir.path()).unwrap();
    app.move_down();
    assert_eq!(app.activate_selected().unwrap(), BrowserAction::ToggledDirectory);
    app.move_down();
    assert_eq!(app.selected_entry().unwrap().name, "赋格.ogg");

    assert!(app.move_left());
    assert_eq!(app.selected_entry().unwrap().name, "管风琴");

    assert!(app.move_left());
    assert_eq!(app.selected_entry().unwrap().name, "管风琴");

    assert!(app.move_left());
    assert_eq!(app.selected_entry().unwrap().name, "巴赫");
}
