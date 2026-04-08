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

use crate::model::content_search::{ContentSearchOptions, SearchEvent, SearchMatch};

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
) {
    // Empty query → Done immediately, no file traversal.
    if query.is_empty() {
        let _ = tx.send(SearchEvent::Done);
        return;
    }

    if roots.is_empty() {
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
                    let _ = tx.send(SearchEvent::Done);
                    return;
                }
            }
        }

        builder.build_parallel()
    };

    // Shared match counter across walker threads.
    let match_count = Arc::new(AtomicUsize::new(0));

    // Run the parallel walker. Each thread gets its own Searcher + Matcher.
    walker.run(|| {
        let tx = tx.clone();
        let cancel = cancel.clone();
        let matcher = matcher.clone();
        let match_count = match_count.clone();

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

            if search_result.is_err() || cancel.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }

            WalkState::Continue
        })
    });

    let _ = tx.send(SearchEvent::Done);
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
        search(query, roots, options, tx, cancel);
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
}
