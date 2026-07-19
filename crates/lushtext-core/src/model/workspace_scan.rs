// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain ownership policy for one materialized workspace directory scan.
//!
//! GTK payloads remain in the sidebar adapter. This model only decides which
//! scalar ticket may be active, which latest ticket may wait, and which
//! completion is current enough to advance or terminate the flight.

/// Identity carried by one admitted or pending directory scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceScanTicket {
    /// Section lifetime that admitted the request.
    pub lifetime: u64,
    /// Latest target generation for this store at submission time.
    pub target_generation: u64,
    /// Unique scan generation within this store flight.
    pub scan_generation: u64,
}

/// Result of submitting one new compact request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScanSubmission {
    /// No scan was active, so this ticket receives worker admission now.
    Start(WorkspaceScanTicket),
    /// One scan remains active while this ticket replaces the pending request.
    QueueLatest {
        /// Latest ticket retained as the sole pending request.
        ticket: WorkspaceScanTicket,
        /// Active ticket whose cooperative cancellation should be requested.
        cancel_active: WorkspaceScanTicket,
        /// Whether this submission displaced an older pending ticket.
        replaced_pending: bool,
    },
}

/// Result of releasing one active ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceScanFinish {
    /// The completion was stale or already released.
    Stale,
    /// The latest pending ticket now receives worker admission.
    StartLatest(WorkspaceScanTicket),
    /// The current flight reached terminal idle state.
    Terminal,
}

/// Direct scalar ownership evidence for one store flight.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceScanFlightMetrics {
    /// Maximum simultaneous active tickets; always at most one.
    pub active_high_water: usize,
    /// Maximum simultaneous pending tickets; always at most one.
    pub pending_high_water: usize,
    /// Tickets that received worker admission.
    pub starts: u64,
    /// Requests that superseded an active ticket.
    pub cancellation_requests: u64,
    /// Pending tickets replaced before admission.
    pub pending_replacements: u64,
    /// Current flights that reached terminal idle state.
    pub terminals: u64,
    /// Late or duplicate completions rejected by ticket identity.
    pub stale_completions: u64,
}

/// One-active plus one-latest policy for a materialized child store.
#[derive(Debug, Default)]
pub struct WorkspaceScanFlight {
    next_scan_generation: u64,
    next_target_generation: u64,
    active: Option<WorkspaceScanTicket>,
    pending: Option<WorkspaceScanTicket>,
    metrics: WorkspaceScanFlightMetrics,
}

impl WorkspaceScanFlight {
    /// Submit a request and either admit it or replace the sole pending ticket.
    pub fn submit(&mut self, lifetime: u64) -> WorkspaceScanSubmission {
        self.next_scan_generation = self.next_scan_generation.wrapping_add(1);
        self.next_target_generation = self.next_target_generation.wrapping_add(1);
        let ticket = WorkspaceScanTicket {
            lifetime,
            target_generation: self.next_target_generation,
            scan_generation: self.next_scan_generation,
        };

        let Some(active) = self.active else {
            self.active = Some(ticket);
            self.metrics.starts = self.metrics.starts.saturating_add(1);
            self.metrics.active_high_water = 1;
            return WorkspaceScanSubmission::Start(ticket);
        };

        let replaced_pending = self.pending.replace(ticket).is_some();
        self.metrics.cancellation_requests = self.metrics.cancellation_requests.saturating_add(1);
        self.metrics.pending_high_water = 1;
        if replaced_pending {
            self.metrics.pending_replacements = self.metrics.pending_replacements.saturating_add(1);
        }
        WorkspaceScanSubmission::QueueLatest {
            ticket,
            cancel_active: active,
            replaced_pending,
        }
    }

    /// Return whether `ticket` is the active and latest request for this lifetime.
    #[must_use]
    pub fn is_current(&self, ticket: WorkspaceScanTicket, lifetime: u64) -> bool {
        self.active == Some(ticket)
            && ticket.lifetime == lifetime
            && ticket.target_generation == self.next_target_generation
    }

