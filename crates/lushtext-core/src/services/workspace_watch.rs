// SPDX-License-Identifier: GPL-3.0-or-later

//! Filesystem watching for workspace sidebar refresh.
//!
//! This service stays GTK-free and converts backend watcher events into a small,
//! pollable update stream that the sidebar adapter can consume on the main
//! thread. The UI layer decides how to refresh its tree models; this module only
//! owns watch setup and path extraction from debounced events.

use notify_debouncer_full::notify::event::{AccessKind, ModifyKind};
use notify_debouncer_full::notify::{self, EventKind, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use thiserror::Error;

/// Debounce window applied inside the watcher backend before the UI decides
/// whether to do a subtree refresh or rebuild a whole section.
const WATCH_DEBOUNCE_MS: u64 = 150;

/// Concrete debouncer type used by the recursive workspace watcher.
type WorkspaceDebouncer = Debouncer<notify::RecommendedWatcher, RecommendedCache>;

/// One root path to watch for sidebar refresh.
#[derive(Debug, Clone, Eq, PartialEq)]
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
    /// recursively descending through every child of a broad configured root.
    #[must_use]
    pub fn directory(path: PathBuf) -> Self {
        Self {
            path,
            recursive: false,
        }
    }

    /// Create a non-recursive watch target for a single file root.
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

/// Stable, UI-friendly summary of one watcher batch.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorkspaceWatchUpdate {
    /// Unique affected paths from one debounced batch that can actually change
    /// the rendered tree shape.
    ///
    /// Rename events contribute both the source and destination paths so the
    /// sidebar can refresh the old parent, the new parent, or both. Pure access
    /// noise such as `Access(Open(...))` is filtered out before reaching the UI.
    pub changed_paths: Vec<PathBuf>,
}

/// One non-blocking poll result from a live workspace watcher.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorkspaceWatchPoll {
    /// One or more filesystem paths changed and may need a sidebar refresh.
    Update(WorkspaceWatchUpdate),
    /// The watcher backend reported an error or disconnected.
    Error(String),
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
    /// One specific root could not be watched.
    #[error("failed to watch {path}: {source}")]
    WatchRoot {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}

/// Live recursive watcher for one workspace section.
///
/// The UI keeps this object on the main thread and polls it with `try_poll()`.
/// The backend watcher thread stays entirely inside the debouncer crate.
#[derive(Debug)]
pub struct WorkspaceWatcher {
    /// Keep the debouncer alive for as long as the section wants updates.
    _debouncer: WorkspaceDebouncer,
    /// Debounced backend events delivered from the watcher thread.
    receiver: Receiver<DebounceEventResult>,
    /// Number of roots handed to the backend watcher at startup.
    root_count: usize,
}

impl WorkspaceWatcher {
    /// Start watching the given roots with backend debouncing already enabled.
    ///
    /// # Errors
    ///
    /// Returns an error if the debouncer backend cannot be created or any root
    /// cannot be registered with the watcher.
    pub fn start(targets: &[WorkspaceWatchTarget]) -> Result<Self, WorkspaceWatchError> {
        let (sender, receiver) = mpsc::channel();
        let mut debouncer = new_debouncer(Duration::from_millis(WATCH_DEBOUNCE_MS), None, sender)
            .map_err(|source| WorkspaceWatchError::Create { source })?;

        for target in targets {
            debouncer
                .watch(&target.path, target.recursive_mode())
                .map_err(|source| WorkspaceWatchError::WatchRoot {
                    path: target.path.clone(),
                    source,
                })?;
        }

        Ok(Self {
            _debouncer: debouncer,
            receiver,
            root_count: targets.len(),
        })
    }

    /// Poll the next watcher batch without blocking the GTK main loop.
    #[must_use]
    pub fn try_poll(&self) -> Option<WorkspaceWatchPoll> {
        match self.receiver.try_recv() {
            Ok(Ok(events)) => {
                let changed_paths = collect_changed_paths(events);
                if changed_paths.is_empty() {
                    None
                } else {
                    Some(WorkspaceWatchPoll::Update(WorkspaceWatchUpdate {
                        changed_paths,
                    }))
                }
            }
            Ok(Err(errors)) => Some(WorkspaceWatchPoll::Error(format_errors(&errors))),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(WorkspaceWatchPoll::Error(
                "Workspace auto-refresh disconnected.".to_string(),
            )),
        }
    }

    /// Count of actively watched roots, kept mainly so tests can sanity-check
    /// that a watcher was created even before the first filesystem event.
    #[must_use]
    pub fn watched_root_count(&self) -> usize {
        self.root_count
    }
}

