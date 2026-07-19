// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem watching for workspace sidebar refresh.
//!
//! This service stays GTK-free and normalizes backend watcher events into one
//! bounded, pollable mailbox that the sidebar adapter can consume on the main
//! thread. The UI layer decides how to refresh its tree models; this module owns
//! watch setup, path extraction, coalescing, and overflow promotion.

use notify_debouncer_full::notify::event::{AccessKind, ModifyKind, RenameMode};
use notify_debouncer_full::notify::{self, Event, EventKind, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};
use thiserror::Error;

/// Maximum unique changed paths retained before targeted work becomes a full refresh.
///
/// The same cap is enforced by the GTK refresh planner. It is intentionally
/// large enough to preserve precise handling for ordinary save and rename
/// bursts while bounding retained paths and per-poll work during bulk changes.
pub const WORKSPACE_WATCH_PATH_CAP: usize = 1_024;

/// Diagnostics are secondary to conservative refresh delivery and remain bounded too.
const WATCH_ERROR_MESSAGE_CAP_BYTES: usize = 1_024;

/// One materialized path to watch for sidebar refresh.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceWatchTarget {
    /// Absolute filesystem path that should produce refresh events.
    pub path: PathBuf,
    /// Whether changes under descendants should also be observed.
    pub recursive: bool,
}

impl WorkspaceWatchTarget {
    /// Create a non-recursive watch target for one materialized directory.
    ///
    /// The sidebar watches the directories it has actually loaded rather than
    /// recursively descending through every child of a broad configured folder.
    #[must_use]
    pub fn directory(path: PathBuf) -> Self {
        Self {
            path,
            recursive: false,
        }
    }

    /// Create a non-recursive watch target for a single file entry.
    #[must_use]
    pub fn file(path: PathBuf) -> Self {
        Self {
            path,
            recursive: false,
        }
    }

    fn recursive_mode(&self) -> RecursiveMode {
        if self.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        }
    }
}

/// Bounded tree refresh need accumulated since the previous poll.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorkspaceWatchChange {
    /// Unique affected paths that can actually change the rendered tree shape.
    ///
    /// Rename events contribute both the source and destination paths so the
    /// sidebar can refresh the old parent, the new parent, or both. Pure access
    /// noise such as `Access(Open(...))` is filtered out before reaching the UI.
    Paths(Vec<PathBuf>),
    /// Target precision was conservatively compressed after exceeding the cap.
    FullRefresh,
}

/// One already-normalized bounded notice taken from a live watcher mailbox.
///
/// Change, diagnostic, and disconnect state are independent so a failure cannot
/// overwrite a pending conservative refresh need.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceWatchNotice {
    /// Targeted paths, a conservative full refresh, or no tree change.
    pub change: Option<WorkspaceWatchChange>,
    /// Latest bounded backend diagnostic since the previous poll.
    pub error: Option<String>,
    /// Whether the backend callback was retired or disconnected.
    pub disconnected: bool,
}

impl WorkspaceWatchNotice {
    fn is_empty(&self) -> bool {
        self.change.is_none() && self.error.is_none() && !self.disconnected
    }
}

/// Bounded retained-state evidence used by deterministic tests and benchmarks.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct WorkspaceWatchMailboxSnapshot {
    /// Unique paths currently retained; always at most [`WORKSPACE_WATCH_PATH_CAP`].
    pub retained_paths: usize,
    /// Whether retained paths were promoted to one full refresh.
    pub full_refresh: bool,
    /// Whether one bounded diagnostic is retained.
    pub has_error: bool,
    /// Whether callback retirement/disconnection is retained.
    pub disconnected: bool,
    /// Whether the producer currently owns the mailbox lock.
    pub busy: bool,
}

#[derive(Debug, Default)]
enum PendingChange {
    #[default]
    Empty,
    Paths(BTreeSet<PathBuf>),
    FullRefresh,
}

#[derive(Debug, Default)]
struct WorkspaceWatchMailboxState {
    change: PendingChange,
    error: Option<String>,
    disconnected: bool,
}

