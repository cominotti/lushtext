// SPDX-License-Identifier: GPL-3.0-or-later

//! Generated invariants for transient file-load admission and UTF-8 slicing.

use std::collections::BTreeSet;

use lushtext_core::model::file_load::{
    FileLoadAdmissionPolicy, FileLoadAdmissionRequest, FileLoadPriority,
    TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES, TRANSIENT_LOAD_SOURCE_MULTIPLIER, next_install_boundary,
    transient_load_weight,
};
use proptest::prelude::*;

proptest! {
    #[test]
    fn transient_weight_never_wraps(source_bytes in any::<u64>()) {
        let weight = transient_load_weight(source_bytes);
        let expected = source_bytes
            .saturating_mul(TRANSIENT_LOAD_SOURCE_MULTIPLIER)
            .saturating_add(TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES);
        prop_assert_eq!(weight, expected);
        prop_assert!(weight >= TRANSIENT_LOAD_FIXED_OVERHEAD_BYTES);
    }

    #[test]
    fn install_boundaries_reconstruct_exact_unicode(
        characters in prop::collection::vec(any::<char>(), 0..100_000)
    ) {
        let text = characters.into_iter().collect::<String>();
        let mut reconstructed = String::new();
        let mut start = 0usize;
        while start < text.len() {
            let end = next_install_boundary(&text, start);
            prop_assert!(end > start);
            prop_assert!(end <= text.len());
            prop_assert!(text.is_char_boundary(end));
            reconstructed.push_str(&text[start..end]);
            start = end;
        }
        prop_assert_eq!(reconstructed, text);
    }

    #[test]
    fn queued_requests_eventually_release_without_duplicates(
        weights in prop::collection::vec(1u64..=200, 0..64),
        active_flags in prop::collection::vec(any::<bool>(), 0..64),
    ) {
        let mut policy = FileLoadAdmissionPolicy::new(100, 1);
        for (index, weight) in weights.iter().copied().enumerate() {
            let request_id = u64::try_from(index + 1).unwrap_or(u64::MAX);
            policy.queue(FileLoadAdmissionRequest {
                request_id,
                owner_id: request_id % 3,
                sequence: request_id,
                weight,
                priority: if active_flags.get(index).copied().unwrap_or(false) {
                    FileLoadPriority::Active
                } else {
                    FileLoadPriority::Normal
                },
            });
        }

        let mut admitted = BTreeSet::new();
        while let Some(grant) = policy.admit_next(false) {
            let snapshot = policy.snapshot();
            if grant.exclusive {
                prop_assert_eq!(snapshot.active_count, 1);
            } else {
                prop_assert!(snapshot.active_weight <= 100);
            }
            prop_assert!(admitted.insert(grant.request_id));
            prop_assert!(policy.release(grant.request_id));
            prop_assert!(!policy.release(grant.request_id));
        }

        prop_assert_eq!(admitted.len(), weights.len());
        prop_assert_eq!(policy.snapshot().active_weight, 0);
        prop_assert_eq!(policy.snapshot().queued_count, 0);
    }
}
