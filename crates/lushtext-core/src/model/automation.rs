// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded automation snapshot and readiness value objects.
//!
//! These model types intentionally avoid GTK dependencies. UI adapters fill
//! them from live widgets, while documentation, smoke helpers, and D-Bus code
//! can serialize the same contract without learning about widget internals.

use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

/// Maximum number of workflow events retained for D-Bus readers.
///
/// Events are diagnostic breadcrumbs, not an audit log. Keeping the most recent
/// 128 transitions is enough for smoke scenarios while bounding memory and
/// serialized JSON size for long-running editor sessions.
pub const AUTOMATION_WORKFLOW_EVENT_LIMIT: usize = 128;

/// Stable workflow ID for editor file-load activity.
pub const AUTOMATION_WORKFLOW_FILE_LOAD: &str = "file-load";
/// Stable workflow ID for editor save activity.
pub const AUTOMATION_WORKFLOW_SAVE: &str = "save";
/// Stable workflow ID for in-document search counting.
pub const AUTOMATION_WORKFLOW_SEARCH: &str = "search";
/// Stable workflow ID for workspace refresh, persistence, and index debounce.
pub const AUTOMATION_WORKFLOW_WORKSPACE_REFRESH: &str = "workspace-refresh";
/// Stable workflow ID for workspace-wide content search.
pub const AUTOMATION_WORKFLOW_CONTENT_SEARCH: &str = "content-search";
/// Stable workflow ID for Replace All preview generation.
pub const AUTOMATION_WORKFLOW_REPLACE_PREVIEW: &str = "replace-preview";
/// Stable workflow ID for startup session restore.
pub const AUTOMATION_WORKFLOW_SESSION_RESTORE: &str = "session-restore";

/// One live workflow observation supplied by the UI automation adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutomationWorkflowObservation {
    /// Stable workflow ID.
    pub workflow_id: &'static str,
    /// Whether the workflow is currently active.
    pub active: bool,
    /// Stable readiness blocker that explains the active state, when available.
    pub blocker: Option<&'static str>,
}

impl AutomationWorkflowObservation {
    /// Build one workflow observation from already-mounted UI state.
    #[must_use]
    pub const fn new(
        workflow_id: &'static str,
        active: bool,
        blocker: Option<&'static str>,
    ) -> Self {
        Self {
            workflow_id,
            active,
            blocker,
        }
    }
}

/// One bounded workflow state-change event exposed through automation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationWorkflowEvent {
    /// Monotonic per-process event sequence.
    pub sequence: u64,
    /// Stable workflow ID.
    pub workflow_id: String,
    /// Event phase: `started` or `finished`.
    ///
    /// Phase names the transition edge. `status` below names the resulting
    /// stable state so future phases can still map to agent-friendly states.
    pub phase: &'static str,
    /// Bounded status after this transition: `running` or `settled`.
    pub status: &'static str,
    /// Human-readable summary for smoke artifacts.
    pub summary: String,
    /// Stable readiness blocker associated with the transition, if known.
    pub blocker: Option<String>,
}

/// Bounded event list returned by the automation D-Bus surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationWorkflowEventsSnapshot {
    /// Highest sequence number emitted so far, or `0` before any event exists.
    pub last_sequence: u64,
    /// Whether older events were ever dropped from the bounded list.
    ///
    /// This stays true after the first cap so clients can treat gaps before the
    /// first retained sequence as expected in long-running sessions.
    pub capped: bool,
    /// Recent workflow state-change events in sequence order.
    pub events: Vec<AutomationWorkflowEvent>,
}

/// Small in-memory recorder for workflow start/finish transitions.
///
/// This is a pull-derived detector: callers repeatedly pass the latest
/// workflow observations, and the log records transitions between observed
/// states. If a workflow stops being reported altogether, the previous state is
/// preserved rather than guessed; callers must report `active=false` to finish it.
#[derive(Debug)]
pub struct AutomationWorkflowEventLog {
    /// Next monotonic sequence to assign; starts at 1 so 0 is the empty sentinel.
    next_sequence: u64,
    /// Last observed active state per workflow so repeated polls emit only transitions.
    active_by_workflow: BTreeMap<&'static str, bool>,
    /// Recent transition events retained in FIFO order.
    events: VecDeque<AutomationWorkflowEvent>,
    /// Sticky marker set after the first event is dropped from retention.
    capped: bool,
}

