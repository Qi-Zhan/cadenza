use std::{
    io,
    path::Path,
    time::Duration,
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, LineGauge, List, ListItem, Paragraph, Wrap},
};

use crate::{
    app::{AppState, BrowserAction, PlaybackState},
    library::{BrowserEntry, BrowserEntryKind},
    player::{PlaybackProgress, PlayerController},
};

const VISUALIZER_BUCKETS: usize = 64;
const VISUALIZER_WINDOW_SIZE: usize = 8192;
const NEEDLE_GLYPH: char = '│';
const HEAD_GLYPH: char = '●';
const PEAK_GLYPH: char = '•';
const GLOW_GLYPH: char = '·';
const TARGET_CENTER_WEIGHT: f32 = 0.6;
const TARGET_NEIGHBOR_WEIGHT: f32 = 0.2;
const SURFACE_ATTACK: f32 = 0.34;
const SURFACE_RELEASE: f32 = 0.16;
const SURFACE_DAMPING: f32 = 0.72;
const SURFACE_REVERSAL_DAMPING: f32 = 0.05;
const PEAK_LIFT: f32 = 0.09;
const PEAK_FALL_RATE: f32 = 0.028;
const ORB_BASE_OFFSET: f32 = 0.018;
const ORB_PEAK_INFLUENCE: f32 = 0.35;
const ORB_MAX_OFFSET: f32 = 0.14;
pub fn render_browser_labels(
    entries: &[BrowserEntry],
    state: &AppState,
    limit: usize,
) -> Vec<String> {
    let selected = state.selected_index();
    let visible = if limit == 0 {
        entries.len()
    } else {
        entries.len().min(limit)
    };
    let start = if visible == 0 || selected < visible {
        0
    } else {
        selected + 1 - visible
    };

    entries
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(index, entry)| {
            let prefix = if index == selected { "> " } else { "  " };
            let icon = match entry.kind {
                BrowserEntryKind::Directory if entry.expanded => "▾",
                BrowserEntryKind::Directory => "▸",
                BrowserEntryKind::AudioFile => "♪",
            };
            let indent = "  ".repeat(entry.depth);
            format!("{prefix}{indent}{icon} {}", entry.name)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct VisualizerFrameState {
    smoothed: Vec<f32>,
    surface_velocities: Vec<f32>,
    peaks: Vec<f32>,
    orb_positions: Vec<f32>,
}

impl VisualizerFrameState {
    pub fn new(bucket_count: usize) -> Self {
        Self {
            smoothed: vec![0.0; bucket_count],
            surface_velocities: vec![0.0; bucket_count],
            peaks: vec![0.0; bucket_count],
            orb_positions: vec![0.0; bucket_count],
        }
    }

    pub fn advance(&mut self, raw: &[f32]) {
        if raw.is_empty() {
            self.clear();
            return;
        }

        if self.smoothed.len() != raw.len() {
            self.smoothed.resize(raw.len(), 0.0);
            self.surface_velocities.resize(raw.len(), 0.0);
            self.peaks.resize(raw.len(), 0.0);
            self.orb_positions.resize(raw.len(), 0.0);
        }

        let targets = raw
            .iter()
            .enumerate()
            .map(|(index, _)| smoothed_target(raw, index))
            .collect::<Vec<_>>();

        for (index, target) in targets.into_iter().enumerate() {
            let previous = self.smoothed[index];
            let response = if target >= previous {
                SURFACE_ATTACK
            } else {
                SURFACE_RELEASE
            };
            let previous_velocity = self.surface_velocities[index];
            let carried_velocity = if (target - previous).signum() != previous_velocity.signum()
                && previous_velocity != 0.0
            {
                previous_velocity * SURFACE_REVERSAL_DAMPING
            } else {
                previous_velocity * SURFACE_DAMPING
            };
            let velocity = carried_velocity + (target - previous) * response;
            let surface = (previous + velocity).clamp(0.0, 1.0);

            self.surface_velocities[index] = velocity;
            self.smoothed[index] = surface;

            let lifted_peak = (surface + PEAK_LIFT + (target - surface).max(0.0) * 0.18).min(1.0);
            let peak = if surface >= self.peaks[index] {
                lifted_peak.max(surface)
            } else {
                (self.peaks[index] - PEAK_FALL_RATE).max(surface)
            };
            self.peaks[index] = peak.clamp(0.0, 1.0);

            let desired_orb = (surface
                + ORB_BASE_OFFSET
                + (self.peaks[index] - surface).max(0.0) * ORB_PEAK_INFLUENCE)
                .min(surface + ORB_MAX_OFFSET)
                .min(1.0);
            self.orb_positions[index] = desired_orb.max(surface);
        }
    }

    pub fn clear(&mut self) {
        self.smoothed.fill(0.0);
        self.surface_velocities.fill(0.0);
        self.peaks.fill(0.0);
        self.orb_positions.fill(0.0);
    }

    pub fn smoothed(&self) -> &[f32] {
        &self.smoothed
    }

    pub fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    pub fn orb_positions(&self) -> &[f32] {
        &self.orb_positions
    }
}

fn smoothed_target(raw: &[f32], index: usize) -> f32 {
    if raw.is_empty() {
        return 0.0;
    }

    let left = raw.get(index.saturating_sub(1)).copied().unwrap_or(raw[index]);
    let center = raw[index];
    let right = raw.get(index + 1).copied().unwrap_or(raw[index]);

    (left * TARGET_NEIGHBOR_WEIGHT
        + center * TARGET_CENTER_WEIGHT
        + right * TARGET_NEIGHBOR_WEIGHT)
        .clamp(0.0, 1.0)
}

pub fn status_line(entry_count: usize, _state: &AppState) -> String {
    if entry_count == 0 {
        "No playable files found. add files under your library directory and restart cadenza."
            .to_string()
    } else {
        format!("{entry_count} entries loaded")
    }
}

pub fn run_ui(state: &mut AppState) -> anyhow::Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut playback = PlaybackState::Stopped;
    let mut player = None::<PlayerController>;
    let mut status_message = String::new();
    let mut visualizer = VisualizerFrameState::new(VISUALIZER_BUCKETS);

    loop {
        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };

            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('j') | KeyCode::Down => state.move_down(),
                KeyCode::Char('k') | KeyCode::Up => state.move_up(),
                KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                    if state.move_left() {
                        if let Some(entry) = state.selected_entry() {
                            status_message = format!("browse {}", entry.name);
                        }
                    }
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    if state.move_right() {
                        if let Some(entry) = state.selected_entry() {
                            status_message = format!("browse {}", entry.name);
                        }
                    }
                }
                KeyCode::Enter => match state.activate_selected()? {
                    BrowserAction::None => {}
                    BrowserAction::ToggledDirectory => {
                        if let Some(entry) = state.selected_entry() {
                            status_message = format!("browse {}", entry.name);
                        }
                    }
                    BrowserAction::PlayFile(path) => match PlayerController::from_path(&path) {
                        Ok(controller) => {
                            playback = PlaybackState::Playing;
                            status_message = format!("playing {}", display_name_from_path(&path));
                            state.set_now_playing(Some(path));
                            visualizer.clear();
                            player = Some(controller);
                        }
                        Err(error) => {
                            playback = PlaybackState::Stopped;
                            status_message = format!("playback failed: {error}");
                        }
                    },
                },
                KeyCode::Char(' ') => {
                    if let Some(active_player) = player.as_ref() {
                        match active_player.toggle_pause() {
                            Ok(state_value) => playback = state_value,
                            Err(error) => status_message = format!("pause failed: {error}"),
                        }
                    } else {
                        playback.toggle_pause();
                    }
                }
                _ => {}
            }
        }

        let raw_buckets = player
            .as_ref()
            .map(|active| active.current_buckets(VISUALIZER_BUCKETS, VISUALIZER_WINDOW_SIZE))
            .unwrap_or_default();
        let playback_progress = player.as_ref().map(|active| active.progress());
        if let Some(active_player) = player.as_ref() {
            playback = active_player.playback_state();
        }

        match playback {
            PlaybackState::Playing => visualizer.advance(&raw_buckets),
            PlaybackState::Paused => {}
            PlaybackState::Stopped => visualizer.clear(),
        }

        terminal.draw(|frame| {
            draw_ui(
                frame,
                state,
                &playback,
                playback_progress.as_ref(),
                &visualizer,
                &status_message,
            );
        })?;
    }

    Ok(())
}