impl WorkspaceWatchMailboxState {
    fn merge_change(&mut self, change: PendingChange) {
        match (&mut self.change, change) {
            (_, PendingChange::Empty) | (PendingChange::FullRefresh, _) => {}
            (current @ PendingChange::Empty, change) => *current = change,
            (current, PendingChange::FullRefresh) => *current = PendingChange::FullRefresh,
            (PendingChange::Paths(retained), PendingChange::Paths(paths)) => {
                for path in paths {
                    retained.insert(path);
                    if retained.len() > WORKSPACE_WATCH_PATH_CAP {
                        self.change = PendingChange::FullRefresh;
                        return;
                    }
                }
            }
        }
    }

    fn merge_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        if matches!(self.change, PendingChange::FullRefresh) {
            return;
        }

        if matches!(self.change, PendingChange::Empty) {
            self.change = PendingChange::Paths(BTreeSet::new());
        }
        let PendingChange::Paths(retained) = &mut self.change else {
            return;
        };
        for path in paths {
            retained.insert(path);
            if retained.len() > WORKSPACE_WATCH_PATH_CAP {
                self.change = PendingChange::FullRefresh;
                return;
            }
        }
    }

    fn merge_error(&mut self, message: String) {
        self.error = Some(bound_message(message));
    }

    fn mark_disconnected(&mut self) {
        self.disconnected = true;
    }

    fn into_notice(self) -> Option<WorkspaceWatchNotice> {
        let change = match self.change {
            PendingChange::Empty => None,
            PendingChange::Paths(paths) if paths.is_empty() => None,
            PendingChange::Paths(paths) => {
                Some(WorkspaceWatchChange::Paths(paths.into_iter().collect()))
            }
            PendingChange::FullRefresh => Some(WorkspaceWatchChange::FullRefresh),
        };
        let notice = WorkspaceWatchNotice {
            change,
            error: self.error,
            disconnected: self.disconnected,
        };
        (!notice.is_empty()).then_some(notice)
    }

    fn snapshot(&self) -> WorkspaceWatchMailboxSnapshot {
        let (retained_paths, full_refresh) = match &self.change {
            PendingChange::Empty => (0, false),
            PendingChange::Paths(paths) => (paths.len(), false),
            PendingChange::FullRefresh => (0, true),
        };
        WorkspaceWatchMailboxSnapshot {
            retained_paths,
            full_refresh,
            has_error: self.error.is_some(),
            disconnected: self.disconnected,
            busy: false,
        }
    }
}

/// One constant-notice mailbox shared only by a watcher handle and its callback.
#[derive(Debug, Default)]
pub struct WorkspaceWatchMailbox {
    state: Mutex<WorkspaceWatchMailboxState>,
    // Backend callbacks never wait for GTK or another producer. Losing path
    // precision under contention is represented by this constant-space latch.
    full_refresh_latched: AtomicBool,
    error_latched: AtomicBool,
    disconnected_latched: AtomicBool,
}