impl Default for AutomationWorkflowEventLog {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            active_by_workflow: BTreeMap::new(),
            events: VecDeque::new(),
            capped: false,
        }
    }
}

impl AutomationWorkflowEventLog {
    /// Observe the latest workflow states and record start/finish transitions.
    ///
    /// The observations represent the complete current state for the workflows
    /// the caller wants to track in this pass. Repeating the same active state
    /// does not emit duplicate events.
    pub fn observe(
        &mut self,
        observations: impl IntoIterator<Item = AutomationWorkflowObservation>,
    ) {
        for observation in observations {
            let was_active = self
                .active_by_workflow
                .insert(observation.workflow_id, observation.active)
                .unwrap_or(false);
            match (was_active, observation.active) {
                (false, true) => self.push_event(
                    observation.workflow_id,
                    "started",
                    "running",
                    format!("{} started", observation.workflow_id),
                    observation.blocker,
                ),
                (true, false) => self.push_event(
                    observation.workflow_id,
                    "finished",
                    "settled",
                    format!("{} finished", observation.workflow_id),
                    observation.blocker,
                ),
                _ => {}
            }
        }
    }

    /// Return the current bounded event snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AutomationWorkflowEventsSnapshot {
        AutomationWorkflowEventsSnapshot {
            last_sequence: self.next_sequence.saturating_sub(1),
            capped: self.capped,
            events: self.events.iter().cloned().collect(),
        }
    }

    /// Append one event and enforce the retention cap immediately.
    fn push_event(
        &mut self,
        workflow_id: &'static str,
        phase: &'static str,
        status: &'static str,
        summary: String,
        blocker: Option<&'static str>,
    ) {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.events.push_back(AutomationWorkflowEvent {
            sequence,
            workflow_id: workflow_id.to_string(),
            phase,
            status,
            summary,
            blocker: blocker.map(ToOwned::to_owned),
        });
        while self.events.len() > AUTOMATION_WORKFLOW_EVENT_LIMIT {
            self.events.pop_front();
            self.capped = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_event_log_records_start_and_finish_transitions() {
        let mut log = AutomationWorkflowEventLog::default();

        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            true,
            Some(READINESS_BLOCKER_FILE_LOAD),
        )]);
        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            true,
            Some(READINESS_BLOCKER_FILE_LOAD),
        )]);
        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            false,
            None,
        )]);

        let snapshot = log.snapshot();
        assert_eq!(snapshot.last_sequence, 2);
        assert!(!snapshot.capped);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].sequence, 1);
        assert_eq!(snapshot.events[0].phase, "started");
        assert_eq!(snapshot.events[0].status, "running");
        assert_eq!(
            snapshot.events[0].blocker.as_deref(),
            Some(READINESS_BLOCKER_FILE_LOAD)
        );
        assert_eq!(snapshot.events[1].phase, "finished");
        assert_eq!(snapshot.events[1].status, "settled");
    }

    #[test]
    fn workflow_event_log_uses_zero_as_empty_sequence_sentinel() {
        let mut log = AutomationWorkflowEventLog::default();

        assert_eq!(log.snapshot().last_sequence, 0);
        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_SAVE,
            true,
            Some(READINESS_BLOCKER_SAVE),
        )]);

        let snapshot = log.snapshot();
        assert_eq!(snapshot.last_sequence, 1);
        assert_eq!(snapshot.events[0].sequence, 1);
    }

    #[test]
    fn workflow_event_log_caps_old_events() {
        let mut log = AutomationWorkflowEventLog::default();

        for _ in 0..=AUTOMATION_WORKFLOW_EVENT_LIMIT {
            log.observe([AutomationWorkflowObservation::new(
                AUTOMATION_WORKFLOW_SAVE,
                true,
                Some(READINESS_BLOCKER_SAVE),
            )]);
            log.observe([AutomationWorkflowObservation::new(
                AUTOMATION_WORKFLOW_SAVE,
                false,
                None,
            )]);
        }

        let snapshot = log.snapshot();
        assert!(snapshot.capped);
        assert_eq!(snapshot.events.len(), AUTOMATION_WORKFLOW_EVENT_LIMIT);
        assert_eq!(
            snapshot.events.first().map(|event| event.sequence),
            Some(snapshot.last_sequence + 1 - AUTOMATION_WORKFLOW_EVENT_LIMIT as u64)
        );
    }

    #[test]
    fn workflow_event_log_keeps_state_until_inactive_observed() {
        let mut log = AutomationWorkflowEventLog::default();

        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            true,
            Some(READINESS_BLOCKER_FILE_LOAD),
        )]);
        log.observe([]);

        let snapshot = log.snapshot();
        assert_eq!(snapshot.last_sequence, 1);
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].phase, "started");

        log.observe([AutomationWorkflowObservation::new(
            AUTOMATION_WORKFLOW_FILE_LOAD,
            false,
            None,
        )]);

        let snapshot = log.snapshot();
        assert_eq!(snapshot.last_sequence, 2);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[1].phase, "finished");
        assert_eq!(snapshot.events[1].status, "settled");
    }

    #[test]
    fn automation_snapshot_serializes_disabled_no_window_state() {
        let snapshot = AutomationSnapshot {
            interface_version: 1,
            enabled: false,
            app_id: "dev.cominotti.lushtext".to_string(),
            app_version: "0.0.0-test".to_string(),
            build_profile: "test".to_string(),
            idle: false,
            idle_blocker: Some(READINESS_BLOCKER_APP_STARTUP.to_string()),
            window: None,
        };

        let value = serde_json::to_value(snapshot).expect("snapshot should serialize");

        assert_eq!(value["interface_version"], 1);
        assert_eq!(value["enabled"], false);
        assert_eq!(value["app_id"], "dev.cominotti.lushtext");
        assert_eq!(value["app_version"], "0.0.0-test");
        assert_eq!(value["build_profile"], "test");
        assert_eq!(value["idle"], false);
        assert_eq!(value["idle_blocker"], READINESS_BLOCKER_APP_STARTUP);
        assert!(value["window"].is_null());
    }

    #[test]
    fn tab_snapshot_serializes_redacted_file_and_draft_metadata_only() {
        let tab = AutomationTabSnapshot {
            index: 0,
            active: true,
            title: "Draft tab".to_string(),
            document_kind: "file".to_string(),
            path: Some("/tmp/lushtext-automation.txt".to_string()),
            modified: true,
            saving: false,
            load_state: "loaded".to_string(),
            file_size: Some(42),
            draft_present: true,
            evicted: false,
            pinned: true,
        };

        let value = serde_json::to_value(tab).expect("tab snapshot should serialize");
        let fields = value
            .as_object()
            .expect("tab snapshot should serialize as an object");

        assert_eq!(fields["draft_present"], true);
        assert_eq!(fields["path"], "/tmp/lushtext-automation.txt");
        assert_eq!(fields["file_size"], 42);
        assert!(!fields.contains_key("draft_id"));
        assert!(!fields.contains_key("document_text"));
        assert!(!fields.contains_key("content"));
        assert!(!fields.contains_key("bookmark_ids"));
        assert!(!fields.contains_key("local_history_snapshots"));
    }

    #[test]
    fn workflow_events_snapshot_serializes_stable_contract() {
        let snapshot = AutomationWorkflowEventsSnapshot {
            last_sequence: 7,
            capped: true,
            events: vec![AutomationWorkflowEvent {
                sequence: 7,
                workflow_id: AUTOMATION_WORKFLOW_SAVE.to_string(),
                phase: "finished",
                status: "settled",
                summary: "save finished".to_string(),
                blocker: Some(READINESS_BLOCKER_SAVE.to_string()),
            }],
        };

        let value =
            serde_json::to_value(snapshot).expect("workflow event snapshot should serialize");

        assert_eq!(value["last_sequence"], 7);
        assert_eq!(value["capped"], true);
        assert_eq!(value["events"][0]["sequence"], 7);
        assert_eq!(value["events"][0]["workflow_id"], AUTOMATION_WORKFLOW_SAVE);
        assert_eq!(value["events"][0]["phase"], "finished");
        assert_eq!(value["events"][0]["status"], "settled");
        assert_eq!(value["events"][0]["summary"], "save finished");
        assert_eq!(value["events"][0]["blocker"], READINESS_BLOCKER_SAVE);
    }
}

