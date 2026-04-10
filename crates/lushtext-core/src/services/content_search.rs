// SPDX-License-Identifier: GPL-3.0-or-later

//! Content search service — workspace-wide file content search with streaming results.
//!
//! Pure Rust with no GTK dependencies. Uses the ripgrep engine (`grep-*` crates)
//! for fast, parallel, gitignore-aware searching and `crossbeam-channel` for
//! streaming results to the caller.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crossbeam_channel::Sender;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

use crate::model::content_search::{
    ContentSearchOptions, ReplaceResult, Replacement, SearchEvent, SearchMatch,
};

/// Maximum number of matches before the search stops. Approximate under
/// parallel walkers — concurrent threads may overshoot by up to the thread
/// count before observing the cancel flag. The UI should clamp display to
/// this value.
const RESULT_CAP: usize = 10_000;

/// Searches file contents across workspace roots with streaming results.
///
/// Blocks until search completes or is cancelled. Call from a dedicated thread.
/// Results are sent through `tx` as `SearchEvent` variants.
///
/// The `tx` channel should be `bounded(1024)` in production to apply backpressure.
/// Using `unbounded()` is acceptable in tests.
pub fn search(
    query: &str,
    roots: &[&Path],
    options: &ContentSearchOptions,
    tx: Sender<SearchEvent>,
    cancel: Arc<AtomicBool>,
    progress_counter: Option<Arc<AtomicUsize>>,
    completion_flag: Option<Arc<AtomicBool>>,
) {
    // Empty query → Done immediately, no file traversal.
    if query.is_empty() {
        if let Some(flag) = &completion_flag {
            flag.store(true, Ordering::Relaxed);
        }
        let _ = tx.send(SearchEvent::Done);
        return;
    }

    if roots.is_empty() {
        if let Some(flag) = &completion_flag {
            flag.store(true, Ordering::Relaxed);
        }
        let _ = tx.send(SearchEvent::Done);
        return;
    }

    // Build the regex matcher.
    let matcher = {
        let mut builder = RegexMatcherBuilder::new();
        builder
            .case_insensitive(!options.case_sensitive)
            .word(options.whole_word);

        if !options.regex {
            builder.fixed_strings(true);
        }
        let result = builder.build(query);

        match result {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(SearchEvent::Error(e.to_string()));
                if let Some(flag) = &completion_flag {
                    flag.store(true, Ordering::Relaxed);
                }
                let _ = tx.send(SearchEvent::Done);
                return;
            }
        }
    };

    // Build the directory walker.
    let walker = {
        let mut builder = WalkBuilder::new(roots[0]);

        for root in &roots[1..] {
            builder.add(root);
        }

        let threads = std::thread::available_parallelism()
            .map(|n| n.get().min(8))
            .unwrap_or(4);
        builder.threads(threads);
        builder.hidden(true); // skip hidden files (LushText convention)

        if !options.gitignore {
            builder
                .git_ignore(false)
                .git_global(false)
                .git_exclude(false);
        }

        if let Some(ref glob) = options.glob {
            // OverrideBuilder uses "include" semantics: only matching files are visited.
            match OverrideBuilder::new(roots[0])
                .add(glob)
                .and_then(|b| b.build())
            {
                Ok(overrides) => {
                    builder.overrides(overrides);
                }
                Err(e) => {
                    let _ = tx.send(SearchEvent::Error(format!("Invalid glob: {e}")));
                    if let Some(flag) = &completion_flag {
                        flag.store(true, Ordering::Relaxed);
                    }
                    let _ = tx.send(SearchEvent::Done);
                    return;
                }
            }
        }

        builder.build_parallel()
    };

    // Shared match counter across walker threads.
    let match_count = Arc::new(AtomicUsize::new(0));
    // Shared file counter for progress reporting.
    let files_visited = Arc::new(AtomicUsize::new(0));

    // Run the parallel walker. Each thread gets its own Searcher + Matcher.
    walker.run(|| {
        let tx = tx.clone();
        let cancel = cancel.clone();
        let matcher = matcher.clone();
        let match_count = match_count.clone();
        let files_visited = files_visited.clone();
        let progress_counter = progress_counter.clone();

        // Per-thread searcher — reused across all files on this thread.
        // Binary detection: skip files containing NUL bytes.
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(0))
            .build();

        Box::new(move |entry| {
            // Check cancellation.
            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => return WalkState::Continue,
            };

            // Skip directories and non-files.
            if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                return WalkState::Continue;
            }

            let path = entry.into_path();

            // Report progress every 100 files (best-effort via try_send).
            let count = files_visited.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(counter) = &progress_counter {
                counter.store(count, Ordering::Relaxed);
            }
            if count.is_multiple_of(100) {
                let _ = tx.try_send(SearchEvent::Progress(count));
            }

            let search_result = searcher.search_path(
                &matcher,
                &path,
                UTF8(|line_number, line_content| {
                    // Check cancellation inside the sink.
                    if cancel.load(Ordering::Relaxed) {
                        return Ok(false);
                    }

                    // Strip trailing newline before computing match range so byte
                    // offsets are consistent with the stored `line_content`.
                    let content = line_content.trim_end_matches('\n').trim_end_matches('\r');
                    let match_range = find_match_range(&matcher, content.as_bytes());

                    let search_match = SearchMatch {
                        path: path.clone(),
                        line_number,
                        line_content: content.to_string(),
                        match_range,
                    };

                    // Increment match counter and check cap.
                    let prev = match_count.fetch_add(1, Ordering::Relaxed);
                    if prev >= RESULT_CAP {
                        // Already past cap — don't send this match.
                        return Ok(false);
                    }

                    let _ = tx.send(SearchEvent::Match(search_match));

                    if prev + 1 >= RESULT_CAP {
                        let _ = tx.send(SearchEvent::ResultCap);
                        cancel.store(true, Ordering::Relaxed);
                        return Ok(false);
                    }

                    Ok(true)
                }),
            );

            if let Err(e) = search_result {
                tracing::warn!("Skipping {} during search: {e}", path.display());
            }

            if cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            WalkState::Continue
        })
    });

    if let Some(flag) = &completion_flag {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = tx.send(SearchEvent::Done);
}

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
pub fn apply_replacements(
    replacements: &[Replacement],
    skip_paths: &std::collections::HashSet<std::path::PathBuf>,
    cancel: &AtomicBool,
) -> anyhow::Result<(
    ReplaceResult,
    std::collections::HashMap<std::path::PathBuf, Vec<u8>>,
)> {
    use std::collections::{BTreeMap, HashMap};

    // Group replacements by file path.
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

        // Read original content. On error, skip this file and continue.
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

        // Store original for undo backup.
        backup.insert(path.clone(), original_bytes);

        // Sort in reverse order: last line first, rightmost match first within a line.
        file_replacements.sort_by(|a, b| {
            b.line_number
                .cmp(&a.line_number)
                .then(b.match_range.start.cmp(&a.match_range.start))
        });

        // Detect line ending style to preserve it.
        let line_ending = detect_line_ending(&original_text);

        // Split into lines, apply replacements, rejoin.
        let mut lines: Vec<String> = original_text.lines().map(String::from).collect();
        let has_trailing_newline = original_text.ends_with('\n');

        // TOCTOU guard: validate all targeted lines match their original content
        // BEFORE applying any replacements. This catches external modifications
        // since the search was run. We check each unique line only once (multiple
        // replacements on the same line share the same original_line).
        let mut file_stale = false;
        for r in &file_replacements {
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
            // Nothing was actually replaced — remove from backup.
            backup.remove(&path);
            continue;
        }

        // Rejoin lines preserving original line ending style.
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

    // If no files could be processed at all and we had errors, return the first error.
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

/// Detect the predominant line ending style in a string.
/// Returns `"\r\n"` if CRLF is found, otherwise `"\n"`.
fn detect_line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Restore files from backup (undo Replace All).
///
/// Writes each file atomically (temp file + rename). Continues on per-file errors
/// so that partial undo is possible. Returns the count of files restored.
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

/// Atomically write bytes to a file (temp file + rename, matching json_store::save pattern).
fn atomic_write(path: &Path, content: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or(Path::new("."));
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
    })
}

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

