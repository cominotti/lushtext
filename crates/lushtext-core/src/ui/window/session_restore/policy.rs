// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decisions for the session-restore workflow.
//!
//! Two stage orders' worth of policy, with no GTK anywhere: the **bounded-turn
//! admission** that decides how many pages one main-loop turn may create and how
//! many file-planning operations may be in flight, and the **journal's** pure
//! half — session-tab identity, the merge that preserves not-yet-admitted
//! descriptors across a close, the startup preload graph's fit to its disposal
//! reservation, and the recovery-diagnostic summary the user is shown.
//!
//! The admission half was already explicit policy before this migration and is
//! **relocated**; the journal half was inline in the GTK adapter and is a
//! **gain from zero**. `evidence/mutation-session-restore-policy.md` reports the
//! two separately.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::model::draft::{PreloadedDraftRestore, PreloadedDraftSkip};
use crate::model::session::{SessionData, SessionTab};
use crate::services::recovery_metadata::{
    RecoveryDiagnostic, RecoveryPreservation, RecoveryProblem,
};

/// Maximum restored pages created by one GTK main-loop turn.
pub(super) const SESSION_RESTORE_PAGES_PER_TURN: usize = 4;
/// Maximum concurrent background file-planning operations started by restore.
pub(super) const SESSION_RESTORE_FILE_PLAN_PERMITS: usize = 2;

/// Generation-bound ownership ticket for one admitted file-planning operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SessionRestorePlanPermit {
    generation: u64,
    id: u64,
}

impl SessionRestorePlanPermit {
    #[must_use]
    pub(super) const fn generation(self) -> u64 {
        self.generation
    }
}

/// One descriptor admitted for page creation in the current GTK turn.
#[derive(Debug)]
pub(super) struct SessionRestoreAdmission {
    pub(super) ordinal: usize,
    pub(super) tab: SessionTab,
    pub(super) permit: Option<SessionRestorePlanPermit>,
}

/// Why a bounded policy turn stopped admitting descriptors.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SessionRestoreTurnState {
    PageBudget,
    PlanningCapacity,
    AwaitingPlanningTerminal,
    Terminal,
}

/// Owned actions and scheduling state produced by one pure policy turn.
#[derive(Debug)]
pub(super) struct SessionRestoreTurn {
    pub(super) admissions: Vec<SessionRestoreAdmission>,
    #[cfg(test)]
    pub(super) state: SessionRestoreTurnState,
}

/// One generation's boundedness and terminal accounting.
///
/// The workflow's *observable* surface is `evidence::SessionRestoreEvidence`,
/// which projects these counters. This type is the policy's own bookkeeping and
/// is retained past the runtime's life as a last-restore outcome record.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionRestoreTurnMetrics {
    pub(super) generation: u64,
    pub(super) total_descriptors: usize,
    pub(super) pages_created: usize,
    pub(super) gtk_turns: usize,
    pub(super) max_pages_in_one_turn: usize,
    pub(super) max_inflight_file_plans: usize,
    pub(super) planning_terminals: usize,
    pub(super) pending_descriptors: usize,
    pub(super) active_file_plans: usize,
    pub(super) terminal_projection_publications: usize,
    pub(super) cancelled: bool,
}

/// Plain-Rust active-plus-pending restore coordinator for one window generation.
pub(super) struct SessionRestorePolicy {
    generation: u64,
    pending: VecDeque<SessionTab>,
    next_ordinal: usize,
    active_permits: HashSet<u64>,
    next_permit_id: u64,
    pages_per_turn: usize,
    file_plan_permits: usize,
    requested_active_ordinal: Option<usize>,
    terminal: bool,
    cancelled: bool,
    turn_metrics: SessionRestoreTurnMetrics,
}

impl SessionRestorePolicy {
    pub(super) fn new(
        generation: u64,
        tabs: Vec<SessionTab>,
        requested_active_ordinal: Option<usize>,
    ) -> Self {
        Self::with_limits(
            generation,
            tabs,
            requested_active_ordinal,
            SESSION_RESTORE_PAGES_PER_TURN,
            SESSION_RESTORE_FILE_PLAN_PERMITS,
        )
    }

    fn with_limits(
        generation: u64,
        tabs: Vec<SessionTab>,
        requested_active_ordinal: Option<usize>,
        pages_per_turn: usize,
        file_plan_permits: usize,
    ) -> Self {
        let total_descriptors = tabs.len();
        Self {
            generation,
            pending: VecDeque::from(tabs),
            next_ordinal: 0,
            active_permits: HashSet::new(),
            next_permit_id: 0,
            pages_per_turn: pages_per_turn.max(1),
            file_plan_permits: file_plan_permits.max(1),
            requested_active_ordinal,
            terminal: total_descriptors == 0,
            cancelled: false,
            turn_metrics: SessionRestoreTurnMetrics {
                generation,
                total_descriptors,
                ..SessionRestoreTurnMetrics::default()
            },
        }
    }