/// Stable serialized blocker ID for startup before an active window exists.
pub const READINESS_BLOCKER_APP_STARTUP: &str = "app-startup";
/// Stable serialized blocker ID for close or quit safety flows.
pub const READINESS_BLOCKER_CLOSE_SAFETY: &str = "close-safety";
/// Stable serialized blocker ID for command-palette file-index debounce work.
pub const READINESS_BLOCKER_COMMAND_PALETTE_INDEX: &str = "command-palette-index";
/// Stable serialized blocker ID for draft autosaves.
pub const READINESS_BLOCKER_DRAFT_AUTOSAVE: &str = "draft-autosave";
/// Stable serialized blocker ID for in-document search occurrence counting.
pub const READINESS_BLOCKER_EDITOR_SEARCH: &str = "editor-search";
/// Stable serialized blocker ID for editor file loading.
pub const READINESS_BLOCKER_FILE_LOAD: &str = "file-load";
/// Stable serialized blocker ID for Markdown preview layout animation.
pub const READINESS_BLOCKER_PREVIEW_ANIMATION: &str = "preview-animation";
/// Stable serialized blocker ID for Replace All preview generation.
pub const READINESS_BLOCKER_REPLACE_PREVIEW: &str = "replace-preview";
/// Stable serialized blocker ID for editor saves.
pub const READINESS_BLOCKER_SAVE: &str = "save";
/// Stable serialized blocker ID for startup session and draft restoration.
pub const READINESS_BLOCKER_SESSION_RESTORE: &str = "session-restore";
/// Stable serialized blocker ID for workspace state persistence.
pub const READINESS_BLOCKER_WORKSPACE_PERSIST: &str = "workspace-persist";
/// Stable serialized blocker ID for workspace filter layout animation.
pub const READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION: &str = "workspace-filter-animation";
/// Stable serialized blocker ID for workspace-wide content search.
pub const READINESS_BLOCKER_WORKSPACE_SEARCH: &str = "workspace-search";

