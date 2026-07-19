// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded window-owned session-restore admission and GTK runtime state.
//!
//! The policy is plain Rust: it retains compact session descriptors, preserves
//! persisted order, caps page creation per GTK turn, and admits a fixed number
//! of file-planning permits. The window adapter owns source IDs, tab pages, and
//! terminal projection publication separately.

use std::collections::{HashSet, VecDeque};

use crate::model::session::SessionTab;

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

/// Direct boundedness and terminal-accounting evidence for one generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SessionRestoreEvidence {
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
    evidence: SessionRestoreEvidence,
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
            evidence: SessionRestoreEvidence {
                generation,
                total_descriptors,
                ..SessionRestoreEvidence::default()
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

    #[must_use]
    pub(super) fn needs_next_turn(&self) -> bool {
        if self.terminal || self.cancelled {
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

        self.evidence.gtk_turns = self.evidence.gtk_turns.saturating_add(1);
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

        self.evidence.pages_created = self.evidence.pages_created.saturating_add(admissions.len());
        self.evidence.max_pages_in_one_turn =
            self.evidence.max_pages_in_one_turn.max(admissions.len());
        self.evidence.max_inflight_file_plans = self
            .evidence
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
        self.evidence.planning_terminals = self.evidence.planning_terminals.saturating_add(1);
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
        self.evidence.cancelled = true;
        self.refresh_evidence_counts();
    }

    pub(super) fn note_terminal_projection_publication(&mut self) -> bool {
        if !self.terminal || self.cancelled || self.evidence.terminal_projection_publications > 0 {
            return false;
        }
        self.evidence.terminal_projection_publications = 1;
        true
    }

    #[must_use]
    pub(super) fn evidence(&self) -> SessionRestoreEvidence {
        let mut evidence = self.evidence;
        evidence.pending_descriptors = self.pending.len();
        evidence.active_file_plans = self.active_permits.len();
        evidence
    }

    fn refresh_terminal_state(&mut self) {
        if self.pending.is_empty() && self.active_permits.is_empty() {
            self.terminal = true;
        }
        self.refresh_evidence_counts();
    }

    fn refresh_evidence_counts(&mut self) {
        self.evidence.pending_descriptors = self.pending.len();
        self.evidence.active_file_plans = self.active_permits.len();
    }
}

/// GTK-owned state layered around the pure policy for one active generation.
pub(super) struct SessionRestoreRuntime {
    pub(super) policy: SessionRestorePolicy,
    pub(super) scheduled_source: Option<glib::SourceId>,
    pub(super) preserve_existing_selection: bool,
    pub(super) selected_before: Option<glib::WeakRef<libadwaita::TabPage>>,
    pub(super) requested_page: Option<glib::WeakRef<libadwaita::TabPage>>,
    pub(super) projection_batch_owned: bool,
    pub(super) cleanup_allowed_on_terminal: bool,
    /// Selection intent generation captured before restore-owned tab mutations begin.
    pub(super) selection_generation: u64,
}

impl SessionRestoreRuntime {
    pub(super) fn new(
        policy: SessionRestorePolicy,
        preserve_existing_selection: bool,
        selected_before: Option<glib::WeakRef<libadwaita::TabPage>>,
        cleanup_allowed_on_terminal: bool,
        selection_generation: u64,
    ) -> Self {
        Self {
            policy,
            scheduled_source: None,
            preserve_existing_selection,
            selected_before,
            requested_page: None,
            projection_batch_owned: true,
            cleanup_allowed_on_terminal,
            selection_generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let evidence = policy.evidence();
        assert_eq!(evidence.gtk_turns, 4);
        assert_eq!(evidence.max_pages_in_one_turn, 3);
        assert_eq!(evidence.terminal_projection_publications, 0);
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
        let evidence = policy.evidence();
        assert_eq!(evidence.max_inflight_file_plans, 2);
        assert_eq!(evidence.planning_terminals, 3);
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
        let evidence = policy.evidence();
        assert!(evidence.cancelled);
        assert_eq!(evidence.pending_descriptors, 0);
        assert_eq!(evidence.active_file_plans, 0);
    }
}