    #[must_use]
    pub(super) const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub(super) const fn requested_active_ordinal(&self) -> Option<usize> {
        self.requested_active_ordinal
    }

    /// Snapshot descriptors that have not yet produced mounted pages.
    #[must_use]
    pub(super) fn pending_descriptors(&self) -> Vec<(usize, SessionTab)> {
        self.pending
            .iter()
            .cloned()
            .enumerate()
            .map(|(offset, tab)| (self.next_ordinal.saturating_add(offset), tab))
            .collect()
    }

    #[must_use]
    pub(super) const fn is_terminal(&self) -> bool {
        self.terminal
    }

    /// Whether another bounded turn has work to admit.
    ///
    /// Checks `terminal` alone, not `terminal || cancelled`: `cancel` sets both,
    /// and it is an invariant that a terminal generation has an empty pending
    /// queue, so the second term could never change the answer. Stating only the
    /// condition that decides keeps the guard truthful — and testable, since an
    /// unreachable term is a mutation-equivalent one. `plan_turn` genuinely needs
    /// both, because it counts a turn before it inspects the queue.
    #[must_use]
    pub(super) fn needs_next_turn(&self) -> bool {
        debug_assert!(
            !self.terminal || self.pending.is_empty(),
            "a terminal generation has nothing pending"
        );
        if self.terminal {
            return false;
        }
        self.pending.front().is_some_and(|tab| {
            tab.path.is_none() || self.active_permits.len() < self.file_plan_permits
        })
    }

    pub(super) fn plan_turn(&mut self) -> SessionRestoreTurn {
        if self.terminal || self.cancelled {
            return SessionRestoreTurn {
                admissions: Vec::new(),
                #[cfg(test)]
                state: SessionRestoreTurnState::Terminal,
            };
        }

        self.turn_metrics.gtk_turns = self.turn_metrics.gtk_turns.saturating_add(1);
        let mut admissions = Vec::with_capacity(self.pages_per_turn);
        while admissions.len() < self.pages_per_turn {
            let Some(next) = self.pending.front() else {
                break;
            };
            if next.path.is_some() && self.active_permits.len() >= self.file_plan_permits {
                break;
            }

            let tab = self
                .pending
                .pop_front()
                .expect("front descriptor exists before bounded admission");
            let ordinal = self.next_ordinal;
            self.next_ordinal = self.next_ordinal.saturating_add(1);
            let permit = tab.path.as_ref().map(|_| {
                let id = self.next_permit_id;
                self.next_permit_id = self.next_permit_id.wrapping_add(1);
                let inserted = self.active_permits.insert(id);
                debug_assert!(inserted, "restore permit identity must be unique");
                SessionRestorePlanPermit {
                    generation: self.generation,
                    id,
                }
            });
            admissions.push(SessionRestoreAdmission {
                ordinal,
                tab,
                permit,
            });
        }

        self.turn_metrics.pages_created = self
            .turn_metrics
            .pages_created
            .saturating_add(admissions.len());
        self.turn_metrics.max_pages_in_one_turn = self
            .turn_metrics
            .max_pages_in_one_turn
            .max(admissions.len());
        self.turn_metrics.max_inflight_file_plans = self
            .turn_metrics
            .max_inflight_file_plans
            .max(self.active_permits.len());
        self.refresh_terminal_state();

        #[cfg(test)]
        let state = if self.terminal {
            SessionRestoreTurnState::Terminal
        } else if self.pending.is_empty() {
            SessionRestoreTurnState::AwaitingPlanningTerminal
        } else if admissions.len() == self.pages_per_turn {
            SessionRestoreTurnState::PageBudget
        } else {
            SessionRestoreTurnState::PlanningCapacity
        };
        SessionRestoreTurn {
            admissions,
            #[cfg(test)]
            state,
        }
    }

    /// Release one current planning permit exactly once.
    pub(super) fn release_permit(&mut self, permit: SessionRestorePlanPermit) -> bool {
        if self.cancelled
            || permit.generation != self.generation
            || !self.active_permits.remove(&permit.id)
        {
            return false;
        }
        self.turn_metrics.planning_terminals =
            self.turn_metrics.planning_terminals.saturating_add(1);
        self.refresh_terminal_state();
        true
    }

    /// Cancel every compact descriptor and admitted permit for this generation.
    pub(super) fn cancel(&mut self) {
        if self.terminal {
            return;
        }
        self.pending.clear();
        self.active_permits.clear();
        self.cancelled = true;
        self.terminal = true;
        self.turn_metrics.cancelled = true;
    }

    pub(super) fn note_terminal_projection_publication(&mut self) -> bool {
        if !self.terminal
            || self.cancelled
            || self.turn_metrics.terminal_projection_publications > 0
        {
            return false;
        }
        self.turn_metrics.terminal_projection_publications = 1;
        true
    }