/// Blocker universe for the broad `idle` predicate.
///
/// The order is diagnostic priority: app-wide startup/session/close work reports
/// before narrower editor/search blockers so timeout details point at the root gate.
const IDLE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_SESSION_RESTORE,
    READINESS_BLOCKER_CLOSE_SAFETY,
    READINESS_BLOCKER_DRAFT_AUTOSAVE,
    READINESS_BLOCKER_PREVIEW_ANIMATION,
    READINESS_BLOCKER_WORKSPACE_SEARCH,
    READINESS_BLOCKER_COMMAND_PALETTE_INDEX,
    READINESS_BLOCKER_REPLACE_PREVIEW,
    READINESS_BLOCKER_WORKSPACE_PERSIST,
    READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION,
    READINESS_BLOCKER_FILE_LOAD,
    READINESS_BLOCKER_SAVE,
    READINESS_BLOCKER_EDITOR_SEARCH,
];
/// Work that must settle before automation treats startup as complete.
///
/// This includes restore/load follow-up and startup-triggered workspace/index work,
/// but excludes user-triggered search/save workflows.
const APP_STARTUP_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_SESSION_RESTORE,
    READINESS_BLOCKER_FILE_LOAD,
    READINESS_BLOCKER_DRAFT_AUTOSAVE,
    READINESS_BLOCKER_COMMAND_PALETTE_INDEX,
    READINESS_BLOCKER_WORKSPACE_PERSIST,
    READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION,
];
/// Minimal gate for querying the active window's `org.gtk.Actions` group.
const WINDOW_ACTIONS_EXPORTED_BLOCKERS: &[&str] = &[READINESS_BLOCKER_APP_STARTUP];
/// Load-state blockers for a file-open workflow.
const FILE_OPEN_COMPLETE_BLOCKERS: &[&str] =
    &[READINESS_BLOCKER_APP_STARTUP, READINESS_BLOCKER_FILE_LOAD];
/// Search-related blockers that can affect visible result counts or previews.
const SEARCH_COMPLETE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_EDITOR_SEARCH,
    READINESS_BLOCKER_WORKSPACE_SEARCH,
    READINESS_BLOCKER_REPLACE_PREVIEW,
];
/// Save-related blockers that can affect durable write or close-safety outcomes.
const SAVE_COMPLETE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_SAVE,
    READINESS_BLOCKER_CLOSE_SAFETY,
    READINESS_BLOCKER_DRAFT_AUTOSAVE,
];
/// Workspace blockers that affect visible scope state or command-palette file sources.
const WORKSPACE_REFRESH_COMPLETE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_WORKSPACE_PERSIST,
    READINESS_BLOCKER_WORKSPACE_FILTER_ANIMATION,
    READINESS_BLOCKER_COMMAND_PALETTE_INDEX,
];
/// Startup session blockers that can add tabs or restore draft-backed state.
const SESSION_RESTORE_COMPLETE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_SESSION_RESTORE,
    READINESS_BLOCKER_FILE_LOAD,
    READINESS_BLOCKER_DRAFT_AUTOSAVE,
];
/// Recovery restore blockers that can leave visible tabs, workspace state, or indexes stale.
const RECOVERY_RESTORE_COMPLETE_BLOCKERS: &[&str] = &[
    READINESS_BLOCKER_APP_STARTUP,
    READINESS_BLOCKER_SESSION_RESTORE,
    READINESS_BLOCKER_FILE_LOAD,
    READINESS_BLOCKER_DRAFT_AUTOSAVE,
    READINESS_BLOCKER_WORKSPACE_PERSIST,
    READINESS_BLOCKER_COMMAND_PALETTE_INDEX,
];