impl WorkspaceWatchMailbox {
    /// Create an empty mailbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn state(&self) -> MutexGuard<'_, WorkspaceWatchMailboxState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn try_state(&self) -> Option<MutexGuard<'_, WorkspaceWatchMailboxState>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    fn merge_change(&self, change: PendingChange) {
        if matches!(change, PendingChange::Empty) {
            return;
        }
        if matches!(change, PendingChange::FullRefresh) {
            self.full_refresh_latched.store(true, Ordering::Release);
            return;
        }
        let Some(mut state) = self.try_state() else {
            self.full_refresh_latched.store(true, Ordering::Release);
            return;
        };
        state.merge_change(change);
    }

    fn merge_backend_result(&self, result: notify::Result<Event>) {
        match result {
            Ok(event) => self.merge_change(normalize_raw_event(event)),
            Err(error) => self.merge_error(format_error(&error)),
        }
    }

    /// Feed one raw backend event through production filtering for benchmarks.
    #[doc(hidden)]
    pub fn merge_backend_result_for_benchmark(&self, result: notify::Result<Event>) {
        self.merge_backend_result(result);
    }

    /// Merge already-normalized paths for deterministic pressure benchmarks.
    #[doc(hidden)]
    pub fn merge_paths(&self, paths: impl IntoIterator<Item = PathBuf>) {
        let Some(mut state) = self.try_state() else {
            self.full_refresh_latched.store(true, Ordering::Release);
            return;
        };
        state.merge_paths(paths);
    }

    /// Retain only the latest bounded diagnostic.
    #[doc(hidden)]
    pub fn merge_error(&self, message: impl Into<String>) {
        let Some(mut state) = self.try_state() else {
            self.full_refresh_latched.store(true, Ordering::Release);
            self.error_latched.store(true, Ordering::Release);
            return;
        };
        state.merge_error(message.into());
    }

    /// Record callback retirement/disconnection without replacing pending changes.
    #[doc(hidden)]
    pub fn mark_disconnected(&self) {
        self.disconnected_latched.store(true, Ordering::Release);
    }

    /// Take at most one bounded notice and reset the retained state.
    #[must_use]
    pub fn take_notice(&self) -> Option<WorkspaceWatchNotice> {
        let pending = {
            let mut state = self.try_state()?;
            let mut pending = std::mem::take(&mut *state);
            drop(state);
            if self.full_refresh_latched.swap(false, Ordering::AcqRel) {
                pending.merge_change(PendingChange::FullRefresh);
            }
            if self.error_latched.swap(false, Ordering::AcqRel) && pending.error.is_none() {
                pending.merge_error(
                    "Workspace auto-refresh lost backend detail while the mailbox was busy"
                        .to_string(),
                );
            }
            if self.disconnected_latched.swap(false, Ordering::AcqRel) {
                pending.mark_disconnected();
            }
            pending
        };
        // Keep this local mutable binding so all atomic latch state is folded
        // into the same single notice before conversion.
        pending.into_notice()
    }

    /// Inspect only scalar retained-state evidence; no private paths are exposed.
    #[must_use]
    pub fn snapshot(&self) -> WorkspaceWatchMailboxSnapshot {
        let latched_full = self.full_refresh_latched.load(Ordering::Acquire);
        let latched_error = self.error_latched.load(Ordering::Acquire);
        let latched_disconnected = self.disconnected_latched.load(Ordering::Acquire);
        self.try_state().map_or_else(
            || WorkspaceWatchMailboxSnapshot {
                retained_paths: 0,
                full_refresh: latched_full,
                has_error: latched_error,
                disconnected: latched_disconnected,
                busy: true,
            },
            |state| {
                let mut snapshot = state.snapshot();
                snapshot.full_refresh |= latched_full;
                snapshot.has_error |= latched_error;
                snapshot.disconnected |= latched_disconnected;
                snapshot
            },
        )
    }
}

struct WorkspaceWatchCallback {
    mailbox: Arc<WorkspaceWatchMailbox>,
}

impl WorkspaceWatchCallback {
    fn handle(&self, result: notify::Result<Event>) {
        self.mailbox.merge_backend_result(result);
    }
}

impl Drop for WorkspaceWatchCallback {
    fn drop(&mut self) {
        self.mailbox.mark_disconnected();
    }
}

