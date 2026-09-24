// SPDX-License-Identifier: GPL-3.0-or-later

//! Set-aside retention: the soft bound, the review notice, and the bulk
//! deletion plan for `drafts/set-aside/`.
//!
//! GTK-free and I/O-free. The set-aside area ([`super::set_aside`]) owns every
//! draft body that left the journal without being applied, and **nothing here
//! deletes one**: crossing the soft bound only makes a review notice due, and
//! the only plan that names bodies to delete is the one built from the user's
//! own confirmed "Delete All Preserved Drafts…" decision. Kani (`kani_proofs`
//! below) and a property test check the plan's safety properties:
//!
//! - **R1** with no user decision, the plan is empty;
//! - **R2** every planned body's current fingerprint is one the user confirmed;
//! - **R3** a body that changed after the confirmation, or that the
//!   confirmation did not list, is never planned;
//! - **R4** the bound and notice decisions never panic, and none of them feeds
//!   the plan.
//!
//! The per-body decision is [`may_delete`]; [`deletion_plan`] is that decision
//! applied to every body found, and the service applies it to each body again
//! immediately before removing it.

/// Preserved bodies past which the user is asked to review the area.
pub const SOFT_BOUND_BODIES: u64 = 100;
/// Preserved bytes past which the user is asked to review the area (four
/// maximum-size draft bodies).
pub const SOFT_BOUND_BYTES: u64 = 256 * 1024 * 1024;
/// Growth, in percent of the last notified count or size, that makes another
/// notice due after a placement in the same process.
pub const MATERIAL_GROWTH_PERCENT: u64 = 25;

/// What one scan of the set-aside area found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetAsideTotals {
    /// Preserved bodies counted (a lower bound when `complete` is false).
    pub count: u64,
    /// Their total size in bytes (a lower bound when `complete` is false).
    pub bytes: u64,
    /// The scan reached the end of the area within its budget.
    pub complete: bool,
}

/// Where the set-aside area stands against the soft bound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundStatus {
    /// Within both limits.
    Within,
    /// Past at least one limit. An incomplete scan counts as past the count
    /// limit, because the scan budget is far above it.
    Over {
        /// Past [`SOFT_BOUND_BODIES`].
        by_count: bool,
        /// Past [`SOFT_BOUND_BYTES`].
        by_bytes: bool,
    },
    /// The area could not be scanned.
    Unknown,
}

/// Classify one scan against the soft bound; `None` is a failed scan.
#[must_use]
pub const fn bound_status(totals: Option<SetAsideTotals>) -> BoundStatus {
    let Some(totals) = totals else {
        return BoundStatus::Unknown;
    };
    let by_count = totals.count > SOFT_BOUND_BODIES || !totals.complete;
    let by_bytes = totals.bytes > SOFT_BOUND_BYTES;
    if by_count || by_bytes {
        BoundStatus::Over { by_count, by_bytes }
    } else {
        BoundStatus::Within
    }
}

/// Whether `now` has grown by at least [`MATERIAL_GROWTH_PERCENT`] past `last`.
const fn grown_materially(last: u64, now: u64) -> bool {
    // u128 so neither side can overflow for any u64 input.
    now > last && (now as u128) * 100 >= (last as u128) * (100 + MATERIAL_GROWTH_PERCENT as u128)
}

/// Whether a review notice is due, from this process's in-memory rate limit.
///
/// Due only while the area is over the bound, and then only when no notice was
/// published yet in this process, or the area has grown materially in count
/// or size past the totals last notified. Never due again in a process once
/// the user has run the review action. Nothing here is persisted.
#[must_use]
pub const fn notice_due(
    current: Option<SetAsideTotals>,
    last_notified: Option<SetAsideTotals>,
    reviewed_this_process: bool,
) -> bool {
    if reviewed_this_process {
        return false;
    }
    let (BoundStatus::Over { .. }, Some(current)) = (bound_status(current), current) else {
        return false;
    };
    match last_notified {
        None => true,
        Some(last) => {
            grown_materially(last.count, current.count)
                || grown_materially(last.bytes, current.bytes)
        }
    }
}

