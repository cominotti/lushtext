// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain-Rust policy deciding when a user-attention moment refreshes the
//! workspace surfaces.
//!
//! **Cross-cutting pure policy with two owning workflows**, `WFR-COMMAND-PALETTE`
//! and `WFR-WORKSPACE-TREE`. It is shared rather than placed beside either one,
//! because `.agents/rules/workflow-convention.md` counts owning workflows and a
//! forked copy of this throttle could drift while both halves still read as
//! correct — the two surfaces must refuse and admit on exactly the same tick.
//!
//! # Relationship to the other refusal throttle in the tree
//!
//! `ui::accessibility::AnnouncementThrottler::should_announce_at` applies the
//! same core rule — refuse when the gap since the last accepted event is under
//! a cooldown, and update state *only* on acceptance — and the two are
//! deliberately not merged. That one keys a fixed cooldown by announcement
//! lane; this one derives its interval from the previous pass's cost and also
//! carries an in-flight state, so the shared part is the comparison rather than
//! the policy. If a third refusal throttle appears, that is the point at which
//! extracting the cooldown core stops being premature.
//!
//! # Why the interval is adaptive rather than a constant
//!
//! The trigger is the window becoming active, which is not a rare event: on a
//! focus-follows-mouse desktop it fires constantly. Meanwhile the work being
//! throttled is a filesystem traversal whose cost differs by orders of
//! magnitude between a local NVMe checkout and an NFS mount. A constant tuned
//! on the former is an I/O storm on the latter. Deriving the interval from the
//! previous pass's observed duration lets one rule serve both: a cheap refresh
//! stays responsive at the floor, an expensive one throttles itself toward the
//! ceiling.

use std::time::Duration;

/// Shortest gap between two refreshes, however cheap the previous one was.
///
/// Two seconds is below the threshold at which a user returning to the window
/// would perceive the palette as stale, while still collapsing the burst of
/// activations produced by alt-tabbing through a window list.
pub const ATTENTION_REFRESH_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Longest gap the adaptive rule may impose, however expensive the previous
/// refresh was.
///
/// A minute bounds the worst case so a single pathologically slow pass — a
/// stalled network mount, say — cannot effectively disable refreshing for the
/// rest of the session.
pub const ATTENTION_REFRESH_MAX_INTERVAL: Duration = Duration::from_secs(60);

/// Multiple of the previous pass's duration used as the next interval.
///
/// Ten keeps the amortized cost of refreshing near a tenth of one core's time
/// in the worst case where attention moments arrive continuously.
pub const ATTENTION_REFRESH_DURATION_MULTIPLIER: u32 = 10;

/// Live state the attention-refresh decision reads.
///
/// Every field is a scalar the GTK side already owns, so the decision never
/// retains a widget and stays testable without a display server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttentionRefreshFacts {
    /// Whether a refresh for this workspace folder set has not yet settled.
    pub in_flight: bool,
    /// Time since the previous refresh settled, or `None` if none ever has.
    pub since_previous: Option<Duration>,
    /// Wall time the previous refresh took, or `None` if none ever has.
    pub previous_duration: Option<Duration>,
    /// Whether a save, close transaction, or draft autosave is in flight.
    pub user_work_in_flight: bool,
}

/// What the shell does with one attention moment.
///
/// The refusals are distinct variants rather than one `bool` because each one
/// tells a reader — and a test — something different about what would have to
/// change for the answer to change. [`Self::RefuseUserWorkInFlight`] is the
/// only one that needs an external event; the other two give way on their own.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionRefreshAdmission {
    /// Start a refresh now.
    Start,
    /// Refuse: an operation that protects unsaved work is running.
    RefuseUserWorkInFlight,
    /// Refuse: a refresh for this folder set has not settled yet.
    RefuseInFlight,
    /// Refuse: the adaptive interval has not elapsed.
    RefuseThrottled,
    /// Refuse: the current workspace scope has no folders to refresh.
    ///
    /// Reported by the shell rather than decided here, because it is a fact
    /// about the workspace rather than about refresh history. It has its own
    /// variant so the shell never has to borrow an unrelated refusal to say it.
    RefuseNoWorkspace,
}

/// The gap required before another refresh may start.
///
/// Saturating arithmetic keeps an absurd previous duration — a multi-hour
/// stall on a dead mount — from wrapping into a short interval.
#[must_use]
pub fn attention_refresh_interval(previous_duration: Option<Duration>) -> Duration {
    let Some(previous) = previous_duration else {
        return ATTENTION_REFRESH_MIN_INTERVAL;
    };
    previous
        .saturating_mul(ATTENTION_REFRESH_DURATION_MULTIPLIER)
        .clamp(
            ATTENTION_REFRESH_MIN_INTERVAL,
            ATTENTION_REFRESH_MAX_INTERVAL,
        )
}

