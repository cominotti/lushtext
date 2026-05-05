// SPDX-License-Identifier: GPL-3.0-or-later

//! Replace-all and undo flows for workspace content search.
//!
//! This is the command side of the content-search service. It performs file
//! locking, atomic writes, rollback on cancellation, and undo backup handling
//! without depending on any GTK types.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

use crate::model::content_search::{ReplaceResult, Replacement};
use crate::services::durable_write;

/// Apply replacements to files on disk.
///
/// Groups replacements by file, reads each file, applies replacements in reverse order
/// (to avoid offset shifting), and writes atomically (temp file + rename). Returns the
/// replacement summary and a backup `HashMap` mapping file paths to their original content
/// (for undo).
///
/// Per-file errors are collected (not early-returned) so that already-replaced files
/// remain in the backup for undo. Only returns `Err` if zero files could be processed.
///
/// `skip_paths` lists files that should NOT be replaced (e.g., open tabs with unsaved changes).
/// Skipped files are excluded from the result count but included in `ReplaceResult::skipped_paths`.
///
/// # Errors
///
/// Returns an error if every candidate file fails to process or if the replace
/// operation is cancelled and rollback cannot complete cleanly.
pub fn apply_replacements(
    replacements: &[Replacement],
    skip_paths: &std::collections::HashSet<std::path::PathBuf>,
    cancel: &AtomicBool,
) -> anyhow::Result<(
    ReplaceResult,
    std::collections::HashMap<std::path::PathBuf, Vec<u8>>,
)> {
    use std::collections::{BTreeMap, HashMap};

    let mut by_file: BTreeMap<std::path::PathBuf, Vec<&Replacement>> = BTreeMap::new();
    for r in replacements {
        by_file.entry(r.path.clone()).or_default().push(r);
    }

    let mut backup: HashMap<std::path::PathBuf, Vec<u8>> = HashMap::new();
    let mut replaced_count = 0usize;
    let mut files_affected = 0usize;
    let mut skipped_paths = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut applied_paths: Vec<std::path::PathBuf> = Vec::new();
    let mut held_locks: Vec<ReplaceFileLock> = Vec::new();
    let mut cancelled = false;

    for (path, mut file_replacements) in by_file {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        if skip_paths.contains(&path) {
            skipped_paths.push(path);
            continue;
        }

        let lock = match ReplaceFileLock::acquire(&path) {
            Ok(lock) => lock,
            Err(e) => {
                errors.push(format!("Failed to lock {}: {e}", path.display()));
                continue;
            }
        };
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }

        let original_bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) => {
                errors.push(format!("Failed to read {}: {e}", path.display()));
                continue;
            }
        };

        let original_text = match String::from_utf8(original_bytes.clone()) {
            Ok(text) => text,
            Err(e) => {
                errors.push(format!("Non-UTF8 file {}: {e}", path.display()));
                continue;
            }
        };

        backup.insert(path.clone(), original_bytes);

        // Apply replacements from the bottom of the file upward so byte ranges
        // on earlier lines stay valid even after later replacements change length.
        file_replacements.sort_by(|a, b| {
            b.line_number
                .cmp(&a.line_number)
                .then(b.match_range.start.cmp(&a.match_range.start))
        });

        let line_ending = detect_line_ending(&original_text);
        let mut lines: Vec<String> = original_text.lines().map(String::from).collect();
        let has_trailing_newline = original_text.ends_with('\n');

        // Validate against the original line snapshot before mutating anything
        // so stale search results skip the whole file instead of partially applying.
        let mut file_stale = false;
        for r in &file_replacements {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Search result line numbers stay within usize indexing limits on supported editor workloads"
            )]
            let line_idx = r.line_number.saturating_sub(1) as usize;
            if line_idx < lines.len() && lines[line_idx] != r.original_line {
                errors.push(format!(
                    "Skipped {}: line {} changed since search",
                    path.display(),
                    r.line_number,
                ));
                file_stale = true;
                break;
            }
        }
        if file_stale {
            backup.remove(&path);
            continue;
        }

        let mut file_replaced = 0usize;
        for r in &file_replacements {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Search result line numbers stay within usize indexing limits on supported editor workloads"
            )]
            let line_idx = r.line_number.saturating_sub(1) as usize;
            if line_idx < lines.len() {
                let line = &mut lines[line_idx];
                let start = line.floor_char_boundary(r.match_range.start.min(line.len()));
                let end = line.ceil_char_boundary(r.match_range.end.min(line.len()));
                if start <= end {
                    line.replace_range(start..end, &r.replacement);
                    file_replaced += 1;
                }
            }
        }

        if file_replaced == 0 {
            backup.remove(&path);
            continue;
        }

        let mut new_content = lines.join(line_ending);
        if has_trailing_newline {
            new_content.push_str(line_ending);
        }

        if let Err(e) = atomic_write(&path, new_content.as_bytes()) {
            errors.push(format!("Failed to write {}: {e}", path.display()));
            backup.remove(&path);
            continue;
        }

        applied_paths.push(path.clone());
        held_locks.push(lock);
        replaced_count += file_replaced;
        files_affected += 1;
    }

    if cancelled {
        let rollback_errors = rollback_applied_files(&backup, &applied_paths);
        if rollback_errors.is_empty() {
            return Err(anyhow::anyhow!("Replace cancelled"));
        }
        return Err(anyhow::anyhow!(
            "Replace cancelled; rollback failed: {}",
            rollback_errors.join("; ")
        ));
    }

    if files_affected == 0 && !errors.is_empty() {
        return Err(anyhow::anyhow!("{}", errors.join("; ")));
    }

    let result = ReplaceResult {
        replaced_count,
        files_affected,
        skipped_paths,
        errors,
    };

    Ok((result, backup))
}

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically (temp file + rename). Continues on per-file errors
/// so that partial undo is possible. Returns the count of files restored.
///
/// # Errors
///
/// Returns an error if every restore attempt fails.
pub fn undo_replacements(
    backup: &std::collections::HashMap<std::path::PathBuf, Vec<u8>>,
) -> anyhow::Result<usize> {
    let mut restored = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for (path, original_bytes) in backup {
        let _lock = match ReplaceFileLock::acquire(path) {
            Ok(lock) => lock,
            Err(e) => {
                errors.push(format!("Failed to lock {}: {e}", path.display()));
                continue;
            }
        };
        if let Err(e) = atomic_write(path, original_bytes) {
            errors.push(format!("Failed to restore {}: {e}", path.display()));
            continue;
        }
        restored += 1;
    }
    if restored == 0 && !errors.is_empty() {
        return Err(anyhow::anyhow!("{}", errors.join("; ")));
    }
    Ok(restored)
}