/// Named readiness predicates exposed through the automation D-Bus surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomationReadinessPredicate {
    /// The application has an active window and startup-owned restore work settled.
    AppStartup,
    /// The active window exists; smoke helpers still probe `org.gtk.Actions` externally.
    WindowActionsExported,
    /// File-backed editor loads have finished or reported a failure.
    FileOpenComplete,
    /// In-document search, workspace search, and Replace All preview work settled.
    SearchComplete,
    /// Save, close-safety, and draft autosave work settled.
    SaveComplete,
    /// Workspace persistence, filter animation, and index refresh debounce settled.
    WorkspaceRefreshComplete,
    /// Startup session restore and its immediate file/draft follow-up work settled.
    SessionRestoreComplete,
    /// Startup recovery restore and immediate post-restore indexing/persistence settled.
    RecoveryRestoreComplete,
    /// Every app-owned blocker known to Automation1 has settled.
    Idle,
}

impl AutomationReadinessPredicate {
    /// Stable predicate list returned to agents.
    ///
    /// Keep this order append-only so generated docs, D-Bus artifacts, and smoke
    /// outputs diff predictably across releases.
    pub const ALL: [Self; 9] = [
        Self::AppStartup,
        Self::WindowActionsExported,
        Self::FileOpenComplete,
        Self::SearchComplete,
        Self::SaveComplete,
        Self::WorkspaceRefreshComplete,
        Self::SessionRestoreComplete,
        Self::RecoveryRestoreComplete,
        Self::Idle,
    ];

    /// Parse the D-Bus predicate name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|predicate| predicate.as_str() == name)
    }

    /// Stable D-Bus and documentation name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppStartup => "app-startup",
            Self::WindowActionsExported => "window-actions-exported",
            Self::FileOpenComplete => "file-open-complete",
            Self::SearchComplete => "search-complete",
            Self::SaveComplete => "save-complete",
            Self::WorkspaceRefreshComplete => "workspace-refresh-complete",
            Self::SessionRestoreComplete => "session-restore-complete",
            Self::RecoveryRestoreComplete => "recovery-restore-complete",
            Self::Idle => "idle",
        }
    }

    /// Stable documentation anchor in `docs/automation-reference.md`.
    #[must_use]
    pub fn anchor(self) -> String {
        format!("readiness-predicate-{}", self.as_str())
    }

    /// Human-facing predicate meaning.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::AppStartup => {
                "Application startup has produced an active window and settled startup-owned restore work."
            }
            Self::WindowActionsExported => {
                "The active window exists; smoke helpers still probe its org.gtk.Actions object externally."
            }
            Self::FileOpenComplete => "File-backed editor tabs are no longer loading.",
            Self::SearchComplete => {
                "Editor search, workspace search, and Replace All preview work are no longer pending."
            }
            Self::SaveComplete => {
                "Editor saves, close-safety checks, and draft autosaves are no longer pending."
            }
            Self::WorkspaceRefreshComplete => {
                "Workspace persistence, scope filter animation, and command-palette index debounce are settled."
            }
            Self::SessionRestoreComplete => {
                "Session restore and immediate file/draft follow-up work are settled."
            }
            Self::RecoveryRestoreComplete => {
                "Startup recovery restore and immediate post-restore indexing or persistence work are settled."
            }
            Self::Idle => "Every tracked app-owned readiness blocker is settled.",
        }
    }

    /// Blockers that keep this predicate from reporting ready.
    #[must_use]
    pub const fn blockers(self) -> &'static [&'static str] {
        match self {
            Self::AppStartup => APP_STARTUP_BLOCKERS,
            Self::WindowActionsExported => WINDOW_ACTIONS_EXPORTED_BLOCKERS,
            Self::FileOpenComplete => FILE_OPEN_COMPLETE_BLOCKERS,
            Self::SearchComplete => SEARCH_COMPLETE_BLOCKERS,
            Self::SaveComplete => SAVE_COMPLETE_BLOCKERS,
            Self::WorkspaceRefreshComplete => WORKSPACE_REFRESH_COMPLETE_BLOCKERS,
            Self::SessionRestoreComplete => SESSION_RESTORE_COMPLETE_BLOCKERS,
            Self::RecoveryRestoreComplete => RECOVERY_RESTORE_COMPLETE_BLOCKERS,
            Self::Idle => IDLE_BLOCKERS,
        }
    }

    /// Whether the named blocker applies to this predicate.
    #[must_use]
    pub fn includes_blocker(self, blocker: &str) -> bool {
        self.blockers().contains(&blocker)
    }

    /// Serializable rows exposed to developer tools and documentation checks.
    #[must_use]
    pub fn reference_rows() -> Vec<AutomationReadinessPredicateReference> {
        Self::ALL
            .into_iter()
            .map(AutomationReadinessPredicateReference::from)
            .collect()
    }
}