    /// This generation's counters, with the two live ones derived at read time.
    ///
    /// `pending_descriptors` and `active_file_plans` are recomputed here rather
    /// than mirrored into `turn_metrics` on every transition. A mirror would be
    /// dead weight: this is the only reader, so any stored copy would be
    /// overwritten before anyone saw it — and an unobservable write is an
    /// untestable one.
    #[must_use]
    pub(super) fn metrics(&self) -> SessionRestoreTurnMetrics {
        let mut metrics = self.turn_metrics;
        metrics.pending_descriptors = self.pending.len();
        metrics.active_file_plans = self.active_permits.len();
        metrics
    }

    fn refresh_terminal_state(&mut self) {
        if self.pending.is_empty() && self.active_permits.is_empty() {
            self.terminal = true;
        }
    }
}

// --- the journal's pure half ------------------------------------------------
//
// Extracted from the GTK adapter by slot 4: none of it touches a widget, and all
// of it decides what the *user* gets back after a restart.

/// What makes two persisted tab descriptors the same document.
///
/// A file-backed tab is identified by its path; an untitled one by its draft ID.
/// A tab with neither is unidentifiable and must never be merged with another,
/// which is why this returns `Option` rather than inventing a key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SessionTabIdentity {
    Path(std::path::PathBuf),
    Draft(String),
}

/// Resolve one descriptor's merge identity, or `None` when it has none.
///
/// An empty draft ID is deliberately treated as *no* identity: two untitled tabs
/// with empty IDs are different documents, and merging them would silently drop
/// one of the user's buffers.
#[must_use]
pub(super) fn session_tab_identity(tab: &SessionTab) -> Option<SessionTabIdentity> {
    tab.path.clone().map(SessionTabIdentity::Path).or_else(|| {
        tab.draft_id
            .as_ref()
            .filter(|draft_id| !draft_id.is_empty())
            .cloned()
            .map(SessionTabIdentity::Draft)
    })
}

/// Index descriptors by identity, keeping the **first** occurrence.
///
/// First rather than last: the earlier index is the one already-mounted pages
/// were assigned, so preferring it keeps merged ordinals stable.
#[must_use]
pub(super) fn index_session_tabs(tabs: &[SessionTab]) -> HashMap<SessionTabIdentity, usize> {
    let mut indices = HashMap::with_capacity(tabs.len());
    for (index, tab) in tabs.iter().enumerate() {
        if let Some(identity) = session_tab_identity(tab) {
            indices.entry(identity).or_insert(index);
        }
    }
    indices
}

/// Merge one descriptor into an indexed list, returning where it landed.
///
/// An unidentifiable descriptor always appends, because it cannot be proven to
/// be a document the list already holds.
pub(super) fn merge_session_tab(
    tabs: &mut Vec<SessionTab>,
    indices: &mut HashMap<SessionTabIdentity, usize>,
    tab: SessionTab,
    replace_existing: bool,
) -> usize {
    let identity = session_tab_identity(&tab);
    if let Some(index) = identity
        .as_ref()
        .and_then(|identity| indices.get(identity).copied())
    {
        if replace_existing {
            tabs[index] = tab;
        }
        return index;
    }

    let index = tabs.len();
    tabs.push(tab);
    if let Some(identity) = identity {
        indices.insert(identity, index);
    }
    index
}

/// Preserve not-yet-loaded descriptors while layering current pages over them.
///
/// The load-bearing case is a close during a still-running restore: the persisted
/// file holds descriptors this session never admitted, and writing only the
/// mounted pages would delete the rest of the user's session.
#[must_use]
pub(super) fn merge_persisted_session_with_current(
    mut persisted: SessionData,
    current: SessionData,
) -> SessionData {
    let mut indices = index_session_tabs(&persisted.tabs);
    persisted.tabs.reserve(current.tabs.len());
    let mut current_active_index = None;
    for (current_index, tab) in current.tabs.into_iter().enumerate() {
        let merged_index = merge_session_tab(&mut persisted.tabs, &mut indices, tab, true);
        if current.active_tab_index == Some(current_index) {
            current_active_index = Some(merged_index);
        }
    }
    persisted.active_tab_index = current_active_index.or_else(|| {
        persisted
            .active_tab_index
            .filter(|index| *index < persisted.tabs.len())
    });
    persisted
}

/// Total retained bytes of one startup preload graph, keys and shell included.
#[must_use]
pub(super) fn startup_preloads_retained_bytes(
    preloaded: &HashMap<String, PreloadedDraftRestore>,
) -> u64 {
    let preload_bytes = preloaded
        .iter()
        .fold(0usize, |total, (id, restore)| {
            total
                .saturating_add(id.capacity())
                .saturating_add(match restore {
                    PreloadedDraftRestore::Content(content) => content.capacity(),
                    PreloadedDraftRestore::Skip(_) => 0,
                })
        })
        .saturating_add(preloaded.capacity().saturating_mul(
            std::mem::size_of::<(String, PreloadedDraftRestore)>().saturating_add(1),
        ));
    u64::try_from(
        std::mem::size_of::<HashMap<String, PreloadedDraftRestore>>().saturating_add(preload_bytes),
    )
    .unwrap_or(u64::MAX)
}

