use std::path::{Path, PathBuf};

use anyhow::Context;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserEntryKind {
    Directory,
    AudioFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserEntry {
    pub name: String,
    pub path: PathBuf,
    pub kind: BrowserEntryKind,
    pub depth: usize,
    pub expanded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub kind: BrowserEntryKind,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub composer: String,
    pub title: String,
    pub path: PathBuf,
}

pub fn scan_entries(root: &Path) -> anyhow::Result<Vec<BrowserEntry>> {
    Ok(flatten_tree(&build_tree(root)?))
}

pub fn build_tree(root: &Path) -> anyhow::Result<Vec<TreeNode>> {
    scan_tree_dir(root)
}

pub fn expand_first_level(nodes: &mut [TreeNode]) {
    for node in nodes {
        if node.kind == BrowserEntryKind::Directory {
            node.expanded = true;
        }
    }
}

pub fn flatten_tree(nodes: &[TreeNode]) -> Vec<BrowserEntry> {
    let mut entries = Vec::new();
    flatten_nodes(nodes, 0, &mut entries);
    entries
}

pub fn scan_library(root: &Path) -> anyhow::Result<Vec<Track>> {
    let mut tracks = Vec::new();
    scan_dir(root, root, &mut tracks)?;
    tracks.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(tracks)
}

fn scan_dir(root: &Path, current: &Path, tracks: &mut Vec<Track>) -> anyhow::Result<()> {
    for entry in
        std::fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if is_hidden_name(&name) {
            continue;
        }

        if path.is_dir() {
            scan_dir(root, &path, tracks)?;
            continue;
        }

        if !is_supported_audio_file(&path) {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("failed to compute relative path for {}", path.display()))?;

        let composer = relative
            .components()
            .next()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .unwrap_or_else(|| "Unknown".to_string());

        let title = path
            .file_stem()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "untitled".to_string());

        tracks.push(Track {
            composer,
            title,
            path,
        });
    }

    Ok(())
}

pub fn is_supported_audio_file(path: &Path) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };

    matches!(extension, "mp3" | "flac" | "wav" | "ogg")
}

fn scan_tree_dir(current: &Path) -> anyhow::Result<Vec<TreeNode>> {
    let mut nodes = Vec::new();

    for entry in
        std::fs::read_dir(current).with_context(|| format!("failed to read {}", current.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if is_hidden_name(&name) {
            continue;
        }

        let node = if path.is_dir() {
            Some(TreeNode {
                name,
                path: path.clone(),
                kind: BrowserEntryKind::Directory,
                expanded: false,
                children: scan_tree_dir(&path)?,
            })
        } else if is_supported_audio_file(&path) {
            Some(TreeNode {
                name,
                path,
                kind: BrowserEntryKind::AudioFile,
                expanded: false,
                children: Vec::new(),
            })
        } else {
            None
        };

        if let Some(node) = node {
            nodes.push(node);
        }
    }

    nodes.sort_by(|left, right| match (&left.kind, &right.kind) {
        (BrowserEntryKind::Directory, BrowserEntryKind::AudioFile) => std::cmp::Ordering::Less,
        (BrowserEntryKind::AudioFile, BrowserEntryKind::Directory) => std::cmp::Ordering::Greater,
        _ => left.name.cmp(&right.name),
    });

    Ok(nodes)
}

fn flatten_nodes(nodes: &[TreeNode], depth: usize, output: &mut Vec<BrowserEntry>) {
    for node in nodes {
        output.push(BrowserEntry {
            name: node.name.clone(),
            path: node.path.clone(),
            kind: node.kind.clone(),
            depth,
            expanded: node.expanded,
        });

        if node.kind == BrowserEntryKind::Directory && node.expanded {
            flatten_nodes(&node.children, depth + 1, output);
        }
    }
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}
