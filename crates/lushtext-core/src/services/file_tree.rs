// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree scanning: read directory contents sorted for sidebar display.
//!
//! Pure I/O service with no GTK dependencies. Returns standard Rust types
//! that the UI layer converts into GObject models.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use unicase::UniCase;

use crate::services::filesystem::{DirectoryScanPolicy, FileKind, tree as fs_tree};

/// A single sidebar-visible filesystem entry returned by a directory scan.
///
/// This stays in the service layer as plain Rust data so callers in the UI,
/// benchmarks, and other services can share one documented shape without
/// depending on tuple field positions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryEntry {
    /// Absolute filesystem path for the file or directory.
    pub path: PathBuf,
    /// True when the path points to a directory.
    pub is_dir: bool,
    /// Empty-directory hint for sidebar affordances.
    ///
    /// `Some(true)` means the directory was checked and found empty,
    /// `Some(false)` means it was checked and contains visible entries,
    /// and `None` means emptiness was not checked (files, or directories past
    /// the scan's lookahead budget).
    pub is_empty: Option<bool>,
}

impl Ord for DirectoryEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_entries(
            (self.path.as_path(), self.is_dir),
            (other.path.as_path(), other.is_dir),
        )
    }
}

impl PartialOrd for DirectoryEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Plain projection retained for one materialized GTK child store.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DirectoryRowState {
    /// Filesystem path for a real row; absent only for the truncation placeholder.
    pub path: Option<PathBuf>,
    /// Whether a real row represents a directory.
    pub is_dir: bool,
    /// Bounded empty-directory hint copied from the accepted scan.
    pub is_empty: Option<bool>,
    /// Whether this row is the synthetic truncation placeholder.
    pub is_placeholder: bool,
}

impl DirectoryRowState {
    /// Convert one scanned filesystem entry into its retained row projection.
    #[must_use]
    pub fn from_entry(entry: DirectoryEntry) -> Self {
        Self {
            path: Some(entry.path),
            is_dir: entry.is_dir,
            is_empty: entry.is_empty,
            is_placeholder: false,
        }
    }

    /// Build the single synthetic row appended after a truncated scan.
    #[must_use]
    pub const fn truncation_placeholder() -> Self {
        Self {
            path: None,
            is_dir: false,
            is_empty: None,
            is_placeholder: true,
        }
    }
}

/// Compact changed-middle plan computed without reading GTK objects.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum DirectoryReconciliationPlan {
    /// Current and desired row mirrors are already identical.
    Unchanged,
    /// Replace one middle range while preserving equal prefix and suffix rows.
    Splice {
        /// First changed row in the current and desired mirrors.
        position: usize,
        /// Current rows removed from that position.
        removed: usize,
        /// Desired changed rows inserted at that position.
        replacement: Vec<DirectoryRowState>,
        /// Removed directory roots whose materialized descendant state must retire.
        removed_directory_roots: Vec<PathBuf>,
    },
}

