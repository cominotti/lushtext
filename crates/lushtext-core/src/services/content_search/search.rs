// SPDX-License-Identifier: GPL-3.0-or-later

//! Streaming workspace-wide content search.
//!
//! This is the query side of the content-search service. It stays pure Rust
//! and sends `SearchEvent` values over a channel so GTK adapters can render
//! incremental results without taking a dependency on the ripgrep engine here.
//! Its direct `ignore` and `grep_searcher` calls are an approved read-only
//! engine adapter exception to the normal filesystem boundary: traversal,
//! gitignore/glob filtering, binary detection, and streaming file reads stay
//! inside the ripgrep stack, while mutation, undo backup, and persistence remain
//! routed through `services::filesystem`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crossbeam_channel::Sender;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::overrides::OverrideBuilder;
use ignore::{WalkBuilder, WalkState};

use crate::model::content_search::{ContentSearchOptions, SearchEvent, SearchMatch};
use crate::services::filesystem::metadata as fs_metadata;

/// Maximum number of matches before the search stops. Approximate under
/// parallel walkers — concurrent threads may overshoot by up to the thread
/// count before observing the cancel flag. The UI should clamp display to
/// this value.
pub(super) const RESULT_CAP: usize = 10_000;

/// Searches file contents across the supplied workspace folders with streaming results.
///
/// Blocks until search completes or is cancelled. Call from a dedicated thread.
/// Results are sent through `tx` as `SearchEvent` variants.
///
/// The `tx` channel should be `bounded(1024)` in production to apply backpressure.
/// Using `unbounded()` is acceptable in tests.
#[expect(
    clippy::needless_pass_by_value,
    reason = "The sender is cloned into parallel walker closures, so taking ownership keeps the thread boundary explicit"
)]
pub fn search(
    query: &str,
    workspace_folders: &[&Path],
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

    if workspace_folders.is_empty() {
        if let Some(flag) = &completion_flag {
            flag.store(true, Ordering::Relaxed);
        }
        let _ = tx.send(SearchEvent::Done);
        return;
    }

    // Build the regex matcher once, then clone it into walker threads.
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

    // Cap per-folder walker parallelism so broad workspace searches remain
    // cooperative with GTK and other app-owned background work.
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8));
    // Shared counters across walker threads and ordered workspace folders.
    let match_count = Arc::new(AtomicUsize::new(0));
    let files_visited = Arc::new(AtomicUsize::new(0));
    let visited_files = Arc::new(Mutex::new(HashSet::new()));
    let file_identity_mode = FileIdentityMode::for_workspace_folders(workspace_folders);

    for folder in workspace_folders {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let mut builder = WalkBuilder::new(folder);
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
            match OverrideBuilder::new(folder)
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

        let walker = builder.build_parallel();

        walker.run(|| {
            let tx = tx.clone();
            let cancel = cancel.clone();
            let matcher = matcher.clone();
            let match_count = match_count.clone();
            let files_visited = files_visited.clone();
            let progress_counter = progress_counter.clone();
            let visited_files = visited_files.clone();

            // Per-thread searcher — reused across all files on this thread.
            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0))
                .build();

            Box::new(move |entry| {
                if cancel.load(Ordering::Relaxed) {
                    return WalkState::Quit;
                }

                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };

                // Skip directories and non-files.
                if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                    return WalkState::Continue;
                }

                let path = entry.into_path();
                if !claim_file_identity(&path, &visited_files, file_identity_mode) {
                    return WalkState::Continue;
                }

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
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(false);
                        }

                        // Strip trailing newline before computing match range so byte
                        // offsets are consistent with the stored `line_content`.
                        let content = line_content.trim_end_matches('\n').trim_end_matches('\r');
                        let match_range = find_match_range(&matcher, content.as_bytes());

                        let search_match =
                            SearchMatch::new(path.clone(), line_number, content, match_range);

                        // Increment match counter and enforce the shared cap.
                        let prev = match_count.fetch_add(1, Ordering::Relaxed);
                        if prev >= RESULT_CAP {
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
    }

    if let Some(flag) = &completion_flag {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = tx.send(SearchEvent::Done);
}

