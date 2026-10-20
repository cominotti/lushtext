// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the document scale policy:
//! [`DocumentScale::for_profile`], [`DocumentScale::for_total_bytes`] and
//! [`DocumentScale::join`].
//!
//! Domain: every `u64` component of every profile, unbounded. The policy has
//! no loops, so no unwind bound is needed. The tier harness compares with the
//! frozen transcription of the retired byte-only classification
//! (`retired_byte_tiers`), not with production code.
//!
//! Compiled only under `cfg(kani)` (`make kani`).

use super::{
    DocumentScale, LocalHistoryAvailability, ProfileSummary, ReductionReason, ScaleFeature,
    retired_byte_tiers,
};

fn any_profile() -> ProfileSummary {
    ProfileSummary {
        total_bytes: kani::any(),
        longest_paragraph_chars: kani::any(),
        worst_batch_chars: kani::any(),
    }
}

/// The presentation outputs of a scale, the part the session floor governs.
fn presentation(scale: DocumentScale) -> (bool, bool, bool, bool, bool, bool) {
    (
        scale.syntax_enabled,
        scale.bracket_matching_enabled,
        scale.minimap_allowed,
        scale.invisibles_allowed,
        scale.search_highlight_allowed,
        scale.protected_view,
    )
}

/// Whether `later` enables no presentation feature `earlier` disables and
/// keeps `protected_view` when `earlier` has it.
fn never_reenables(earlier: DocumentScale, later: DocumentScale) -> bool {
    (!later.syntax_enabled || earlier.syntax_enabled)
        && (!later.bracket_matching_enabled || earlier.bracket_matching_enabled)
        && (!later.minimap_allowed || earlier.minimap_allowed)
        && (!later.invisibles_allowed || earlier.invisibles_allowed)
        && (!later.search_highlight_allowed || earlier.search_highlight_allowed)
        && (!earlier.protected_view || later.protected_view)
}

fn byte_tiers(scale: DocumentScale) -> (bool, bool, u64, LocalHistoryAvailability) {
    (
        scale.open_allowed,
        scale.undo_enabled,
        scale.estimated_buffer_multiplier,
        scale.local_history,
    )
}

#[kani::proof]
fn scale_is_monotone() {
    let small = any_profile();
    let large = any_profile();
    kani::assume(small.total_bytes <= large.total_bytes);
    kani::assume(small.longest_paragraph_chars <= large.longest_paragraph_chars);
    kani::assume(small.worst_batch_chars <= large.worst_batch_chars);
    let (s, l) = (
        DocumentScale::for_profile(small),
        DocumentScale::for_profile(large),
    );
    assert!(never_reenables(s, l));
    assert!(!l.open_allowed || s.open_allowed);
    assert!(!l.undo_enabled || s.undo_enabled);
    assert!(l.local_history <= s.local_history);
    assert!(l.estimated_buffer_multiplier <= s.estimated_buffer_multiplier);
    for feature in ScaleFeature::ALL {
        kani::cover!(s.allows(feature), "feature on");
        kani::cover!(!l.allows(feature), "feature off");
    }
    kani::cover!(
        l.local_history == LocalHistoryAvailability::Full,
        "history full"
    );
    kani::cover!(
        l.local_history == LocalHistoryAvailability::SaveOnly,
        "history save-only"
    );
    kani::cover!(
        l.local_history == LocalHistoryAvailability::Unavailable,
        "history unavailable"
    );
}

