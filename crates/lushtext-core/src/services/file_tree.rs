// SPDX-License-Identifier: GPL-3.0-or-later

//! File tree scanning: read directory contents sorted for sidebar display.
//!
//! Pure I/O service with no GTK dependencies. Returns standard Rust types
//! that the UI layer converts into GObject models.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
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
    DirectoryReconciliationPlan::Splice {
        position: prefix,
        removed,
        replacement: desired[prefix..replacement_end].to_vec(),
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
    /// True if the directory had more entries than `max_entries`.
    pub truncated: bool,
    /// True if the cancellation token was set during scanning.
    pub cancelled: bool,
    /// Human-readable scan error when the directory could not be read.
    pub error: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct SortedEntry {
    path: PathBuf,
    is_dir: bool,
    is_empty: Option<bool>,
}

impl Ord for SortedEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_entries(
            (self.path.as_path(), self.is_dir),
            (other.path.as_path(), other.is_dir),
        )
    }
}

impl PartialOrd for SortedEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
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
    mut is_cancelled: F,
) -> DirectoryScan
where
    F: FnMut() -> bool,
{
    // Keep only the best bounded rows while scanning so enormous directories do
    // not require materializing every entry before sorting.
    let mut heap = BinaryHeap::with_capacity(max_entries.saturating_add(1).min(256));
    let mut truncated = false;
    let mut dirs_checked = 0;
    let mut cancelled = false;
    let mut examined_entries = 0usize;
    let mut peak_retained_entries = 0usize;

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
                dirs_checked += 1;
                is_empty = Some(is_dir_empty(&entry.path));
            }

            heap.push(SortedEntry {
                path: entry.path,
                is_dir,
                is_empty,
            });
            if heap.len() > max_entries {
                heap.pop();
                truncated = true;
            }
            peak_retained_entries = peak_retained_entries.max(heap.len());
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
                peak_retained_entries,
                error: Some(message),
                ..DirectoryScan::default()
            };
        }
    }

    DirectoryScan {
        entries: drain_sorted_entries(heap),
        examined_entries,
        peak_retained_entries,
        truncated,
        cancelled,
        error: None,
    }
}

fn drain_sorted_entries(heap: BinaryHeap<SortedEntry>) -> Vec<DirectoryEntry> {
    heap.into_sorted_vec()
        .into_iter()
        .map(|entry| DirectoryEntry {
            path: entry.path,
            is_dir: entry.is_dir,
            is_empty: entry.is_empty,
        })
        .collect()
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
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        let b = path_b
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        UniCase::new(a).cmp(&UniCase::new(b))
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

    fn apply_plan(
        mut current: Vec<DirectoryRowState>,
        plan: DirectoryReconciliationPlan,
    ) -> Vec<DirectoryRowState> {
        if let DirectoryReconciliationPlan::Splice {
            position,
            removed,
            replacement,
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
        } = &plan
        else {
            panic!("middle change should produce a splice");
        };
        assert_eq!(*position, 2_500);
        assert_eq!(*removed, 5_000);
        assert_eq!(replacement.len(), 5_000);
        assert_eq!(apply_plan(current, plan), desired);
    }
}
