use cadenza::{
    app::{AppState, PlaybackState},
    library::{BrowserEntry, BrowserEntryKind},
    player::PlaybackProgress,
    ui::{
        VisualizerFrameState, catalog_panel_title, now_panel_title, now_playing_panel_text,
        now_progress_label, now_progress_ratio, render_browser_labels, short_footer_text,
        spectrum_panel_title, status_line, visualizer_header_text, visualizer_panel_text,
        visualizer_panel_text_styled,
    },
};

fn entry(name: &str, kind: BrowserEntryKind) -> BrowserEntry {
    BrowserEntry {
        name: name.into(),
        path: format!("music/{name}").into(),
        kind,
        depth: 0,
        expanded: false,
    }
}

#[test]
fn marks_the_selected_browser_entry() {
    let entries = vec![
        entry("巴赫", BrowserEntryKind::Directory),
        entry("月光奏鸣曲.ogg", BrowserEntryKind::AudioFile),
    ];
    let mut state = AppState::with_track_count(entries.len());
    state.move_down();

    let labels = render_browser_labels(&entries, &state, 4);

    assert_eq!(labels.len(), 2);
    assert!(labels[0].starts_with("  "));
    assert!(labels[1].starts_with("> "));
}

#[test]
fn browser_labels_include_entry_icons() {
    let entries = vec![
        BrowserEntry {
            expanded: true,
            ..entry("巴赫", BrowserEntryKind::Directory)
        },
        entry("月光奏鸣曲.ogg", BrowserEntryKind::AudioFile),
    ];
    let state = AppState::with_track_count(entries.len());

    let labels = render_browser_labels(&entries, &state, 4);

    assert!(labels[0].contains("▾"));
    assert!(labels[1].contains("♪"));
}

#[test]
fn browser_labels_render_tree_indentation() {
    let entries = vec![
        BrowserEntry {
            expanded: true,
            ..entry("巴赫", BrowserEntryKind::Directory)
        },
        BrowserEntry {
            depth: 1,
            ..entry("管风琴", BrowserEntryKind::Directory)
        },
        BrowserEntry {
            depth: 2,
            ..entry("赋格.ogg", BrowserEntryKind::AudioFile)
        },
    ];
    let state = AppState::with_track_count(entries.len());

    let labels = render_browser_labels(&entries, &state, 8);

    assert!(labels[0].contains("▾ 巴赫"));
    assert!(labels[1].contains("  ▸ 管风琴"));
    assert!(labels[2].contains("    ♪ 赋格.ogg"));
}

#[test]
fn shows_fetch_hint_when_library_is_empty() {
    let state = AppState::with_track_count(0);

    assert!(status_line(0, &state).contains("add files"));
}

#[test]
fn shows_an_intentional_idle_visualizer_message() {
    let frame_state = VisualizerFrameState::new(8);
    let panel = visualizer_panel_text(40, 10, &frame_state);

    assert!(panel.contains("Press Enter"));
    assert!(!panel.contains('█'));
}

#[test]
fn smooths_visualizer_drops_between_frames() {
    let mut frame_state = VisualizerFrameState::new(3);
    frame_state.advance(&[1.0, 0.5, 0.25]);
    let first = frame_state.smoothed().to_vec();

    frame_state.advance(&[0.0, 0.0, 0.0]);
    let second = frame_state.smoothed().to_vec();

    assert!(second[0] > 0.0);
    assert!(second[0] < first[0]);
    assert!(frame_state.peaks()[0] >= second[0]);
}

#[test]
fn tempers_the_first_rise_of_a_strong_hit() {
    let mut frame_state = VisualizerFrameState::new(1);

    frame_state.advance(&[1.0]);

    assert!(frame_state.smoothed()[0] < 0.6);
}

#[test]
fn spreads_an_isolated_spike_into_neighboring_buckets() {
    let mut frame_state = VisualizerFrameState::new(5);
    frame_state.advance(&[0.0, 0.0, 1.0, 0.0, 0.0]);

    assert!(frame_state.smoothed()[1] > 0.0);
    assert!(frame_state.smoothed()[3] > 0.0);
    assert!(frame_state.smoothed()[2] > frame_state.smoothed()[1]);
}