/// Demote eager bodies until the complete retained preload graph fits its permit.
///
/// A missing preload entry already falls back to the serialized lazy reader, so
/// clearing an unusually metadata-heavy map is safe and keeps release builds
/// from silently owning more memory than the progress lane accounted for.
pub(super) fn fit_startup_preloads_to_reservation(
    preloaded: &mut HashMap<String, PreloadedDraftRestore>,
    retained_byte_limit: u64,
) -> u64 {
    let mut retained_bytes = startup_preloads_retained_bytes(preloaded);
    if retained_bytes <= retained_byte_limit {
        return retained_bytes;
    }

    for restore in preloaded.values_mut() {
        let content = match std::mem::replace(
            restore,
            PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget),
        ) {
            PreloadedDraftRestore::Content(content) => content,
            compact @ PreloadedDraftRestore::Skip(_) => {
                *restore = compact;
                continue;
            }
        };
        retained_bytes =
            retained_bytes.saturating_sub(u64::try_from(content.capacity()).unwrap_or(u64::MAX));
        if retained_bytes <= retained_byte_limit {
            return retained_bytes;
        }
    }

    // A pathological count/key payload can outweigh the lane even after every
    // body became lazy. Dropping only the hints preserves the manifest and lets
    // each restored page take the normal bounded lazy path.
    *preloaded = HashMap::new();
    startup_preloads_retained_bytes(preloaded)
}