fn draw_ui(
    frame: &mut ratatui::Frame<'_>,
    state: &AppState,
    playback: &PlaybackState,
    playback_progress: Option<&PlaybackProgress>,
    visualizer: &VisualizerFrameState,
    status_message: &str,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(frame.area());

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(48)])
        .split(layout[0]);

    let left_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(6)])
        .split(panels[0]);

    let spectrum_sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(panels[1]);

    let labels = render_browser_labels(
        state.entries(),
        state,
        left_sections[1].height.saturating_sub(2) as usize,
    );
    let items = if labels.is_empty() {
        vec![ListItem::new(Line::from("No files yet"))]
    } else {
        labels
            .into_iter()
            .map(|label| {
                let style = if label.starts_with("> ") {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Rgb(224, 176, 92))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(214, 204, 190))
                };
                ListItem::new(Line::from(Span::styled(label, style)))
            })
            .collect::<Vec<_>>()
    };

    let now_block = Block::default()
        .title(now_panel_title(playback))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(212, 140, 72)));
    let now_inner = now_block.inner(left_sections[0]);
    let now_label = current_now_label(state, playback);
    let now_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1), Constraint::Length(1)])
        .split(now_inner);
    let now_name = Paragraph::new(now_playing_panel_text(
        now_label.as_deref(),
        now_layout[0].width as usize,
        now_layout[0].height as usize,
    ))
    .style(Style::default().fg(Color::Rgb(241, 231, 218)))
    .alignment(Alignment::Center);
    let now_time = Paragraph::new(now_progress_label(playback_progress))
        .style(Style::default().fg(Color::Rgb(196, 182, 162)))
        .alignment(Alignment::Center);
    let now_gauge = LineGauge::default()
        .filled_style(Style::default().fg(Color::Rgb(224, 176, 92)))
        .unfilled_style(Style::default().fg(Color::Rgb(92, 78, 64)))
        .ratio(now_progress_ratio(playback_progress))
        .line_set(ratatui::symbols::line::THICK)
        .label("");

    let tracks_widget = List::new(items).block(
        Block::default()
            .title(catalog_panel_title())
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(145, 124, 106))),
    );

    let spectrum_header = Paragraph::new(visualizer_header_text())
        .style(Style::default().fg(Color::Rgb(241, 231, 218)))
        .block(
            Block::default()
                .title(spectrum_panel_title())
                .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(Color::Rgb(212, 140, 72))),
        )
        .wrap(Wrap { trim: true });

    let spectrum = Paragraph::new(visualizer_panel_text_styled(
        spectrum_sections[1].width.saturating_sub(2) as usize,
        spectrum_sections[1].height.saturating_sub(1) as usize,
        visualizer,
    ))
    .block(
        Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(212, 140, 72))),
    )
    .wrap(Wrap { trim: false });

    let footer = Paragraph::new(footer_text(state.entries().len(), state, playback, status_message))
        .style(Style::default().fg(Color::Rgb(241, 231, 218)))
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(145, 124, 106))),
        );

    frame.render_widget(now_block, left_sections[0]);
    frame.render_widget(now_name, now_layout[0]);
    frame.render_widget(now_time, now_layout[1]);
    frame.render_widget(now_gauge, now_layout[2]);
    frame.render_widget(tracks_widget, left_sections[1]);
    frame.render_widget(spectrum_header, spectrum_sections[0]);
    frame.render_widget(spectrum, spectrum_sections[1]);
    frame.render_widget(footer, layout[1]);
}