    /// Release the matching active ticket and hand off only the latest pending one.
    pub fn finish(&mut self, ticket: WorkspaceScanTicket) -> WorkspaceScanFinish {
        if self.active != Some(ticket) {
            self.metrics.stale_completions = self.metrics.stale_completions.saturating_add(1);
            return WorkspaceScanFinish::Stale;
        }

        self.active = None;
        if let Some(latest) = self.pending.take() {
            self.active = Some(latest);
            self.metrics.starts = self.metrics.starts.saturating_add(1);
            WorkspaceScanFinish::StartLatest(latest)
        } else {
            self.metrics.terminals = self.metrics.terminals.saturating_add(1);
            WorkspaceScanFinish::Terminal
        }
    }

    /// Invalidate all ownership when a store or section lifetime ends.
    pub fn cancel_all(&mut self) {
        if self.active.take().is_some() {
            self.metrics.cancellation_requests =
                self.metrics.cancellation_requests.saturating_add(1);
        }
        self.pending = None;
        self.next_target_generation = self.next_target_generation.wrapping_add(1);
    }

    /// Return the active ticket without exposing any adapter payload.
    #[must_use]
    pub fn active(&self) -> Option<WorkspaceScanTicket> {
        self.active
    }

    /// Return the sole latest pending ticket, if one exists.
    #[must_use]
    pub fn pending(&self) -> Option<WorkspaceScanTicket> {
        self.pending
    }

    /// Return direct scalar ownership and terminal evidence.
    #[must_use]
    pub fn metrics(&self) -> WorkspaceScanFlightMetrics {
        self.metrics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapid_submissions_keep_one_active_and_only_the_latest_pending_ticket() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(first) = flight.submit(7) else {
            panic!("first request should start");
        };
        let WorkspaceScanSubmission::QueueLatest { ticket: second, .. } = flight.submit(7) else {
            panic!("second request should wait");
        };
        let WorkspaceScanSubmission::QueueLatest {
            ticket: latest,
            cancel_active,
            replaced_pending,
        } = flight.submit(7)
        else {
            panic!("latest request should replace pending work");
        };

        assert_eq!(cancel_active, first);
        assert!(replaced_pending);
        assert_eq!(flight.active(), Some(first));
        assert_eq!(flight.pending(), Some(latest));
        assert_ne!(second, latest);
        assert_eq!(flight.metrics().active_high_water, 1);
        assert_eq!(flight.metrics().pending_high_water, 1);
        assert_eq!(flight.metrics().pending_replacements, 1);
    }

    #[test]
    fn active_completion_starts_latest_and_only_current_ticket_reaches_terminal() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(first) = flight.submit(3) else {
            panic!("first request should start");
        };
        let WorkspaceScanSubmission::QueueLatest { ticket: latest, .. } = flight.submit(3) else {
            panic!("latest request should wait");
        };

        assert!(!flight.is_current(first, 3));
        assert_eq!(
            flight.finish(first),
            WorkspaceScanFinish::StartLatest(latest)
        );
        assert!(flight.is_current(latest, 3));
        assert_eq!(flight.finish(first), WorkspaceScanFinish::Stale);
        assert_eq!(flight.finish(latest), WorkspaceScanFinish::Terminal);
        assert_eq!(flight.metrics().starts, 2);
        assert_eq!(flight.metrics().terminals, 1);
        assert_eq!(flight.metrics().stale_completions, 1);
    }

    #[test]
    fn lifetime_change_and_cancel_all_fail_closed() {
        let mut flight = WorkspaceScanFlight::default();
        let WorkspaceScanSubmission::Start(ticket) = flight.submit(11) else {
            panic!("first request should start");
        };

        assert!(!flight.is_current(ticket, 12));
        flight.cancel_all();

        assert_eq!(flight.active(), None);
        assert_eq!(flight.pending(), None);
        assert_eq!(flight.finish(ticket), WorkspaceScanFinish::Stale);
    }
}
