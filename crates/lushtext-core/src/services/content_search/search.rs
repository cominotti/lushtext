// SPDX-License-Identifier: GPL-3.0-or-later

//! Streaming workspace-wide content search.
//!
//! This is the query side of the content-search service. It stays pure Rust
//! and sends `SearchEvent` values over a channel so GTK adapters can render
//! incremental results without taking a dependency on the ripgrep engine here.

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

use crate::model::content_search::{ContentSearchOptions, SearchEvent, SearchMatch};

/// Maximum number of matches before the search stops. Approximate under
/// parallel walkers — concurrent threads may overshoot by up to the thread
/// count before observing the cancel flag. The UI should clamp display to
/// this value.
pub(super) const RESULT_CAP: usize = 10_000;

/// Searches file contents across workspace roots with streaming results.
///
/// Blocks until search completes or is cancelled. Call from a dedicated thread.
/// Results are sent through `tx` as `SearchEvent` variants.
///
/// The `tx` channel should be `bounded(1024)` in production to apply backpressure.
/// Using `unbounded()` is acceptable in tests.
#[allow(clippy::needless_pass_by_value)] // Intentional: cloned into parallel walker thread closures
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

    // Build the directory walker once so worker threads share traversal config.
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

    // Shared counters across walker threads.
    let match_count = Arc::new(AtomicUsize::new(0));
    let files_visited = Arc::new(AtomicUsize::new(0));

    walker.run(|| {
        let tx = tx.clone();
        let cancel = cancel.clone();
        let matcher = matcher.clone();
        let match_count = match_count.clone();
        let files_visited = files_visited.clone();
        let progress_counter = progress_counter.clone();

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

                    let search_match = SearchMatch {
                        path: path.clone(),
                        line_number,
                        line_content: content.to_string(),
                        match_range,
                    };

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

    if let Some(flag) = &completion_flag {
        flag.store(true, Ordering::Relaxed);
    }
    let _ = tx.send(SearchEvent::Done);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

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
    fn cancel_stops_search() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        for i in 0..200 {
            fs::write(root.join(format!("file_{i}.txt")), "needle\n".repeat(100)).unwrap();
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

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
        assert!(
            count < 20_000,
            "cancel should have stopped early, got {count} matches"
        );
    }

    #[test]
    fn binary_files_skipped() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let mut png_data = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png_data.extend_from_slice(b"needle somewhere in binary\x00\x00");
        fs::write(root.join("image.png"), &png_data).unwrap();
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
        assert!(matches[0].path.ends_with("code.rs"));
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

    #[test]
    fn gitignore_respected() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();

        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("target/ignored.rs"), "needle\n").unwrap();
        fs::write(root.join("visible.rs"), "needle\n").unwrap();

        let events = search_collect("needle", &[root], &ContentSearchOptions::default());
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
        let events = search_collect("needle", &[root], &opts);
        assert_ends_with_done(&events);
        assert_eq!(
            count_matches(&events),
            2,
            "all files should be included with gitignore off"
        );
    }

    #[test]
    fn result_cap_at_10000() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        for i in 0..20 {
            let content = "needle\n".repeat(600);
            fs::write(root.join(format!("big_{i}.txt")), content).unwrap();
        }

        let events = search_collect("needle", &[root], &ContentSearchOptions::default());
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

        assert_eq!(
            count_matches(&events),
            2,
            "should find matches in both roots"
        );
    }

    #[test]
    fn empty_query_returns_done() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "some content\n").unwrap();

        let events = search_collect("", &[dir.path()], &ContentSearchOptions::default());

        assert_eq!(events.len(), 1, "should only contain Done");
        assert!(matches!(events[0], SearchEvent::Done));
    }

    #[test]
    fn progress_events_emitted() {
        let dir = tempdir().unwrap();
        let root = dir.path();

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

    #[test]
    fn invalid_regex_returns_error() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.rs"), "content\n").unwrap();

        let opts = ContentSearchOptions {
            regex: true,
            ..Default::default()
        };
        let events = search_collect(r"fn\s+[", &[dir.path()], &opts);

        assert!(events.len() >= 2);
        assert!(matches!(events[0], SearchEvent::Error(_)));
        assert_ends_with_done(&events);
    }
}
