// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust single-flight ownership for workspace content searches.

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

#[cfg(test)]
mod tests {
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
