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

use std::path::Path;
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
use crate::model::workspace_search::{
    WorkspaceSearchFallbackClaim, WorkspaceSearchFallbackLedger, WorkspaceSearchFallbackLimits,
    WorkspaceSearchFallbackMetrics, WorkspaceSearchTraversalPlan,
};
use crate::services::filesystem::metadata as fs_metadata;

/// Maximum number of matches before the search stops. Approximate under
/// parallel walkers — concurrent threads may overshoot by up to the thread
/// count before observing the stop signal. The UI should clamp display to
/// this value.
pub(super) const RESULT_CAP: usize = 10_000;

/// Why the parallel walk should stop scanning, and who owns each reason.
///
/// The distinction matters at the service boundary. `cancelled` is the
/// **caller's** flag: a superseding query or a closing panel sets it to say
/// "nothing from this flight is wanted any more", and the consumer is entitled
/// to discard every event still in flight. A service-internal termination is
/// the opposite claim: production stops, but every event already queued —
/// including the terminating `ResultCap` or `Incomplete` event itself — is the
/// honest result the consumer still has to render. Terminating by writing the
/// caller's flag conflated the two and silently discarded the cap notice plus
/// up to a channel's worth of already-found matches.
#[derive(Clone)]
struct WalkStop {
    /// Caller-owned supersede/user cancellation. Never written here.
    cancelled: Arc<AtomicBool>,
    /// One-shot claim on sending the single `Incomplete` event.
    incomplete_sent: Arc<AtomicBool>,
    /// Set once the shared result cap has been reported.
    result_capped: Arc<AtomicBool>,
}