/// Detect the predominant line ending style in a string.
/// Returns `"\r\n"` if CRLF is found, otherwise `"\n"`.
fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Atomically write bytes to a file (temp file + rename, matching json_store::save pattern).
fn atomic_write(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let tmp_path = parent.join(format!(".{file_name}.replace-tmp"));
    let file = std::fs::File::create(&tmp_path)
        .map_err(|e| anyhow::anyhow!("Failed to create {}: {}", tmp_path.display(), e))?;
    let mut writer = std::io::BufWriter::new(file);
    let write_result = writer
        .write_all(content)
        .map_err(|e| anyhow::anyhow!("Failed to write {}: {}", tmp_path.display(), e))
        .and_then(|()| {
            writer
                .flush()
                .map_err(|e| anyhow::anyhow!("Failed to flush {}: {}", tmp_path.display(), e))
                .and_then(|()| {
                    writer.get_ref().sync_all().map_err(|e| {
                        anyhow::anyhow!("Failed to sync {}: {}", tmp_path.display(), e)
                    })
                })
        });
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(e);
    }
    std::fs::rename(&tmp_path, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        anyhow::anyhow!(
            "Failed to rename {} to {}: {}",
            tmp_path.display(),
            path.display(),
            e
        )
    })?;
    durable_write::sync_parent_dir(path).map_err(|e| {
        anyhow::anyhow!(
            "Failed to sync parent directory for {}: {}",
            path.display(),
            e
        )
    })
}

/// Restore already-written files in reverse order when cancellation interrupts a run.
fn rollback_applied_files(
    backup: &std::collections::HashMap<std::path::PathBuf, Vec<u8>>,
    applied_paths: &[std::path::PathBuf],
) -> Vec<String> {
    let mut errors = Vec::new();
    for path in applied_paths.iter().rev() {
        let Some(original_bytes) = backup.get(path) else {
            continue;
        };
        if let Err(e) = atomic_write(path, original_bytes) {
            errors.push(format!("Failed to restore {}: {e}", path.display()));
        }
    }
    errors
}

#[cfg(unix)]
struct ReplaceFileLock(std::fs::File);

