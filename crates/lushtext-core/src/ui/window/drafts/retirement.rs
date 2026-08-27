// SPDX-License-Identifier: GPL-3.0-or-later

//! Deferred, off-GTK destruction of preloaded draft recovery bodies.
//!
//! The `retirement` coordination job for `WFR-DRAFT-RECOVERY`: it hands
//! document-sized recovery bodies to a worker for destruction while keeping the
//! compact markers the restore stage order still needs. It is the counterpart to
//! `journal`, which *maintains* the durable record — this module destroys a
//! payload the workflow is finished with.
//!
//! Startup can eagerly preload several recovery bodies under one aggregate
//! disposal reservation. Once restore has taken the ones it needs, the remainder
//! are document-sized allocations GTK should not free on the main thread, so they
//! leave through the disposal lane instead.

use std::collections::HashMap;

use crate::model::draft::{PreloadedDraftRestore, PreloadedDraftSkip};

/// Release every eager preload body to a worker, keeping the compact markers.
///
/// The markers must survive: a slow file load can still need one, and losing it
/// would let that tab bypass the serialized lazy admission queue.
pub(super) fn release_eager_preloads(
    preloaded: &mut crate::ui::plain_disposal::DisposalOwned<
        HashMap<String, PreloadedDraftRestore>,
    >,
) {
    let guarded = std::mem::take(preloaded);
    let (compact, retiring) = guarded.split_for_worker_retirement(detach_eager_preload_bodies);
    *preloaded = crate::ui::plain_disposal::DisposalOwned::small_unreserved(compact);
    drop(retiring);
}

/// Swap every body out for a lazy marker, returning the bodies to be retired.
///
/// Entries that are *already* compact keep their own marker rather than being
/// rewritten to `LazyAggregateBudget`: an `Oversized` skip records why that draft
/// was never eligible for eager preload at all, and flattening it would lose that
/// distinction and re-queue a body the workflow already declined.
fn detach_eager_preload_bodies(
    preloaded: &mut HashMap<String, PreloadedDraftRestore>,
) -> Vec<String> {
    let mut retiring = Vec::new();
    for restore in preloaded.values_mut() {
        match std::mem::replace(
            restore,
            PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget),
        ) {
            PreloadedDraftRestore::Content(content) => retiring.push(content),
            compact @ PreloadedDraftRestore::Skip(_) => *restore = compact,
        }
    }
    retiring
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Releasing eager bodies must leave a marker behind for **every** entry,
    /// and must preserve the specific marker an already-compact entry carried.
    ///
    /// A slow file load can still reach for its preload after the release pass
    /// has run. If the release dropped the entry outright, that tab would find
    /// nothing and bypass the serialized lazy admission queue — restoring
    /// unbounded recovery text outside the aggregate reservation. If it instead
    /// flattened `Oversized` into `LazyAggregateBudget`, a draft that was
    /// declined for its size would be re-queued as if it were merely deferred.
    #[test]
    fn eager_preload_release_preserves_lazy_markers_for_slow_file_loads() {
        let mut preloaded = HashMap::from([
            (
                "eager".to_string(),
                PreloadedDraftRestore::Content("body".to_string()),
            ),
            (
                "lazy".to_string(),
                PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget),
            ),
            (
                "oversized".to_string(),
                PreloadedDraftRestore::Skip(PreloadedDraftSkip::Oversized),
            ),
        ]);

        let retired = detach_eager_preload_bodies(&mut preloaded);

        assert_eq!(
            preloaded,
            HashMap::from([
                (
                    "eager".to_string(),
                    PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget)
                ),
                (
                    "lazy".to_string(),
                    PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget)
                ),
                (
                    "oversized".to_string(),
                    PreloadedDraftRestore::Skip(PreloadedDraftSkip::Oversized)
                ),
            ]),
            "every entry keeps a marker, and an already-compact marker is preserved"
        );
        assert_eq!(
            retired,
            vec!["body".to_string()],
            "only real bodies are handed to the worker"
        );
    }

    /// An all-compact map retires nothing and is left byte-identical.
    #[test]
    fn eager_preload_release_with_no_bodies_retires_nothing() {
        let original = HashMap::from([
            (
                "a".to_string(),
                PreloadedDraftRestore::Skip(PreloadedDraftSkip::Oversized),
            ),
            (
                "b".to_string(),
                PreloadedDraftRestore::Skip(PreloadedDraftSkip::LazyAggregateBudget),
            ),
        ]);
        let mut preloaded = original.clone();

        let retired = detach_eager_preload_bodies(&mut preloaded);

        assert!(retired.is_empty());
        assert_eq!(preloaded, original);
    }

    /// An empty map is a no-op, not a panic.
    #[test]
    fn eager_preload_release_handles_an_empty_map() {
        let mut preloaded: HashMap<String, PreloadedDraftRestore> = HashMap::new();
        assert!(detach_eager_preload_bodies(&mut preloaded).is_empty());
        assert!(preloaded.is_empty());
    }
}
