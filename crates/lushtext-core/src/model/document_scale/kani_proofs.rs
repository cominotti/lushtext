// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the document scale policy:
//! [`DocumentScale::for_profile`], [`DocumentScale::for_profile_in`],
//! [`DocumentScale::for_total_bytes`] and [`DocumentScale::join`].
//!
//! Domain: every `u64` component of every profile, unbounded, in every
//! [`SyntaxLanguage`] (`bound-post-open-settle` added the language input; the
//! monotonicity and join harnesses are stated over it). The policy has
//! no loops, so no unwind bound is needed. The tier harness compares with the
//! frozen transcription of the retired byte-only classification
//! (`retired_byte_tiers`), not with production code.
//!
//! Compiled only under `cfg(kani)` (`make kani`).

use super::{
    DocumentScale, LocalHistoryAvailability, MODEL_MARKDOWN_CAP, ProfileSummary, ReductionReason,
    ScaleFeature, SyntaxLanguage, retired_byte_tiers,
};

/// The shipped table (`None`) or the model cap the chunked tests use
/// (`chunked-highlight-attach`): the language harnesses prove both.
fn any_markdown_cap() -> Option<u64> {
    if kani::any() {
        Some(MODEL_MARKDOWN_CAP)
    } else {
        None
    }
}

fn any_language() -> SyntaxLanguage {
    let index: usize = kani::any();
    kani::assume(index < SyntaxLanguage::ALL.len());
    SyntaxLanguage::ALL[index]
}

fn any_profile() -> ProfileSummary {
    ProfileSummary {
        total_bytes: kani::any(),
        longest_paragraph_chars: kani::any(),
        worst_batch_chars: kani::any(),
    }
}

/// The presentation outputs of a scale, the part the session floor governs.
fn presentation(scale: DocumentScale) -> (bool, bool, bool, bool, bool, bool, bool) {
    (
        scale.syntax_enabled,
        scale.bracket_matching_enabled,
        scale.minimap_allowed,
        scale.invisibles_allowed,
        scale.search_highlight_allowed,
        scale.wrap_allowed,
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
        && (!later.wrap_allowed || earlier.wrap_allowed)
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
    let cap = any_markdown_cap();
    let small = any_profile();
    let large = any_profile();
    kani::assume(small.total_bytes <= large.total_bytes);
    kani::assume(small.longest_paragraph_chars <= large.longest_paragraph_chars);
    kani::assume(small.worst_batch_chars <= large.worst_batch_chars);
    let language = any_language();
    let (s, l) = (
        DocumentScale::for_profile_in_with(small, language, cap),
        DocumentScale::for_profile_in_with(large, language, cap),
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

/// A language with a lower total-size threshold never enables a feature a
/// language with a higher one disables, and the language changes nothing but
/// the total-size syntax and bracket-matching decisions.
#[kani::proof]
fn scale_is_monotone_in_the_language_threshold() {
    let cap = any_markdown_cap();
    let profile = any_profile();
    let (low, high) = (any_language(), any_language());
    kani::assume(low.syntax_size_threshold_with(cap) <= high.syntax_size_threshold_with(cap));
    let (strict, lax) = (
        DocumentScale::for_profile_in_with(profile, low, cap),
        DocumentScale::for_profile_in_with(profile, high, cap),
    );
    assert!(never_reenables(lax, strict));
    assert!(byte_tiers(strict) == byte_tiers(lax));
    assert!(strict.minimap_allowed == lax.minimap_allowed);
    assert!(strict.invisibles_allowed == lax.invisibles_allowed);
    assert!(strict.search_highlight_allowed == lax.search_highlight_allowed);
    assert!(strict.protected_view == lax.protected_view);
    assert!(
        strict.syntax_enabled
            == (lax.syntax_enabled && profile.total_bytes <= low.syntax_size_threshold_with(cap))
    );
    // Plain text keeps C1's threshold.
    assert!(
        DocumentScale::for_profile_in(profile, SyntaxLanguage::PlainText)
            == DocumentScale::for_profile(profile)
    );
    kani::cover!(
        strict.syntax_enabled != lax.syntax_enabled,
        "the language decides syntax"
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
    // Exactly the entry rule C2 consumes (design D0: the longest paragraph
    // alone; density never decides it).
    assert!(s.protected_view == (small.longest_paragraph_chars > super::L_PROTECT_EDIT));
    for reason in [ReductionReason::TotalSize, ReductionReason::LongParagraph] {
        kani::cover!(
            l.reduced.has_reason(ScaleFeature::Syntax, reason)
                || l.reduced.has_reason(ScaleFeature::ProtectedView, reason),
            "each reason class"
        );
    }
}

#[kani::proof]
fn scale_join_is_a_meet_on_presentation_features() {
    let cap = any_markdown_cap();
    let a = DocumentScale::for_profile_in_with(any_profile(), any_language(), cap);
    let b = DocumentScale::for_profile_in_with(any_profile(), any_language(), cap);
    let c = DocumentScale::for_profile_in_with(any_profile(), any_language(), cap);
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
    let cap = any_markdown_cap();
    let p = any_profile();
    let q = any_profile();
    let language = any_language();
    let joined = DocumentScale::for_profile_in_with(p, language, cap)
        .join(DocumentScale::for_profile_in_with(q, language, cap));
    let of_max = DocumentScale::for_profile_in_with(p.max(q), language, cap);
    assert!(presentation(of_max) == presentation(joined));
    for feature in ScaleFeature::ALL {
        if feature.is_presentation() {
            for reason in [ReductionReason::TotalSize, ReductionReason::LongParagraph] {
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