fn footer_text(
    entry_count: usize,
    state: &AppState,
    playback: &PlaybackState,
    status_message: &str,
) -> String {
    if entry_count == 0 {
        return status_line(entry_count, state);
    }

    let base = short_footer_text(entry_count, state, playback);

    if status_message.is_empty() {
        base
    } else {
        format!("{base} {status_message}")
    }
}

pub fn short_footer_text(entry_count: usize, state: &AppState, playback: &PlaybackState) -> String {
    let playback_label = match playback {
        PlaybackState::Stopped => "•",
        PlaybackState::Playing => "▶",
        PlaybackState::Paused => "⏸",
    };
    let action_label = match playback {
        PlaybackState::Playing => "Space pause",
        PlaybackState::Paused => "Space resume",
        PlaybackState::Stopped => "Enter play",
    };
    let audio_count = if state.entries().is_empty() {
        entry_count
    } else {
        state.audio_file_count()
    };
    let selected_audio = if state.entries().is_empty() {
        state.selected_index() + 1
    } else {
        state.selected_audio_position().unwrap_or(0)
    };

    format!(
        "♫ {audio_count} files | {playback_label} {selected_audio}/{audio_count} | {action_label} | q quit",
    )
}

pub fn now_playing_panel_text(label: Option<&str>, width: usize, height: usize) -> String {
    let title = match label {
        Some(label) => fit_inline(label, width),
        None => fit_inline("No selection", width),
    };

    let centered_lines = title
        .lines()
        .map(|line| center_line(line, width))
        .collect::<Vec<_>>();
    let top_padding = height.saturating_sub(centered_lines.len()) / 2;

    let mut lines = vec![String::new(); top_padding];
    lines.extend(centered_lines);
    lines.join("\n")
}