#[kani::proof]
fn scale_protected_view_is_upward_closed() {
    let small = any_profile();
    let large = any_profile();
    kani::assume(small.total_bytes <= large.total_bytes);
    kani::assume(small.longest_paragraph_chars <= large.longest_paragraph_chars);
    kani::assume(small.worst_batch_chars <= large.worst_batch_chars);
    let s = DocumentScale::for_profile(small);
    let l = DocumentScale::for_profile(large);
    assert!(!s.protected_view || l.protected_view);
    // Exactly the entry rule C2 consumes.
    assert!(
        s.protected_view
            == (small.longest_paragraph_chars > super::L_PROTECT
                || small.worst_batch_chars > super::V_BATCH)
    );
    for reason in [
        ReductionReason::TotalSize,
        ReductionReason::LongParagraph,
        ReductionReason::Density,
    ] {
        kani::cover!(
            l.reduced.has_reason(ScaleFeature::Syntax, reason)
                || l.reduced.has_reason(ScaleFeature::ProtectedView, reason),
            "each reason class"
        );
    }
}

#[kani::proof]
fn scale_join_is_a_meet_on_presentation_features() {
    let a = DocumentScale::for_profile(any_profile());
    let b = DocumentScale::for_profile(any_profile());
    let c = DocumentScale::for_profile(any_profile());
    // Commutative, associative and idempotent on the presentation features.
    assert!(presentation(a.join(b)) == presentation(b.join(a)));
    assert!(a.join(b).reduced.any_presentation() == b.join(a).reduced.any_presentation());
    assert!(presentation(a.join(b).join(c)) == presentation(a.join(b.join(c))));
    assert!(presentation(a.join(a)) == presentation(a));
    assert!(a.join(a).reduced == a.reduced);
    // Never re-enabling a feature either argument disables.
    assert!(never_reenables(a, a.join(b)));
    assert!(never_reenables(b, a.join(b)));
    // Byte tiers from the second argument.
    assert!(byte_tiers(a.join(b)) == byte_tiers(b));
    kani::cover!(
        a.syntax_enabled && !b.syntax_enabled,
        "join turns syntax off"
    );
}

#[kani::proof]
fn scale_of_a_profile_join_is_the_join_of_scales() {
    let p = any_profile();
    let q = any_profile();
    let joined = DocumentScale::for_profile(p).join(DocumentScale::for_profile(q));
    let of_max = DocumentScale::for_profile(p.max(q));
    assert!(presentation(of_max) == presentation(joined));
    for feature in ScaleFeature::ALL {
        if feature.is_presentation() {
            for reason in [
                ReductionReason::TotalSize,
                ReductionReason::LongParagraph,
                ReductionReason::Density,
            ] {
                assert!(
                    of_max.reduced.has_reason(feature, reason)
                        == joined.reduced.has_reason(feature, reason)
                );
            }
        }
    }
}

#[kani::proof]
fn scale_byte_tiers_match_the_retired_tiers() {
    let bytes: u64 = kani::any();
    let scale = DocumentScale::for_total_bytes(bytes);
    let retired = retired_byte_tiers::classify(bytes);
    assert!(scale.open_allowed == retired_byte_tiers::open_allowed(retired));
    assert!(scale.undo_enabled == retired_byte_tiers::undo_enabled(retired));
    assert!(
        scale.estimated_buffer_multiplier
            == retired_byte_tiers::estimated_buffer_multiplier(retired)
    );
    assert!(scale.local_history == retired_byte_tiers::local_history(retired));
    // A full profile's byte tiers depend on total bytes alone.
    let profile = ProfileSummary {
        total_bytes: bytes,
        longest_paragraph_chars: kani::any(),
        worst_batch_chars: kani::any(),
    };
    assert!(byte_tiers(DocumentScale::for_profile(profile)) == byte_tiers(scale));
    kani::cover!(!scale.open_allowed, "refused");
    kani::cover!(
        scale.local_history == LocalHistoryAvailability::SaveOnly,
        "save-only tier"
    );
}

/// Replacing the session scale with the saved bytes' total-size scale,
/// instead of joining it, turns a presentation feature back on (the
/// rename-after-save defect the join fixes).
#[kani::proof]
#[kani::should_panic]
fn scale_save_without_join_reenables_a_feature() {
    let session = DocumentScale::for_profile(any_profile());
    let saved: u64 = kani::any();
    let after_save = DocumentScale::for_total_bytes(saved);
    assert!(never_reenables(session, after_save));
}