/// Compute one prefix/middle/suffix plan from bounded plain row projections.
#[must_use]
pub fn plan_directory_reconciliation(
    current: &[DirectoryRowState],
    desired: &[DirectoryRowState],
) -> DirectoryReconciliationPlan {
    if current == desired {
        return DirectoryReconciliationPlan::Unchanged;
    }
    let prefix = current
        .iter()
        .zip(desired.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = current[prefix..]
        .iter()
        .rev()
        .zip(desired[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let removed = current.len().saturating_sub(prefix + suffix);
    let replacement_end = desired.len().saturating_sub(suffix);
    let desired_paths = desired
        .iter()
        .filter(|row| row.is_dir)
        .filter_map(|row| row.path.as_deref())
        .collect::<HashSet<_>>();
    let candidates = current[prefix..prefix.saturating_add(removed)]
        .iter()
        .filter(|row| row.is_dir)
        .filter_map(|row| row.path.as_ref())
        .filter(|path| !desired_paths.contains(path.as_path()))
        .cloned()
        .collect::<HashSet<_>>();
    let mut removed_directory_roots = candidates
        .iter()
        .filter(|path| {
            !path
                .ancestors()
                .skip(1)
                .any(|ancestor| candidates.contains(ancestor))
        })
        .cloned()
        .collect::<Vec<_>>();
    removed_directory_roots.sort_unstable();
    DirectoryReconciliationPlan::Splice {
        position: prefix,
        removed,
        replacement: desired[prefix..replacement_end].to_vec(),
        removed_directory_roots,
    }
}

/// Result of a bounded directory scan.
#[derive(Debug, Default)]
pub struct DirectoryScan {
    /// Sorted entries for the directory, directories-first then alphabetical.
    pub entries: Vec<DirectoryEntry>,
    /// Visible filesystem entries examined before completion or cancellation.
    pub examined_entries: usize,
    /// Largest number of directory rows retained during bounded selection.
    pub peak_retained_entries: usize,
    /// Complete retained heap weight of the returned entry batch.
    pub retained_bytes: u64,
    /// Largest conservatively charged heap weight during selection.
    pub peak_retained_bytes: u64,
    /// True if the directory had more entries than `max_entries`.
    pub truncated: bool,
    /// True when the caller-supplied scratch-byte ceiling omitted an entry.
    pub byte_truncated: bool,
    /// True if the cancellation token was set during scanning.
    pub cancelled: bool,
    /// Human-readable scan error when the directory could not be read.
    pub error: Option<String>,
}

/// Scan a directory and return sorted entries (directories first, then alphabetical).
/// Skips hidden files (starting with `.`).
#[must_use]
pub fn scan_directory(dir_path: &Path) -> Vec<DirectoryEntry> {
    scan_directory_bounded(dir_path, usize::MAX, 1000, None).entries
}

/// Peek into a directory to see if it contains any visible (non-hidden) entries.
#[must_use]
pub fn is_dir_empty(path: &Path) -> bool {
    let mut has_visible_entry = false;
    let scan = fs_tree::visit_directory(
        path,
        DirectoryScanPolicy {
            max_entries: 1,
            include_hidden: false,
        },
        |_| {
            has_visible_entry = true;
            false
        },
    );
    if scan.is_err() {
        return false; // Assume not empty on error to allow user to try expanding it
    }
    !has_visible_entry
}

/// Scan a directory while bounding memory and allowing cooperative cancellation.
///
/// The result is still sorted directories-first and alphabetically, but for
/// very large folders only the best `max_entries` rows are retained in memory.
#[must_use]
pub fn scan_directory_bounded(
    dir_path: &Path,
    max_entries: usize,
    lookahead_cap: usize,
    cancel: Option<&AtomicBool>,
) -> DirectoryScan {
    scan_directory_bounded_with_cancel(dir_path, max_entries, lookahead_cap, || {
        cancel.is_some_and(|flag| flag.load(AtomicOrdering::Acquire))
    })
}

/// Scan a directory with a workflow-owned cooperative cancellation predicate.
pub fn scan_directory_bounded_with_cancel<F>(
    dir_path: &Path,
    max_entries: usize,
    lookahead_cap: usize,
    is_cancelled: F,
) -> DirectoryScan
where
    F: FnMut() -> bool,
{
    scan_directory_without_byte_limit(dir_path, max_entries, lookahead_cap, is_cancelled)
}

/// Scan one directory under both row and complete retained-byte ceilings.
pub fn scan_directory_bounded_with_cancel_and_bytes<F>(
    dir_path: &Path,
    max_entries: usize,
    lookahead_cap: usize,
    max_retained_bytes: u64,
    mut is_cancelled: F,
) -> DirectoryScan
where
    F: FnMut() -> bool,
{
    // Count-only scans do not need the file-index byte classifier's discovery
    // pass: top-k replacement is deterministic for a fixed row limit. Keeping
    // that common sidebar path single-pass avoids doubling directory I/O.
    if max_retained_bytes == u64::MAX {
        return scan_directory_without_byte_limit(
            dir_path,
            max_entries,
            lookahead_cap,
            is_cancelled,
        );
    }

    // The backend does not promise enumeration order. First collect only scalar
    // evidence, then select a deterministic top-k on a second bounded pass.
    // Charging every retained row at the largest graph weight observed in the
    // first pass makes the byte-derived capacity independent of encounter order.
    let mut cancelled = false;
    let mut examined_entries = 0usize;
    let mut maximum_graph_bytes = 0u64;

    let scan = fs_tree::visit_directory(
        dir_path,
        DirectoryScanPolicy::visible_workspace(),
        |entry| {
            if is_cancelled() {
                cancelled = true;
                return false;
            }

            match entry.kind {
                FileKind::Directory | FileKind::File => {}
                FileKind::Other => return true,
            }
            examined_entries = examined_entries.saturating_add(1);
            maximum_graph_bytes = maximum_graph_bytes.max(directory_path_graph_bytes(&entry.path));
            true
        },
    );

    match scan {
        Ok(()) => {}
        Err(error) => {
            let message = format!("Cannot read {}: {}", dir_path.display(), error);
            tracing::warn!("{message}");
            return DirectoryScan {
                examined_entries,
                error: Some(message),
                ..DirectoryScan::default()
            };
        }
    }
    if cancelled {
        return DirectoryScan {
            examined_entries,
            cancelled: true,
            ..DirectoryScan::default()
        };
    }

    let mut selector = BoundedDirectorySelector::new(
        examined_entries,
        max_entries,
        max_retained_bytes,
        maximum_graph_bytes,
    );
    let mut dirs_checked = 0usize;
    let scan = fs_tree::visit_directory(
        dir_path,
        DirectoryScanPolicy::visible_workspace(),
        |entry| {
            if is_cancelled() {
                cancelled = true;
                return false;
            }
            let is_dir = match entry.kind {
                FileKind::Directory => true,
                FileKind::File => false,
                FileKind::Other => return true,
            };
            let mut is_empty = None;
            if is_dir && dirs_checked < lookahead_cap {
                dirs_checked = dirs_checked.saturating_add(1);
                is_empty = Some(is_dir_empty(&entry.path));
            }
            selector.consider(DirectoryEntry {
                path: entry.path,
                is_dir,
                is_empty,
            });
            true
        },
    );
    if let Err(error) = scan {
        let message = format!("Cannot read {}: {}", dir_path.display(), error);
        tracing::warn!("{message}");
        return DirectoryScan {
            examined_entries,
            peak_retained_entries: selector.peak_retained_entries,
            peak_retained_bytes: selector.peak_retained_bytes,
            error: Some(message),
            ..DirectoryScan::default()
        };
    }

    let selection = selector.finish();
    DirectoryScan {
        entries: selection.entries,
        examined_entries,
        peak_retained_entries: selection.peak_retained_entries,
        retained_bytes: selection.retained_bytes,
        peak_retained_bytes: selection.peak_retained_bytes,
        truncated: selection.truncated,
        byte_truncated: selection.byte_truncated,
        cancelled,
        error: None,
    }
}

fn scan_directory_without_byte_limit<F>(
    dir_path: &Path,
    max_entries: usize,
    lookahead_cap: usize,
    mut is_cancelled: F,
) -> DirectoryScan
where
    F: FnMut() -> bool,
{
    let mut heap = BinaryHeap::with_capacity(max_entries.min(256));
    let mut retained_graph_bytes = 0u64;
    let mut retained_shell_bytes = retained_heap_shell_bytes(&heap);
    let mut truncated = false;
    let mut dirs_checked = 0usize;
    let mut cancelled = false;
    let mut examined_entries = 0usize;
    let mut peak_retained_entries = 0usize;
    let mut peak_retained_bytes = retained_shell_bytes;

    let scan = fs_tree::visit_directory(
        dir_path,
        DirectoryScanPolicy::visible_workspace(),
        |entry| {
            if is_cancelled() {
                cancelled = true;
                return false;
            }

            let is_dir = match entry.kind {
                FileKind::Directory => true,
                FileKind::File => false,
                FileKind::Other => return true,
            };
            examined_entries = examined_entries.saturating_add(1);

            let mut is_empty = None;
            if is_dir && dirs_checked < lookahead_cap {
                dirs_checked = dirs_checked.saturating_add(1);
                is_empty = Some(is_dir_empty(&entry.path));
            }
            let candidate = DirectoryEntry {
                path: entry.path,
                is_dir,
                is_empty,
            };

            if heap.len() == max_entries {
                truncated = true;
                if heap.peek().is_none_or(|worst| candidate >= *worst) {
                    return true;
                }
                let removed = heap.pop().expect("nonzero full scan heap");
                retained_graph_bytes =
                    retained_graph_bytes.saturating_sub(directory_entry_graph_bytes(&removed));
            }

            retained_graph_bytes =
                retained_graph_bytes.saturating_add(directory_entry_graph_bytes(&candidate));
            heap.push(candidate);
            retained_shell_bytes = retained_heap_shell_bytes(&heap);
            peak_retained_entries = peak_retained_entries.max(heap.len());
            peak_retained_bytes =
                peak_retained_bytes.max(retained_shell_bytes.saturating_add(retained_graph_bytes));
            true
        },
    );

    if let Err(error) = scan {
        let message = format!("Cannot read {}: {}", dir_path.display(), error);
        tracing::warn!("{message}");
        return DirectoryScan {
            examined_entries,
            peak_retained_entries,
            peak_retained_bytes,
            error: Some(message),
            ..DirectoryScan::default()
        };
    }

    let retained_bytes = retained_shell_bytes.saturating_add(retained_graph_bytes);
    DirectoryScan {
        entries: drain_sorted_entries(heap),
        examined_entries,
        peak_retained_entries,
        retained_bytes,
        peak_retained_bytes: peak_retained_bytes.max(retained_bytes),
        truncated,
        byte_truncated: false,
        cancelled,
        error: None,
    }
}

struct BoundedDirectorySelector {
    heap: BinaryHeap<DirectoryEntry>,
    retained_shell_bytes: u64,
    retained_graph_bytes: u64,
    maximum_graph_bytes: u64,
    truncated: bool,
    byte_truncated: bool,
    peak_retained_entries: usize,
    peak_retained_bytes: u64,
}

struct BoundedDirectorySelection {
    entries: Vec<DirectoryEntry>,
    retained_bytes: u64,
    peak_retained_entries: usize,
    peak_retained_bytes: u64,
    truncated: bool,
    byte_truncated: bool,
}

impl BoundedDirectorySelector {
    fn new(
        eligible_entries: usize,
        max_entries: usize,
        max_retained_bytes: u64,
        maximum_graph_bytes: u64,
    ) -> Self {
        let row_charge = retained_u64(std::mem::size_of::<DirectoryEntry>())
            .saturating_add(maximum_graph_bytes)
            .max(1);
        let byte_capacity = usize::try_from(max_retained_bytes / row_charge).unwrap_or(usize::MAX);
        let desired_capacity = eligible_entries.min(max_entries);
        let capacity = desired_capacity.min(byte_capacity);
        let heap = BinaryHeap::from(Vec::with_capacity(capacity));
        let retained_shell_bytes = retained_heap_shell_bytes(&heap);
        Self {
            heap,
            retained_shell_bytes,
            retained_graph_bytes: 0,
            maximum_graph_bytes,
            truncated: eligible_entries > max_entries,
            byte_truncated: capacity < desired_capacity,
            peak_retained_entries: 0,
            peak_retained_bytes: retained_shell_bytes,
        }
    }

    fn consider(&mut self, entry: DirectoryEntry) {
        let graph_bytes = directory_entry_graph_bytes(&entry);
        if graph_bytes > self.maximum_graph_bytes {
            self.byte_truncated = true;
            return;
        }
        if self.heap.capacity() == 0 {
            self.byte_truncated = true;
            return;
        }
        if self.heap.len() == self.heap.capacity() {
            if self.heap.peek().is_some_and(|worst| entry < *worst) {
                let removed = self.heap.pop().expect("full selector has a worst row");
                self.retained_graph_bytes = self
                    .retained_graph_bytes
                    .saturating_sub(directory_entry_graph_bytes(&removed));
            } else {
                self.truncated = true;
                return;
            }
            self.truncated = true;
        }
        self.retained_graph_bytes = self.retained_graph_bytes.saturating_add(graph_bytes);
        self.heap.push(entry);
        self.peak_retained_entries = self.peak_retained_entries.max(self.heap.len());
        self.peak_retained_bytes = self.peak_retained_bytes.max(
            self.retained_shell_bytes
                .saturating_add(self.retained_graph_bytes),
        );
    }

    fn finish(self) -> BoundedDirectorySelection {
        let entries = drain_sorted_entries(self.heap);
        let retained_bytes = self
            .retained_shell_bytes
            .saturating_add(self.retained_graph_bytes);
        BoundedDirectorySelection {
            entries,
            retained_bytes,
            peak_retained_entries: self.peak_retained_entries,
            peak_retained_bytes: self.peak_retained_bytes.max(retained_bytes),
            truncated: self.truncated,
            byte_truncated: self.byte_truncated,
        }
    }
}

fn retained_u64(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

fn retained_heap_shell_bytes(heap: &BinaryHeap<DirectoryEntry>) -> u64 {
    retained_u64(
        heap.capacity()
            .saturating_mul(std::mem::size_of::<DirectoryEntry>()),
    )
}

fn directory_entry_graph_bytes(entry: &DirectoryEntry) -> u64 {
    directory_path_graph_bytes(&entry.path)
}

fn directory_path_graph_bytes(path: &PathBuf) -> u64 {
    retained_u64(
        path.capacity()
            .saturating_add(std::mem::size_of::<usize>().saturating_mul(2)),
    )
}

fn drain_sorted_entries(heap: BinaryHeap<DirectoryEntry>) -> Vec<DirectoryEntry> {
    heap.into_sorted_vec()
}

/// Sort order: directories before files, then case-insensitive alphabetical.
/// Uses `UniCase` for Unicode-aware comparison without allocation.
fn compare_entries(
    (path_a, is_dir_a): (&Path, bool),
    (path_b, is_dir_b): (&Path, bool),
) -> Ordering {
    is_dir_b.cmp(&is_dir_a).then_with(|| {
        let a = path_a
            .file_name()
            .map(OsStr::to_string_lossy)
            .unwrap_or_default();
        let b = path_b
            .file_name()
            .map(OsStr::to_string_lossy)
            .unwrap_or_default();
        UniCase::new(a)
            .cmp(&UniCase::new(b))
            .then_with(|| path_a.cmp(path_b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use tempfile::TempDir;

    /// Helper: extract file names from scan results.
    fn names(entries: &[DirectoryEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .expect("expected operation to succeed")
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    fn row(name: impl Into<String>) -> DirectoryRowState {
        DirectoryRowState {
            path: Some(PathBuf::from(name.into())),
            is_dir: false,
            is_empty: None,
            is_placeholder: false,
        }
    }

    fn directory_row(name: impl Into<String>) -> DirectoryRowState {
        DirectoryRowState {
            is_dir: true,
            ..row(name)
        }
    }

    fn apply_plan(
        mut current: Vec<DirectoryRowState>,
        plan: DirectoryReconciliationPlan,
    ) -> Vec<DirectoryRowState> {
        if let DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
            ..
        } = plan
        {
            current.splice(position..position + removed, replacement);
        }
        current
    }

    #[test]
    fn test_empty_directory() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_hidden_files_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(".hidden"), "");
        fixture::write_text(&dir.path().join(".gitignore"), "");
        fixture::write_text(&dir.path().join("visible.txt"), "");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["visible.txt"]);
    }

    #[test]
    fn test_hidden_directories_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join(".git"));
        fixture::create_dir(&dir.path().join("src"));

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(names(&entries), vec!["src"]);
    }

    #[test]
    fn test_files_marked_not_dir() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("file.txt"), "hello");

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].is_dir);
    }

    #[test]
    fn test_directories_marked_as_dir() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join("subdir"));

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir);
    }

    #[test]
    fn test_directories_sorted_before_files() {
        let dir = TempDir::new().expect("expected operation to succeed");
        // File sorts alphabetically before directory, but dirs should come first
        fixture::write_text(&dir.path().join("aaa.txt"), "");
        fixture::create_dir(&dir.path().join("zzz_dir"));

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        assert!(entries[0].is_dir, "first entry should be directory");
        assert!(!entries[1].is_dir, "second entry should be file");
    }

    #[test]
    fn test_alphabetical_case_insensitive_sort() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("Banana.txt"), "");
        fixture::write_text(&dir.path().join("apple.txt"), "");
        fixture::write_text(&dir.path().join("Cherry.txt"), "");

        let entries = scan_directory(dir.path());
        assert_eq!(
            names(&entries),
            vec!["apple.txt", "Banana.txt", "Cherry.txt"]
        );
    }

    #[test]
    fn test_nonexistent_directory_returns_empty() {
        let entries = scan_directory(Path::new("/nonexistent/path/that/does/not/exist"));
        assert!(entries.is_empty());
    }

    #[test]
    fn test_mixed_entries_sorted_correctly() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("readme.md"), "");
        fixture::create_dir(&dir.path().join("src"));
        fixture::write_text(&dir.path().join("Cargo.toml"), "");
        fixture::create_dir(&dir.path().join("docs"));

        let entries = scan_directory(dir.path());
        assert_eq!(
            names(&entries),
            vec!["docs", "src", "Cargo.toml", "readme.md"]
        );
    }

    #[test]
    fn test_all_hidden_entries_produces_empty() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join(".gitignore"), "");
        fixture::create_dir(&dir.path().join(".git"));
        fixture::write_text(&dir.path().join(".env"), "");

        let entries = scan_directory(dir.path());
        assert!(entries.is_empty());
    }

    #[test]
    fn test_is_dir_empty_ignores_hidden_entries_and_detects_visible_entries() {
        let dir = TempDir::new().expect("expected operation to succeed");

        assert!(is_dir_empty(dir.path()));
        fixture::write_text(&dir.path().join(".hidden"), "");
        assert!(is_dir_empty(dir.path()), "hidden files should not count");
        fixture::write_text(&dir.path().join("visible.txt"), "");
        assert!(!is_dir_empty(dir.path()), "visible files should count");
        assert!(!is_dir_empty(&dir.path().join("missing")));
    }

    #[test]
    fn test_paths_are_absolute() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("file.txt"), "");

        let entries = scan_directory(dir.path());
        assert!(entries[0].path.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn test_broken_symlinks_skipped() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("real.txt"), "");
        fixture::symlink(Path::new("/nonexistent/target"), &dir.path().join("broken"));

        let entries = scan_directory(dir.path());
        let result_names = names(&entries);
        assert_eq!(result_names, vec!["real.txt"]);
    }

    #[cfg(unix)]
    #[test]
    fn test_valid_symlinks_included() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let real = dir.path().join("real.txt");
        fixture::write_text(&real, "content");
        fixture::symlink(&real, &dir.path().join("link.txt"));

        let entries = scan_directory(dir.path());
        assert_eq!(entries.len(), 2);
        let result_names = names(&entries);
        assert!(result_names.contains(&"real.txt".to_string()));
        assert!(result_names.contains(&"link.txt".to_string()));
    }

    #[test]
    fn test_bounded_scan_keeps_sorted_top_entries_and_marks_truncated() {
        let dir = TempDir::new().expect("expected operation to succeed");
        for name in ["zeta.txt", "alpha.txt", "docs", "src", "notes.md"] {
            let path = dir.path().join(name);
            if name.contains('.') {
                fixture::write_text(&path, "");
            } else {
                fixture::create_dir(&path);
            }
        }

        let scan = scan_directory_bounded(dir.path(), 3, 1000, None);
        assert!(scan.truncated);
        assert!(!scan.cancelled);
        assert_eq!(names(&scan.entries), vec!["docs", "src", "alpha.txt"]);
    }

    #[test]
    fn test_bounded_scan_checks_only_directory_lookahead_cap() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::create_dir(&dir.path().join("alpha"));
        fixture::create_dir(&dir.path().join("beta"));
        fixture::write_text(&dir.path().join("file.txt"), "");

        let scan = scan_directory_bounded(dir.path(), 10, 1, None);
        let checked_dirs = scan
            .entries
            .iter()
            .filter(|entry| entry.is_dir && entry.is_empty.is_some())
            .count();

        assert_eq!(checked_dirs, 1);
        assert!(
            scan.entries
                .iter()
                .filter(|entry| !entry.is_dir)
                .all(|entry| entry.is_empty.is_none()),
            "file entries should never consume directory lookahead"
        );
    }

    #[test]
    fn test_bounded_scan_honors_cancel_token() {
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("visible.txt"), "");
        let cancel = AtomicBool::new(true);

        let scan = scan_directory_bounded(dir.path(), 10, 1000, Some(&cancel));
        assert!(scan.cancelled);
        assert!(scan.entries.is_empty());
    }

    #[test]
    fn test_bounded_scan_reports_read_errors() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing = dir.path().join("missing");

        let scan = scan_directory_bounded(&missing, 10, 1000, None);

        assert!(scan.entries.is_empty());
        assert!(!scan.cancelled);
        assert!(
            scan.error
                .as_deref()
                .is_some_and(|message| message.contains(missing.to_string_lossy().as_ref())),
            "scan error should identify the unreadable folder"
        );
    }

    #[test]
    fn byte_bounded_scan_accepts_its_exact_peak_and_rejects_one_byte_less() {
        let dir = TempDir::new().expect("byte-bounded directory scan tempdir");
        fixture::write_text(&dir.path().join("unicode-界-🙂.rs"), "");

        let baseline =
            scan_directory_bounded_with_cancel_and_bytes(dir.path(), 1, 0, u64::MAX, || false);
        assert_eq!(baseline.entries.len(), 1);
        let exact_limit = baseline.peak_retained_bytes;

        let exact =
            scan_directory_bounded_with_cancel_and_bytes(dir.path(), 1, 0, exact_limit, || false);
        assert_eq!(names(&exact.entries), vec!["unicode-界-🙂.rs"]);
        assert!(!exact.byte_truncated);
        assert!(exact.peak_retained_bytes <= exact_limit);
        assert!(exact.retained_bytes <= exact_limit);

        let one_under = scan_directory_bounded_with_cancel_and_bytes(
            dir.path(),
            1,
            0,
            exact_limit.saturating_sub(1),
            || false,
        );
        assert!(one_under.byte_truncated);
        assert!(one_under.entries.is_empty());
        assert!(one_under.peak_retained_bytes < exact_limit);
    }

    #[test]
    fn byte_bounded_selector_is_identical_for_reversed_encounter_order() {
        let entries = ["d.txt", "b.txt", "a.txt", "c.txt"]
            .into_iter()
            .map(|name| DirectoryEntry {
                path: PathBuf::from(name),
                is_dir: false,
                is_empty: None,
            })
            .collect::<Vec<_>>();
        let maximum_graph = entries
            .iter()
            .map(directory_entry_graph_bytes)
            .max()
            .expect("fixture entries");
        let two_row_limit =
            retained_u64(2usize.saturating_mul(std::mem::size_of::<DirectoryEntry>()))
                .saturating_add(maximum_graph.saturating_mul(2));
        let select = |input: Vec<DirectoryEntry>, limit| {
            let mut selector =
                BoundedDirectorySelector::new(input.len(), input.len(), limit, maximum_graph);
            for entry in input {
                selector.consider(entry);
            }
            selector.finish()
        };

        let exact_forward = select(entries.clone(), two_row_limit);
        let exact_reverse = select(entries.iter().rev().cloned().collect(), two_row_limit);
        assert_eq!(names(&exact_forward.entries), vec!["a.txt", "b.txt"]);
        assert_eq!(exact_forward.entries, exact_reverse.entries);
        assert_eq!(exact_forward.byte_truncated, exact_reverse.byte_truncated);

        let one_under_forward = select(entries.clone(), two_row_limit - 1);
        let one_under_reverse = select(entries.iter().rev().cloned().collect(), two_row_limit - 1);
        assert_eq!(names(&one_under_forward.entries), vec!["a.txt"]);
        assert_eq!(one_under_forward.entries, one_under_reverse.entries);
        assert!(one_under_forward.byte_truncated);
        assert_eq!(
            one_under_forward.byte_truncated,
            one_under_reverse.byte_truncated
        );
    }

    #[test]
    fn reconciliation_plan_compacts_a_ten_thousand_row_prefix_change() {
        let current = (0..10_000)
            .map(|index| row(format!("row-{index:05}")))
            .collect::<Vec<_>>();
        let mut desired = Vec::with_capacity(current.len() + 1);
        desired.push(row("prefix-new"));
        desired.extend(current.iter().cloned());

        let plan = plan_directory_reconciliation(&current, &desired);

        assert_eq!(
            plan,
            DirectoryReconciliationPlan::Splice {
                position: 0,
                removed: 0,
                replacement: vec![row("prefix-new")],
                removed_directory_roots: Vec::new(),
            }
        );
        assert_eq!(apply_plan(current, plan), desired);
    }

    #[test]
    fn reconciliation_plan_compacts_a_ten_thousand_row_middle_change() {
        let current = (0..10_000)
            .map(|index| row(format!("row-{index:05}")))
            .collect::<Vec<_>>();
        let mut desired = current.clone();
        desired.splice(
            2_500..7_500,
            (0..5_000).map(|index| row(format!("changed-{index:05}"))),
        );

        let plan = plan_directory_reconciliation(&current, &desired);

        let DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
            removed_directory_roots,
        } = &plan
        else {
            panic!("middle change should produce a splice");
        };
        assert_eq!(*position, 2_500);
        assert_eq!(*removed, 5_000);
        assert_eq!(replacement.len(), 5_000);
        assert!(removed_directory_roots.is_empty());
        assert_eq!(apply_plan(current, plan), desired);
    }

    #[test]
    fn reconciliation_plan_derives_ten_thousand_removed_roots_on_worker_data() {
        let current = (0..10_000)
            .map(|index| directory_row(format!("old/dir-{index:05}")))
            .collect::<Vec<_>>();
        let desired = (0..10_000)
            .map(|index| directory_row(format!("new/dir-{index:05}")))
            .collect::<Vec<_>>();

        let plan = plan_directory_reconciliation(&current, &desired);
        let DirectoryReconciliationPlan::Splice {
            removed,
            replacement,
            removed_directory_roots,
            ..
        } = &plan
        else {
            panic!("full directory churn should produce one splice");
        };

        assert_eq!(*removed, 10_000);
        assert_eq!(replacement.len(), 10_000);
        assert_eq!(removed_directory_roots.len(), 10_000);
        assert_eq!(apply_plan(current, plan), desired);
    }

    #[test]
    fn reconciliation_retires_directory_state_when_same_path_becomes_a_file() {
        let current = vec![directory_row("shared")];
        let desired = vec![row("shared")];

        let DirectoryReconciliationPlan::Splice {
            removed_directory_roots,
            ..
        } = plan_directory_reconciliation(&current, &desired)
        else {
            panic!("kind replacement should produce a splice");
        };

        assert_eq!(removed_directory_roots, vec![PathBuf::from("shared")]);
    }

    // --- Triage of the inherited byte-bounded early-return survivors (slot 5b) ---
    //
    // `services/file_tree.rs` carries 12 struct-field-deletion mutants in the
    // early-return `DirectoryScan` literals of its two scan functions. Slots 4 and
    // 5a handed 11 of them on as surviving baseline; slot 5b owns the triage.
    //
    // Reachability, established from the backend rather than assumed: the only
    // deterministically reachable error in `sys::visit_directory_entries` is the
    // initial `openat`, which fails *before any entry is visited*, and a per-entry
    // `statat` failure `continue`s rather than erroring. So on every reachable error
    // path `examined_entries`, `peak_retained_entries`, and `peak_retained_bytes` are
    // all **already zero**, which is exactly what deleting them yields — those
    // mutants are equivalent without a fault-injection seam. The three tests below
    // cover the mutants that *are* distinguishable: the byte-bounded path's `error`
    // flag, and its cancellation path's `cancelled` flag and entry count.

    #[test]
    fn byte_bounded_scan_reports_read_errors_on_its_own_path() {
        // The pre-existing error test uses `scan_directory_bounded`, which routes to
        // the no-byte-limit variant, so the byte-bounded function's own error
        // literal was never asserted.
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing = dir.path().join("missing");

        let scan = scan_directory_bounded_with_cancel_and_bytes(&missing, 10, 1000, 4096, || false);

        assert!(
            scan.error
                .as_deref()
                .is_some_and(|message| message.contains(missing.to_string_lossy().as_ref())),
            "the byte-bounded scan must report which folder it could not read"
        );
        assert!(scan.entries.is_empty());
        assert!(!scan.cancelled, "a read error is not a cancellation");
        // Zero because `openat` fails before any entry is visited. Asserted so the
        // reachability argument above is checked rather than merely written down.
        assert_eq!(scan.examined_entries, 0);
    }

    #[test]
    fn byte_bounded_scan_reports_cancellation_with_the_entries_it_had_examined() {
        // Cancelling *mid-walk* rather than pre-cancelling is what makes
        // `examined_entries` observable on this path: the cancel check runs before
        // the counter increments, so allowing two entries through and refusing the
        // third leaves exactly two examined.
        let dir = TempDir::new().expect("expected operation to succeed");
        for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
            fixture::write_text(&dir.path().join(name), "");
        }

        let mut checks = 0usize;
        let scan =
            scan_directory_bounded_with_cancel_and_bytes(dir.path(), 10, 1000, 64 * 1024, || {
                let seen = checks;
                checks += 1;
                seen >= 2
            });

        assert!(scan.cancelled, "the scan must report that it was cancelled");
        assert_eq!(
            scan.examined_entries, 2,
            "a mid-walk cancellation reports the entries examined before it"
        );
        assert!(
            scan.entries.is_empty(),
            "a cancelled byte-bounded scan publishes no entries"
        );
        assert!(scan.error.is_none(), "a cancellation is not a read error");
    }

    #[test]
    fn a_pre_cancelled_byte_bounded_scan_examines_nothing() {
        // The boundary case of the test above, and the shape the pre-existing
        // no-byte-limit cancel test already covers: cancelling before the first
        // entry leaves the counter at zero.
        let dir = TempDir::new().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("visible.txt"), "");

        let scan =
            scan_directory_bounded_with_cancel_and_bytes(dir.path(), 10, 1000, 4096, || true);

        assert!(scan.cancelled);
        assert_eq!(scan.examined_entries, 0);
        assert!(scan.entries.is_empty());
    }
}