/// Stable readiness statuses shared by app waits and host-side smoke helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutomationReadinessStatus {
    /// The requested predicate is satisfied.
    Ready,
    /// The predicate remained blocked until the timeout expired.
    PredicateTimeout,
    /// The workflow reached a failed state instead of the requested ready state.
    WorkflowFailure,
    /// The automation object or expected application/action state was unavailable.
    AutomationUnavailable,
    /// A host-side tool needed before or around a smoke run is missing or unsupported.
    UnsupportedHostTooling,
    /// The caller requested a predicate unknown to this interface version.
    UnknownPredicate,
}

impl AutomationReadinessStatus {
    /// Stable D-Bus and documentation name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::PredicateTimeout => "predicate-timeout",
            Self::WorkflowFailure => "workflow-failure",
            Self::AutomationUnavailable => "automation-unavailable",
            Self::UnsupportedHostTooling => "unsupported-host-tooling",
            Self::UnknownPredicate => "unknown-predicate",
        }
    }
}

/// One readiness predicate row returned by `GetReadinessPredicates`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationReadinessPredicateReference {
    /// Stable D-Bus predicate name.
    pub predicate: &'static str,
    /// Stable documentation anchor.
    pub anchor: String,
    /// Human-facing meaning.
    pub description: &'static str,
    /// Blocker names that must be absent for the predicate to become ready.
    pub blockers: Vec<&'static str>,
    /// Stability level for agents and tools.
    pub stability: &'static str,
}

impl From<AutomationReadinessPredicate> for AutomationReadinessPredicateReference {
    fn from(predicate: AutomationReadinessPredicate) -> Self {
        Self {
            predicate: predicate.as_str(),
            anchor: predicate.anchor(),
            description: predicate.description(),
            blockers: predicate.blockers().to_vec(),
            stability: "stable",
        }
    }
}

/// Result shape used internally by the readiness D-Bus adapter and tests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationReadinessResult {
    /// Predicate requested by the caller.
    pub predicate: String,
    /// Whether the predicate reached readiness.
    pub ok: bool,
    /// Stable readiness status.
    pub status: &'static str,
    /// Human-readable, bounded diagnostic detail.
    pub detail: String,
    /// First active blocker, if the wait ended before readiness.
    pub blocker: Option<String>,
}

/// Complete read-only state returned by the automation D-Bus surface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationSnapshot {
    /// Version of the app-owned automation interface.
    pub interface_version: u32,
    /// Whether the automation surface is active in this process.
    pub enabled: bool,
    /// Application ID that owns the D-Bus name.
    pub app_id: String,
    /// LushText build version.
    pub app_version: String,
    /// Build profile used for diagnostics, usually `debug` or `release`.
    pub build_profile: String,
    /// Whether LushText currently has no tracked foreground/background workflow blocker.
    pub idle: bool,
    /// First tracked workflow blocker, if the app is not idle.
    pub idle_blocker: Option<String>,
    /// Current active-window snapshot, if a LushText window exists.
    pub window: Option<AutomationWindowSnapshot>,
}