/// Extract a stable unique path set from one debounced backend batch.
///
/// The sidebar tree only needs events that can change visible rows:
/// create/remove/rename. Access and data-only changes are intentionally ignored
/// to avoid refresh loops caused by ordinary directory scans or file reads.
fn collect_changed_paths(events: Vec<notify_debouncer_full::DebouncedEvent>) -> Vec<PathBuf> {
    events
        .into_iter()
        .filter(|event| event_kind_changes_tree(event.event.kind))
        .flat_map(|event| event.event.paths.into_iter())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn event_kind_changes_tree(kind: EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)) => {
            true
        }
        EventKind::Access(
            AccessKind::Open(_)
            | AccessKind::Close(_)
            | AccessKind::Read
            | AccessKind::Any
            | AccessKind::Other,
        )
        | EventKind::Modify(_)
        | EventKind::Any
        | EventKind::Other => false,
    }
}

/// Collapse one backend error batch into a short status-bar friendly message.
fn format_errors(errors: &[notify::Error]) -> String {
    let count = errors.len();
    let first = errors
        .first()
        .map_or_else(|| "Unknown watcher error".to_string(), ToString::to_string);
    if count == 1 {
        format!("Workspace auto-refresh failed: {first}")
    } else {
        format!("Workspace auto-refresh failed ({count} errors): {first}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::filesystem::fixture;
    use notify_debouncer_full::notify::ErrorKind;
    use notify_debouncer_full::notify::event::{AccessMode, RenameMode};
    use std::thread;
    use std::time::Instant;
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
            | WorkspaceWatchError::WatchRoot { source, .. } => notify_resource_exhausted(source),
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

    fn wait_for_poll(watcher: &WorkspaceWatcher, timeout: Duration) -> Option<WorkspaceWatchPoll> {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if let Some(poll) = watcher.try_poll() {
                return Some(poll);
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn watching_directory_root_reports_created_file() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let Some(watcher) =
            start_watcher_or_skip_on_resource_limit(&[WorkspaceWatchTarget::directory(
                dir.path().to_path_buf(),
            )])
        else {
            return;
        };

        assert_eq!(watcher.watched_root_count(), 1);

        let created = dir.path().join("alpha.txt");
        fixture::write_text(&created, "alpha");

        let poll = wait_for_poll(&watcher, Duration::from_secs(5))
            .expect("directory watcher should report the created file");
        assert_eq!(
            poll,
            WorkspaceWatchPoll::Update(WorkspaceWatchUpdate {
                changed_paths: vec![created]
            })
        );
    }

    #[test]
    fn watched_root_count_reports_all_registered_roots() {
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

        assert_eq!(watcher.watched_root_count(), 2);
    }

    #[test]
    fn watching_file_root_reports_file_rename() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let file_path = dir.path().join("root.txt");
        fixture::write_text(&file_path, "before");

        let Some(watcher) = start_watcher_or_skip_on_resource_limit(&[WorkspaceWatchTarget::file(
            file_path.clone(),
        )]) else {
            return;
        };

        assert_eq!(watcher.watched_root_count(), 1);

        let renamed_path = dir.path().join("renamed.txt");
        fixture::rename(&file_path, &renamed_path);

        let poll = wait_for_poll(&watcher, Duration::from_secs(5))
            .expect("file watcher should report file rename");
        match poll {
            WorkspaceWatchPoll::Update(update) => {
                assert!(
                    update.changed_paths.contains(&file_path)
                        || update.changed_paths.contains(&renamed_path)
                );
            }
            other @ WorkspaceWatchPoll::Error(_) => {
                panic!("expected update poll, got {other:?}");
            }
        }
    }

    #[test]
    fn starting_with_missing_root_returns_error() {
        let dir = TempDir::new().expect("expected operation to succeed");
        let missing = dir.path().join("missing");

        let error =
            match WorkspaceWatcher::start(&[WorkspaceWatchTarget::directory(missing.clone())]) {
                Ok(_) => panic!("missing roots should fail watcher startup"),
                Err(error) if watcher_backend_resource_exhausted(&error) => {
                    eprintln!("skipping workspace watcher integration test: {error}");
                    return;
                }
                Err(error) => error,
            };

        match error {
            WorkspaceWatchError::WatchRoot { path, .. } => {
                assert_eq!(path, missing);
            }
            other @ WorkspaceWatchError::Create { .. } => {
                panic!("expected WatchRoot error, got {other:?}");
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
        let events = vec![notify_debouncer_full::DebouncedEvent::new(
            notify::Event::new(EventKind::Access(AccessKind::Open(AccessMode::Any))).add_path(path),
            Instant::now(),
        )];

        assert!(collect_changed_paths(events).is_empty());
    }

    #[test]
    fn rename_events_keep_both_paths() {
        let from = PathBuf::from("/tmp/from");
        let to = PathBuf::from("/tmp/to");
        let events = vec![notify_debouncer_full::DebouncedEvent::new(
            notify::Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path(from.clone())
                .add_path(to.clone()),
            Instant::now(),
        )];

        assert_eq!(collect_changed_paths(events), vec![from, to]);
    }
}