pub fn now_progress_label(progress: Option<&PlaybackProgress>) -> String {
    let (position_secs, total_secs) = progress
        .map(|progress| (progress.position_secs, progress.total_secs))
        .unwrap_or((0, 0));

    format!(
        "{} / {}",
        format_clock(position_secs),
        format_clock(total_secs),
    )
}

pub fn now_progress_ratio(progress: Option<&PlaybackProgress>) -> f64 {
    progress.map(|progress| progress.ratio).unwrap_or(0.0)
}

pub fn now_panel_title(playback: &PlaybackState) -> &'static str {
    match playback {
        PlaybackState::Playing => "▶ Now",
        PlaybackState::Paused => "⏸ Now",
        PlaybackState::Stopped => "• Now",
    }
}

pub fn catalog_panel_title() -> &'static str {
    "♫ Catalog"
}

pub fn spectrum_panel_title() -> &'static str {
    "▥ Spectrum"
}

pub fn visualizer_panel_text(
    width: usize,
    height: usize,
    frame_state: &VisualizerFrameState,
) -> String {
    visualizer_panel_text_styled(width, height, frame_state)
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn visualizer_panel_text_styled(
    width: usize,
    height: usize,
    frame_state: &VisualizerFrameState,
) -> Text<'static> {
    if frame_state.smoothed().iter().all(|value| *value <= 0.0) {
        return Text::from(build_idle_visualizer_panel(width, height));
    }

    let buckets = frame_state.smoothed();
    let peaks = frame_state.peaks();
    let orbs = frame_state.orb_positions();
    let inner_width = width.max(1);
    let inner_height = height.max(1);
    let signal_height = inner_height.max(1);
    let mut rows = Vec::with_capacity(inner_height);

    for row in 0..inner_height {
        let mut spans = Vec::with_capacity(inner_width);
        for column in 0..inner_width {
            let bucket_index = column * buckets.len() / inner_width;
            let raw_height = (buckets[bucket_index] * signal_height as f32)
                .ceil()
                .clamp(0.0, signal_height as f32) as usize;
            let raw_peak = (peaks[bucket_index] * signal_height as f32)
                .ceil()
                .clamp(0.0, signal_height as f32) as usize;
            let raw_orb = (orbs[bucket_index] * signal_height as f32)
                .ceil()
                .clamp(0.0, signal_height as f32) as usize;
            let height_cells = if raw_height > 0 {
                raw_height.max(2).min(signal_height)
            } else {
                0
            };
            let peak_cells = if raw_peak > 0 {
                raw_peak.max(2).min(signal_height)
            } else {
                0
            };
            let orb_cells = if raw_orb > 0 {
                raw_orb.max(height_cells.max(2)).min(signal_height)
            } else {
                height_cells
            };
            let distance_from_bottom = inner_height.saturating_sub(1 + row);
            let top_extent = peak_cells.max(orb_cells).max(if height_cells > 0 { height_cells } else { 1 });

            let role = if height_cells == 0 && distance_from_bottom == 0 {
                VisualizerCellRole::Line
            } else if orb_cells > 0 && distance_from_bottom + 1 == orb_cells {
                VisualizerCellRole::Head
            } else if distance_from_bottom < height_cells {
                VisualizerCellRole::Line
            } else if distance_from_bottom < peak_cells.max(orb_cells) {
                VisualizerCellRole::Peak
            } else if top_extent >= 2 && distance_from_bottom == top_extent {
                VisualizerCellRole::Glow
            } else {
                VisualizerCellRole::Empty
            };

            spans.push(Span::styled(
                role.glyph().to_string(),
                glyph_style(column, inner_width, role),
            ));
        }
        rows.push(Line::from(spans));
    }

    Text::from(rows)
}