/// What identifies one set-aside body as the user saw it: its name, size, file
/// identity, and modification time. Names are never reused while their file exists, and a new
/// body under a freed name has a new identity, so a match means "the body the
/// confirmation listed, unchanged". A false mismatch only keeps a body.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SetAsideFingerprint {
    /// The file name inside the set-aside area.
    pub file_name: String,
    /// Its size in bytes.
    pub byte_size: u64,
    /// Device and inode, when the filesystem reports them.
    pub identity: Option<(u64, u64)>,
    /// Modification time in nanoseconds, when reported. Set-aside files are
    /// never rewritten in place, so this also tells a new file apart from an
    /// earlier one that reused its freed name and inode.
    pub modified_at_nanos: Option<u128>,
}

/// The user's decision about preserved bodies.
#[derive(Debug, PartialEq, Eq)]
pub enum UserDecision<'a, F = SetAsideFingerprint> {
    /// No decision: nothing may be deleted.
    None,
    /// The user confirmed "Delete All Preserved Drafts…" over exactly these
    /// bodies, as the confirmation dialog listed them.
    DeleteAll {
        /// The fingerprints the confirmation showed.
        confirmed: &'a [F],
    },
}

// Manual impls: a derive would require `F: Copy`, but the decision only
// borrows its fingerprints.
impl<F> Clone for UserDecision<'_, F> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<F> Copy for UserDecision<'_, F> {}

/// Whether one body, as its fingerprint reads **now** (re-read just before
/// deleting), may be deleted under the user's decision: only when that
/// fingerprint is one the user confirmed (R1–R3).
#[must_use]
pub fn may_delete<F: PartialEq>(now: &F, decision: UserDecision<'_, F>) -> bool {
    match decision {
        UserDecision::None => false,
        UserDecision::DeleteAll { confirmed } => confirmed.contains(now),
    }
}

/// Which of the bodies found now may be deleted: indices into the `current`
/// slice passed to [`deletion_plan`], in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeletionPlan {
    /// Positions of the bodies to delete.
    pub indices: Vec<usize>,
}