#[test]
fn keeps_the_top_accent_close_to_the_surface_after_a_strong_hit() {
    let mut frame_state = VisualizerFrameState::new(1);
    frame_state.advance(&[1.0]);

    assert!(frame_state.orb_positions()[0] > frame_state.smoothed()[0]);
    assert!(frame_state.orb_positions()[0] - frame_state.smoothed()[0] < 0.16);
}

#[test]
fn orb_heads_fall_back_without_sinking_below_the_line() {
    let mut frame_state = VisualizerFrameState::new(1);
    frame_state.advance(&[1.0]);
    frame_state.advance(&[1.0]);
    let launched = frame_state.orb_positions()[0];

    frame_state.advance(&[0.2]);

    assert!(frame_state.orb_positions()[0] < launched);
    assert!(frame_state.orb_positions()[0] >= frame_state.smoothed()[0]);
}

#[test]
fn graph_body_uses_needle_lines() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]);
    let panel = visualizer_panel_text(40, 10, &frame_state);

    assert!(panel.contains('│'));
}

#[test]
fn graph_body_restores_head_peak_and_glow_markers() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]);
    let panel = visualizer_panel_text(40, 10, &frame_state);

    assert!(panel.contains('│'));
    assert!(panel.contains('●'));
    assert!(panel.contains('•'));
    assert!(panel.contains('·'));
}

#[test]
fn graph_body_uses_multiple_gradient_colors() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]);
    let text = visualizer_panel_text_styled(40, 10, &frame_state);

    let colors = text
        .lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .filter_map(|span| span.style.fg)
        .collect::<Vec<_>>();

    assert!(colors.len() > 4);
    assert!(colors.windows(2).any(|pair| pair[0] != pair[1]));
}

#[test]
fn graph_body_uses_the_full_available_height() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]);
    let panel = visualizer_panel_text(40, 18, &frame_state);

    assert_eq!(panel.lines().count(), 18);
}

#[test]
fn graph_body_uses_the_full_available_width() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.1, 0.2, 0.3, 0.45, 0.6, 0.75, 0.9, 1.0]);
    let panel = visualizer_panel_text(40, 10, &frame_state);

    assert!(panel.lines().all(|line| line.chars().count() == 40));
}

#[test]
fn graph_body_keeps_low_energy_columns_anchored_to_the_baseline() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.02, 0.03, 0.04, 0.05, 0.02, 0.03, 0.04, 0.05]);
    let panel = visualizer_panel_text(40, 10, &frame_state);
    let rows = panel.lines().collect::<Vec<_>>();
    let bottom_row = rows[rows.len().saturating_sub(1)];

    assert!(bottom_row.contains('│'));
}

#[test]
fn graph_body_keeps_a_continuous_bottom_edge_while_active() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.12, 0.0, 0.08, 0.0, 0.04, 0.0, 0.02, 0.0]);
    let panel = visualizer_panel_text(40, 10, &frame_state);
    let rows = panel.lines().collect::<Vec<_>>();
    let bottom_row = rows[rows.len().saturating_sub(1)];

    assert!(bottom_row.chars().all(|glyph| glyph == '│'));
}

#[test]
fn graph_body_keeps_bottom_row_free_of_top_markers() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.12, 0.28, 0.44, 0.18, 0.62, 0.31, 0.8, 0.24]);
    let panel = visualizer_panel_text(40, 10, &frame_state);
    let rows = panel.lines().collect::<Vec<_>>();
    let bottom_row = rows[rows.len().saturating_sub(1)];

    assert!(!bottom_row.contains('●'));
    assert!(!bottom_row.contains('•'));
    assert!(!bottom_row.contains('·'));
}

