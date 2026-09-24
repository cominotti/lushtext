// SPDX-License-Identifier: GPL-3.0-or-later

//! Set-aside retention: the soft bound, the review notice, and the deletion
//! decision for `drafts/set-aside/`.
//!
//! GTK-free and I/O-free. The set-aside area ([`super::set_aside`]) owns every
//! draft body that left the journal without being applied, and **nothing here
//! deletes one**: crossing the soft bound only makes a review notice due, and
//! the only decision that admits a deletion is [`may_delete`] under the user's
//! own confirmed Delete. The service (`set_aside::delete_confirmed`) applies it
//! to each body immediately before removing it, so the set of bodies one
//! confirmation deletes is exactly the bodies it admits. Kani (`kani_proofs`
//! below) and a property test check its safety properties:
//!
//! - **R1** with no user decision, nothing is deleted;
//! - **R2** every deleted body's current fingerprint is one the user confirmed;
//! - **R3** a body that changed after the confirmation, or that the
//!   confirmation did not list, is never deleted;
//! - **R4** the bound and notice decisions never panic, and none of them feeds
//!   the deletion decision.

use crate::services::filesystem::FileIdentity;

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

impl SetAsideTotals {
    /// Whether some of the counted bodies are not among the `listed` rows.
    #[must_use]
    pub fn is_truncated(self, listed: usize) -> bool {
        !self.complete || u64::try_from(listed).unwrap_or(u64::MAX) < self.count
    }
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
    pub identity: Option<FileIdentity>,
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
    /// The user confirmed a Delete (one row, or "Delete All Preserved
    /// Drafts…") over exactly these bodies, as the confirmation listed them.
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
    fn only_confirmed_unchanged_bodies_may_be_deleted() {
        let confirmed = vec![1u8, 2, 3];
        let decision = UserDecision::DeleteAll {
            confirmed: &confirmed,
        };
        assert!(!may_delete(&1u8, UserDecision::None));
        assert!(may_delete(&3u8, decision));
        assert!(
            !may_delete(&9u8, decision),
            "9 appeared (or changed) after the confirmation"
        );
        assert!(!may_delete(
            &1u8,
            UserDecision::DeleteAll { confirmed: &[] }
        ));
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

    /// The bodies one confirmed deletion removes: [`may_delete`] applied to
    /// each body as it reads now, as `set_aside::delete_confirmed` does. Swap in
    /// a broken decision here to see the properties below fail (task 2.2 did,
    /// with "every listed body").
    fn deleted_under_test(
        current: &[(usize, u8)],
        decision: UserDecision<'_, (usize, u8)>,
    ) -> Vec<usize> {
        current
            .iter()
            .enumerate()
            .filter(|(_, now)| may_delete(*now, decision))
            .map(|(index, _)| index)
            .collect()
    }

    proptest! {
        /// R1–R3 over arbitrary bodies: names are unique (the index), and a
        /// body is "changed" when its current version differs from the one
        /// the confirmation showed.
        #[test]
        fn deletion_never_reaches_past_the_confirmed_bodies(
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

            let deleted = deleted_under_test(&current, decision);

            if !decided {
                prop_assert!(deleted.is_empty(), "R1");
            }
            for &index in &deleted {
                prop_assert!(confirmed.contains(&current[index]), "R2");
                let body = &bodies[index];
                prop_assert!(body.listed && body.current_version == body.shown_version, "R3");
            }
            if decided {
                // Nothing confirmed and unchanged is left out.
                for (index, body) in bodies.iter().enumerate() {
                    if body.listed && body.current_version == body.shown_version {
                        prop_assert!(deleted.contains(&index));
                    }
                }
            }
        }
    }
}

#[cfg(kani)]
mod kani_proofs;