/// Bounded state for one application window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationWindowSnapshot {
    /// Number of open editor tabs.
    pub tab_count: u32,
    /// Index of the selected tab, if any.
    pub active_tab_index: Option<u32>,
    /// Metadata for every tab, capped to non-content fields.
    pub tabs: Vec<AutomationTabSnapshot>,
    /// Shell and secondary-surface state.
    pub surfaces: AutomationSurfaceSnapshot,
    /// In-document and workspace-search state.
    pub search: AutomationSearchSnapshot,
    /// Workspace configuration and current scope state.
    pub workspace: AutomationWorkspaceSnapshot,
    /// Command palette query, mode, and index summary.
    pub command_palette: AutomationCommandPaletteSnapshot,
    /// Notes and bookmarks state that is already live in the window.
    pub notes: AutomationNotesSnapshot,
    /// Local-history state that can be answered without reading snapshot files.
    pub local_history: AutomationLocalHistorySnapshot,
    /// Workspace content-search and Replace All summary.
    pub content_search: AutomationContentSearchSnapshot,
    /// Current notification summary for status and progress assertions.
    pub notifications: AutomationNotificationSnapshot,
}

/// Non-content metadata for one tab.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationTabSnapshot {
    /// Zero-based tab index in the visible tab model.
    pub index: u32,
    /// Whether this tab is currently selected.
    pub active: bool,
    /// Display title shown for the tab.
    pub title: String,
    /// `file` for saved-file tabs, `untitled` for draft-backed tabs.
    pub document_kind: String,
    /// Absolute or user-visible path for file-backed tabs.
    pub path: Option<String>,
    /// Whether the buffer has unsaved edits.
    pub modified: bool,
    /// Whether a save is currently in flight for this tab.
    pub saving: bool,
    /// Current load lifecycle: `untitled`, `loading`, `loaded`, or `failed`.
    pub load_state: String,
    /// On-disk byte size when known.
    pub file_size: Option<u64>,
    /// Whether this tab has draft identity without exposing the internal ID.
    pub draft_present: bool,
    /// Whether the tab content was evicted for memory pressure.
    pub evicted: bool,
    /// Whether the tab is pinned in the tab strip.
    pub pinned: bool,
}

/// Window-level surface state that scenario helpers commonly assert.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationSurfaceSnapshot {
    /// Rendered workspace sidebar visibility.
    pub workspace_sidebar_visible: bool,
    /// User-requested workspace sidebar visibility.
    pub workspace_sidebar_requested: bool,
    /// Rendered document properties visibility.
    pub document_properties_visible: bool,
    /// User-requested document properties visibility.
    pub document_properties_requested: bool,
    /// Compact layout slot owner, if any.
    pub compact_surface: Option<String>,
    /// Command palette revealer state.
    pub command_palette_visible: bool,
    /// Workspace search panel revealer state.
    pub search_panel_visible: bool,
    /// Side-by-side Markdown preview pane state.
    pub preview_pane_visible: bool,
    /// Preview-only Markdown mode state.
    pub preview_mode: bool,
    /// Focus Mode state.
    pub focus_mode: bool,
    /// Minimap preference state; document policy may still suppress rendering.
    pub minimap_requested: bool,
    /// Status bar widget visibility.
    pub status_bar_visible: bool,
    /// Topmost transient surface known to the shell.
    pub active_transient_surface: Option<String>,
}

/// Search-related state with bounded query/count fields only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationSearchSnapshot {
    /// Whether the selected editor's search bar is visible.
    pub editor_search_visible: bool,
    /// Current selected-editor query when the search UI is visible.
    pub editor_query: Option<String>,
    /// Current selected-editor occurrence count when available.
    pub editor_match_count: Option<i32>,
    /// Whether the workspace search panel is visible.
    pub workspace_search_visible: bool,
    /// Current workspace search query.
    pub workspace_query: String,
    /// Whether workspace search is currently running.
    pub workspace_searching: bool,
    /// Total workspace matches accumulated for the current query.
    pub workspace_match_count: u32,
    /// Number of files with matches for the current query.
    pub workspace_file_count: u32,
    /// Whether the workspace search result cap was reached.
    pub workspace_result_capped: bool,
}

