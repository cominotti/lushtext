// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the set-aside retention core: R1–R4 over four bodies
//! with arbitrary fingerprints, listing, and change facts, and an arbitrary
//! user decision.
//!
//! The harnesses check [`may_delete`], the per-body decision the service
//! applies immediately before each removal, over fixed-size arrays (no heap),
//! which keeps them to seconds. Its composition over every confirmed body is
//! covered by the property test in the parent module.

use super::{
    BoundStatus, MATERIAL_GROWTH_PERCENT, SOFT_BOUND_BODIES, SOFT_BOUND_BYTES, SetAsideTotals,
    UserDecision, bound_status, may_delete, notice_due,
};

/// Bodies in the model.
const BODIES: usize = 4;

/// A body's fingerprint in the model: its (unique) name and a version.
type ModelFingerprint = (u8, u8);

/// Four bodies: each has a current fingerprint, and the confirmation may have
/// listed it under the version it showed then.
struct Model {
    current: [ModelFingerprint; BODIES],
    /// The confirmed fingerprints; the first `confirmed_len` entries count.
    confirmed: [ModelFingerprint; BODIES],
    confirmed_len: usize,
    /// The confirmation listed the body and it has not changed since.
    confirmed_unchanged: [bool; BODIES],
}

impl Model {
    fn any() -> Self {
        let mut model = Self {
            current: [(0, 0); BODIES],
            confirmed: [(0, 0); BODIES],
            confirmed_len: 0,
            confirmed_unchanged: [false; BODIES],
        };
        for name in 0..BODIES {
            let shown: u8 = kani::any();
            let now: u8 = kani::any();
            let listed: bool = kani::any();
            let tag = u8::try_from(name).unwrap_or(u8::MAX);
            model.current[name] = (tag, now);
            if listed {
                model.confirmed[model.confirmed_len] = (tag, shown);
                model.confirmed_len += 1;
            }
            model.confirmed_unchanged[name] = listed && shown == now;
        }
        model
    }

    fn confirmed(&self) -> &[ModelFingerprint] {
        &self.confirmed[..self.confirmed_len]
    }

    fn any_decision(&self) -> UserDecision<'_, ModelFingerprint> {
        if kani::any() {
            UserDecision::DeleteAll {
                confirmed: self.confirmed(),
            }
        } else {
            UserDecision::None
        }
    }
}

fn check_decision(decide: fn(&ModelFingerprint, UserDecision<'_, ModelFingerprint>) -> bool) {
    let model = Model::any();
    let decision = model.any_decision();
    for (index, now) in model.current.iter().enumerate() {
        if !decide(now, decision) {
            continue;
        }
        assert!(
            !matches!(decision, UserDecision::None),
            "R1: no decision, no deletion"
        );
        assert!(
            model.confirmed().contains(now),
            "R2: a body the user did not confirm may be deleted"
        );
        assert!(
            model.confirmed_unchanged[index],
            "R3: a changed or unlisted body may be deleted"
        );
    }
}

/// R1–R3 for the production decision, and their converse: under a confirmed
/// decision every listed, unchanged body may be deleted, so "Delete All" is not
/// satisfied by deleting nothing. The converse was added by
/// `measure-proof-strength-with-mutation`: `may_delete -> false` survived
/// every harness, because R1–R3 alone hold for a decision that never deletes.
#[kani::proof]
#[kani::unwind(6)]
fn set_aside_retention_deletes_only_confirmed_bodies() {
    check_decision(may_delete);
    let model = Model::any();
    let decision = UserDecision::DeleteAll {
        confirmed: model.confirmed(),
    };
    for (index, now) in model.current.iter().enumerate() {
        if model.confirmed_unchanged[index] {
            assert!(
                may_delete(now, decision),
                "a confirmed, unchanged body may not be deleted"
            );
        }
    }
}

/// R4: the bound and notice decisions are total, and the deletion decision
/// takes no input from them: with no user decision it admits no body whatever
/// the status.
#[kani::proof]
#[kani::unwind(6)]
fn set_aside_retention_bound_and_notice_never_plan_a_deletion() {
    let totals = |fail: bool| {
        (!fail).then(|| SetAsideTotals {
            count: kani::any(),
            bytes: kani::any(),
            complete: kani::any(),
        })
    };
    let current = totals(kani::any());
    let last = totals(kani::any());
    let _ = bound_status(current);
    let _ = notice_due(current, last, kani::any());
    let model = Model::any();
    for now in &model.current {
        assert!(!may_delete(now, UserDecision::None));
    }
}

/// R5: the soft bound and the notice rate limit, as the programme record
/// states them (step 8a). The area is over the bound past 100 bodies or
/// 256 MiB, or when the scan was incomplete; a notice is due only while it is
/// over the bound and the user has not reviewed it in this process, and then
/// on the first evaluation or once the count or the size has grown by at least
/// 25 % past the totals last notified. Added by
/// `measure-proof-strength-with-mutation`: R4 only states that these decisions
/// are total, so every mutant of `bound_status`, `notice_due`, and their growth
/// test survived every harness.
#[kani::proof]
fn set_aside_retention_notice_follows_the_bound_and_its_rate_limit() {
    assert_eq!(SOFT_BOUND_BODIES, 100);
    assert_eq!(SOFT_BOUND_BYTES, 256 * 1024 * 1024);
    assert_eq!(MATERIAL_GROWTH_PERCENT, 25);
    let current = SetAsideTotals {
        count: kani::any(),
        bytes: kani::any(),
        complete: kani::any(),
    };
    let by_count = current.count > 100 || !current.complete;
    let by_bytes = current.bytes > 256 * 1024 * 1024;
    let status = bound_status(Some(current));
    if by_count || by_bytes {
        assert_eq!(status, BoundStatus::Over { by_count, by_bytes });
    } else {
        assert_eq!(status, BoundStatus::Within);
    }
    assert_eq!(bound_status(None), BoundStatus::Unknown);

    let grown = |last: u64, now: u64| now > last && u128::from(now) * 4 >= u128::from(last) * 5;
    let last = SetAsideTotals {
        count: kani::any(),
        bytes: kani::any(),
        complete: kani::any(),
    };
    let over = by_count || by_bytes;
    let reviewed: bool = kani::any();
    assert_eq!(notice_due(Some(current), None, reviewed), over && !reviewed);
    assert_eq!(
        notice_due(Some(current), Some(last), reviewed),
        over && !reviewed && (grown(last.count, current.count) || grown(last.bytes, current.bytes))
    );
    assert!(!notice_due(None, Some(last), false));
    kani::cover!(
        over && grown(last.count, current.count),
        "a grown area is due again"
    );
}

/// A decision that deletes every body once the user decided anything,
/// whatever the confirmation listed, breaks R2, so the harness above catches
/// that defect.
#[kani::proof]
#[kani::should_panic]
#[kani::unwind(6)]
fn set_aside_retention_deleting_every_body_breaks_r2() {
    fn every_body(_now: &ModelFingerprint, decision: UserDecision<'_, ModelFingerprint>) -> bool {
        !matches!(decision, UserDecision::None)
    }
    check_decision(every_body);
}