/// Finds the byte range of the first match within a line.
///
/// Falls back to `0..0` if the matcher cannot locate the match
/// (should not happen in practice since the line already matched).
fn find_match_range(matcher: &grep_regex::RegexMatcher, line: &[u8]) -> std::ops::Range<usize> {
    match matcher.find_at(line, 0) {
        Ok(Some(m)) => m.start()..m.end(),
        _ => 0..0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use tempfile::tempdir;

    use crate::model::content_search::ContentSearchOptions;

    /// Helper: run a search and collect all events into a Vec.
    fn search_collect(
        query: &str,
        roots: &[&Path],
        options: &ContentSearchOptions,
    ) -> Vec<SearchEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        search(query, roots, options, tx, cancel, None, None);
        rx.iter().collect()
    }

    /// Count Match events in a list of SearchEvents.
    fn count_matches(events: &[SearchEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, SearchEvent::Match(_)))
            .count()
    }

    /// Check that the last event is Done.
    fn assert_ends_with_done(events: &[SearchEvent]) {
        assert!(
            matches!(events.last(), Some(SearchEvent::Done)),
            "last event should be Done, got: {:?}",
            events.last()
        );
    }

    // AC #3: Literal search finds matches.
    #[test]
    fn literal_search_finds_matches() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("a.rs"), "fn hello() {}\nfn world() {}\n").unwrap();
        fs::write(root.join("b.rs"), "fn hello_again() {}\n").unwrap();
        fs::write(root.join("c.rs"), "no match here\n").unwrap();

        let events = search_collect("hello", &[root], &ContentSearchOptions::default());
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 2, "should find 2 matches");

        // All matches should contain "hello" in their line content.
        for m in &matches {
            assert!(
                m.line_content.contains("hello"),
                "line should contain 'hello': {}",
                m.line_content
            );
            assert_eq!(m.line_number, 1, "match should be on line 1");
            assert!(!m.match_range.is_empty(), "match range should be non-empty");
        }
    }

    // AC #4: Cancellation stops search.
    #[test]
    fn cancel_stops_search() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create enough files that the search won't finish instantly.
        for i in 0..200 {
            fs::write(root.join(format!("file_{i}.txt")), "needle\n".repeat(100)).unwrap();
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        // Cancel after receiving a few matches.
        let handle = std::thread::spawn(move || {
            let mut count = 0;
            for event in rx.iter() {
                if matches!(event, SearchEvent::Match(_)) {
                    count += 1;
                    if count >= 5 {
                        cancel_clone.store(true, Ordering::Relaxed);
                    }
                }
                if matches!(event, SearchEvent::Done) {
                    break;
                }
            }
            count
        });

        search(
            "needle",
            &[root],
            &ContentSearchOptions::default(),
            tx,
            cancel,
            None,
            None,
        );

        let count = handle.join().unwrap();
        // Should have found some matches but far fewer than all 20,000.
        assert!(
            count < 20_000,
            "cancel should have stopped early, got {count} matches"
        );
    }

    // AC #5: Binary files are skipped.
    #[test]
    fn binary_files_skipped() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create a binary file (PNG-like header with null bytes).
        let mut png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png_data.extend_from_slice(b"needle somewhere in binary\x00\x00");
        fs::write(root.join("image.png"), &png_data).unwrap();

        // Create a text file with the same search term.
        fs::write(root.join("code.rs"), "let needle = 42;\n").unwrap();

        let events = search_collect("needle", &[root], &ContentSearchOptions::default());
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 1, "only text file should match");
        assert!(
            matches[0].path.ends_with("code.rs"),
            "match should be from code.rs"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_does_not_abort_search() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let root = dir.path();
        let visible = root.join("visible.rs");
        let unreadable = root.join("secret.rs");

        fs::write(&visible, "needle\n").unwrap();
        fs::write(&unreadable, "needle\n").unwrap();

        let mut perms = fs::metadata(&unreadable).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&unreadable, perms).unwrap();
        assert!(
            fs::File::open(&unreadable).is_err(),
            "test requires an unreadable file"
        );

        let events = search_collect("needle", &[root], &ContentSearchOptions::default());

        let mut restore = fs::metadata(&unreadable).unwrap().permissions();
        restore.set_mode(0o644);
        fs::set_permissions(&unreadable, restore).unwrap();

        assert_ends_with_done(&events);
        let matches: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                SearchEvent::Match(search_match) => Some(search_match),
                _ => None,
            })
            .collect();
        assert_eq!(matches.len(), 1, "unreadable file should be skipped");
        assert!(matches[0].path.ends_with("visible.rs"));
    }

    // AC #6: Gitignore rules are respected.
    #[test]
    fn gitignore_respected() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Initialize a git repo so .gitignore is respected.
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();

        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target/ignored.rs"), "needle\n").unwrap();
        fs::write(root.join("visible.rs"), "needle\n").unwrap();

        // With gitignore ON (default).
        let events = search_collect("needle", &[root], &ContentSearchOptions::default());
        assert_ends_with_done(&events);
        let match_count = count_matches(&events);
        assert_eq!(match_count, 1, "gitignored file should be excluded");

        // With gitignore OFF.
        let opts = ContentSearchOptions {
            gitignore: false,
            ..Default::default()
        };
        let events = search_collect("needle", &[root], &opts);
        assert_ends_with_done(&events);
        let match_count = count_matches(&events);
        assert_eq!(
            match_count, 2,
            "all files should be included with gitignore off"
        );
    }

    // AC #7: Result cap at 10,000 matches.
    #[test]
    fn result_cap_at_10000() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create files with enough matches to exceed the cap.
        // 20 files × 600 matches each = 12,000 potential matches.
        for i in 0..20 {
            let content = "needle\n".repeat(600);
            fs::write(root.join(format!("big_{i}.txt")), content).unwrap();
        }

        let events = search_collect("needle", &[root], &ContentSearchOptions::default());
        assert_ends_with_done(&events);

        let match_count = count_matches(&events);
        // Cap is approximate — parallel threads can overshoot by up to thread count.
        let max_allowed = RESULT_CAP + 8;
        assert!(
            match_count <= max_allowed,
            "should not exceed {max_allowed} matches, got {match_count}"
        );

        // Should have a ResultCap event.
        let has_cap = events.iter().any(|e| matches!(e, SearchEvent::ResultCap));
        assert!(has_cap, "should emit ResultCap event");
    }

    // AC #8: Regex search.
    #[test]
    fn regex_search() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("code.rs"),
            "fn hello() {}\nlet x = 42;\nfn world() {}\n",
        )
        .unwrap();

        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        let events = search_collect(r"fn\s+\w+", &[root], &opts);
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 2, "should find 2 fn declarations");
        for m in &matches {
            assert!(
                m.line_content.starts_with("fn "),
                "line should start with 'fn ': {}",
                m.line_content
            );
        }
    }

    // AC #9: Case-sensitive search.
    #[test]
    fn case_sensitive_search() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("code.rs"), "Error happened\nerror happened\n").unwrap();

        let opts = ContentSearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let events = search_collect("Error", &[root], &opts);
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 1, "only capitalized 'Error' should match");
        assert!(matches[0].line_content.starts_with("Error"));
    }

    // AC #10: Whole-word search.
    #[test]
    fn whole_word_search() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(
            root.join("code.rs"),
            "let port = 8080;\nlet report = true;\nlet export = false;\n",
        )
        .unwrap();

        let opts = ContentSearchOptions {
            whole_word: true,
            ..Default::default()
        };
        let events = search_collect("port", &[root], &opts);
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 1, "only standalone 'port' should match");
        assert!(matches[0].line_content.contains("8080"));
    }

    // AC #11: Glob filter.
    #[test]
    fn glob_filter() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("code.rs"), "needle\n").unwrap();
        fs::write(root.join("notes.txt"), "needle\n").unwrap();
        fs::write(root.join("data.json"), "needle\n").unwrap();

        let opts = ContentSearchOptions {
            glob: Some("*.rs".to_string()),
            ..Default::default()
        };
        let events = search_collect("needle", &[root], &opts);
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 1, "only .rs file should match");
        assert!(matches[0].path.ends_with("code.rs"));
    }

    // AC #12: Multi-root search.
    #[test]
    fn multi_root_search() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        fs::write(dir1.path().join("a.rs"), "needle\n").unwrap();
        fs::write(dir2.path().join("b.rs"), "needle\n").unwrap();

        let events = search_collect(
            "needle",
            &[dir1.path(), dir2.path()],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let match_count = count_matches(&events);
        assert_eq!(match_count, 2, "should find matches in both roots");
    }

    // AC #13: Empty query returns Done immediately.
    #[test]
    fn empty_query_returns_done() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "some content\n").unwrap();

        let events = search_collect("", &[dir.path()], &ContentSearchOptions::default());

        assert_eq!(events.len(), 1, "should only contain Done");
        assert!(matches!(events[0], SearchEvent::Done));
    }

    // Progress events are emitted every 100 files.
    #[test]
    fn progress_events_emitted() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create 250 files — enough to trigger at least 2 progress events
        // (one at 100 files, one at 200 files).
        for i in 0..250 {
            fs::write(root.join(format!("file_{i}.txt")), "content\n").unwrap();
        }

        let events = search_collect(
            "nonexistent_needle",
            &[root],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let progress_events: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Progress(count) => Some(*count),
                _ => None,
            })
            .collect();

        // With 250 files, we expect progress at file 100 and 200.
        assert!(
            progress_events.len() >= 2,
            "expected at least 2 progress events for 250 files, got {}",
            progress_events.len()
        );

        // Progress counts should be multiples of 100.
        for count in &progress_events {
            assert!(
                count.is_multiple_of(100),
                "progress count {count} should be a multiple of 100"
            );
        }
    }

    #[test]
    fn progress_counter_tracks_all_visited_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        for i in 0..250 {
            fs::write(root.join(format!("file_{i}.txt")), "content\n").unwrap();
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress_counter = Arc::new(AtomicUsize::new(0));

        search(
            "nonexistent_needle",
            &[root],
            &ContentSearchOptions::default(),
            tx,
            cancel,
            Some(progress_counter.clone()),
            None,
        );

        let events: Vec<_> = rx.iter().collect();
        assert_ends_with_done(&events);
        assert_eq!(progress_counter.load(Ordering::Relaxed), 250);
    }

    // SearchEvent::Progress variant can be constructed and pattern-matched.
    #[test]
    fn progress_variant_construction() {
        let event = SearchEvent::Progress(42);
        assert!(matches!(event, SearchEvent::Progress(42)));
    }

    #[test]
    fn completion_flag_is_set_before_done_send_unblocks() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();

        for i in 0..100 {
            fs::write(root.join(format!("file_{i}.txt")), "content\n").unwrap();
        }

        let (tx, rx) = crossbeam_channel::bounded(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let completion_flag = Arc::new(AtomicBool::new(false));
        let completion_flag_for_search = completion_flag.clone();

        let handle = std::thread::spawn(move || {
            search(
                "nonexistent_needle",
                &[root.as_path()],
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                Some(completion_flag_for_search),
            );
        });

        for _ in 0..100 {
            if completion_flag.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        assert!(
            completion_flag.load(Ordering::Relaxed),
            "completion flag should be set even if Done is still backpressured"
        );

        let events: Vec<_> = rx.iter().take(2).collect();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, SearchEvent::Done))
        );
        handle.join().unwrap();
    }

    // AC #14: Invalid regex returns Error.
    #[test]
    fn invalid_regex_returns_error() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "content\n").unwrap();

        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        let events = search_collect(r"fn\s+[", &[dir.path()], &opts);

        // Should have Error then Done.
        assert!(events.len() >= 2);
        assert!(
            matches!(events[0], SearchEvent::Error(_)),
            "first event should be Error"
        );
        assert_ends_with_done(&events);
    }

    // --- Story 2.1: Replace All unit tests ---

    use crate::model::content_search::Replacement;
    use std::collections::HashSet;

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
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "let hello = 1;\nlet world = 2;\n").unwrap();
        fs::write(&file_b, "fn hello() {}\n").unwrap();

        let replacements = vec![
            make_replacement(&file_a, 1, "let hello = 1;", "goodbye", 4..9),
            make_replacement(&file_b, 1, "fn hello() {}", "goodbye", 3..8),
        ];

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel).unwrap();

        assert_eq!(result.replaced_count, 2);
        assert_eq!(result.files_affected, 2);
        assert!(result.skipped_paths.is_empty());

        let content_a = fs::read_to_string(&file_a).unwrap();
        assert!(
            content_a.contains("goodbye"),
            "a.rs should have replacement"
        );
        assert!(
            !content_a.contains("hello"),
            "a.rs should not have original"
        );

        let content_b = fs::read_to_string(&file_b).unwrap();
        assert!(
            content_b.contains("goodbye"),
            "b.rs should have replacement"
        );

        assert_eq!(backup.len(), 2, "backup should contain both files");
    }

    #[test]
    fn test_apply_replacements_preserves_backup() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fs::write(&file, original).unwrap();

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel).unwrap();

        assert_eq!(backup[&file], original.as_bytes());
    }

    #[test]
    fn test_undo_replacements_restores_content() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rs");
        let original = "let needle = 42;\n";
        fs::write(&file, original).unwrap();

        let replacements = vec![make_replacement(
            &file,
            1,
            "let needle = 42;",
            "haystack",
            4..10,
        )];

        let cancel = AtomicBool::new(false);
        let (_, backup) = apply_replacements(&replacements, &HashSet::new(), &cancel).unwrap();

        // File should be changed.
        assert!(fs::read_to_string(&file).unwrap().contains("haystack"));

        // Undo should restore.
        let restored = undo_replacements(&backup).unwrap();
        assert_eq!(restored, 1);
        assert_eq!(fs::read_to_string(&file).unwrap(), original);
    }

    #[test]
    fn test_apply_replacements_reverse_order() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("test.rs");
        // Two matches on the same line: "ab" at 0..2 and "cd" at 3..5.
        fs::write(&file, "ab cd\n").unwrap();

        let replacements = vec![
            make_replacement(&file, 1, "ab cd", "XY", 0..2),
            make_replacement(&file, 1, "ab cd", "ZW", 3..5),
        ];

        let cancel = AtomicBool::new(false);
        let (result, _) = apply_replacements(&replacements, &HashSet::new(), &cancel).unwrap();
        assert_eq!(result.replaced_count, 2);

        let content = fs::read_to_string(&file).unwrap();
        assert_eq!(
            content, "XY ZW\n",
            "both replacements should apply correctly"
        );
    }

    #[test]
    fn test_apply_replacements_cancel() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fs::write(&file_a, "needle\n").unwrap();
        fs::write(&file_b, "needle\n").unwrap();

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        // Cancel immediately — at least one file may be skipped.
        let cancel = AtomicBool::new(true);
        let result = apply_replacements(&replacements, &HashSet::new(), &cancel);
        assert!(result.is_err(), "cancelled replace should abort");
        assert_eq!(fs::read_to_string(&file_a).unwrap(), "needle\n");
        assert_eq!(fs::read_to_string(&file_b).unwrap(), "needle\n");
    }

    #[test]
    #[cfg(unix)]
    fn test_apply_replacements_cancel_rolls_back_applied_files() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");
        fs::write(&file_a, "needle\n").unwrap();
        fs::write(&file_b, "needle\n").unwrap();
        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let cancel = Arc::new(AtomicBool::new(false));
        let lock_b = ReplaceFileLock::acquire(&file_b).unwrap();
        let cancel_for_worker = cancel.clone();
        let worker = std::thread::spawn(move || {
            apply_replacements(&replacements, &HashSet::new(), cancel_for_worker.as_ref())
        });

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if fs::read_to_string(&file_a)
                .map(|content| content.contains("replaced"))
                .unwrap_or(false)
            {
                cancel.store(true, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        drop(lock_b);

        let result = worker.join().unwrap();

        assert!(result.is_err(), "cancelled replace should roll back");
        assert_eq!(fs::read_to_string(&file_a).unwrap(), "needle\n");
        assert_eq!(fs::read_to_string(&file_b).unwrap(), "needle\n");
    }

    #[test]
    fn test_apply_replacements_nonexistent_file() {
        let dir = tempdir().unwrap();
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
        // With zero files successfully processed and errors present, returns Err.
        assert!(result.is_err(), "should fail when only file is nonexistent");
    }

    #[test]
    fn test_apply_replacements_skip_paths() {
        let dir = tempdir().unwrap();
        let file_a = dir.path().join("a.rs");
        let file_b = dir.path().join("b.rs");
        fs::write(&file_a, "needle\n").unwrap();
        fs::write(&file_b, "needle\n").unwrap();

        let replacements = vec![
            make_replacement(&file_a, 1, "needle", "replaced", 0..6),
            make_replacement(&file_b, 1, "needle", "replaced", 0..6),
        ];

        let mut skip = HashSet::new();
        skip.insert(file_b.clone());

        let cancel = AtomicBool::new(false);
        let (result, backup) = apply_replacements(&replacements, &skip, &cancel).unwrap();

        assert_eq!(result.replaced_count, 1, "only a.rs should be replaced");
        assert_eq!(result.files_affected, 1);
        assert_eq!(result.skipped_paths.len(), 1);
        assert_eq!(result.skipped_paths[0], file_b);
        assert!(backup.contains_key(&file_a));
        assert!(!backup.contains_key(&file_b));

        // b.rs should be unchanged.
        assert_eq!(fs::read_to_string(&file_b).unwrap(), "needle\n");
    }
}