/// Workspace state that can be read from the mounted sidebar model.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationWorkspaceSnapshot {
    /// Current scope kind: `all` or `workspace`.
    pub scope_kind: String,
    /// Concrete workspace id when the current scope targets one workspace.
    pub scope_workspace_id: Option<String>,
    /// User-visible workspace name for the selected workspace, if any.
    pub scope_workspace_name: Option<String>,
    /// Total persisted workspaces.
    pub workspace_count: u32,
    /// Total configured folder memberships across all workspaces.
    pub folder_count: u32,
    /// Folder memberships covered by the current scope.
    pub scoped_folder_count: u32,
    /// Whether no persisted workspaces exist.
    pub no_workspaces: bool,
    /// Whether the sidebar is writing workspace state in the background.
    pub persistence_inflight: bool,
    /// Whether another workspace save is pending after the in-flight write.
    pub persistence_dirty: bool,
    /// Whether the workspace filter fade sequence is active.
    pub filter_animation_active: bool,
}

/// Command palette state that avoids exposing result row bodies.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationCommandPaletteSnapshot {
    /// Whether the palette overlay is currently revealed.
    pub visible: bool,
    /// Current palette query text.
    pub query: String,
    /// Current search mode: `all`, `files`, `notes`, or `commands`.
    pub mode: String,
    /// Number of rendered result rows, including section headers.
    pub result_count: u32,
    /// Number of indexed workspace files.
    pub file_index_count: u32,
    /// Number of open file-backed tabs supplied as a palette source.
    pub open_tab_source_count: u32,
    /// Queued file-index mutations waiting for debounce flush.
    pub pending_index_update_count: u32,
}

/// Notes and bookmark state that is safe to expose without sidecar reads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationNotesSnapshot {
    /// Whether the notes menu popover is currently open.
    pub notes_menu_open: bool,
    /// Whether the active document is file-backed and can own document notes/bookmarks.
    pub active_document_file_backed: bool,
    /// Live bookmark count for the active editor tab.
    pub active_document_bookmark_count: u32,
    /// Whether the active cursor line has a bookmark.
    pub active_line_has_bookmark: bool,
    /// Whether the active document can open the document-note workflow.
    pub document_note_available: bool,
    /// Whether a folder-note action is meaningful for the current workspace scope.
    pub folder_note_available: bool,
}

/// Local-history state that can be read from the active editor policy.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationLocalHistorySnapshot {
    /// Whether the active document can browse local history.
    pub browse_available: bool,
    /// Whether the active document can capture automatic baseline/periodic snapshots.
    pub automatic_capture_available: bool,
    /// Size-policy classification for the active document.
    pub availability: String,
    /// Whether the active document is file-backed.
    pub active_document_file_backed: bool,
}

/// Workspace content search and Replace All state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationContentSearchSnapshot {
    /// Whether the workspace search panel is visible.
    pub visible: bool,
    /// Current workspace search query.
    pub query: String,
    /// Whether regex mode is enabled.
    pub regex_enabled: bool,
    /// Whether case-sensitive mode is enabled.
    pub case_sensitive: bool,
    /// Whether whole-word mode is enabled.
    pub whole_word_enabled: bool,
    /// Whether .gitignore filtering is enabled.
    pub gitignore_enabled: bool,
    /// Glob filter text when present.
    pub glob_filter: Option<String>,
    /// Whether a search worker is currently running.
    pub searching: bool,
    /// Number of files with matches.
    pub file_count: u32,
    /// Total match count.
    pub match_count: u32,
    /// Whether the search result cap was reached.
    pub result_capped: bool,
    /// Current replacement text.
    pub replace_query: String,
    /// Whether Replace All preview rows are visible.
    pub replace_preview_mode: bool,
    /// Whether preview generation is pending.
    pub replace_preview_pending: bool,
    /// Number of generated replacement preview rows.
    pub replace_preview_count: u32,
    /// Number of checked replacement preview rows.
    pub checked_replacement_count: u32,
    /// Whether a Replace All undo backup is available.
    pub has_undo_backup: bool,
    /// Number of recent history rows loaded into the panel.
    pub history_count: u32,
    /// Number of named saved searches loaded into the panel.
    pub saved_search_count: u32,
    /// Number of flat match navigation targets.
    pub navigation_match_count: u32,
    /// Current match navigation index, if any.
    pub current_navigation_match_index: Option<u32>,
}

/// Notification state for bounded status/progress assertions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AutomationNotificationSnapshot {
    /// Current visible status-bar message text, if any.
    pub status_text: Option<String>,
    /// Severity for the visible status-bar message.
    pub status_severity: Option<String>,
    /// Notification-bus generation for detecting visible-view changes.
    pub generation: u64,
    /// Whether the delayed workspace-search progress message is allowed to render.
    pub search_progress_visible: bool,
}
