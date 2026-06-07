// SPDX-License-Identifier: GPL-3.0-or-later

//! Workspace file indexing for the command palette.
//!
//! This slice owns directory traversal, root interning, and incremental path
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

/// In-memory index of all files across workspace roots.
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    files: Vec<IndexedFile>,
    /// Deduplicated workspace roots for O(k) prefix lookups (k is usually small).
    roots: Vec<Arc<PathBuf>>,
}

impl FileIndex {
    /// Build a file index by recursively scanning all workspace root directories.
    #[must_use]
    pub fn rebuild(roots: &[PathBuf]) -> Self {
        Self::rebuild_with_hint(roots, 10_000)
    }

    /// Like [`Self::rebuild`], but uses `capacity_hint` for the initial `Vec` allocation.
    #[must_use]
    pub fn rebuild_with_hint(roots: &[PathBuf], capacity_hint: usize) -> Self {
        let mut files = Vec::with_capacity(capacity_hint.max(64));
        let mut visited = HashSet::new();
        let mut root_arcs = Vec::new();
        for root in roots {
            let Ok(canonical_root) = fs_metadata::canonical_path(root) else {
                continue;
            };
            let root_arc = Arc::new(root.clone());
            collect_files_recursive(
                root,
                &root_arc,
                &mut files,
                &mut visited,
                &canonical_root,
                0,
            );
            root_arcs.push(root_arc);
        }
        truncate_to_index_limit(&mut files, MAX_INDEXED_FILES);
        Self {
            files,
            roots: root_arcs,
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
        intern_root(&mut self.roots, &file.workspace_root);
        self.files.push(file);
    }

    /// Remove a file (or all files under a directory) from the index.
    pub fn remove_path(&mut self, path: &Path) {
        let before = self.files.len();
        self.files
            .retain(|file| file.path != path && !file.path.starts_with(path));
        if should_compact_after_removal(before, self.files.len()) {
            self.files.shrink_to_fit();
            self.roots.retain(|root| {
                self.files
                    .iter()
                    .any(|file| Arc::ptr_eq(&file.workspace_root, root))
            });
        }
    }

    /// Rename a file or directory in the index.
    pub fn rename_path(&mut self, old_path: &Path, new_path: &Path) {
        for file in &mut self.files {
            if file.path == old_path {
                let root = Arc::clone(&file.workspace_root);
                *file = IndexedFile::new(new_path.to_path_buf(), root);
            } else if let Ok(suffix) = file.path.strip_prefix(old_path) {
                file.path = new_path.join(suffix);
            }
        }
    }

    /// Find the workspace root that contains the given path.
    pub fn workspace_root_for(&self, path: &Path) -> Option<Arc<PathBuf>> {
        self.roots
            .iter()
            .find(|root| path.starts_with(root.as_path()))
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

/// Truncate oversized index results after scanning all roots.
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

/// Return whether a removal changed the index enough to justify compacting roots.
///
/// Root compaction scans the remaining files, so it is reserved for larger
/// removals; small deletions keep the root cache intact for cheap lookups.
pub(super) fn should_compact_after_removal(before: usize, after: usize) -> bool {
    after < before * 3 / 4
}

impl From<Vec<IndexedFile>> for FileIndex {
    fn from(files: Vec<IndexedFile>) -> Self {
        let mut roots = Vec::new();
        for file in &files {
            intern_root(&mut roots, &file.workspace_root);
        }
        Self { files, roots }
    }
}

fn intern_root(roots: &mut Vec<Arc<PathBuf>>, root: &Arc<PathBuf>) {
    if !roots.iter().any(|existing| Arc::ptr_eq(existing, root)) {
        roots.push(Arc::clone(root));
    }
}

fn is_ignored_index_dir(dir: &Path) -> bool {
    dir.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| IGNORED_INDEX_DIRS.contains(&name))
}

fn collect_files_recursive(
    dir: &Path,
    workspace_root: &Arc<PathBuf>,
    out: &mut Vec<IndexedFile>,
    visited: &mut HashSet<PathBuf>,
    canonical_root: &Path,
    depth: u32,
) {
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

    if !canonical.starts_with(canonical_root) {
        return;
    }

    if !visited.insert(canonical) {
        return;
    }

    for entry in file_tree::scan_directory(dir) {
        if entry.is_dir {
            if !is_ignored_index_dir(&entry.path) {
                collect_files_recursive(
                    &entry.path,
                    workspace_root,
                    out,
                    visited,
                    canonical_root,
                    depth + 1,
                );
            }
        } else {
            out.push(IndexedFile::new(entry.path, Arc::clone(workspace_root)));
        }
    }
}