/// Decide what one attention moment does.
///
/// The order of refusals is the order of importance, not of cost: yielding to
/// user work is a data-safety guard and is reported even when the throttle
/// would also have refused, so a reader of the evidence surface sees why.
#[must_use]
pub fn admit_attention_refresh(facts: AttentionRefreshFacts) -> AttentionRefreshAdmission {
    if facts.user_work_in_flight {
        return AttentionRefreshAdmission::RefuseUserWorkInFlight;
    }
    if facts.in_flight {
        return AttentionRefreshAdmission::RefuseInFlight;
    }
    let Some(since_previous) = facts.since_previous else {
        return AttentionRefreshAdmission::Start;
    };
    if since_previous < attention_refresh_interval(facts.previous_duration) {
        return AttentionRefreshAdmission::RefuseThrottled;
    }
    AttentionRefreshAdmission::Start
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Facts describing a settled surface that has never refreshed.
    const fn never_refreshed() -> AttentionRefreshFacts {
        AttentionRefreshFacts {
            in_flight: false,
            since_previous: None,
            previous_duration: None,
            user_work_in_flight: false,
        }
    }

    #[test]
    fn test_first_attention_moment_starts_a_refresh() {
        assert_eq!(
            admit_attention_refresh(never_refreshed()),
            AttentionRefreshAdmission::Start
        );
    }

    #[test]
    fn test_user_work_refuses_before_every_other_reason() {
        // The throttle and the in-flight guard would also refuse here; the
        // data-safety reason is the one reported, so evidence readers see why.
        let facts = AttentionRefreshFacts {
            in_flight: true,
            since_previous: Some(Duration::ZERO),
            previous_duration: Some(Duration::from_secs(1)),
            user_work_in_flight: true,
        };
        assert_eq!(
            admit_attention_refresh(facts),
            AttentionRefreshAdmission::RefuseUserWorkInFlight
        );
    }

    #[test]
    fn test_user_work_refuses_even_when_nothing_else_would() {
        let facts = AttentionRefreshFacts {
            user_work_in_flight: true,
            ..never_refreshed()
        };
        assert_eq!(
            admit_attention_refresh(facts),
            AttentionRefreshAdmission::RefuseUserWorkInFlight
        );
    }

    #[test]
    fn test_in_flight_refresh_refuses_a_second_one() {
        let facts = AttentionRefreshFacts {
            in_flight: true,
            ..never_refreshed()
        };
        assert_eq!(
            admit_attention_refresh(facts),
            AttentionRefreshAdmission::RefuseInFlight
        );
    }

    #[test]
    fn test_rapid_second_attention_moment_is_throttled() {
        let facts = AttentionRefreshFacts {
            since_previous: Some(Duration::from_millis(10)),
            previous_duration: Some(Duration::from_millis(5)),
            ..never_refreshed()
        };
        assert_eq!(
            admit_attention_refresh(facts),
            AttentionRefreshAdmission::RefuseThrottled
        );
    }

    #[test]
    fn test_elapsed_interval_admits_again() {
        let facts = AttentionRefreshFacts {
            since_previous: Some(ATTENTION_REFRESH_MIN_INTERVAL),
            previous_duration: Some(Duration::from_millis(5)),
            ..never_refreshed()
        };
        assert_eq!(
            admit_attention_refresh(facts),
            AttentionRefreshAdmission::Start
        );
    }

    #[test]
    fn test_a_negligible_refresh_still_waits_the_floor() {
        // A refresh that costs nothing must not make the trigger effectively
        // unthrottled, which is what a bare multiplier would do.
        assert_eq!(
            attention_refresh_interval(Some(Duration::ZERO)),
            ATTENTION_REFRESH_MIN_INTERVAL
        );
        assert_eq!(
            attention_refresh_interval(Some(Duration::from_micros(1))),
            ATTENTION_REFRESH_MIN_INTERVAL
        );
    }

    #[test]
    fn test_a_slow_refresh_lengthens_the_interval_proportionally() {
        let previous = Duration::from_secs(3);
        assert_eq!(
            attention_refresh_interval(Some(previous)),
            previous * ATTENTION_REFRESH_DURATION_MULTIPLIER
        );
    }

    #[test]
    fn test_a_pathological_refresh_stops_at_the_ceiling() {
        assert_eq!(
            attention_refresh_interval(Some(Duration::from_secs(600))),
            ATTENTION_REFRESH_MAX_INTERVAL
        );
        // Saturating multiplication must not wrap a near-maximum duration into
        // a short interval, which would invert the throttle exactly when it
        // matters most.
        assert_eq!(
            attention_refresh_interval(Some(Duration::MAX)),
            ATTENTION_REFRESH_MAX_INTERVAL
        );
    }

    #[test]
    fn test_no_previous_refresh_uses_the_floor() {
        assert_eq!(
            attention_refresh_interval(None),
            ATTENTION_REFRESH_MIN_INTERVAL
        );
    }

    #[test]
    fn test_interval_is_always_inside_its_declared_bounds() {
        for millis in [0, 1, 199, 200, 201, 6_000, 6_001, 100_000] {
            let interval = attention_refresh_interval(Some(Duration::from_millis(millis)));
            assert!(
                (ATTENTION_REFRESH_MIN_INTERVAL..=ATTENTION_REFRESH_MAX_INTERVAL)
                    .contains(&interval),
                "interval {interval:?} for previous {millis}ms escaped its bounds"
            );
        }
    }

    #[test]
    fn test_a_throttled_moment_admits_once_its_own_interval_elapses() {
        // The distinguishing property of RefuseThrottled: it passes on its own.
        let previous_duration = Duration::from_millis(500);
        let interval = attention_refresh_interval(Some(previous_duration));
        let throttled = AttentionRefreshFacts {
            since_previous: Some(interval.saturating_sub(Duration::from_millis(1))),
            previous_duration: Some(previous_duration),
            ..never_refreshed()
        };
        assert_eq!(
            admit_attention_refresh(throttled),
            AttentionRefreshAdmission::RefuseThrottled
        );
        let elapsed = AttentionRefreshFacts {
            since_previous: Some(interval),
            ..throttled
        };
        assert_eq!(
            admit_attention_refresh(elapsed),
            AttentionRefreshAdmission::Start
        );
    }
}
