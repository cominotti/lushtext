// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests for the attention-refresh throttle.
//!
//! The throttle's whole purpose is a negative invariant: **no sequence of
//! attention moments, however adversarial, may produce two refreshes closer
//! together than the declared floor.** The trigger is window activation, which
//! on a focus-follows-mouse desktop fires continuously, so a hole here is an
//! I/O storm rather than a cosmetic defect. A closed-form unit test cannot
//! cover it, because the hole would be in the interaction between the
//! admission decision and the state it leaves behind.
//!
//! The simulation below therefore replays generated trigger sequences through
//! the real decision, advancing a virtual clock, and asserts the invariant over
//! the refreshes that actually started.

use lushtext_core::model::attention_refresh::{
    ATTENTION_REFRESH_MAX_INTERVAL, ATTENTION_REFRESH_MIN_INTERVAL, AttentionRefreshAdmission,
    AttentionRefreshFacts, admit_attention_refresh, attention_refresh_interval,
};
use proptest::prelude::*;
use std::time::Duration;

/// One generated attention moment.
#[derive(Clone, Copy, Debug)]
struct Trigger {
    /// Virtual milliseconds since the previous trigger.
    gap_ms: u64,
    /// Milliseconds the refresh takes, if this trigger starts one.
    refresh_ms: u64,
    /// Whether user work was in flight at this moment.
    user_work: bool,
}

/// Milliseconds biased toward the small values the invariants actually turn on.
///
/// A flat `0..120_000` looks thorough and is nearly vacuous here: the floor is
/// 2,000 ms and the ceiling is reached by any previous duration at or above
/// 6,000 ms, so a uniform draw spends almost every case in the saturated region
/// where the decision is constant. Verified — with a flat generator these
/// properties passed against a build with the floor clamp removed. The weights
/// below put most cases in the sub-floor band where a hole would be.
fn millis() -> impl Strategy<Value = u64> {
    prop_oneof![
        6 => 0u64..3_000,       // around and below the floor
        3 => 3_000u64..12_000,  // around the ceiling knee
        1 => 12_000u64..600_000 // saturated
    ]
}

fn trigger() -> impl Strategy<Value = Trigger> {
    (millis(), millis(), proptest::bool::ANY).prop_map(|(gap_ms, refresh_ms, user_work)| Trigger {
        gap_ms,
        refresh_ms,
        user_work,
    })
}

/// Replay a trigger sequence, returning the virtual start time of each refresh
/// that the policy actually admitted.
///
/// The simulation models the one thing the unit tests cannot: a refresh occupies
/// the surface for its whole duration, so a trigger arriving inside that window
/// must be refused as in-flight rather than throttled.
fn admitted_starts(triggers: &[Trigger]) -> Vec<Duration> {
    let mut now = Duration::ZERO;
    let mut starts: Vec<Duration> = Vec::new();
    // When the in-flight refresh settles, and how long it took.
    let mut settles_at: Option<Duration> = None;
    let mut previous_settled: Option<Duration> = None;
    let mut previous_duration: Option<Duration> = None;

    for trigger in triggers {
        now = now.saturating_add(Duration::from_millis(trigger.gap_ms));
        if let Some(settle) = settles_at
            && now >= settle
        {
            previous_settled = Some(settle);
            settles_at = None;
        }
        let facts = AttentionRefreshFacts {
            in_flight: settles_at.is_some(),
            since_previous: previous_settled.map(|settled| now.saturating_sub(settled)),
            previous_duration,
            user_work_in_flight: trigger.user_work,
        };
        if admit_attention_refresh(facts) == AttentionRefreshAdmission::Start {
            starts.push(now);
            let duration = Duration::from_millis(trigger.refresh_ms);
            previous_duration = Some(duration);
            settles_at = Some(now.saturating_add(duration));
        }
    }
    starts
}

proptest! {
    /// The invariant the throttle exists for.
    #[test]
    fn prop_admitted_refreshes_never_start_closer_than_the_floor(
        triggers in proptest::collection::vec(trigger(), 0..40)
    ) {
        for pair in admitted_starts(&triggers).windows(2) {
            let gap = pair[1].saturating_sub(pair[0]);
            prop_assert!(
                gap >= ATTENTION_REFRESH_MIN_INTERVAL,
                "two refreshes started {gap:?} apart, inside the {ATTENTION_REFRESH_MIN_INTERVAL:?} floor"
            );
        }
    }

    /// User work is an absolute veto: no generated sequence may start a refresh
    /// at a moment when a save, close, or draft autosave was in flight.
    #[test]
    fn prop_user_work_is_never_overridden(
        triggers in proptest::collection::vec(trigger(), 0..40)
    ) {
        let always_busy: Vec<Trigger> = triggers
            .iter()
            .map(|trigger| Trigger { user_work: true, ..*trigger })
            .collect();
        prop_assert!(
            admitted_starts(&always_busy).is_empty(),
            "a refresh started while user work was continuously in flight"
        );
    }

    /// The interval is total and bounded for every representable input, so no
    /// previous duration can produce an interval outside the declared range.
    #[test]
    fn prop_interval_stays_within_its_bounds(previous_ms in millis()) {
        let interval = attention_refresh_interval(Some(Duration::from_millis(previous_ms)));
        prop_assert!(interval >= ATTENTION_REFRESH_MIN_INTERVAL);
        prop_assert!(interval <= ATTENTION_REFRESH_MAX_INTERVAL);
    }

    /// The interval is monotonic in the previous duration: a slower refresh can
    /// never earn a shorter cooldown than a faster one.
    #[test]
    fn prop_interval_is_monotonic(a_ms in millis(), b_ms in millis()) {
        let (slower, faster) = if a_ms >= b_ms { (a_ms, b_ms) } else { (b_ms, a_ms) };
        prop_assert!(
            attention_refresh_interval(Some(Duration::from_millis(slower)))
                >= attention_refresh_interval(Some(Duration::from_millis(faster)))
        );
    }
}
