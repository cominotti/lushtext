// SPDX-License-Identifier: GPL-3.0-or-later

//! Pure decision logic for the workspace search panel workflow.
//!
//! Everything here is free of GTK-family imports so the default mutation scope
//! reaches it through the `ui/**/policy.rs` convention:
//!
//! * [`WorkspaceSearchFlight`] — single-flight ownership of the one active
//!   search plus one replaceable latest query.
//! * [`SearchRetirementSliceBudget`] — the per-turn ownership budget that lets
//!   one GTK retirement turn release at most its row budget.
//! * [`ReplacePreviewTicket`] plus [`ReplacePreviewFacts`] — the Replace All
//!   preview freshness seam, validated as one unit by
//!   [`ReplacePreviewTicket::is_current`].

use std::path::PathBuf;
use std::sync::Arc;

use crate::model::content_search::SearchQuerySpec;
use crate::services::single_flight::SingleFlightCoordinator;

/// Compact latest query retained while one active search disconnects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSearchRequest {
    pub spec: SearchQuerySpec,
    pub folders: Arc<[PathBuf]>,
}

/// One request admitted to become the only active controller/walker group.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSearchStart {
    pub generation: u64,
    pub request: WorkspaceSearchRequest,
}

/// Result of submitting one latest query to the single-flight policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkspaceSearchSubmission {
    Start(WorkspaceSearchStart),
    Supersede { active_generation: u64 },
}

/// Direct ownership counters for readiness and concurrency evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceSearchFlightSnapshot {
    pub active: usize,
    pub pending: usize,
    pub active_generation: Option<u64>,
}

/// At most one active search plus one replaceable latest pending request.
///
/// A thin wrapper over the shared [`SingleFlightCoordinator`] that adapts the
/// generic submit/finish results into workspace-search evidence: workspace
/// search does not use the coordinator's cancellation token (the content-search
/// walker owns cancellation), and `submit` reports the superseded generation
/// rather than dropping it.
#[derive(Debug, Default)]
pub struct WorkspaceSearchFlight {
    coordinator: SingleFlightCoordinator<WorkspaceSearchRequest>,
}

impl WorkspaceSearchFlight {
    /// Start immediately when idle, otherwise replace the compact pending query.
    ///
    /// # Panics
    ///
    /// Panics only if the shared coordinator rejects a submission while
    /// reporting no active generation, which its one-active/one-latest contract
    /// makes impossible.
    pub fn submit(&mut self, request: WorkspaceSearchRequest) -> WorkspaceSearchSubmission {
        let active_generation = self.coordinator.active_generation();
        match self.coordinator.submit(request) {
            Some(start) => WorkspaceSearchSubmission::Start(WorkspaceSearchStart {
                generation: start.generation,
                request: start.request,
            }),
            None => WorkspaceSearchSubmission::Supersede {
                active_generation: active_generation
                    .expect("a superseded submission always has an active generation"),
            },
        }
    }

    /// Finish only the current generation and admit the retained latest query.
    pub fn finish(&mut self, generation: u64) -> Option<WorkspaceSearchStart> {
        self.coordinator
            .finish(generation)
            .map(|start| WorkspaceSearchStart {
                generation: start.generation,
                request: start.request,
            })
    }

    /// Cancel pending ownership while the active generation drains externally.
    pub fn clear_pending(&mut self) {
        self.coordinator.clear_pending();
    }

    #[must_use]
    pub fn snapshot(&self) -> WorkspaceSearchFlightSnapshot {
        let snapshot = self.coordinator.snapshot();
        WorkspaceSearchFlightSnapshot {
            active: snapshot.active,
            pending: snapshot.pending,
            active_generation: self.coordinator.active_generation(),
        }
    }
}

/// Saturating counter that lets one GTK adapter turn release at most its row budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchRetirementSliceBudget {
    remaining: usize,
    retired: usize,
}

impl SearchRetirementSliceBudget {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            remaining: limit,
            retired: 0,
        }
    }

    /// Reserve bounded work from the next deterministic ownership category.
    pub fn take(&mut self, available: usize) -> usize {
        let count = self.remaining.min(available);
        self.remaining = self.remaining.saturating_sub(count);
        self.retired = self.retired.saturating_add(count);
        count
    }

    /// Charge one independently owned value before its caller releases it.
    pub fn take_one(&mut self) -> bool {
        self.take(1) == 1
    }

    #[must_use]
    pub fn exhausted(self) -> bool {
        self.remaining == 0
    }

    #[must_use]
    pub fn retired(self) -> usize {
        self.retired
    }
}