/// Plan a bulk deletion from the bodies' **current** fingerprints and the
/// user's decision: exactly the bodies [`may_delete`] admits, so a body that
/// appeared or changed after the confirmation dialog opened is kept.
#[must_use]
pub fn deletion_plan<F: PartialEq>(current: &[F], decision: UserDecision<'_, F>) -> DeletionPlan {
    if matches!(decision, UserDecision::None) {
        return DeletionPlan::default();
    }
    DeletionPlan {
        indices: current
            .iter()
            .enumerate()
            .filter(|(_, body)| may_delete(*body, decision))
            .map(|(index, _)| index)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn totals(count: u64, bytes: u64) -> Option<SetAsideTotals> {
        Some(SetAsideTotals {
            count,
            bytes,
            complete: true,
        })
    }

    #[test]
    fn the_bound_is_crossed_by_count_or_size_and_a_truncated_scan_counts_as_over() {
        assert_eq!(bound_status(None), BoundStatus::Unknown);
        assert_eq!(bound_status(totals(0, 0)), BoundStatus::Within);
        assert_eq!(
            bound_status(totals(SOFT_BOUND_BODIES, SOFT_BOUND_BYTES)),
            BoundStatus::Within
        );
        assert_eq!(
            bound_status(totals(SOFT_BOUND_BODIES + 1, 0)),
            BoundStatus::Over {
                by_count: true,
                by_bytes: false
            }
        );
        assert_eq!(
            bound_status(totals(1, SOFT_BOUND_BYTES + 1)),
            BoundStatus::Over {
                by_count: false,
                by_bytes: true
            }
        );
        assert_eq!(
            bound_status(Some(SetAsideTotals {
                count: 3,
                bytes: 3,
                complete: false,
            })),
            BoundStatus::Over {
                by_count: true,
                by_bytes: false
            }
        );
    }

    #[test]
    fn a_notice_is_due_once_then_only_on_material_growth_and_never_after_review() {
        let over = totals(120, 1);
        assert!(!notice_due(totals(5, 5), None, false), "within the bound");
        assert!(!notice_due(None, None, false), "a failed scan says nothing");
        assert!(notice_due(over, None, false), "the first notice");
        assert!(!notice_due(over, None, true), "the user already reviewed");
        assert!(
            !notice_due(totals(149, 1), over, false),
            "under 25 % growth"
        );
        assert!(notice_due(totals(150, 1), over, false), "25 % more bodies");
        assert!(
            notice_due(totals(120, 2), over, false),
            "the size doubled even though the count did not grow"
        );
        assert!(!notice_due(totals(150, 1), over, true));
        assert!(!notice_due(
            totals(u64::MAX, u64::MAX),
            totals(u64::MAX, u64::MAX),
            false
        ));
    }

    #[test]
    fn only_confirmed_unchanged_bodies_are_planned() {
        let confirmed = vec![1u8, 2, 3];
        assert_eq!(
            deletion_plan(&[1u8, 2, 3], UserDecision::None),
            DeletionPlan::default()
        );
        assert_eq!(
            deletion_plan(
                &[3u8, 9, 1],
                UserDecision::DeleteAll {
                    confirmed: &confirmed
                }
            )
            .indices,
            vec![0, 2],
            "9 appeared (or changed) after the confirmation"
        );
        assert!(
            deletion_plan::<u8>(
                &[],
                UserDecision::DeleteAll {
                    confirmed: &confirmed
                }
            )
            .indices
            .is_empty()
        );
    }

    /// One body in the property model: its name, the version the
    /// confirmation saw (when it listed it), and its current version.
    #[derive(Clone, Debug)]
    struct ModelBody {
        listed: bool,
        shown_version: u8,
        current_version: u8,
    }

    fn model_bodies() -> impl Strategy<Value = Vec<ModelBody>> {
        prop::collection::vec(
            (any::<bool>(), 0u8..3, 0u8..3).prop_map(|(listed, shown, current)| ModelBody {
                listed,
                shown_version: shown,
                current_version: current,
            }),
            0..8,
        )
    }

    /// The plan the properties below check; swap in a broken plan here to see
    /// them fail (task 2.2 did, with "every listed body").
    fn plan_under_test(
        current: &[(usize, u8)],
        decision: UserDecision<'_, (usize, u8)>,
    ) -> DeletionPlan {
        deletion_plan(current, decision)
    }

    proptest! {
        /// R1–R3 over arbitrary bodies: names are unique (the index), and a
        /// body is "changed" when its current version differs from the one
        /// the confirmation showed.
        #[test]
        fn the_plan_never_reaches_past_the_confirmed_bodies(
            bodies in model_bodies(),
            decided in any::<bool>(),
        ) {
            let current: Vec<(usize, u8)> = bodies
                .iter()
                .enumerate()
                .map(|(name, body)| (name, body.current_version))
                .collect();
            let confirmed: Vec<(usize, u8)> = bodies
                .iter()
                .enumerate()
                .filter(|(_, body)| body.listed)
                .map(|(name, body)| (name, body.shown_version))
                .collect();
            let decision = if decided {
                UserDecision::DeleteAll { confirmed: &confirmed }
            } else {
                UserDecision::None
            };

            let plan = plan_under_test(&current, decision);

            if !decided {
                prop_assert!(plan.indices.is_empty(), "R1");
            }
            for &index in &plan.indices {
                prop_assert!(confirmed.contains(&current[index]), "R2");
                let body = &bodies[index];
                prop_assert!(body.listed && body.current_version == body.shown_version, "R3");
            }
            if decided {
                // Nothing confirmed and unchanged is left out.
                for (index, body) in bodies.iter().enumerate() {
                    if body.listed && body.current_version == body.shown_version {
                        prop_assert!(plan.indices.contains(&index));
                    }
                }
            }
        }
    }
}

#[cfg(kani)]
mod kani_proofs;