fn build_idle_visualizer_panel(width: usize, height: usize) -> String {
    let inner_width = width.max(1);
    let inner_height = height.max(1);
    let top_padding = inner_height.saturating_sub(1) / 2;
    let prompt = "Press Enter";

    let mut lines = vec![String::new(); top_padding];
    for line in prompt.lines() {
        lines.push(center_line(line, inner_width));
    }

    while lines.len() < inner_height {
        lines.push(String::new());
    }

    lines.join("\n")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisualizerCellRole {
    Empty,
    Line,
    Head,
    Peak,
    Glow,
}

impl VisualizerCellRole {
    fn glyph(self) -> char {
        match self {
            Self::Empty => ' ',
            Self::Line => NEEDLE_GLYPH,
            Self::Head => HEAD_GLYPH,
            Self::Peak => PEAK_GLYPH,
            Self::Glow => GLOW_GLYPH,
        }
    }
}

fn glyph_style(column: usize, width: usize, role: VisualizerCellRole) -> Style {
    let base = gradient_color(column, width);

    match role {
        VisualizerCellRole::Head => Style::default()
            .fg(lighten_color(base, 34))
            .add_modifier(Modifier::BOLD),
        VisualizerCellRole::Peak => Style::default().fg(lighten_color(base, 16)),
        VisualizerCellRole::Glow => Style::default()
            .fg(lighten_color(base, 22))
            .add_modifier(Modifier::DIM),
        VisualizerCellRole::Line => Style::default().fg(base),
        VisualizerCellRole::Empty => Style::default(),
    }
}

fn gradient_color(column: usize, width: usize) -> Color {
    const STOPS: [(u8, u8, u8); 6] = [
        (242, 126, 84),
        (236, 214, 96),
        (114, 202, 94),
        (108, 210, 231),
        (92, 120, 242),
        (214, 109, 214),
    ];

    if width <= 1 {
        let (r, g, b) = STOPS[0];
        return Color::Rgb(r, g, b);
    }

    let position = column as f32 / (width - 1) as f32;
    let scaled = position * (STOPS.len() - 1) as f32;
    let left = scaled.floor() as usize;
    let right = scaled.ceil() as usize;
    let mix = scaled - left as f32;
    let (lr, lg, lb) = STOPS[left];
    let (rr, rg, rb) = STOPS[right.min(STOPS.len() - 1)];

    Color::Rgb(
        lerp_channel(lr, rr, mix),
        lerp_channel(lg, rg, mix),
        lerp_channel(lb, rb, mix),
    )
}

fn lighten_color(color: Color, amount: u8) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(amount),
            g.saturating_add(amount),
            b.saturating_add(amount),
        ),
        other => other,
    }
}

fn lerp_channel(left: u8, right: u8, mix: f32) -> u8 {
    ((left as f32) + (right as f32 - left as f32) * mix)
        .round()
        .clamp(0.0, 255.0) as u8
}

pub fn visualizer_header_text() -> String {
    String::new()
}

fn current_now_label(state: &AppState, playback: &PlaybackState) -> Option<String> {
    if matches!(playback, PlaybackState::Playing | PlaybackState::Paused) {
        return state
            .now_playing_path()
            .map(display_name_from_path);
    }

    state.selected_entry().map(|entry| entry.name.clone())
}

fn display_name_from_path(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn center_line(line: &str, width: usize) -> String {
    if line.len() >= width {
        return line.to_string();
    }

    let left_padding = (width - line.len()) / 2;
    format!("{}{}", " ".repeat(left_padding), line)
}

fn format_clock(total_secs: u64) -> String {
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn fit_inline(line: &str, width: usize) -> String {
    if width == 0 || line.chars().count() <= width {
        return line.to_string();
    }

    if width <= 1 {
        return "…".to_string();
    }

    let mut fitted = line.chars().take(width - 1).collect::<String>();
    fitted.push('…');
    fitted
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}