#[cfg(unix)]
impl ReplaceFileLock {
    fn acquire(path: &Path) -> anyhow::Result<Self> {
        use std::fs::OpenOptions;

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| anyhow::anyhow!("failed to open {} for locking: {}", path.display(), e))?;
        let fd = file.as_raw_fd();
        // SAFETY: `fd` comes from a live `File` we just opened, and `flock`
        // only borrows that valid descriptor for the duration of the syscall.
        let result = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if result != 0 {
            return Err(anyhow::anyhow!(
                "failed to lock {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self(file))
    }
}

#[cfg(unix)]
impl Drop for ReplaceFileLock {
    fn drop(&mut self) {
        // SAFETY: the descriptor still belongs to `self.0`, and releasing the
        // advisory lock is valid while that file handle remains open in `Drop`.
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(not(unix))]
struct ReplaceFileLock;

#[cfg(not(unix))]
impl ReplaceFileLock {
    fn acquire(_path: &Path) -> anyhow::Result<Self> {
        Ok(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::sync::Arc;
    use tempfile::tempdir;

    /// Helper: create a Replacement struct for testing.
    fn make_replacement(
        path: &Path,
        line_number: u64,
        original_line: &str,
        replacement: &str,
        match_range: std::ops::Range<usize>,
    ) -> Replacement {
        let mut replaced_line = original_line.to_string();
        let start = match_range.start.min(replaced_line.len());
        let end = match_range.end.min(replaced_line.len());
        replaced_line.replace_range(start..end, replacement);
        Replacement {
            path: path.to_path_buf(),
            line_number,
            original_line: original_line.to_string(),
            replaced_line,
            replacement: replacement.to_string(),
            match_range,
        }
    }

    #[test]
    fn test_apply_replacements_literal() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "let hello = 1;\nlet world = 2;\n")
            .expect("expected operation to succeed");
        fs::write(&file_b, "fn hello() {}\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
            make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel)
            .expect("expected operation to succeed");

        assert_eq!(result.replaced_count, 2);
        assert_eq!(result.files_affected, 2);
        assert!(result.skipped_paths.is_empty());

        let content_a = fs::read_to_string(&file_a).expect("expected operation to succeed");
        assert!(
            content_a.contains("goodbye"),
            "a.rs should have replacement"
        );
        assert!(
            !content_a.contains("hello"),
            "a.rs should not have original"
        );

        let content_b = fs::read_to_string(&file_b).expect("expected operation to succeed");
        assert!(
            content_b.contains("goodbye"),
            "b.rs should have replacement"
        );

        assert_eq!(backup.len(), 2, "backup should contain both files");
    }

    #[test]
    fn test_apply_replacements_preserves_backup() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fs::write(&file, original).expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel)
            .expect("expected operation to succeed");

        assert_eq!(backup[&file], original.as_bytes());
    }

    #[test]
    fn test_undo_replacements_restores_content() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fs::write(&file, original).expect("expected operation to succeed");

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel)
            .expect("expected operation to succeed");

        assert!(
            fs::read_to_string(&file)
                .expect("expected operation to succeed")
                .contains("haystack")
        );

        let restored = undo_replacements(&backup).expect("expected operation to succeed");
        assert_eq!(restored, 1);
        assert_eq!(
            fs::read_to_string(&file).expect("expected operation to succeed"),
            original
        );
    }

    #[test]
    fn test_apply_replacements_reverse_order() {
        let dir = tempdir().expect("expected operation to succeed");
        let file = dir.path().join("test.rs");
        fs::write(&file, "ab cd\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file, 1, "ab cd", "XY", 0..2),
            make_replacement(&file, 1, "ab cd", "ZW", 3..5),
        ];

        let cancel = AtomicBool::new(false);
        let (result, _) = apply_replacements(&replacements, &HashSet::new(), &cancel)
            .expect("expected operation to succeed");
        assert_eq!(result.replaced_count, 2);

        let content = fs::read_to_string(&file).expect("expected operation to succeed");
        assert_eq!(
            content, "XY ZW\n",
            "both replacements should apply correctly"
        );
    }

    #[test]
    fn test_apply_replacements_cancel() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = AtomicBool::new(true);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel);
        assert!(result.is_err(), "cancelled replace should abort");
        assert_eq!(
            fs::read_to_string(&file_a).expect("expected operation to succeed"),
            "needle\n"
        );
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_apply_replacements_cancel_rolls_back_applied_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");
        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = Arc::new(AtomicBool::new(false));
        let lock_b = ReplaceFileLock::acquire(&file_b).expect("expected operation to succeed");
        let cancel_for_worker = cancel.clone();
        let worker = std::thread::spawn(move || {
            apply_replacements(&replacements, &HashSet::new(), cancel_for_worker.as_ref())
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if fs::read_to_string(&file_a).is_ok_and(|content| content.contains("replaced")) {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        drop(lock_b);

        let result = worker.join().expect("expected operation to succeed");

        assert!(result.is_err(), "cancelled replace should roll back");
        assert_eq!(
            fs::read_to_string(&file_a).expect("expected operation to succeed"),
            "needle\n"
        );
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
        );
    }

    #[test]
    fn test_apply_replacements_nonexistent_file() {
        let dir = tempdir().expect("expected operation to succeed");
        let missing_file = dir.path().join("does_not_exist.rs");

        let replacements = vec![make_replacement(
            &missing_file,
            1,
            "phantom line",
            "replacement",
            0..7,
        )];

        let cancel = AtomicBool::new(false);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel);
        assert!(result.is_err(), "should fail when only file is nonexistent");
    }

    #[test]
    fn test_apply_replacements_skip_paths() {
        let dir = tempdir().expect("expected operation to succeed");
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "needle\n").expect("expected operation to succeed");
        fs::write(&file_b, "needle\n").expect("expected operation to succeed");

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let mut skip = HashSet::new();
        skip.insert(file_b.clone());

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &skip, &cancel)
            .expect("expected operation to succeed");

        assert_eq!(result.replaced_count, 1, "only a.rs should be replaced");
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.skipped_paths.len(), 1);
        assert_eq!(result.skipped_paths[0], file_b);
        assert!(backup.contains_key(&file_a));
        assert!(!backup.contains_key(&file_b));
        assert_eq!(
            fs::read_to_string(&file_b).expect("expected operation to succeed"),
            "needle\n"
        );
    }
}
