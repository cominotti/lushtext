// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain main-thread ordering state for draft persistence commands.

use std::collections::HashMap;

/// User intent assigned before a draft snapshot or deletion starts background work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftMutationIntent {
    pub(super) draft_id: String,
    pub(super) sequence: u64,
    pub(super) epoch: u64,
}

/// Per-window allocator for globally ordered commands and per-draft freshness epochs.
#[derive(Debug, Default)]
pub(super) struct DraftMutationOrder {
    next_sequence: u64,
    epochs: HashMap<String, u64>,
}

impl DraftMutationOrder {
    /// Assign intent synchronously, before document-sized or filesystem work can reorder it.
    pub(super) fn advance(&mut self, draft_id: &str) -> DraftMutationIntent {
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let epoch = self
            .epochs
            .entry(draft_id.to_string())
            .and_modify(|epoch| *epoch = epoch.wrapping_add(1))
            .or_insert(1);
        DraftMutationIntent {
            draft_id: draft_id.to_string(),
            sequence: self.next_sequence,
            epoch: *epoch,
        }
    }

    /// Equality, rather than numeric ordering, keeps freshness correct across wraparound.
    pub(super) fn is_current(&self, intent: &DraftMutationIntent) -> bool {
        self.epochs.get(&intent.draft_id).copied() == Some(intent.epoch)
    }

    /// Drop a completed identity only when no later user intent superseded it.
    pub(super) fn retire_if_current(&mut self, intent: &DraftMutationIntent) {
        if self.is_current(intent) {
            self.epochs.remove(&intent.draft_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_delete_invalidates_older_autosave_intent() {
        let mut order = DraftMutationOrder::default();
        let autosave = order.advance("draft-a");
        let delete = order.advance("draft-a");

        assert!(!order.is_current(&autosave));
        assert!(order.is_current(&delete));
        assert_eq!(delete.sequence, autosave.sequence + 1);
    }

    #[test]
    fn later_edit_can_create_recovery_after_delete() {
        let mut order = DraftMutationOrder::default();
        let _autosave = order.advance("draft-a");
        let delete = order.advance("draft-a");
        let later_edit = order.advance("draft-a");

        assert!(!order.is_current(&delete));
        assert!(order.is_current(&later_edit));
    }

    #[test]
    fn one_draft_does_not_invalidate_another() {
        let mut order = DraftMutationOrder::default();
        let first = order.advance("draft-a");
        let second = order.advance("draft-b");

        assert!(order.is_current(&first));
        assert!(order.is_current(&second));
        assert!(second.sequence > first.sequence);
    }

    #[test]
    fn wraparound_uses_exact_epoch_equality() {
        let mut order = DraftMutationOrder {
            next_sequence: u64::MAX,
            epochs: HashMap::from([("draft-a".to_string(), u64::MAX)]),
        };
        let wrapped = order.advance("draft-a");

        assert_eq!(wrapped.sequence, 0);
        assert_eq!(wrapped.epoch, 0);
        assert!(order.is_current(&wrapped));
        assert!(!order.is_current(&DraftMutationIntent {
            draft_id: "draft-a".to_string(),
            sequence: u64::MAX,
            epoch: u64::MAX,
        }));
    }

    #[test]
    fn completed_delete_retires_only_the_current_identity() {
        let mut order = DraftMutationOrder::default();
        let current = order.advance("draft-a");
        let stale = order.advance("draft-b");
        let later = order.advance("draft-b");

        order.retire_if_current(&current);
        order.retire_if_current(&stale);

        assert!(!order.is_current(&current));
        assert!(order.is_current(&later));
    }
}