/// Summarise startup recovery diagnostics for one status message.
///
/// Grouped rather than enumerated on purpose: the message must say what happened
/// to the user's data without leaking file paths or recovery contents.
#[must_use]
pub(super) fn startup_recovery_status_message(diagnostics: &[RecoveryDiagnostic]) -> String {
    let damaged = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.problem,
                RecoveryProblem::Malformed { .. }
                    | RecoveryProblem::UnsupportedFormat { .. }
                    | RecoveryProblem::UnsupportedVersion { .. }
                    | RecoveryProblem::Unreadable { .. }
                    | RecoveryProblem::UnsupportedFileKind { .. }
                    | RecoveryProblem::Oversized { .. }
            )
        })
        .count();
    let repaired = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::Repaired { .. }))
        .count();
    let skipped = diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic.problem, RecoveryProblem::RepairSkipped { .. }))
        .count();
    let preserved = diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.preservation,
                RecoveryPreservation::Quarantined { .. }
                    | RecoveryPreservation::CopiedToQuarantine { .. }
                    | RecoveryPreservation::PreservedInPlace
            )
        })
        .count();

    match (damaged > 0, repaired > 0, skipped > 0, preserved > 0) {
        (true, true, _, true) => format!(
            "Some recovery data was repaired; {damaged} issue(s) were preserved for inspection"
        ),
        (true, false, true, true) => format!(
            "Some recovery data could not be loaded; {damaged} issue(s) were preserved for inspection"
        ),
        (true, _, _, _) => {
            format!("Some recovery data could not be loaded ({damaged} issue(s))")
        }
        (false, true, true, _) => {
            "Some recovery data was partially repaired; other items were preserved".to_string()
        }
        (false, true, false, _) => "Some recovery data was repaired".to_string(),
        (false, false, true, _) => {
            "Some recovery data could not be repaired automatically".to_string()
        }
        (false, false, false, _) => "Recovery data changed during startup".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use crate::services::recovery_metadata::{RecoveryMetadataClass, RecoveryPreservation};

    fn tab(path: Option<&str>) -> SessionTab {
        SessionTab {
            path: path.map(PathBuf::from),
            draft_id: path.is_none().then(|| "untitled-policy".to_string()),
            cursor_line: 0,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }
    }

    fn session_tab(
        path: Option<PathBuf>,
        draft_id: Option<String>,
        cursor_line: u32,
    ) -> SessionTab {
        SessionTab {
            path,
            draft_id,
            cursor_line,
            cursor_col: 0,
            scroll_line: 0,
            pinned: false,
        }
    }

    #[test]
    fn a_permit_and_its_policy_report_the_generation_they_were_created_for() {
        // Pins the generation accessors themselves. Every other test compares a
        // permit's generation against the policy's, so both could report the same
        // wrong value and no assertion would notice — which is exactly what makes
        // a stale-generation release look current.
        let mut policy = SessionRestorePolicy::with_limits(7, vec![tab(Some("/a"))], None, 1, 1);
        assert_eq!(policy.generation(), 7);

        let turn = policy.plan_turn();
        let permit = turn.admissions[0].permit.expect("file permit");
        assert_eq!(permit.generation(), 7);

        let metrics = policy.metrics();
        assert_eq!(metrics.generation, 7);
        assert_eq!(metrics.total_descriptors, 1);

        // A different generation is genuinely different, so a permit from one
        // cannot be released against the other.
        let other = SessionRestorePolicy::with_limits(8, vec![tab(Some("/b"))], None, 1, 1);
        assert_eq!(other.generation(), 8);
        assert_ne!(other.generation(), policy.generation());
        assert!(!policy.release_permit(SessionRestorePlanPermit {
            generation: other.generation(),
            id: 0,
        }));
    }

    #[test]
    fn total_descriptors_counts_what_the_generation_started_with() {
        // Separate from the accessor test because it is a different property: the
        // count must survive admission, not merely be reported once.
        let tabs = (0..5).map(|_| tab(None)).collect::<Vec<_>>();
        let mut policy = SessionRestorePolicy::with_limits(3, tabs, None, 2, 2);
        assert_eq!(policy.metrics().total_descriptors, 5);
        while !policy.is_terminal() {
            let _ = policy.plan_turn();
        }
        let metrics = policy.metrics();
        assert_eq!(metrics.total_descriptors, 5);
        assert_eq!(metrics.pages_created, 5);
        assert_eq!(metrics.generation, 3);

        // An empty generation is terminal immediately and reports zero, not the
        // default of some other field.
        let empty = SessionRestorePolicy::with_limits(4, Vec::new(), None, 2, 2);
        assert!(empty.is_terminal());
        assert_eq!(empty.metrics().total_descriptors, 0);
        assert_eq!(empty.metrics().generation, 4);
    }

    #[test]
    fn needs_next_turn_distinguishes_untitled_from_planning_saturated() {
        // An untitled descriptor needs no planning permit, so it can always be
        // admitted; a file-backed one cannot while permits are saturated. Both
        // halves of that disjunction are pinned, plus its `<` boundary — at the
        // limit there is no room, one below it there is exactly one.
        let tabs = vec![tab(Some("/a")), tab(Some("/b"))];
        let mut policy = SessionRestorePolicy::with_limits(1, tabs, None, 1, 1);
        assert!(policy.needs_next_turn(), "a fresh generation has work");

        let first = policy.plan_turn();
        let permit = first.admissions[0].permit.expect("file permit");
        // One permit out of one: the next descriptor is file-backed, so no.
        assert!(
            !policy.needs_next_turn(),
            "a saturated planning budget stops admission"
        );
        assert!(policy.release_permit(permit));
        assert!(
            policy.needs_next_turn(),
            "releasing the permit makes room for the next file-backed descriptor"
        );

        // An untitled descriptor behind a saturated budget is still admissible.
        let tabs = vec![tab(Some("/a")), tab(None)];
        let mut policy = SessionRestorePolicy::with_limits(2, tabs, None, 1, 1);
        let _ = policy.plan_turn();
        assert!(
            policy.needs_next_turn(),
            "an untitled descriptor needs no planning permit"
        );

        // Terminal and cancelled both refuse, and the invariant the guard relies
        // on — a terminal generation has an empty pending queue — is pinned here
        // rather than left to the `debug_assert!`.
        let mut cancelled = SessionRestorePolicy::with_limits(3, vec![tab(None)], None, 1, 1);
        cancelled.cancel();
        assert!(!cancelled.needs_next_turn());
        assert_eq!(
            cancelled.metrics().pending_descriptors,
            0,
            "cancellation clears the queue, so terminal still implies nothing pending"
        );
        let mut drained = SessionRestorePolicy::with_limits(4, vec![tab(None)], None, 1, 1);
        let _ = drained.plan_turn();
        assert!(drained.is_terminal());
        assert!(!drained.needs_next_turn());
        assert_eq!(drained.metrics().pending_descriptors, 0);
    }

    #[test]
    fn pending_descriptors_reports_unmounted_tabs_at_their_real_ordinals() {
        // What a close-time snapshot depends on: the descriptors this generation
        // has *not* mounted, numbered from where admission stopped. Returning an
        // empty vec here would silently drop the rest of the user's session.
        let tabs = (0..4).map(|_| tab(None)).collect::<Vec<_>>();
        let mut policy = SessionRestorePolicy::with_limits(5, tabs, None, 2, 2);
        assert_eq!(policy.pending_descriptors().len(), 4);
        assert_eq!(policy.pending_descriptors()[0].0, 0);

        let _ = policy.plan_turn();
        let pending = policy.pending_descriptors();
        assert_eq!(pending.len(), 2, "two of four admitted");
        assert_eq!(
            pending
                .iter()
                .map(|(ordinal, _)| *ordinal)
                .collect::<Vec<_>>(),
            vec![2, 3],
            "ordinals continue from where admission stopped"
        );

        let _ = policy.plan_turn();
        assert!(policy.pending_descriptors().is_empty());
    }

    #[test]
    fn terminal_projection_publication_refuses_before_terminal_and_after_cancel() {
        // Exactly-once, and only for a generation that genuinely finished. All
        // three refusal conditions are pinned separately, because an `&&` here
        // would let a cancelled or still-running generation publish.
        let mut running =
            SessionRestorePolicy::with_limits(1, vec![tab(None), tab(None)], None, 1, 1);
        assert!(
            !running.note_terminal_projection_publication(),
            "a generation with work left must not publish a terminal projection"
        );

        let mut cancelled = SessionRestorePolicy::with_limits(2, vec![tab(None)], None, 1, 1);
        cancelled.cancel();
        assert!(cancelled.is_terminal());
        assert!(
            !cancelled.note_terminal_projection_publication(),
            "a cancelled generation is terminal but must not publish"
        );

        let mut finished = SessionRestorePolicy::with_limits(3, vec![tab(None)], None, 1, 1);
        let _ = finished.plan_turn();
        assert!(finished.note_terminal_projection_publication());
        assert!(
            !finished.note_terminal_projection_publication(),
            "publication is exactly once"
        );
    }

    #[test]
    fn metric_counts_track_the_live_queue_and_permits_after_every_transition() {
        // The two live counters are derived at read time, so this pins that they
        // follow admission and release rather than being frozen at construction.
        let tabs = vec![tab(Some("/a")), tab(Some("/b")), tab(None)];
        let mut policy = SessionRestorePolicy::with_limits(9, tabs, None, 1, 1);
        assert_eq!(policy.metrics().pending_descriptors, 3);
        assert_eq!(policy.metrics().active_file_plans, 0);

        let turn = policy.plan_turn();
        let permit = turn.admissions[0].permit.expect("file permit");
        assert_eq!(policy.metrics().pending_descriptors, 2);
        assert_eq!(policy.metrics().active_file_plans, 1);

        assert!(policy.release_permit(permit));
        assert_eq!(policy.metrics().active_file_plans, 0);
        assert_eq!(policy.metrics().planning_terminals, 1);

        policy.cancel();
        assert_eq!(policy.metrics().pending_descriptors, 0);
        assert_eq!(policy.metrics().active_file_plans, 0);
    }

    #[test]
    fn an_empty_draft_id_is_not_a_merge_identity() {
        // The `!draft_id.is_empty()` guard. Two untitled tabs with empty IDs are
        // different documents, and treating the empty string as an identity would
        // merge them — silently dropping one of the user's buffers.
        let empty = session_tab(None, Some(String::new()), 0);
        assert!(
            session_tab_identity(&empty).is_none(),
            "an empty draft id is no identity at all"
        );

        let named = session_tab(None, Some("untitled-1".to_string()), 0);
        assert_eq!(
            session_tab_identity(&named),
            Some(SessionTabIdentity::Draft("untitled-1".to_string()))
        );

        // And a merge must therefore keep both empty-id tabs.
        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: vec![session_tab(None, Some(String::new()), 1)],
                active_tab_index: None,
            },
            SessionData {
                tabs: vec![session_tab(None, Some(String::new()), 2)],
                active_tab_index: None,
            },
        );
        assert_eq!(
            merged.tabs.len(),
            2,
            "two unidentifiable untitled tabs are two documents"
        );
    }

    #[test]
    fn plan_turn_refuses_for_a_cancelled_generation_as_well_as_a_terminal_one() {
        // Both halves of the guard, separately: cancellation sets *both* flags, so
        // only a generation that is terminal **without** being cancelled can
        // distinguish them. That case is the ordinary finished restore.
        let mut cancelled =
            SessionRestorePolicy::with_limits(1, vec![tab(None), tab(None)], None, 1, 1);
        cancelled.cancel();
        assert!(cancelled.plan_turn().admissions.is_empty());
        assert_eq!(
            cancelled.plan_turn().state,
            SessionRestoreTurnState::Terminal
        );

        let mut finished = SessionRestorePolicy::with_limits(2, vec![tab(None)], None, 4, 4);
        let first = finished.plan_turn();
        assert_eq!(first.admissions.len(), 1);
        assert!(finished.is_terminal());
        // Terminal but never cancelled: `plan_turn` must still refuse, and its
        // metrics must not record another turn.
        let turns_before = finished.metrics().gtk_turns;
        assert!(finished.plan_turn().admissions.is_empty());
        assert_eq!(finished.metrics().gtk_turns, turns_before);
    }

    #[test]
    fn a_persisted_active_index_past_the_merged_end_is_dropped() {
        // The `<` bound on the persisted active index. A `<=` would keep an index
        // one past the last tab, which selects nothing and leaves the user on a
        // blank shell; a `>` or `==` would discard valid selections.
        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: vec![
                    session_tab(Some(PathBuf::from("/a")), None, 0),
                    session_tab(Some(PathBuf::from("/b")), None, 0),
                ],
                active_tab_index: Some(1),
            },
            SessionData {
                tabs: Vec::new(),
                active_tab_index: None,
            },
        );
        assert_eq!(
            merged.active_tab_index,
            Some(1),
            "the last valid index is in range and must be kept"
        );

        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: vec![session_tab(Some(PathBuf::from("/a")), None, 0)],
                active_tab_index: Some(1),
            },
            SessionData {
                tabs: Vec::new(),
                active_tab_index: None,
            },
        );
        assert_eq!(
            merged.active_tab_index, None,
            "an index one past the end selects nothing and must be dropped"
        );

        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: vec![session_tab(Some(PathBuf::from("/a")), None, 0)],
                active_tab_index: Some(0),
            },
            SessionData {
                tabs: Vec::new(),
                active_tab_index: None,
            },
        );
        assert_eq!(merged.active_tab_index, Some(0));
    }

    #[test]
    fn recovery_summary_distinguishes_one_issue_from_none() {
        // The `> 0` thresholds. A `>= 0` would make every category look present,
        // so a run with nothing wrong would tell the user their data was damaged.
        let repaired_only = vec![RecoveryDiagnostic::repaired(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            "rebuilt one draft",
        )];
        let message = startup_recovery_status_message(&repaired_only);
        assert_eq!(message, "Some recovery data was repaired");
        assert!(
            !message.contains("could not be loaded"),
            "a clean repair must not report damage"
        );

        let skipped_only = vec![RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            "ambiguous draft",
        )];
        assert_eq!(
            startup_recovery_status_message(&skipped_only),
            "Some recovery data could not be repaired automatically"
        );

        // Neither repaired nor skipped nor damaged: the neutral arm.
        let quarantined_only = vec![RecoveryDiagnostic::with_preservation(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            RecoveryProblem::Repaired {
                detail: "x".to_string(),
            },
            RecoveryPreservation::PreservedInPlace,
        )];
        assert_eq!(
            startup_recovery_status_message(&quarantined_only),
            "Some recovery data was repaired"
        );
    }

    #[test]
    fn recovery_summary_only_promises_preservation_when_something_was_preserved() {
        // The `preserved > 0` threshold, which the other cases cannot reach: every
        // one of them either preserved something or had no damage at all. Telling a
        // user their data "was preserved for inspection" when it was not is the
        // failure this pins — they would go looking for a quarantine directory that
        // does not exist.
        let damaged_and_skipped_unpreserved = vec![
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::Malformed {
                    detail: "bad JSON".to_string(),
                },
                RecoveryPreservation::NotNeeded,
            ),
            // `repair_skipped` always preserves in place, so the unpreserved
            // skipped case has to be constructed explicitly. That it is awkward to
            // build is itself informative: in practice a skipped repair *does*
            // preserve, which is why no other test reached this arm.
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::RepairSkipped {
                    detail: "ambiguous draft".to_string(),
                },
                RecoveryPreservation::NotNeeded,
            ),
        ];
        let message = startup_recovery_status_message(&damaged_and_skipped_unpreserved);
        assert_eq!(
            message,
            "Some recovery data could not be loaded (1 issue(s))"
        );
        assert!(
            !message.contains("preserved"),
            "nothing was preserved, so the message must not claim it was"
        );

        let damaged_and_repaired_unpreserved = vec![
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::Malformed {
                    detail: "bad JSON".to_string(),
                },
                RecoveryPreservation::NotNeeded,
            ),
            RecoveryDiagnostic::repaired(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                "rebuilt one draft",
            ),
        ];
        let message = startup_recovery_status_message(&damaged_and_repaired_unpreserved);
        assert_eq!(
            message,
            "Some recovery data could not be loaded (1 issue(s))"
        );
        assert!(!message.contains("preserved"));
    }

    #[test]
    fn high_tab_count_is_admitted_in_order_across_bounded_turns() {
        let tabs = (0..11).map(|_| tab(None)).collect();
        let mut policy = SessionRestorePolicy::with_limits(7, tabs, Some(9), 3, 2);
        let mut ordinals = Vec::new();

        while !policy.is_terminal() {
            let turn = policy.plan_turn();
            assert!(turn.admissions.len() <= 3);
            ordinals.extend(turn.admissions.into_iter().map(|item| item.ordinal));
        }

        assert_eq!(ordinals, (0..11).collect::<Vec<_>>());
        assert_eq!(policy.requested_active_ordinal(), Some(9));
        let metrics = policy.metrics();
        assert_eq!(metrics.gtk_turns, 4);
        assert_eq!(metrics.max_pages_in_one_turn, 3);
        assert_eq!(metrics.terminal_projection_publications, 0);
        assert!(policy.note_terminal_projection_publication());
        assert!(!policy.note_terminal_projection_publication());
    }

    #[test]
    fn planning_saturation_stops_before_later_descriptors_and_releases_exactly_once() {
        let tabs = vec![tab(Some("/a")), tab(Some("/b")), tab(Some("/c")), tab(None)];
        let mut policy = SessionRestorePolicy::with_limits(11, tabs, None, 4, 2);

        let first = policy.plan_turn();
        assert_eq!(first.state, SessionRestoreTurnState::PlanningCapacity);
        assert_eq!(first.admissions.len(), 2);
        let first_permit = first.admissions[0].permit.expect("file permit");
        let stale = SessionRestorePlanPermit {
            generation: 10,
            id: first_permit.id,
        };
        assert!(!policy.release_permit(stale));
        assert!(policy.release_permit(first_permit));
        assert!(!policy.release_permit(first_permit));

        let second = policy.plan_turn();
        assert_eq!(second.admissions.len(), 2);
        assert_eq!(second.admissions[0].ordinal, 2);
        assert_eq!(second.admissions[1].ordinal, 3);
        for permit in first
            .admissions
            .into_iter()
            .chain(second.admissions)
            .filter_map(|item| item.permit)
        {
            let _ = policy.release_permit(permit);
        }

        assert!(policy.is_terminal());
        let metrics = policy.metrics();
        assert_eq!(metrics.max_inflight_file_plans, 2);
        assert_eq!(metrics.planning_terminals, 3);
    }

    #[test]
    fn cancellation_drops_pending_and_active_ownership_once() {
        let tabs = vec![tab(Some("/a")), tab(Some("/b")), tab(None)];
        let mut policy = SessionRestorePolicy::with_limits(5, tabs, None, 1, 1);
        let turn = policy.plan_turn();
        let permit = turn.admissions[0].permit.expect("file permit");

        policy.cancel();
        policy.cancel();

        assert!(policy.is_terminal());
        assert!(!policy.release_permit(permit));
        assert_eq!(policy.plan_turn().state, SessionRestoreTurnState::Terminal);
        let metrics = policy.metrics();
        assert!(metrics.cancelled);
        assert_eq!(metrics.pending_descriptors, 0);
        assert_eq!(metrics.active_file_plans, 0);
    }

    #[test]
    fn preload_graph_demotes_bodies_and_clears_metadata_when_needed() {
        let mut preloaded = HashMap::from([
            (
                "first".to_string(),
                PreloadedDraftRestore::Content(String::with_capacity(2_048)),
            ),
            (
                "second".to_string(),
                PreloadedDraftRestore::Content(String::with_capacity(2_048)),
            ),
        ]);
        let original = startup_preloads_retained_bytes(&preloaded);
        let body_limited = original.saturating_sub(2_048);

        let retained = fit_startup_preloads_to_reservation(&mut preloaded, body_limited);

        assert!(retained <= body_limited);
        assert!(preloaded.values().any(|restore| {
            matches!(
                restore,
                PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget)
            )
        }));

        let metadata_only_limit =
            u64::try_from(std::mem::size_of::<HashMap<String, PreloadedDraftRestore>>())
                .expect("HashMap shell fits u64");
        let retained = fit_startup_preloads_to_reservation(&mut preloaded, metadata_only_limit);
        assert!(preloaded.is_empty());
        assert!(retained <= metadata_only_limit);
    }

    #[test]
    fn session_merge_indexes_large_descriptor_sets_and_overlays_current_pages() {
        let persisted_tabs = (0..20_000)
            .map(|index| {
                session_tab(
                    Some(PathBuf::from(format!("/persisted/{index}.txt"))),
                    None,
                    0,
                )
            })
            .collect::<Vec<_>>();
        let mut current_tabs = (0..10_000)
            .map(|index| {
                session_tab(
                    Some(PathBuf::from(format!("/persisted/{index}.txt"))),
                    None,
                    7,
                )
            })
            .collect::<Vec<_>>();
        current_tabs.push(session_tab(None, Some("new-untitled".to_string()), 11));

        let merged = merge_persisted_session_with_current(
            SessionData {
                tabs: persisted_tabs,
                active_tab_index: Some(19_999),
            },
            SessionData {
                tabs: current_tabs,
                active_tab_index: Some(10_000),
            },
        );

        assert_eq!(merged.tabs.len(), 20_001);
        assert_eq!(merged.tabs[9_999].cursor_line, 7);
        assert_eq!(merged.tabs[19_999].cursor_line, 0);
        assert_eq!(
            merged.tabs[20_000].draft_id.as_deref(),
            Some("new-untitled")
        );
        assert_eq!(merged.active_tab_index, Some(20_000));
    }

    #[test]
    fn startup_recovery_status_groups_damage_and_repair() {
        let diagnostics = vec![
            RecoveryDiagnostic::with_preservation(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                RecoveryProblem::Malformed {
                    detail: "bad JSON".to_string(),
                },
                RecoveryPreservation::Quarantined {
                    path: PathBuf::from("/tmp/quarantine/manifest.json"),
                },
            ),
            RecoveryDiagnostic::repaired(
                RecoveryMetadataClass::DraftManifest,
                PathBuf::from("/tmp/manifest.json"),
                "rebuilt one draft",
            ),
        ];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("repaired"));
        assert!(message.contains("preserved"));
    }

    #[test]
    fn startup_recovery_status_mentions_unrepaired_items() {
        let diagnostics = vec![RecoveryDiagnostic::repair_skipped(
            RecoveryMetadataClass::DraftManifest,
            PathBuf::from("/tmp/manifest.json"),
            "ambiguous draft",
        )];

        let message = startup_recovery_status_message(&diagnostics);

        assert!(message.contains("could not be repaired"));
    }
}