/// Search dedupe strategy for visited files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileIdentityMode {
    /// A single walker root cannot revisit the same regular file through a
    /// configured parent/child overlap, so the walked path is enough.
    WalkedPath,
    /// Multiple roots may overlap, so canonical identity prevents duplicate
    /// results through parent and descendant workspace folders.
    CanonicalPath,
}

impl FileIdentityMode {
    /// Use canonical dedupe only when multiple configured roots may overlap.
    fn for_workspace_folders(workspace_folders: &[&Path]) -> Self {
        if workspace_folders.len() > 1 {
            Self::CanonicalPath
        } else {
            Self::WalkedPath
        }
    }
}

/// Find the first byte range that matched within one line.
///
/// The line has already matched, so falling back to `0..0` should be rare and
/// only happens when the matcher fails to resolve the byte offsets a second time.
fn find_match_range(matcher: &grep_regex::RegexMatcher, line: &[u8]) -> std::ops::Range<usize> {
    match matcher.find_at(line, 0) {
        Ok(Some(m)) => m.start()..m.end(),
        _ => 0..0,
    }
}

/// Claim one file by identity before searching it.
///
/// Overlapping workspace folders can reach the same file through a parent and a
/// descendant tree. Canonicalizing before the lock keeps slow filesystem work
/// outside the shared critical section and is reserved for multi-root searches;
/// a single walker root can use the path reported by `ignore`.
fn claim_file_identity(
    path: &Path,
    visited_files: &Mutex<HashSet<PathBuf>>,
    mode: FileIdentityMode,
) -> bool {
    let identity = match mode {
        FileIdentityMode::WalkedPath => path.to_path_buf(),
        FileIdentityMode::CanonicalPath => {
            fs_metadata::canonical_path(path).unwrap_or_else(|_| path.to_path_buf())
        }
    };
    visited_files
        .lock()
        .is_ok_and(|mut visited| visited.insert(identity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::{fixture, read as fs_read};
    use std::assert_matches;
    use tempfile::tempdir;

    /// Helper: run a search and collect all events into a Vec.
    fn search_collect(
        query: &str,
        workspace_folders: &[&Path],
        options: &ContentSearchOptions,
    ) -> Vec<SearchEvent> {
        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        search(query, workspace_folders, options, tx, cancel, None, None);
        rx.iter().collect()
    }

    /// Count Match events in a list of SearchEvents.
    fn count_matches(events: &[SearchEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, SearchEvent::Match(_)))
            .count()
    }

    /// Borrow Match payloads from an event stream for focused assertions.
    fn search_matches(events: &[SearchEvent]) -> Vec<&SearchMatch> {
        events
            .iter()
            .filter_map(|event| match event {
                SearchEvent::Match(search_match) => Some(search_match),
                _ => None,
            })
            .collect()
    }

    /// Check that the last event is Done.
    fn assert_ends_with_done(events: &[SearchEvent]) {
        assert_matches!(events.last(), Some(SearchEvent::Done));
    }

    #[test]
    fn file_identity_mode_uses_canonical_paths_only_for_multiple_roots() {
        let root = Path::new("/tmp/workspace");

        assert_eq!(
            FileIdentityMode::for_workspace_folders(&[root]),
            FileIdentityMode::WalkedPath
        );
        assert_eq!(
            FileIdentityMode::for_workspace_folders(&[root, Path::new("/tmp/workspace/src")]),
            FileIdentityMode::CanonicalPath
        );
    }

    #[test]
    fn literal_search_finds_matches() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::write_text(
            &workspace_folder.join("a.rs"),
            "fn hello() {}\nfn world() {}\n",
        );
        fixture::write_text(&workspace_folder.join("b.rs"), "fn hello_again() {}\n");
        fixture::write_text(&workspace_folder.join("c.rs"), "no match here\n");

        let events = search_collect(
            "hello",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 2, "should find 2 matches");

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

    #[test]
    fn literal_search_bounds_long_matching_lines() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();
        let prefix = "a".repeat(crate::model::content_search::MAX_SEARCH_MATCH_LINE_BYTES * 2);
        let suffix = "b".repeat(crate::model::content_search::MAX_SEARCH_MATCH_LINE_BYTES * 2);
        fixture::write_text(
            &workspace_folder.join("minified.js"),
            &format!("{prefix}needle{suffix}\n"),
        );

        let events = search_collect(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);
        let matches = search_matches(&events);

        assert_eq!(matches.len(), 1);
        let search_match = matches[0];
        assert!(search_match.line_truncated);
        assert!(
            search_match.line_content.len()
                <= crate::model::content_search::MAX_SEARCH_MATCH_LINE_BYTES
        );
        assert_eq!(
            &search_match.line_content[search_match.match_range.clone()],
            "needle"
        );
    }

    #[test]
    fn cancel_stops_search() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        for i in 0..200 {
            fixture::write_text(
                &workspace_folder.join(format!("file_{i}.txt")),
                &"needle\n".repeat(100),
            );
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        let handle = std::thread::spawn(move || {
            let mut count = 0;
            for event in &rx {
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
            &[workspace_folder],
            &ContentSearchOptions::default(),
            tx,
            cancel,
            None,
            None,
        );

        let count = handle.join().expect("expected operation to succeed");
        assert!(
            count < 20_000,
            "cancel should have stopped early, got {count} matches"
        );
    }

    #[test]
    fn binary_files_skipped() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        let mut png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png_data.extend_from_slice(b"needle somewhere in binary\x00\x00");
        fixture::write_bytes(&workspace_folder.join("image.png"), &png_data);
        fixture::write_text(&workspace_folder.join("code.rs"), "let needle = 42;\n");

        let events = search_collect(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let matches: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                SearchEvent::Match(m) => Some(m),
                _ => None,
            })
            .collect();

        assert_eq!(matches.len(), 1, "only text file should match");
        assert!(matches[0].path.ends_with("code.rs"));
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_does_not_abort_search() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();
        let visible = workspace_folder.join("visible.rs");
        let unreadable = workspace_folder.join("secret.rs");

        fixture::write_text(&visible, "needle\n");
        fixture::write_text(&unreadable, "needle\n");

        fixture::set_mode(&unreadable, 0o000);
        if fs_read::bytes(&unreadable).is_ok() {
            // Some environments (notably privileged CI containers) can still
            // read a 0o000 file, so this fixture cannot prove the unreadable
            // path there. Restore permissions and skip the assertion-only test.
            fixture::set_mode(&unreadable, 0o644);
            return;
        }

        let events = search_collect(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );

        fixture::set_mode(&unreadable, 0o644);

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

    #[test]
    fn gitignore_respected() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::create_dir(&workspace_folder.join(".git"));
        fixture::write_text(&workspace_folder.join(".gitignore"), "target/\n");

        fixture::create_dir(&workspace_folder.join("target"));
        fixture::write_text(&workspace_folder.join("target/ignored.rs"), "needle\n");
        fixture::write_text(&workspace_folder.join("visible.rs"), "needle\n");

        let events = search_collect(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);
        assert_eq!(
            count_matches(&events),
            1,
            "gitignored file should be excluded"
        );

        let opts = ContentSearchOptions {
            gitignore: false,
            ..Default::default()
        };
        let events = search_collect("needle", &[workspace_folder], &opts);
        assert_ends_with_done(&events);
        assert_eq!(
            count_matches(&events),
            2,
            "all files should be included with gitignore off"
        );
    }

    #[test]
    fn result_cap_at_10000() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        for i in 0..20 {
            let content = "needle\n".repeat(600);
            fixture::write_text(&workspace_folder.join(format!("big_{i}.txt")), &content);
        }

        let events = search_collect(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let match_count = count_matches(&events);
        let max_allowed = RESULT_CAP + 8;
        assert!(
            match_count <= max_allowed,
            "should not exceed {max_allowed} matches, got {match_count}"
        );

        let has_cap = events.iter().any(|e| matches!(e, SearchEvent::ResultCap));
        assert!(has_cap, "should emit ResultCap event");
    }

    #[test]
    fn regex_search() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::write_text(
            &workspace_folder.join("code.rs"),
            "fn hello() {}\nlet x = 42;\nfn world() {}\n",
        );

        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        let events = search_collect(r"fn\s+\w+", &[workspace_folder], &opts);
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

    #[test]
    fn case_sensitive_search() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::write_text(
            &workspace_folder.join("code.rs"),
            "Error happened\nerror happened\n",
        );

        let opts = ContentSearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let events = search_collect("Error", &[workspace_folder], &opts);
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

    #[test]
    fn whole_word_search() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::write_text(
            &workspace_folder.join("code.rs"),
            "let port = 8080;\nlet report = true;\nlet export = false;\n",
        );

        let opts = ContentSearchOptions {
            whole_word: true,
            ..Default::default()
        };
        let events = search_collect("port", &[workspace_folder], &opts);
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

    #[test]
    fn glob_filter() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        fixture::write_text(&workspace_folder.join("code.rs"), "needle\n");
        fixture::write_text(&workspace_folder.join("notes.txt"), "needle\n");
        fixture::write_text(&workspace_folder.join("data.json"), "needle\n");

        let opts = ContentSearchOptions {
            glob: Some("*.rs".to_string()),
            ..Default::default()
        };
        let events = search_collect("needle", &[workspace_folder], &opts);
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

    #[test]
    fn multiple_search_folders() {
        let dir1 = tempdir().expect("expected operation to succeed");
        let dir2 = tempdir().expect("expected operation to succeed");

        fixture::write_text(&dir1.path().join("a.rs"), "needle\n");
        fixture::write_text(&dir2.path().join("b.rs"), "needle\n");

        let events = search_collect(
            "needle",
            &[dir1.path(), dir2.path()],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        assert_eq!(
            count_matches(&events),
            2,
            "should find matches in both folders"
        );
    }

    #[test]
    fn overlapping_search_folders_deduplicate_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path().to_path_buf();
        let nested_folder = workspace_folder.join("src");
        let nested_file = nested_folder.join("main.rs");
        fixture::create_dir(&nested_folder);
        fixture::write_text(&nested_file, "needle one\nneedle two\n");

        for folders in [
            vec![workspace_folder.clone(), nested_folder.clone()],
            vec![nested_folder, workspace_folder],
        ] {
            let folder_refs: Vec<&Path> = folders.iter().map(PathBuf::as_path).collect();
            let events = search_collect("needle", &folder_refs, &ContentSearchOptions::default());
            assert_ends_with_done(&events);

            let matches = search_matches(&events);
            assert_eq!(
                matches.len(),
                2,
                "one file's two matching lines should appear once"
            );
            assert!(
                matches
                    .iter()
                    .all(|search_match| search_match.path == nested_file),
                "overlap should not emit a second route to the same canonical file"
            );
        }
    }

    #[test]
    fn overlapping_search_folders_preserve_distinct_parent_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path().to_path_buf();
        let nested_folder = workspace_folder.join("src");
        let parent_file = workspace_folder.join("README.md");
        let nested_file = nested_folder.join("main.rs");
        fixture::create_dir(&nested_folder);
        fixture::write_text(&parent_file, "needle in readme\n");
        fixture::write_text(&nested_file, "needle in main\n");

        let events = search_collect(
            "needle",
            &[workspace_folder.as_path(), nested_folder.as_path()],
            &ContentSearchOptions::default(),
        );
        assert_ends_with_done(&events);

        let paths: HashSet<_> = search_matches(&events)
            .into_iter()
            .map(|search_match| search_match.path.clone())
            .collect();
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&parent_file));
        assert!(paths.contains(&nested_file));
    }

    #[test]
    fn empty_query_returns_done() {
        let dir = tempdir().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("a.rs"), "some content\n");

        let events = search_collect("", &[dir.path()], &ContentSearchOptions::default());

        assert_eq!(events.len(), 1, "should only contain Done");
        assert_matches!(events[0], SearchEvent::Done);
    }

    #[test]
    fn progress_events_emitted() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        for i in 0..250 {
            fixture::write_text(&workspace_folder.join(format!("file_{i}.txt")), "content\n");
        }

        let events = search_collect(
            "nonexistent_needle",
            &[workspace_folder],
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

        assert!(
            progress_events.len() >= 2,
            "expected at least 2 progress events for 250 files, got {}",
            progress_events.len()
        );

        for count in &progress_events {
            assert!(
                count.is_multiple_of(100),
                "progress count {count} should be a multiple of 100"
            );
        }
    }

    #[test]
    fn progress_counter_tracks_all_visited_files() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        for i in 0..250 {
            fixture::write_text(&workspace_folder.join(format!("file_{i}.txt")), "content\n");
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress_counter = Arc::new(AtomicUsize::new(0));

        search(
            "nonexistent_needle",
            &[workspace_folder],
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

    #[test]
    fn progress_variant_construction() {
        let event = SearchEvent::Progress(42);
        assert_matches!(event, SearchEvent::Progress(42));
    }

    #[test]
    fn completion_flag_is_set_before_done_send_unblocks() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path().to_path_buf();

        for i in 0..100 {
            fixture::write_text(&workspace_folder.join(format!("file_{i}.txt")), "content\n");
        }

        let (tx, rx) = crossbeam_channel::bounded(1);
        let cancel = Arc::new(AtomicBool::new(false));
        let completion_flag = Arc::new(AtomicBool::new(false));
        let completion_flag_for_search = completion_flag.clone();

        let handle = std::thread::spawn(move || {
            search(
                "nonexistent_needle",
                &[workspace_folder.as_path()],
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
        handle.join().expect("expected operation to succeed");
    }

    #[test]
    fn bounded_channel_many_matches_completes_with_concurrent_drain() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path().to_path_buf();

        for i in 0..64 {
            fixture::write_text(
                &workspace_folder.join(format!("many_matches_{i}.txt")),
                &"needle\n".repeat(32),
            );
        }

        let (tx, rx) = crossbeam_channel::bounded(16);
        let cancel = Arc::new(AtomicBool::new(false));
        let search_root = workspace_folder;

        let handle = std::thread::spawn(move || {
            search(
                "needle",
                &[search_root.as_path()],
                &ContentSearchOptions::default(),
                tx,
                cancel,
                None,
                None,
            );
        });

        // This mirrors the Criterion harness: the receiver must keep draining
        // while the synchronous producer is active. Moving this drain after
        // `join()` would fill the bounded channel and never reach `Done`.
        let mut match_count = 0;
        loop {
            let event = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("bounded search should keep producing events while drained");
            match event {
                SearchEvent::Match(_) => match_count += 1,
                SearchEvent::Done => break,
                SearchEvent::Progress(_) => {}
                SearchEvent::Error(message) => panic!("search should not emit an error: {message}"),
                SearchEvent::ResultCap => panic!("fixture should stay below the result cap"),
            }
        }

        assert!(
            match_count > 16,
            "fixture should exceed channel capacity to exercise backpressure"
        );
        handle.join().expect("expected operation to succeed");
    }

    #[test]
    fn invalid_regex_returns_error() {
        let dir = tempdir().expect("expected operation to succeed");
        fixture::write_text(&dir.path().join("a.rs"), "content\n");

        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        let events = search_collect(r"fn\s+[", &[dir.path()], &opts);

        assert!(events.len() >= 2);
        assert_matches!(events[0], SearchEvent::Error(_));
        assert_ends_with_done(&events);
    }
}
