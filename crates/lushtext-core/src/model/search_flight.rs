// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust single-flight ownership for workspace content searches.

use std::path::PathBuf;

use crate::model::content_search::SearchQuerySpec;

/// Compact latest query retained while one active search disconnects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSearchRequest {
    pub spec: SearchQuerySpec,
    pub folders: Vec<PathBuf>,
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
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceSearchFlight {
    next_generation: u64,
    active_generation: Option<u64>,
    pending: Option<WorkspaceSearchRequest>,
}

impl WorkspaceSearchFlight {
    /// Start immediately when idle, otherwise replace the compact pending query.
    pub fn submit(&mut self, request: WorkspaceSearchRequest) -> WorkspaceSearchSubmission {
        if let Some(active_generation) = self.active_generation {
            self.pending = Some(request);
            return WorkspaceSearchSubmission::Supersede { active_generation };
        }
        WorkspaceSearchSubmission::Start(self.start(request))
    }

    /// Finish only the current generation and admit the retained latest query.
    pub fn finish(&mut self, generation: u64) -> Option<WorkspaceSearchStart> {
        if self.active_generation != Some(generation) {
            return None;
        }
        self.active_generation = None;
        self.pending.take().map(|request| self.start(request))
    }

    /// Cancel pending ownership while the active generation drains externally.
    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    #[must_use]
    pub fn snapshot(&self) -> WorkspaceSearchFlightSnapshot {
        WorkspaceSearchFlightSnapshot {
            active: usize::from(self.active_generation.is_some()),
            pending: usize::from(self.pending.is_some()),
            active_generation: self.active_generation,
        }
    }

    fn start(&mut self, request: WorkspaceSearchRequest) -> WorkspaceSearchStart {
        self.next_generation = self.next_generation.wrapping_add(1);
        self.active_generation = Some(self.next_generation);
        WorkspaceSearchStart {
            generation: self.next_generation,
            request,
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
            folders: vec![PathBuf::from("/workspace")],
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
}
