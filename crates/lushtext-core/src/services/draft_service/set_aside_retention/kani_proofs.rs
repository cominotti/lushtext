// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses for the set-aside retention core: R1–R4 over four bodies
//! with arbitrary fingerprints, listing, and change facts, and an arbitrary
//! user decision.
//!
//! The harnesses check [`may_delete`], the per-body decision the service
//! applies immediately before each removal, over fixed-size arrays (no heap),
//! which keeps them to seconds. Its composition over every confirmed body is
//! covered by the property test in the parent module.

use super::{SetAsideTotals, UserDecision, bound_status, may_delete, notice_due};

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

/// R1–R3 for the production decision.
#[kani::proof]
#[kani::unwind(6)]
fn set_aside_retention_deletes_only_confirmed_bodies() {
    check_decision(may_delete);
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
