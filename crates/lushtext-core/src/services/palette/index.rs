// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace file indexing for the command palette.
//!
//! This slice owns directory traversal, folder interning, and incremental path
//! updates. It remains GTK-free and returns only domain types.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::model::palette::{IndexedFile, ScoredResult, SearchResultItem};
use crate::services::file_tree;
use crate::services::filesystem::metadata as fs_metadata;

use super::fuzzy::search_items;

/// Maximum recursion depth to prevent runaway scanning in deeply nested trees.
const MAX_SCAN_DEPTH: u32 = 64;
/// Maximum number of files to index. Beyond this, linear scan per query
/// starts to exceed the palette's latency budget on one CPU core.
pub(super) const MAX_INDEXED_FILES: usize = 100_000;
/// Directory names to skip during file-index scanning.
pub(super) const IGNORED_INDEX_DIRS: &[&str] =
    &["node_modules", "target", "__pycache__", "venv", "vendor"];

/// In-memory index of all files across workspace folders.
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    files: Vec<IndexedFile>,
    /// Deduplicated workspace folders for O(k) prefix lookups (k is usually small).
    workspace_folders: Vec<Arc<PathBuf>>,
}

impl FileIndex {
    /// Build a file index by recursively scanning all workspace folders.
    #[must_use]
    pub fn rebuild(workspace_folders: &[PathBuf]) -> Self {
        Self::rebuild_with_hint(workspace_folders, 10_000)
    }

    /// Like [`Self::rebuild`], but uses `capacity_hint` for the initial `Vec` allocation.
    #[must_use]
    pub fn rebuild_with_hint(workspace_folders: &[PathBuf], capacity_hint: usize) -> Self {
        let mut files = Vec::with_capacity(capacity_hint.clamp(64, MAX_INDEXED_FILES));
        let mut visited = HashSet::new();
        let mut folder_arcs = Vec::new();
        for folder in workspace_folders {
            if files.len() >= MAX_INDEXED_FILES {
                break;
            }
            let Ok(canonical_folder) = fs_metadata::canonical_path(folder) else {
                continue;
            };
            let folder_arc = Arc::new(folder.clone());
            collect_files_recursive(
                folder,
                &folder_arc,
                &mut files,
                &mut visited,
                &canonical_folder,
                0,
            );
            folder_arcs.push(folder_arc);
        }
        truncate_to_index_limit(&mut files, MAX_INDEXED_FILES);
        Self {
            files,
            workspace_folders: folder_arcs,
        }
    }

    #[must_use]
    pub fn files(&self) -> &[IndexedFile] {
        &self.files
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Add a single file to the index. Used for incremental sidebar updates.
    pub fn add_file(&mut self, file: IndexedFile) {
        if self.files.len() >= MAX_INDEXED_FILES {
            return;
        }
        intern_folder(&mut self.workspace_folders, &file.workspace_folder);
        self.files.push(file);
    }

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let before = self.files.len();
        self.files
            .retain(|file| file.path != path && !file.path.starts_with(path));
        if should_compact_after_removal(before, self.files.len()) {
            self.files.shrink_to_fit();
            self.workspace_folders.retain(|folder| {
                self.files
                    .iter()
                    .any(|file| Arc::ptr_eq(&file.workspace_folder, folder))
            });
        }
    }

    /// Rename a file or directory in the index.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) {
        for file in &mut self.files {
            if file.path == old_path {
                let folder = Arc::clone(&file.workspace_folder);
                *file = IndexedFile::new(new_path.to_path_buf(), folder);
            } else if let Ok(suffix) = file.path.strip_prefix(old_path) {
                file.path = new_path.join(suffix);
            }
        }
    }

    /// Find the workspace folder that contains the given path.
    pub fn workspace_folder_for(&self, path: &Path) -> Option<Arc<PathBuf>> {
        self.workspace_folders
            .iter()
            .find(|folder| path.starts_with(folder.as_path()))
            .map(Arc::clone)
    }

    /// Search the file index with a fuzzy query, returning up to `max` scored results.
    pub fn search(&self, query: &str, max: usize) -> Vec<ScoredResult<'_>> {
        search_items(
            self.files.iter(),
            |file| &file.name,
            SearchResultItem::File,
            query,
            max,
        )
    }
}

/// Truncate oversized index results after scanning all folders.
///
/// The limit is parameterized so unit tests can exercise the threshold without
/// constructing a six-figure temporary directory tree.
pub(super) fn truncate_to_index_limit(files: &mut Vec<IndexedFile>, limit: usize) {
    if files.len() > limit {
        tracing::warn!(
            "File index truncated: {} files exceeds {} limit",
            files.len(),
            limit
        );
        files.truncate(limit);
    }
}

/// Return whether a removal changed the index enough to justify compacting folders.
///
/// Folder compaction scans the remaining files, so it is reserved for larger
/// removals; small deletions keep the folder cache intact for cheap lookups.
pub(super) fn should_compact_after_removal(before: usize, after: usize) -> bool {
    after < before * 3 / 4
}

impl From<Vec<IndexedFile>> for FileIndex {
    fn from(files: Vec<IndexedFile>) -> Self {
        let mut workspace_folders = Vec::new();
        for file in &files {
            intern_folder(&mut workspace_folders, &file.workspace_folder);
        }
        Self {
            files,
            workspace_folders,
        }
    }
}

fn intern_folder(workspace_folders: &mut Vec<Arc<PathBuf>>, folder: &Arc<PathBuf>) {
    if !workspace_folders
        .iter()
        .any(|existing| Arc::ptr_eq(existing, folder))
    {
        workspace_folders.push(Arc::clone(folder));
    }
}

fn is_ignored_index_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_INDEX_DIRS.contains(&name))
}

fn collect_files_recursive(
    dir: &Path,
    workspace_folder: &Arc<PathBuf>,
    out: &mut Vec<IndexedFile>,
    visited: &mut HashSet<PathBuf>,
    canonical_folder: &Path,
    depth: u32,
) {
    if out.len() >= MAX_INDEXED_FILES {
        return;
    }

    if depth > MAX_SCAN_DEPTH {
        tracing::warn!(
            "Skipping deeply nested directory (depth > {MAX_SCAN_DEPTH}): {}",
            dir.display()
        );
        return;
    }

    let Ok(canonical) = fs_metadata::canonical_path(dir) else {
        return;
    };

    if !canonical.starts_with(canonical_folder) {
        return;
    }

    if !visited.insert(canonical) {
        return;
    }

    for entry in file_tree::scan_directory(dir) {
        if out.len() >= MAX_INDEXED_FILES {
            return;
        }
        if entry.is_dir {
            if !is_ignored_index_dir(&entry.path) {
                collect_files_recursive(
                    &entry.path,
                    workspace_folder,
                    out,
                    visited,
                    canonical_folder,
                    depth + 1,
                );
            }
        } else {
            out.push(IndexedFile::new(entry.path, Arc::clone(workspace_folder)));
        }
    }
}