impl WalkStop {
    fn new(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            incomplete_sent: Arc::new(AtomicBool::new(false)),
            result_capped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether the walk should stop scanning, for any of the three reasons.
    fn stopped(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
            || self.incomplete_sent.load(Ordering::Acquire)
            || self.result_capped.load(Ordering::Acquire)
    }

    /// Claim the exclusive right to send the one `Incomplete` event.
    fn claim_incomplete(&self) -> bool {
        !self.incomplete_sent.swap(true, Ordering::AcqRel)
    }

    /// Stop producing after the cap notice has been sent to the consumer.
    fn record_result_cap(&self) {
        self.result_capped.store(true, Ordering::Release);
    }
}

/// Searches file contents across the supplied workspace folders with streaming results.
///
/// Blocks until search completes or is cancelled. Call from a dedicated thread.
/// Results are sent through `tx` as `SearchEvent` variants.
///
/// `cancel` belongs to the caller: setting it means "discard this flight". The
/// service never writes it, so a result cap or identity-limit stop still ends
/// with the terminating event plus `Done` on the channel for the caller to
/// render (see [`WalkStop`]).
///
/// The `tx` channel should be `bounded(1024)` in production to apply backpressure.
/// Using `unbounded()` is acceptable in tests.
pub fn search(
    query: &str,
    workspace_folders: &[&Path],
    options: &ContentSearchOptions,
    tx: Sender<SearchEvent>,
    cancel: Arc<AtomicBool>,
    progress_counter: Option<Arc<AtomicUsize>>,
    completion_flag: Option<Arc<AtomicBool>>,
) {
    let plan = WorkspaceSearchTraversalPlan::build(
        workspace_folders.iter().copied(),
        fs_metadata::canonical_path,
    );
    search_with_plan(
        query,
        &plan,
        options,
        tx,
        cancel,
        progress_counter,
        completion_flag,
    );
}

/// Search one pre-normalized immutable workspace traversal plan.
///
/// Callers that already own a generation-scoped folder snapshot should build
/// the plan on their worker before entering this service so root identity is
/// resolved exactly once for the entire generation.
pub fn search_with_plan(
    query: &str,
    plan: &WorkspaceSearchTraversalPlan,
    options: &ContentSearchOptions,
    tx: Sender<SearchEvent>,
    cancel: Arc<AtomicBool>,
    progress_counter: Option<Arc<AtomicUsize>>,
    completion_flag: Option<Arc<AtomicBool>>,
) {
    search_with_plan_and_limits(
        query,
        plan,
        options,
        tx,
        cancel,
        SearchTelemetry {
            progress_counter,
            completion_flag,
        },
        WorkspaceSearchFallbackLimits::default(),
    );
}

struct SearchTelemetry {
    progress_counter: Option<Arc<AtomicUsize>>,
    completion_flag: Option<Arc<AtomicBool>>,
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "The sender is cloned into parallel walker closures, so taking ownership keeps the thread boundary explicit"
)]
fn search_with_plan_and_limits(
    query: &str,
    plan: &WorkspaceSearchTraversalPlan,
    options: &ContentSearchOptions,
    tx: Sender<SearchEvent>,
    cancel: Arc<AtomicBool>,
    telemetry: SearchTelemetry,
    fallback_limits: WorkspaceSearchFallbackLimits,
) {
    let SearchTelemetry {
        progress_counter,
        completion_flag,
    } = telemetry;
    // Empty query → Done immediately, no file traversal.
    if query.is_empty() {
        if let Some(flag) = &completion_flag {
            flag.store(true, Ordering::Relaxed);
        }
        let _ = tx.send(SearchEvent::Done);
        return;
    }

    if plan.traversal_roots().is_empty() {
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
    let fallback_ledger = plan.fallback_identity_required().then(|| {
        Arc::new(Mutex::new(WorkspaceSearchFallbackLedger::new(
            fallback_limits,
        )))
    });
    let stop = WalkStop::new(cancel);

    for (traversal_root_index, traversal_root) in plan.traversal_roots().iter().enumerate() {
        if stop.stopped() {
            break;
        }

        let folder = traversal_root.scan_path();

        let mut builder = WalkBuilder::new(folder);
        builder.threads(threads);
        builder.hidden(true); // skip hidden files (LushText convention)
        if !traversal_root.excluded_paths().is_empty() {
            let excluded_paths = traversal_root.excluded_paths().to_vec();
            builder.filter_entry(move |entry| {
                !excluded_paths
                    .iter()
                    .any(|excluded| entry.path() == excluded)
            });
        }

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
            let stop = stop.clone();
            let matcher = matcher.clone();
            let match_count = match_count.clone();
            let files_visited = files_visited.clone();
            let progress_counter = progress_counter.clone();
            let fallback_ledger = fallback_ledger.clone();

            // Per-thread searcher — reused across all files on this thread.
            let mut searcher = SearcherBuilder::new()
                .binary_detection(BinaryDetection::quit(0))
                .build();

            Box::new(move |entry| {
                if stop.stopped() {
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
                if let Some(ledger) = fallback_ledger.as_ref() {
                    let identity =
                        fs_metadata::canonical_path(&path).unwrap_or_else(|_| path.clone());
                    let claim = ledger
                        .lock()
                        .map_or(WorkspaceSearchFallbackClaim::Duplicate, |mut ledger| {
                            ledger.try_claim(identity)
                        });
                    match claim {
                        WorkspaceSearchFallbackClaim::Admitted => {}
                        WorkspaceSearchFallbackClaim::Duplicate => return WalkState::Continue,
                        WorkspaceSearchFallbackClaim::Incomplete(reason) => {
                            if stop.claim_incomplete() {
                                let _ = tx.send(SearchEvent::Incomplete(reason));
                            }
                            return WalkState::Quit;
                        }
                    }
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
                        if stop.stopped() {
                            return Ok(false);
                        }

                        // Strip trailing newline before computing match range so byte
                        // offsets are consistent with the stored `line_content`.
                        let content = line_content.trim_end_matches('\n').trim_end_matches('\r');
                        let match_range = find_match_range(&matcher, content.as_bytes());

                        let search_match =
                            SearchMatch::new(path.clone(), line_number, content, match_range)
                                .with_traversal_root_index(traversal_root_index);

                        // Increment match counter and enforce the shared cap.
                        let prev = match_count.fetch_add(1, Ordering::Relaxed);
                        if prev >= RESULT_CAP {
                            return Ok(false);
                        }

                        let _ = tx.send(SearchEvent::Match(search_match));

                        if prev + 1 >= RESULT_CAP {
                            // Order matters: the cap notice is queued behind the
                            // matches it describes, and only then is production
                            // stopped. The caller's cancellation flag stays
                            // untouched so the consumer keeps draining this
                            // flight's buffered matches, the cap notice, and the
                            // terminal `Done`.
                            let _ = tx.send(SearchEvent::ResultCap);
                            stop.record_result_cap();
                            return Ok(false);
                        }

                        Ok(true)
                    }),
                );

                if let Err(e) = search_result {
                    tracing::warn!("Skipping {} during search: {e}", path.display());
                }

                if stop.stopped() {
                    return WalkState::Quit;
                }

                WalkState::Continue
            })
        });
    }

    if let Some(flag) = &completion_flag {
        flag.store(true, Ordering::Relaxed);
    }
    let fallback_metrics =
        fallback_ledger
            .as_ref()
            .map_or_else(WorkspaceSearchFallbackMetrics::default, |ledger| {
                ledger.lock().map_or_else(
                    |_| WorkspaceSearchFallbackMetrics::default(),
                    |ledger| ledger.metrics(),
                )
            });
    let _ = tx.send(SearchEvent::TraversalMetrics(fallback_metrics));
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
    use crate::services::filesystem::{fixture, read as fs_read};
    use std::assert_matches;
    use std::collections::HashSet;
    use std::path::PathBuf;
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
    fn result_cap_terminates_without_touching_the_caller_cancel_flag() {
        let dir = tempdir().expect("expected operation to succeed");
        let workspace_folder = dir.path();

        for i in 0..20 {
            fixture::write_text(
                &workspace_folder.join(format!("big_{i}.txt")),
                &"needle\n".repeat(600),
            );
        }

        let (tx, rx) = crossbeam_channel::unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        search(
            "needle",
            &[workspace_folder],
            &ContentSearchOptions::default(),
            tx,
            Arc::clone(&cancel),
            None,
            None,
        );
        let events: Vec<_> = rx.iter().collect();

        assert!(
            !cancel.load(Ordering::Acquire),
            "the cap is a service-side stop; flipping the caller's cancel flag makes the \
             consumer discard the cap notice and every buffered match",
        );
        let cap_index = events
            .iter()
            .position(|event| matches!(event, SearchEvent::ResultCap))
            .expect("capped search should report the cap");
        assert_ends_with_done(&events);
        assert!(
            cap_index > 0,
            "the cap notice follows the matches it describes"
        );
        // Concurrent walkers may still be flushing their own matches when the
        // capping thread reports, so only the total is exact enough to assert.
        assert!(
            count_matches(&events) >= RESULT_CAP,
            "a capped search must still deliver every match it found up to the cap",
        );
    }

    #[test]
    fn walk_stop_separates_caller_cancellation_from_service_termination() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = WalkStop::new(Arc::clone(&cancelled));
        assert!(!stop.stopped());

        stop.record_result_cap();
        assert!(stop.stopped());
        assert!(
            !cancelled.load(Ordering::Acquire),
            "service termination must never write the caller's flag",
        );

        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = WalkStop::new(Arc::clone(&cancelled));
        assert!(stop.claim_incomplete(), "the first claim owns the event");
        assert!(!stop.claim_incomplete(), "only one Incomplete may be sent");
        assert!(stop.stopped());
        assert!(!cancelled.load(Ordering::Acquire));

        let cancelled = Arc::new(AtomicBool::new(true));
        assert!(WalkStop::new(cancelled).stopped());
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
    fn large_single_root_no_match_needs_no_fallback_identity_retention() {
        let dir = tempdir().expect("large no-match fixture");
        for index in 0..2_000 {
            fixture::write_text(
                &dir.path().join(format!("file-{index:04}.txt")),
                "ordinary content without the query\n",
            );
        }

        let plan = WorkspaceSearchTraversalPlan::build(
            [dir.path().to_path_buf()],
            crate::services::filesystem::metadata::canonical_path,
        );
        assert_eq!(plan.traversal_roots().len(), 1);
        assert!(!plan.fallback_identity_required());
        let events = search_collect(
            "definitely-absent-search-needle",
            &[dir.path()],
            &ContentSearchOptions::default(),
        );

        assert_ends_with_done(&events);
        assert_eq!(count_matches(&events), 0);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SearchEvent::Incomplete(_)))
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
    fn child_before_parent_keeps_matching_results_in_configured_partition_order() {
        let dir = tempdir().expect("ordered overlap fixture");
        let parent = dir.path().to_path_buf();
        let child = parent.join("src");
        let child_file = child.join("main.rs");
        let sibling_file = parent.join("README.md");
        fixture::create_dir(&child);
        fixture::write_text(&child_file, "needle in child\n");
        fixture::write_text(&sibling_file, "needle in sibling\n");

        let events = search_collect(
            "needle",
            &[child.as_path(), parent.as_path()],
            &ContentSearchOptions::default(),
        );
        let matches = search_matches(&events);

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].path, child_file);
        assert_eq!(matches[1].path, sibling_file);
        assert_eq!(matches[0].traversal_root_index, 0);
        assert_eq!(matches[1].traversal_root_index, 1);
    }

    #[test]
    fn canonical_alias_roots_scan_one_file_once() {
        let dir = tempdir().expect("expected operation to succeed");
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        fixture::create_dir(&real);
        fixture::write_text(&real.join("main.rs"), "needle\n");
        fixture::symlink(&real, &alias);

        let events = search_collect(
            "needle",
            &[alias.as_path(), real.as_path()],
            &ContentSearchOptions::default(),
        );

        assert_ends_with_done(&events);
        assert_eq!(count_matches(&events), 1);
        assert!(
            !events
                .iter()
                .any(|event| matches!(event, SearchEvent::Incomplete(_)))
        );
    }

    #[test]
    fn unavailable_root_does_not_hide_results_from_an_available_root() {
        let dir = tempdir().expect("expected operation to succeed");
        let missing = dir.path().join("missing");
        let available = dir.path().join("available");
        fixture::create_dir(&available);
        fixture::write_text(&available.join("main.rs"), "needle\n");

        let events = search_collect(
            "needle",
            &[missing.as_path(), available.as_path()],
            &ContentSearchOptions::default(),
        );

        assert_ends_with_done(&events);
        assert_eq!(count_matches(&events), 1);
    }

    #[test]
    fn ambiguous_fallback_stops_before_one_over_entry_limit() {
        let first = tempdir().expect("expected operation to succeed");
        let second = tempdir().expect("expected operation to succeed");
        fixture::write_text(&first.path().join("first.rs"), "needle\n");
        fixture::write_text(&second.path().join("second.rs"), "needle\n");
        let plan = WorkspaceSearchTraversalPlan::build(
            [first.path().to_path_buf(), second.path().to_path_buf()],
            |_| Err::<PathBuf, _>(()),
        );
        assert!(plan.fallback_identity_required());

        let (tx, rx) = crossbeam_channel::unbounded();
        search_with_plan_and_limits(
            "needle",
            &plan,
            &ContentSearchOptions::default(),
            tx,
            Arc::new(AtomicBool::new(false)),
            SearchTelemetry {
                progress_counter: None,
                completion_flag: None,
            },
            WorkspaceSearchFallbackLimits {
                entries: 1,
                path_bytes: u64::MAX,
            },
        );
        let events: Vec<_> = rx.iter().collect();

        assert_eq!(count_matches(&events), 1);
        assert!(events.iter().any(|event| matches!(
            event,
            SearchEvent::Incomplete(
                crate::model::workspace_search::WorkspaceSearchIncompleteReason::FallbackEntryLimit
            )
        )));
        assert_ends_with_done(&events);
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

        let mut events = Vec::new();
        loop {
            let event = rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .expect("terminal events should drain after completion publication");
            let done = matches!(event, SearchEvent::Done);
            events.push(event);
            if done {
                break;
            }
        }
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
                SearchEvent::Progress(_) | SearchEvent::TraversalMetrics(_) => {}
                SearchEvent::Error(message) => panic!("search should not emit an error: {message}"),
                SearchEvent::Incomplete(reason) => {
                    panic!("fixture should not be incomplete: {reason:?}")
                }
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
