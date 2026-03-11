use crate::{
    cli::CommandKind,
    library::{BrowserEntry, BrowserEntryKind, TreeNode, build_tree, expand_first_level, flatten_tree},
    ui,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    selected: usize,
    track_count: usize,
    browser: Option<BrowserState>,
    now_playing: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BrowserState {
    root: std::path::PathBuf,
    tree: Vec<TreeNode>,
    entries: Vec<BrowserEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserAction {
    None,
    ToggledDirectory,
    PlayFile(std::path::PathBuf),
}

impl PlaybackState {
    pub fn toggle_pause(&mut self) {
        *self = match self {
            Self::Playing => Self::Paused,
            Self::Paused => Self::Playing,
            Self::Stopped => Self::Stopped,
        };
    }
}

impl AppState {
    pub fn new(root: &std::path::Path) -> anyhow::Result<Self> {
        let mut tree = build_tree(root)?;
        expand_first_level(&mut tree);
        let entries = flatten_tree(&tree);

        Ok(Self {
            selected: 0,
            track_count: entries.len(),
            browser: Some(BrowserState {
                root: root.to_path_buf(),
                tree,
                entries,
            }),
            now_playing: None,
        })
    }

    pub fn with_track_count(track_count: usize) -> Self {
        Self {
            selected: 0,
            track_count,
            browser: None,
            now_playing: None,
        }
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn entries(&self) -> &[BrowserEntry] {
        self.browser
            .as_ref()
            .map(|browser| browser.entries.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_entry(&self) -> Option<&BrowserEntry> {
        self.entries().get(self.selected_index())
    }

    pub fn audio_file_count(&self) -> usize {
        self.entries()
            .iter()
            .filter(|entry| entry.kind == BrowserEntryKind::AudioFile)
            .count()
    }

    pub fn selected_audio_position(&self) -> Option<usize> {
        let selected = self.selected_entry()?;
        if selected.kind != BrowserEntryKind::AudioFile {
            return None;
        }

        Some(
            self.entries()
                .iter()
                .take(self.selected_index() + 1)
                .filter(|entry| entry.kind == BrowserEntryKind::AudioFile)
                .count(),
        )
    }

    pub fn now_playing_path(&self) -> Option<&std::path::Path> {
        self.now_playing.as_deref()
    }

    pub fn set_now_playing(&mut self, path: Option<std::path::PathBuf>) {
        self.now_playing = path;
    }

    pub fn move_down(&mut self) {
        let item_count = self.item_count();
        if item_count == 0 {
            return;
        }

        self.selected = (self.selected + 1).min(item_count.saturating_sub(1));
    }

    pub fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub fn activate_selected(&mut self) -> anyhow::Result<BrowserAction> {
        let Some(entry) = self.selected_entry().cloned() else {
            return Ok(BrowserAction::None);
        };

        match entry.kind {
            BrowserEntryKind::Directory => {
                self.set_directory_expanded(&entry.path, !entry.expanded);
                Ok(BrowserAction::ToggledDirectory)
            }
            BrowserEntryKind::AudioFile => Ok(BrowserAction::PlayFile(entry.path)),
        }
    }

    pub fn move_left(&mut self) -> bool {
        let Some(entry) = self.selected_entry().cloned() else {
            return false;
        };

        if entry.kind == BrowserEntryKind::Directory && entry.expanded {
            self.set_directory_expanded(&entry.path, false);
            return true;
        }

        let Some(parent_index) = self.find_parent_index(self.selected_index()) else {
            return false;
        };

        self.selected = parent_index;
        true
    }

    pub fn move_right(&mut self) -> bool {
        let Some(entry) = self.selected_entry().cloned() else {
            return false;
        };

        if entry.kind == BrowserEntryKind::Directory && !entry.expanded {
            self.set_directory_expanded(&entry.path, true);
            return true;
        }

        false
    }

    fn item_count(&self) -> usize {
        self.browser
            .as_ref()
            .map(|browser| browser.entries.len())
            .unwrap_or(self.track_count)
    }

    fn set_directory_expanded(&mut self, path: &std::path::Path, expanded: bool) {
        let selected_path = self.selected_entry().map(|entry| entry.path.clone());

        if let Some(browser) = self.browser.as_mut() {
            set_tree_expanded(&mut browser.tree, path, expanded);
            browser.entries = flatten_tree(&browser.tree);
            self.track_count = browser.entries.len();
        }

        if let Some(selected_path) = selected_path {
            if let Some(index) = self
                .entries()
                .iter()
                .position(|entry| entry.path == selected_path)
            {
                self.selected = index;
                return;
            }
        }

        self.selected = self.selected.min(self.item_count().saturating_sub(1));
    }

    fn find_parent_index(&self, child_index: usize) -> Option<usize> {
        let child = self.entries().get(child_index)?;
        if child.depth == 0 {
            return None;
        }

        (0..child_index)
            .rev()
            .find(|index| self.entries()[*index].depth == child.depth.saturating_sub(1))
    }
}

pub fn run(command: CommandKind) -> i32
{
    match command {
        CommandKind::RunUi { library_root } => run_ui(&library_root),
    }
}

fn run_ui(library_root: &std::path::Path) -> i32 {
    if let Err(error) = std::fs::create_dir_all(library_root) {
        eprintln!("failed to create {}: {error}", library_root.display());
        return 1;
    }

    let mut state = match AppState::new(library_root) {
        Ok(state) => state,
        Err(error) => {
            eprintln!("failed to prepare browser for {}: {error}", library_root.display());
            return 1;
        }
    };

    if let Err(error) = ui::run_ui(&mut state) {
        eprintln!("ui error: {error}");
        return 1;
    }

    0
}

fn set_tree_expanded(nodes: &mut [TreeNode], target: &std::path::Path, expanded: bool) -> bool {
    for node in nodes {
        if node.path == target {
            if node.kind == BrowserEntryKind::Directory {
                node.expanded = expanded;
            }
            return true;
        }

        if set_tree_expanded(&mut node.children, target, expanded) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_entry(name: &str, kind: BrowserEntryKind) -> BrowserEntry {
        BrowserEntry {
            name: name.into(),
            path: std::path::PathBuf::from(name),
            kind,
            depth: 0,
            expanded: false,
        }
    }

    #[test]
    fn counts_only_audio_files_for_footer_totals() {
        let state = AppState {
            selected: 1,
            track_count: 4,
            browser: Some(BrowserState {
                root: std::path::PathBuf::from("music"),
                tree: Vec::new(),
                entries: vec![
                    browser_entry("巴赫", BrowserEntryKind::Directory),
                    browser_entry("勃兰登堡协奏曲.ogg", BrowserEntryKind::AudioFile),
                    browser_entry("莫扎特", BrowserEntryKind::Directory),
                    browser_entry("小夜曲.ogg", BrowserEntryKind::AudioFile),
                ],
            }),
            now_playing: None,
        };

        assert_eq!(state.audio_file_count(), 2);
    }

    #[test]
    fn counts_selected_audio_position_without_including_directories() {
        let state = AppState {
            selected: 3,
            track_count: 4,
            browser: Some(BrowserState {
                root: std::path::PathBuf::from("music"),
                tree: Vec::new(),
                entries: vec![
                    browser_entry("巴赫", BrowserEntryKind::Directory),
                    browser_entry("勃兰登堡协奏曲.ogg", BrowserEntryKind::AudioFile),
                    browser_entry("莫扎特", BrowserEntryKind::Directory),
                    browser_entry("小夜曲.ogg", BrowserEntryKind::AudioFile),
                ],
            }),
            now_playing: None,
        };

        assert_eq!(state.selected_audio_position(), Some(2));
    }
}