/// Startup failure while constructing a workspace watcher.
#[derive(Debug, Error)]
pub enum WorkspaceWatchError {
    /// Creating the debouncer itself failed.
    #[error("failed to create workspace watcher: {source}")]
    Create {
        #[source]
        source: notify::Error,
    },
    /// One specific target could not be watched.
    #[error("failed to watch {path}: {source}")]
    WatchTarget {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}

/// Live materialized-scope watcher for one workspace section.
///
/// Construction and destruction may touch slow platform resources, so the UI
/// transfers this handle only through background lifecycle work. Once installed,
/// the main thread takes one coalesced notice with `try_poll()` without blocking.
#[derive(Debug)]
pub struct WorkspaceWatcher {
    /// Keep the raw watcher alive for as long as the section wants updates.
    _watcher: notify::RecommendedWatcher,
    /// Bounded state shared only with this handle's backend callback.
    mailbox: Arc<WorkspaceWatchMailbox>,
    /// Number of targets handed to the backend watcher at startup.
    target_count: usize,
}

impl WorkspaceWatcher {
    /// Start watching the given targets with direct raw callback handling.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher backend cannot be created or any target
    /// cannot be registered with the watcher.
    pub fn start(targets: &[WorkspaceWatchTarget]) -> Result<Self, WorkspaceWatchError> {
        let mailbox = Arc::new(WorkspaceWatchMailbox::new());
        let callback = WorkspaceWatchCallback {
            mailbox: Arc::clone(&mailbox),
        };
        let mut watcher = notify::recommended_watcher(move |result| callback.handle(result))
            .map_err(|source| WorkspaceWatchError::Create { source })?;

        for target in targets {
            watcher
                .watch(&target.path, target.recursive_mode())
                .map_err(|source| WorkspaceWatchError::WatchTarget {
                    path: target.path.clone(),
                    source,
                })?;
        }

        Ok(Self {
            _watcher: watcher,
            mailbox,
            target_count: targets.len(),
        })
    }

    /// Take at most one already-normalized notice without blocking GTK.
    #[must_use]
    pub fn try_poll(&self) -> Option<WorkspaceWatchNotice> {
        self.mailbox.take_notice()
    }

    /// Count of actively watched targets, kept mainly so tests can sanity-check
    /// that a watcher was created even before the first filesystem event.
    #[must_use]
    pub fn watched_target_count(&self) -> usize {
        self.target_count
    }

    /// Inspect scalar pending state for GTK readiness without exposing paths.
    #[must_use]
    pub(crate) fn mailbox_snapshot(&self) -> WorkspaceWatchMailboxSnapshot {
        self.mailbox.snapshot()
    }

    /// Merge normalized paths into the installed mailbox for widget pressure tests.
    #[cfg(feature = "test-utils")]
    pub fn merge_paths_for_test(&self, paths: impl IntoIterator<Item = PathBuf>) {
        self.mailbox.merge_paths(paths);
    }

    /// Merge one diagnostic without suppressing pending changes in widget tests.
    #[cfg(feature = "test-utils")]
    pub fn merge_error_for_test(&self, message: &str) {
        self.mailbox.merge_error(message);
    }

    /// Mark the callback disconnected for widget lifecycle tests.
    #[cfg(feature = "test-utils")]
    pub fn mark_disconnected_for_test(&self) {
        self.mailbox.mark_disconnected();
    }

    /// Inspect scalar mailbox pressure without exposing retained filesystem paths.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn mailbox_snapshot_for_test(&self) -> WorkspaceWatchMailboxSnapshot {
        self.mailbox_snapshot()
    }
}

/// Normalize one raw backend event before it can reach GTK.
///
/// The sidebar tree only needs events that can change visible rows:
/// create/remove/rename. Access and data-only changes are intentionally ignored
/// to avoid refresh loops caused by ordinary directory scans or file reads.
fn normalize_raw_event(event: Event) -> PendingChange {
    match event.kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
            collect_paths(event.paths)
        }
        EventKind::Modify(ModifyKind::Name(_)) => PendingChange::FullRefresh,
        EventKind::Create(_) | EventKind::Remove(_) => collect_paths(event.paths),
        EventKind::Access(
            AccessKind::Open(_)
            | AccessKind::Close(_)
            | AccessKind::Read
            | AccessKind::Any
            | AccessKind::Other,
        )
        | EventKind::Modify(_)
        | EventKind::Any
        | EventKind::Other => PendingChange::Empty,
    }
}

fn collect_paths(raw_paths: Vec<PathBuf>) -> PendingChange {
    let mut paths = BTreeSet::new();
    for path in raw_paths {
        paths.insert(path);
        if paths.len() > WORKSPACE_WATCH_PATH_CAP {
            return PendingChange::FullRefresh;
        }
    }
    if paths.is_empty() {
        PendingChange::Empty
    } else {
        PendingChange::Paths(paths)
    }
}