#[test]
fn graph_body_grows_contiguously_from_the_baseline() {
    let mut frame_state = VisualizerFrameState::new(8);
    frame_state.advance(&[0.12, 0.28, 0.44, 0.18, 0.62, 0.31, 0.8, 0.24]);
    let panel = visualizer_panel_text(40, 10, &frame_state);
    let rows = panel.lines().map(|line| line.chars().collect::<Vec<_>>()).collect::<Vec<_>>();
    let width = rows.first().map(|row| row.len()).unwrap_or(0);

    for column in 0..width {
        let mut seen_signal = false;
        let mut seen_gap_after_signal = false;

        for row in (0..rows.len()).rev() {
            let glyph = rows[row][column];
            let is_signal = matches!(glyph, '│' | '●' | '•' | '·');

            if is_signal {
                seen_signal = true;
                assert!(
                    !seen_gap_after_signal,
                    "column {column} has a gap below a rendered signal glyph"
                );
            } else if seen_signal {
                seen_gap_after_signal = true;
            }
        }
    }
}

#[test]
fn visualizer_header_is_empty() {
    assert!(visualizer_header_text().is_empty());
}

#[test]
fn now_title_uses_state_icon() {
    assert_eq!(now_panel_title(&PlaybackState::Playing), "▶ Now");
    assert_eq!(now_panel_title(&PlaybackState::Paused), "⏸ Now");
    assert_eq!(now_panel_title(&PlaybackState::Stopped), "• Now");
}

#[test]
fn catalog_title_uses_music_icon() {
    assert_eq!(catalog_panel_title(), "♫ Catalog");
}

#[test]
fn spectrum_title_uses_visualizer_icon() {
    assert_eq!(spectrum_panel_title(), "▥ Spectrum");
}

#[test]
fn now_panel_body_only_shows_selected_text() {
    let panel = now_playing_panel_text(Some("巴赫 / 托卡塔与赋格 BWV 565"), 32, 4);

    assert!(panel.contains("巴赫"));
    assert!(!panel.contains("Stopped"));
    assert!(!panel.contains("Playing"));
    assert!(!panel.contains("Paused"));
}

#[test]
fn now_panel_body_centers_text() {
    let panel = now_playing_panel_text(Some("巴赫 / 托卡塔"), 28, 4);

    assert!(panel.starts_with('\n'));
    assert!(panel.contains("巴赫 / 托卡塔"));
}

#[test]
fn now_panel_body_centers_text_vertically() {
    let panel = now_playing_panel_text(Some("巴赫 / 托卡塔"), 28, 4);
    let lines = panel.lines().collect::<Vec<_>>();

    assert_eq!(lines.len(), 2);
    assert!(lines[0].trim().is_empty());
    assert!(lines[1].contains("巴赫 / 托卡塔"));
}

#[test]
fn formats_now_progress_time_label() {
    let label = now_progress_label(Some(&PlaybackProgress {
        position_secs: 42,
        total_secs: 271,
        ratio: 42.0 / 271.0,
    }));

    assert_eq!(label, "00:42 / 04:31");
}

#[test]
fn uses_zero_progress_label_when_idle() {
    assert_eq!(now_progress_label(None), "00:00 / 00:00");
}

#[test]
fn exposes_progress_ratio_for_gauge() {
    let ratio = now_progress_ratio(Some(&PlaybackProgress {
        position_secs: 90,
        total_secs: 180,
        ratio: 0.5,
    }));

    assert_eq!(ratio, 0.5);
    assert_eq!(now_progress_ratio(None), 0.0);
}

#[test]
fn footer_text_uses_short_playback_actions() {
    let state = AppState::with_track_count(11);
    let footer = short_footer_text(11, &state, &PlaybackState::Stopped);

    assert!(footer.contains("♫ 11 files"));
    assert!(footer.contains("• 1/11"));
    assert!(footer.contains("Enter play"));
    assert!(footer.contains("q quit"));
    assert!(!footer.contains("selection"));
    assert!(!footer.contains("toggle/play"));
    assert!(!footer.contains("h/l fold"));
}

#[test]
fn footer_text_switches_pause_copy_by_state() {
    let state = AppState::with_track_count(11);

    assert!(short_footer_text(11, &state, &PlaybackState::Playing).contains("▶ 1/11"));
    assert!(short_footer_text(11, &state, &PlaybackState::Playing).contains("Space pause"));
    assert!(short_footer_text(11, &state, &PlaybackState::Paused).contains("⏸ 1/11"));
    assert!(short_footer_text(11, &state, &PlaybackState::Paused).contains("Space resume"));
}