/// Identity of one Replace All preview attempt, captured when it is dispatched.
///
/// The preview seam is inverted twice: generation and replacement previews are
/// built on a worker, and the checked selection is partitioned on a second
/// worker. Both completions resume on GTK and must decide whether the panel they
/// return to still wants their result. Reifying `{generation, query_spec}` here
/// keeps that decision one validated value rather than a clause list repeated at
/// each resumption point, and makes the "same generation, different query" case
/// impossible to compare loosely.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplacePreviewTicket {
    generation: u32,
    query_spec: SearchQuerySpec,
}

/// Live preview state observed on GTK when a preview worker completion resumes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplacePreviewFacts {
    pub generation: u32,
    pub pending: bool,
    pub query_spec: SearchQuerySpec,
}

impl ReplacePreviewTicket {
    /// Capture the expectation for one preview attempt at its entry point.
    #[must_use]
    pub fn new(generation: u32, query_spec: SearchQuerySpec) -> Self {
        Self {
            generation,
            query_spec,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    #[must_use]
    pub const fn query_spec(&self) -> &SearchQuerySpec {
        &self.query_spec
    }

    /// Adopt a newer attempt's expectation when a queued request is superseded,
    /// so the retained request keeps exactly one identity.
    ///
    /// Takes the newer ticket by value: the expectation is still constructed
    /// once, at `issue_preview_ticket`, and moves into the retained request
    /// rather than being re-split into its fields and copied across the seam.
    pub fn supersede(&mut self, newer: Self) {
        *self = newer;
    }

    /// Whether a completed preview result may still be published to the panel.
    ///
    /// All three clauses are part of one decision: the panel must still be on
    /// this generation, must still be waiting for a preview, and must still be
    /// showing the query the preview was generated for.
    #[must_use]
    pub fn is_current(&self, facts: &ReplacePreviewFacts) -> bool {
        self.generation == facts.generation && facts.pending && self.query_spec == facts.query_spec
    }

    /// Whether a retained request may still be dispatched to the worker.
    ///
    /// Re-dispatch deliberately checks only generation and pending state: the
    /// request has not produced a result yet, so the live query is re-read when
    /// its result comes back through [`Self::is_current`].
    #[must_use]
    pub fn may_dispatch(&self, facts: &ReplacePreviewFacts) -> bool {
        self.generation == facts.generation && facts.pending
    }
}

#[cfg(test)]
mod preview_ticket_tests {
    use super::*;
    use crate::model::content_search::ContentSearchOptions;

    fn spec(query: &str) -> SearchQuerySpec {
        SearchQuerySpec {
            query: query.to_string(),
            options: ContentSearchOptions::default(),
        }
    }

    fn facts(generation: u32, pending: bool, query: &str) -> ReplacePreviewFacts {
        ReplacePreviewFacts {
            generation,
            pending,
            query_spec: spec(query),
        }
    }

    #[test]
    fn matching_generation_pending_and_query_publishes() {
        let ticket = ReplacePreviewTicket::new(7, spec("needle"));
        assert_eq!(ticket.generation(), 7);
        assert_eq!(ticket.query_spec(), &spec("needle"));
        assert!(ticket.is_current(&facts(7, true, "needle")));
        assert!(ticket.may_dispatch(&facts(7, true, "needle")));
    }

    #[test]
    fn each_stale_clause_rejects_the_completion() {
        let ticket = ReplacePreviewTicket::new(7, spec("needle"));
        assert!(!ticket.is_current(&facts(8, true, "needle")));
        assert!(!ticket.is_current(&facts(7, false, "needle")));
        assert!(!ticket.is_current(&facts(7, true, "other")));
    }

    #[test]
    fn same_generation_with_changed_options_is_not_current() {
        let mut changed = spec("needle");
        changed.options.regex = true;
        let ticket = ReplacePreviewTicket::new(3, spec("needle"));
        assert!(!ticket.is_current(&ReplacePreviewFacts {
            generation: 3,
            pending: true,
            query_spec: changed,
        }));
    }

    #[test]
    fn dispatch_ignores_query_drift_but_not_generation_or_pending() {
        let ticket = ReplacePreviewTicket::new(2, spec("needle"));
        assert!(ticket.may_dispatch(&facts(2, true, "typed-more")));
        assert!(!ticket.may_dispatch(&facts(3, true, "needle")));
        assert!(!ticket.may_dispatch(&facts(2, false, "needle")));
    }

    #[test]
    fn superseding_a_retained_request_replaces_the_whole_expectation() {
        let mut ticket = ReplacePreviewTicket::new(2, spec("needle"));
        ticket.supersede(ReplacePreviewTicket::new(5, spec("latest")));
        assert_eq!(ticket.generation(), 5);
        assert!(ticket.is_current(&facts(5, true, "latest")));
        assert!(!ticket.is_current(&facts(2, true, "needle")));
    }
}

#[cfg(test)]
mod flight_tests {
    use super::*;
    use crate::model::content_search::ContentSearchOptions;

    fn request(query: &str) -> WorkspaceSearchRequest {
        WorkspaceSearchRequest {
            spec: SearchQuerySpec {
                query: query.to_string(),
                options: ContentSearchOptions::default(),
            },
            folders: Arc::from([PathBuf::from("/workspace")]),
        }
    }

    #[test]
    fn rapid_submissions_keep_one_active_and_only_latest_pending() {
        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(first) = flight.submit(request("first")) else {
            panic!("first request should start");
        };
        for query in ["second", "third", "latest"] {
            assert_eq!(
                flight.submit(request(query)),
                WorkspaceSearchSubmission::Supersede {
                    active_generation: first.generation,
                }
            );
        }
        assert_eq!(
            flight.snapshot(),
            WorkspaceSearchFlightSnapshot {
                active: 1,
                pending: 1,
                active_generation: Some(first.generation),
            }
        );

        let next = flight
            .finish(first.generation)
            .expect("latest should start");
        assert_eq!(next.request.spec.query, "latest");
        assert_eq!(flight.snapshot().active, 1);
        assert_eq!(flight.snapshot().pending, 0);
    }

    #[test]
    fn stale_disconnect_cannot_finish_current_generation() {
        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(first) = flight.submit(request("first")) else {
            panic!("first request should start");
        };
        flight.submit(request("latest"));
        assert!(flight.finish(first.generation.wrapping_add(99)).is_none());
        assert_eq!(flight.snapshot().active_generation, Some(first.generation));
    }

    #[test]
    fn panel_clear_drops_pending_but_waits_for_active_disconnect() {
        let mut flight = WorkspaceSearchFlight::default();
        flight.submit(request("first"));
        flight.submit(request("pending"));
        flight.clear_pending();
        assert_eq!(flight.snapshot().active, 1);
        assert_eq!(flight.snapshot().pending, 0);
    }

    #[test]
    fn active_and_pending_requests_share_immutable_scope_snapshots() {
        let shared =
            Arc::<[PathBuf]>::from([PathBuf::from("/workspace/a"), PathBuf::from("/workspace/b")]);
        let mut first = request("first");
        first.folders = Arc::clone(&shared);
        let mut latest = request("latest");
        latest.folders = Arc::clone(&shared);
        let changed = Arc::<[PathBuf]>::from([PathBuf::from("/workspace/changed")]);

        let mut flight = WorkspaceSearchFlight::default();
        let WorkspaceSearchSubmission::Start(active) = flight.submit(first) else {
            panic!("first request should start");
        };
        flight.submit(latest);
        let pending = flight
            .finish(active.generation)
            .expect("latest request should start");

        assert!(Arc::ptr_eq(&active.request.folders, &shared));
        assert!(Arc::ptr_eq(&pending.request.folders, &shared));
        assert!(!Arc::ptr_eq(&pending.request.folders, &changed));
        assert_eq!(pending.request.folders.as_ref(), shared.as_ref());
    }
}

#[cfg(test)]
mod retirement_budget_tests {
    use super::*;

    #[test]
    fn configured_result_cap_retires_over_bounded_slices() {
        let mut categories = [1usize, 10_000, 1, 10_000, 1];
        let mut slices = 0usize;
        while categories.iter().any(|count| *count > 0) {
            let mut budget = SearchRetirementSliceBudget::new(250);
            for count in &mut categories {
                let retired = budget.take(*count);
                *count = count.saturating_sub(retired);
            }
            assert!(budget.retired() <= 250);
            assert!(budget.retired() > 0);
            slices = slices.saturating_add(1);
        }
        assert!(slices > 1);
    }

    #[test]
    fn zero_budget_and_large_available_counts_saturate_safely() {
        let mut empty = SearchRetirementSliceBudget::new(0);
        assert_eq!(empty.take(usize::MAX), 0);
        assert!(!empty.take_one());
        assert!(empty.exhausted());

        let mut bounded = SearchRetirementSliceBudget::new(250);
        assert!(bounded.take_one());
        assert_eq!(bounded.retired(), 1);
        assert_eq!(bounded.take(usize::MAX), 249);
        assert_eq!(bounded.retired(), 250);
        assert!(bounded.exhausted());
    }
}