/// Collapse one backend error batch into a short status-bar friendly message.
#[cfg(test)]
fn format_errors(errors: &[notify::Error]) -> String {
    let count = errors.len();
    let first = errors
        .first()
        .map_or_else(|| "Unknown watcher error".to_string(), ToString::to_string);
    let message = if count == 1 {
        format!("Workspace auto-refresh failed: {first}")
    } else {
        format!("Workspace auto-refresh failed ({count} errors): {first}")
    };
    bound_message(message)
}

fn format_error(error: &notify::Error) -> String {
    bound_message(format!("Workspace auto-refresh failed: {error}"))
}

fn bound_message(mut message: String) -> String {
    if message.len() <= WATCH_ERROR_MESSAGE_CAP_BYTES {
        return message;
    }
    let suffix = "…";
    let mut end = WATCH_ERROR_MESSAGE_CAP_BYTES.saturating_sub(suffix.len());
    while !message.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    message.truncate(end);
    message.push_str(suffix);
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use notify_debouncer_full::notify::ErrorKind;
    use notify_debouncer_full::notify::event::AccessMode;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    fn start_watcher_or_skip_on_resource_limit(
        targets: &[WorkspaceWatchTarget],
    ) -> Option<WorkspaceWatcher> {
        match WorkspaceWatcher::start(targets) {
            Ok(watcher) => Some(watcher),
            Err(error) if watcher_backend_resource_exhausted(&error) => {
                eprintln!("skipping workspace watcher integration test: {error}");
                None
            }
            Err(error) => panic!("expected operation to succeed: {error}"),
        }
    }

    fn watcher_backend_resource_exhausted(error: &WorkspaceWatchError) -> bool {
        match error {
            WorkspaceWatchError::Create { source }
            | WorkspaceWatchError::WatchTarget { source, .. } => notify_resource_exhausted(source),
        }
    }

    fn notify_resource_exhausted(error: &notify::Error) -> bool {
        match &error.kind {
            // Linux reports inotify instance exhaustion as EMFILE before notify
            // can attach a path-specific watch.
            ErrorKind::Io(error) => error.raw_os_error() == Some(24),
            ErrorKind::MaxFilesWatch => true,
            _ => false,
        }
    }

    fn wait_for_poll(
        watcher: &WorkspaceWatcher,
        timeout: Duration,
    ) -> Option<WorkspaceWatchNotice> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if let Some(notice) = watcher.try_poll() {
                return Some(notice);
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn paths_notice(paths: Vec<PathBuf>) -> WorkspaceWatchNotice {
        WorkspaceWatchNotice {
            change: Some(WorkspaceWatchChange::Paths(paths)),
            error: None,
            disconnected: false,
        }
    }

    #[test]
    fn empty_mailbox_has_no_notice() {
        let mailbox = WorkspaceWatchMailbox::new();
        assert_eq!(mailbox.take_notice(), None);
        assert_eq!(
            mailbox.snapshot(),
            WorkspaceWatchMailboxSnapshot {
                retained_paths: 0,
                full_refresh: false,
                has_error: false,
                disconnected: false,
                busy: false,
            }
        );
    }

    #[test]
    fn mailbox_deduplicates_and_orders_paths_across_batches() {
        let mailbox = WorkspaceWatchMailbox::new();
        let alpha = PathBuf::from("/tmp/alpha");
        let beta = PathBuf::from("/tmp/beta");
        mailbox.merge_paths([beta.clone(), alpha.clone(), beta]);

        assert_eq!(
            mailbox.take_notice(),
            Some(paths_notice(vec![alpha, PathBuf::from("/tmp/beta")]))
        );
        assert_eq!(mailbox.take_notice(), None);
    }

    #[test]
    fn mailbox_promotes_to_full_refresh_above_unique_path_cap() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths(
            (0..=WORKSPACE_WATCH_PATH_CAP).map(|index| PathBuf::from(format!("/tmp/{index}"))),
        );

        assert_eq!(
            mailbox.snapshot(),
            WorkspaceWatchMailboxSnapshot {
                retained_paths: 0,
                full_refresh: true,
                has_error: false,
                disconnected: false,
                busy: false,
            }
        );
        assert_eq!(
            mailbox.take_notice(),
            Some(WorkspaceWatchNotice {
                change: Some(WorkspaceWatchChange::FullRefresh),
                error: None,
                disconnected: false,
            })
        );
    }

    #[test]
    fn mailbox_keeps_targeted_refresh_at_exact_unique_path_cap() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths(
            (0..WORKSPACE_WATCH_PATH_CAP).map(|index| PathBuf::from(format!("/tmp/{index}"))),
        );

        let snapshot = mailbox.snapshot();
        assert_eq!(snapshot.retained_paths, WORKSPACE_WATCH_PATH_CAP);
        assert!(!snapshot.full_refresh);
        assert!(!snapshot.busy);
    }

    #[test]
    fn full_refresh_dominates_later_paths_until_take() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths(
            (0..=WORKSPACE_WATCH_PATH_CAP).map(|index| PathBuf::from(format!("/tmp/{index}"))),
        );
        mailbox.merge_paths([PathBuf::from("/tmp/later")]);

        assert!(mailbox.snapshot().full_refresh);
        assert_eq!(mailbox.snapshot().retained_paths, 0);
        assert!(matches!(
            mailbox.take_notice(),
            Some(WorkspaceWatchNotice {
                change: Some(WorkspaceWatchChange::FullRefresh),
                ..
            })
        ));
    }

    #[test]
    fn errors_and_disconnect_coexist_with_pending_changes_and_stay_bounded() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths([PathBuf::from("/tmp/change")]);
        mailbox.merge_error("first");
        for index in 0..100 {
            mailbox.merge_error(format!("latest-{index}"));
        }
        mailbox.mark_disconnected();

        let snapshot = mailbox.snapshot();
        assert_eq!(snapshot.retained_paths, 1);
        assert!(snapshot.has_error);
        assert!(snapshot.disconnected);
        assert_eq!(
            mailbox.take_notice(),
            Some(WorkspaceWatchNotice {
                change: Some(WorkspaceWatchChange::Paths(vec![PathBuf::from(
                    "/tmp/change"
                )])),
                error: Some("latest-99".to_string()),
                disconnected: true,
            })
        );
    }

    #[test]
    fn diagnostics_are_utf8_safe_and_byte_bounded() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_error("🙂".repeat(WATCH_ERROR_MESSAGE_CAP_BYTES));

        let error = mailbox
            .take_notice()
            .and_then(|notice| notice.error)
            .expect("bounded diagnostic");
        assert!(error.len() <= WATCH_ERROR_MESSAGE_CAP_BYTES);
        assert!(error.ends_with('…'));
    }

    #[test]
    fn events_arriving_after_take_form_the_next_notice() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths([PathBuf::from("/tmp/before")]);
        assert!(mailbox.take_notice().is_some());
        mailbox.merge_paths([PathBuf::from("/tmp/during")]);

        assert_eq!(
            mailbox.take_notice(),
            Some(paths_notice(vec![PathBuf::from("/tmp/during")]))
        );
    }

    #[test]
    fn gtk_reads_do_not_wait_for_a_busy_producer() {
        let mailbox = WorkspaceWatchMailbox::new();
        mailbox.merge_paths([PathBuf::from("/tmp/pending")]);
        let guard = mailbox.state();

        assert_eq!(mailbox.take_notice(), None);
        assert!(mailbox.snapshot().busy);
        drop(guard);

        assert_eq!(
            mailbox.take_notice(),
            Some(paths_notice(vec![PathBuf::from("/tmp/pending")]))
        );
    }

    #[test]
    fn raw_backend_storm_promotes_during_bounded_normalization() {
        let mailbox = WorkspaceWatchMailbox::new();
        for index in 0..=WORKSPACE_WATCH_PATH_CAP {
            mailbox.merge_backend_result(Ok(notify::Event::new(EventKind::Create(
                notify::event::CreateKind::File,
            ))
            .add_path(PathBuf::from(format!("/tmp/raw/{index}")))));
        }

        assert!(mailbox.snapshot().full_refresh);
    }

    #[test]
    fn producer_contention_latches_constant_space_full_refresh_and_diagnostics() {
        let mailbox = WorkspaceWatchMailbox::new();
        let guard = mailbox.state();

        mailbox.merge_paths([PathBuf::from("/tmp/lost-precision")]);
        mailbox.merge_error("backend detail");
        mailbox.mark_disconnected();

        assert_eq!(
            mailbox.snapshot(),
            WorkspaceWatchMailboxSnapshot {
                retained_paths: 0,
                full_refresh: true,
                has_error: true,
                disconnected: true,
                busy: true,
            }
        );
        drop(guard);

        let notice = mailbox.take_notice().expect("latched notice");
        assert_eq!(notice.change, Some(WorkspaceWatchChange::FullRefresh));
        assert!(notice.error.is_some());
        assert!(notice.disconnected);
        assert_eq!(mailbox.take_notice(), None);
    }

    #[test]
    fn concurrent_producers_and_consumer_keep_all_changes_or_full_refresh() {
        let mailbox = Arc::new(WorkspaceWatchMailbox::new());
        let producers =
            (0..4)
                .map(|producer| {
                    let mailbox = Arc::clone(&mailbox);
                    thread::spawn(move || {
                        mailbox.merge_paths((0..512).map(|index| {
                            PathBuf::from(format!("/tmp/producer-{producer}/{index}"))
                        }));
                    })
                })
                .collect::<Vec<_>>();
        for producer in producers {
            producer.join().expect("producer should not panic");
        }

        assert!(matches!(
            mailbox.take_notice(),
            Some(WorkspaceWatchNotice {
                change: Some(WorkspaceWatchChange::FullRefresh),
                ..
            })
        ));
        assert_eq!(mailbox.take_notice(), None);
    }

    #[test]
    fn producer_consumer_race_retains_every_path_or_conservative_full_refresh() {
        let mailbox = Arc::new(WorkspaceWatchMailbox::new());
        let producer_done = Arc::new(AtomicBool::new(false));
        let producer_mailbox = Arc::clone(&mailbox);
        let producer_done_flag = Arc::clone(&producer_done);
        let total = WORKSPACE_WATCH_PATH_CAP * 4;
        let producer = thread::spawn(move || {
            for index in 0..total {
                producer_mailbox.merge_paths([PathBuf::from(format!("/tmp/race/{index}"))]);
            }
            producer_done_flag.store(true, Ordering::Release);
        });

        let mut observed = BTreeSet::new();
        let mut saw_full_refresh = false;
        while !producer_done.load(Ordering::Acquire) {
            if let Some(notice) = mailbox.take_notice() {
                match notice.change {
                    Some(WorkspaceWatchChange::Paths(paths)) => observed.extend(paths),
                    Some(WorkspaceWatchChange::FullRefresh) => saw_full_refresh = true,
                    None => {}
                }
            }
            thread::yield_now();
        }
        producer.join().expect("producer should not panic");
        if let Some(notice) = mailbox.take_notice() {
            match notice.change {
                Some(WorkspaceWatchChange::Paths(paths)) => observed.extend(paths),
                Some(WorkspaceWatchChange::FullRefresh) => saw_full_refresh = true,
                None => {}
            }
        }

        assert!(
            saw_full_refresh || observed.len() == total,
            "a race may retain every precise path or conservatively compress the burst"
        );
        assert!(mailbox.snapshot().retained_paths <= WORKSPACE_WATCH_PATH_CAP);
    }

    #[test]
    fn retired_handle_mailbox_cannot_mutate_current_handle_mailbox() {
        let retired = WorkspaceWatchMailbox::new();
        let current = WorkspaceWatchMailbox::new();
        retired.merge_paths([PathBuf::from("/tmp/stale")]);
        retired.mark_disconnected();
        current.merge_paths([PathBuf::from("/tmp/current")]);

        assert_eq!(
            current.take_notice(),
            Some(paths_notice(vec![PathBuf::from("/tmp/current")]))
        );
        assert!(retired.snapshot().disconnected);
    }

    #[test]
    fn watching_directory_target_reports_created_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let Some(watcher) =
            start_watcher_or_skip_on_resource_limit(&[WorkspaceWatchTarget::directory(
                dir.path().to_path_buf(),
            )])
        else {
            return;
        };

        assert_eq!(watcher.watched_target_count(), 1);

        let created = dir.path().join("alpha.txt");
        fixture::write_text(&created, "alpha");

        let poll = wait_for_poll(&watcher, Duration::from_secs(5))
            .expect("directory watcher should report the created file");
        match poll.change {
            Some(WorkspaceWatchChange::Paths(paths)) => assert_eq!(paths, vec![created]),
            Some(WorkspaceWatchChange::FullRefresh) => {}
            other => panic!("expected targeted or full refresh change, got {other:?}"),
        }
    }

    #[test]
    fn watched_target_count_reports_all_registered_targets() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let one = dir.path().join("one");
        let two = dir.path().join("two");
        fixture::create_dir(&one);
        fixture::create_dir(&two);

        let Some(watcher) = start_watcher_or_skip_on_resource_limit(&[
            WorkspaceWatchTarget::directory(one),
            WorkspaceWatchTarget::directory(two),
        ]) else {
            return;
        };

        assert_eq!(watcher.watched_target_count(), 2);
    }

    #[test]
    fn watching_file_target_reports_file_rename() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("watched.txt");
        fixture::write_text(&file_path, "before");

        let Some(watcher) = start_watcher_or_skip_on_resource_limit(&[WorkspaceWatchTarget::file(
            file_path.clone(),
        )]) else {
            return;
        };

        assert_eq!(watcher.watched_target_count(), 1);

        let renamed_path = dir.path().join("renamed.txt");
        fixture::rename(&file_path, &renamed_path);

        let poll = wait_for_poll(&watcher, Duration::from_secs(5))
            .expect("file watcher should report file rename");
        match poll.change {
            Some(WorkspaceWatchChange::Paths(paths)) => {
                assert!(paths.contains(&file_path) || paths.contains(&renamed_path));
            }
            Some(WorkspaceWatchChange::FullRefresh) => {}
            other => panic!("expected path notice, got {other:?}"),
        }
    }

    #[test]
    fn starting_with_missing_target_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing = dir.path().join("missing");

        let error =
            match WorkspaceWatcher::start(&[WorkspaceWatchTarget::directory(missing.clone())]) {
                Ok(_) => panic!("missing targets should fail watcher startup"),
                Err(error) if watcher_backend_resource_exhausted(&error) => {
                    eprintln!("skipping workspace watcher integration test: {error}");
                    return;
                }
                Err(error) => error,
            };

        match error {
            WorkspaceWatchError::WatchTarget { path, .. } => {
                assert_eq!(path, missing);
            }
            other @ WorkspaceWatchError::Create { .. } => {
                panic!("expected WatchTarget error, got {other:?}");
            }
        }
    }

    #[test]
    fn error_batches_format_as_one_status_message() {
        let message = format_errors(&[
            notify::Error::generic("first"),
            notify::Error::generic("second"),
        ]);
        assert_eq!(message, "Workspace auto-refresh failed (2 errors): first");
    }

    #[test]
    fn access_events_are_filtered_out() {
        let path = PathBuf::from("/tmp/demo");
        let event =
            notify::Event::new(EventKind::Access(AccessKind::Open(AccessMode::Any))).add_path(path);

        assert!(matches!(normalize_raw_event(event), PendingChange::Empty));
    }

    #[test]
    fn rename_events_keep_both_paths() {
        let from = PathBuf::from("/tmp/from");
        let to = PathBuf::from("/tmp/to");
        let event = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(from.clone())
            .add_path(to.clone());

        assert!(matches!(
            normalize_raw_event(event),
            PendingChange::Paths(paths) if paths == BTreeSet::from([from, to])
        ));
    }

    #[test]
    fn ambiguous_rename_promotes_to_full_refresh() {
        let event = notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(PathBuf::from("/tmp/from"));

        assert!(matches!(
            normalize_raw_event(event),
            PendingChange::FullRefresh
        ));
    }
}
