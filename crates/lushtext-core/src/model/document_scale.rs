// SPDX-License-Identifier: GPL-3.0-or-later

//! The document scale policy: which editor features a document can afford.
//!
//! One GTK-free, monotone policy replaces the byte-only size classification.
//! [`DocumentScale::for_profile`] reads a [`ProfileSummary`] (total bytes, the
//! longest paragraph, and the wrap-off batch density of
//! [`paragraph_boundary`](super::paragraph_boundary)) and derives two kinds of
//! output:
//!
//! - **Byte tiers** — open admission, undo, the live-memory multiplier and
//!   local-history availability — depend on total bytes alone and describe the
//!   latest loaded or saved bytes, exactly as the retired `FileSizeCheck` did.
//! - **Presentation features** — syntax highlighting, bracket matching, the
//!   minimap, invisible characters, automatic search highlighting, and the
//!   `protected_view` flag — are the ones an editor session holds monotone:
//!   [`DocumentScale::join`] is their meet, so an edit or a save never turns
//!   one back on.
//!
//! Every threshold is a named constant. The paragraph and density limits are
//! the values `probe-long-line-toolkit-costs` (C0) measured, as finalised by
//! the idle re-confirmation of 2026-10-13 (`docs/next/long-line-documents.md`,
//! "Idle re-confirmation (2026-10-13)", run `rr-c0-reconfirm-20261013`); the
//! budgets those values meet are `model::responsiveness_budget`'s
//! [`B_INTERACT_US`](super::responsiveness_budget::B_INTERACT_US) and
//! [`B_SETTLE_US`](super::responsiveness_budget::B_SETTLE_US), not restated
//! here. Comparisons are strict (`>`), as `FileSizeCheck`'s were, so a
//! component equal to a threshold keeps the feature.
//!
//! Kani proves monotonicity, the meet laws of the join, that the scale of a
//! profile maximum is the join of the scales, and that the byte tiers equal
//! the retired ones, over every `u64` component (`document_scale/kani_proofs.rs`).

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::paragraph_boundary::ProfileSummary;

/// Syntax highlighting and bracket matching turn off above this many bytes.
///
/// **Not derived by C0**: the reference sweep found a size-independent ~42 ms
/// first-highlight stall on `.html` at every size down to its 16,000-byte
/// floor, which no size cutoff removes (`S_syntax` "not derived";
/// `S_syntax_interact` 16,000 bytes is the interaction-only figure). Task 0.1
/// of `document-scale-policy` therefore takes the conservative candidate of
/// the origin proposal's decision 4: about 1 MiB, GNOME Text Editor's figure,
/// in place of the former 10 MB. Recorded as an open question in the design.
pub const S_SYNTAX: u64 = 1024 * 1024;

/// Syntax highlighting and bracket matching turn off above this paragraph
/// length (C0: 2,000 / 3,000; ledger A29 is the context-engine behaviour it
/// avoids).
pub const L_SYNTAX: u64 = 2_000;

/// The minimap turns off above this many bytes. Today's 10 MB value, kept
/// unchanged so a lower [`S_SYNTAX`] does not silently remove the minimap from
/// short-line files between the two.
pub const S_MINIMAP: u64 = 10_000_000;

/// The minimap turns off above this paragraph length (C0 idle
/// re-confirmation: 22,000 / 50,000, a lower bound of the boundary).
pub const L_MINIMAP: u64 = 22_000;

/// Invisible-character drawing turns off above this paragraph length (C0:
/// 50,000 / 74,000).
pub const L_INVISIBLES: u64 = 50_000;

/// Automatic search highlighting turns off above this paragraph length (C0
/// idle re-confirmation: 10,000 / 15,000).
pub const L_SEARCH_HIGHLIGHT: u64 = 10_000;

/// `protected_view` holds above this paragraph length.
///
/// C0 did not derive `L_protect` (its input-to-frame half fails at every
/// length on the headless lane's own frame floor); the stall half,
/// `L_protect_stall`, is 500 (500 / 710), and C2 (`long-line-protected-view`)
/// takes that stall basis. C1 only reports the flag.
pub const L_PROTECT: u64 = 500;

/// Plain-text scroll stays within budget up to this paragraph length (C0:
/// 500,000 / 710,000). The largest per-paragraph threshold the profile counts.
pub const N_VISIBLE: u64 = 500_000;

/// `protected_view` holds when any [`R_BATCH`] consecutive paragraphs sum to
/// more characters than this (C0: a dense 500-character line × A31's 112 rows
/// per 2000 px batch).
pub const V_BATCH: u64 = 56_000;

/// Paragraphs in one density window: A31's 112 rows per 2000 px validation
/// batch at the default font, doubled for the 50 % minimum zoom (C0).
pub const R_BATCH: usize = 224;

/// Undo history is off above this many bytes (unchanged).
pub const UNDO_TOTAL_BYTES: u64 = 50_000_000;

/// Local history keeps only save-boundary capture above this many bytes
/// (unchanged).
pub const HISTORY_SAVE_ONLY_BYTES: u64 = 10_000_000;

/// Local history is unavailable above this many bytes (unchanged).
pub const HISTORY_UNAVAILABLE_BYTES: u64 = 50_000_000;

/// A file above this many bytes is refused (unchanged): `set_text` for 500 MB
/// allocates about 1 GB and blocks the main thread for seconds.
pub const REFUSE_TO_OPEN: u64 = 500_000_000;

/// Live-memory multiplier while undo history is on.
const MULTIPLIER_WITH_UNDO: u64 = 3;

/// Live-memory multiplier once undo history is off.
const MULTIPLIER_WITHOUT_UNDO: u64 = 2;

/// Local-history behaviour for a document's size, best first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LocalHistoryAvailability {
    /// Local history is unavailable for this document size.
    Unavailable,
    /// Only save-boundary capture is allowed, but browsing stored history is still allowed.
    SaveOnly,
    /// Full capture cadence and browse surface are available.
    Full,
}

impl LocalHistoryAvailability {
    /// Whether baseline and periodic automatic capture should run.
    #[must_use]
    pub fn allows_automatic_capture(self) -> bool {
        self == Self::Full
    }

    /// Whether the browser and restore workflow should be available.
    #[must_use]
    pub fn allows_browsing(self) -> bool {
        self != Self::Unavailable
    }
}

/// A feature the scale policy can reduce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleFeature {
    /// GtkSourceView syntax highlighting (the language is cleared).
    Syntax,
    /// Matching-bracket highlighting.
    BracketMatching,
    /// The per-tab minimap.
    Minimap,
    /// Invisible-character drawing.
    Invisibles,
    /// Automatic highlighting of search matches.
    SearchHighlight,
    /// The document is marked for Protected View.
    ProtectedView,
    /// Undo history (a byte tier).
    Undo,
    /// Local history (a byte tier).
    LocalHistory,
}

impl ScaleFeature {
    /// Every feature, presentation features first, in notice order.
    pub const ALL: [Self; 8] = [
        Self::Syntax,
        Self::BracketMatching,
        Self::Minimap,
        Self::Invisibles,
        Self::SearchHighlight,
        Self::ProtectedView,
        Self::Undo,
        Self::LocalHistory,
    ];

    const fn bit(self) -> u8 {
        1 << (self as u8)
    }

    /// Whether the session floor holds this feature monotone.
    #[must_use]
    pub const fn is_presentation(self) -> bool {
        !matches!(self, Self::Undo | Self::LocalHistory)
    }
}

impl ScaleFeature {
    /// The feature's name in a sentence ("syntax highlighting is off ...").
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Syntax => "syntax highlighting",
            Self::BracketMatching => "bracket matching",
            Self::Minimap => "the minimap",
            Self::Invisibles => "invisible characters",
            Self::SearchHighlight => "search highlighting",
            Self::ProtectedView => "editing",
            Self::Undo => "undo history",
            Self::LocalHistory => "automatic local history",
        }
    }
}

/// Join `items` as "a", "a and b", or "a, b and c".
fn join_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [only] => (*only).to_string(),
        [head @ .., last] => format!("{} and {last}", head.join(", ")),
    }
}

/// Upper-case the first character of a sentence.
fn capitalize(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// Why a feature was reduced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionReason {
    /// The document's total size.
    TotalSize,
    /// A very long paragraph.
    LongParagraph,
    /// Dense long lines: a batch of consecutive paragraphs is too large.
    Density,
}

impl ReductionReason {
    /// The reason as the end of a sentence.
    #[must_use]
    pub const fn because(self) -> &'static str {
        match self {
            Self::TotalSize => "because of the document's total size",
            Self::LongParagraph => "because of very long lines",
            Self::Density => "because of dense long lines",
        }
    }
}

/// Bits of the presentation features in a [`ReducedFeatures`] mask.
///
/// The presentation features are declared first, so their bits are exactly
/// the bits below the first byte tier's (`Undo`); the unit test
/// `presentation_mask_is_exactly_the_presentation_features` pins that against
/// [`ScaleFeature::is_presentation`]. (An OR of the six bits would carry five
/// `|`/`^` mutants no test can tell apart, since the bits are disjoint.)
const PRESENTATION_MASK: u8 = ScaleFeature::Undo.bit() - 1;

/// Which features are reduced, and for which reasons (a feature may have
/// several).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReducedFeatures {
    total_size: u8,
    long_paragraph: u8,
    density: u8,
}

impl ReducedFeatures {
    /// Nothing reduced.
    pub const NONE: Self = Self {
        total_size: 0,
        long_paragraph: 0,
        density: 0,
    };

    #[must_use]
    const fn with(mut self, feature: ScaleFeature, reason: ReductionReason) -> Self {
        let bit = feature.bit();
        match reason {
            ReductionReason::TotalSize => self.total_size |= bit,
            ReductionReason::LongParagraph => self.long_paragraph |= bit,
            ReductionReason::Density => self.density |= bit,
        }
        self
    }

    #[must_use]
    const fn with_if(
        self,
        condition: bool,
        feature: ScaleFeature,
        reason: ReductionReason,
    ) -> Self {
        if condition {
            self.with(feature, reason)
        } else {
            self
        }
    }

    const fn mask(self) -> u8 {
        self.total_size | self.long_paragraph | self.density
    }

    /// Whether `feature` is reduced for any reason.
    #[must_use]
    pub const fn contains(self, feature: ScaleFeature) -> bool {
        self.mask() & feature.bit() != 0
    }

    /// Whether `feature` is reduced for `reason`.
    #[must_use]
    pub const fn has_reason(self, feature: ScaleFeature, reason: ReductionReason) -> bool {
        let bits = match reason {
            ReductionReason::TotalSize => self.total_size,
            ReductionReason::LongParagraph => self.long_paragraph,
            ReductionReason::Density => self.density,
        };
        bits & feature.bit() != 0
    }

    /// Whether nothing is reduced.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.mask() == 0
    }

    /// Whether any presentation feature is reduced (only these raise the
    /// reduced-features notice).
    #[must_use]
    pub const fn any_presentation(self) -> bool {
        self.mask() & PRESENTATION_MASK != 0
    }

    /// The presentation features reduced here and not in `earlier`.
    #[must_use]
    pub const fn newly_reduced_presentation(self, earlier: Self) -> Self {
        let fresh = self.mask() & !earlier.mask() & PRESENTATION_MASK;
        Self {
            total_size: self.total_size & fresh,
            long_paragraph: self.long_paragraph & fresh,
            density: self.density & fresh,
        }
    }

    /// Only the presentation features.
    #[must_use]
    pub const fn presentation_only(self) -> Self {
        Self {
            total_size: self.total_size & PRESENTATION_MASK,
            long_paragraph: self.long_paragraph & PRESENTATION_MASK,
            density: self.density & PRESENTATION_MASK,
        }
    }

    /// The primary reason a reduced feature is reported under: a long
    /// paragraph first, then density, then total size.
    const fn primary_reason(self, feature: ScaleFeature) -> Option<ReductionReason> {
        if self.has_reason(feature, ReductionReason::LongParagraph) {
            Some(ReductionReason::LongParagraph)
        } else if self.has_reason(feature, ReductionReason::Density) {
            Some(ReductionReason::Density)
        } else if self.has_reason(feature, ReductionReason::TotalSize) {
            Some(ReductionReason::TotalSize)
        } else {
            None
        }
    }

    /// One plain sentence per reason class naming the reduced features, for
    /// the reduced-features notice and the editor's accessible description.
    /// `None` when nothing is reduced. It names features and reasons only,
    /// never document content, and its length is bounded by the fixed set of
    /// features.
    #[must_use]
    pub fn describe(self) -> Option<String> {
        let mut sentences = Vec::new();
        for reason in [
            ReductionReason::LongParagraph,
            ReductionReason::Density,
            ReductionReason::TotalSize,
        ] {
            let features: Vec<&str> = ScaleFeature::ALL
                .into_iter()
                .filter(|&feature| {
                    feature != ScaleFeature::ProtectedView
                        && self.primary_reason(feature) == Some(reason)
                })
                .map(ScaleFeature::label)
                .collect();
            let because = reason.because();
            if !features.is_empty() {
                let verb = if features.len() == 1 { "is" } else { "are" };
                sentences.push(format!(
                    "{} {verb} off {because}.",
                    capitalize(&join_list(&features))
                ));
            }
            if self.primary_reason(ScaleFeature::ProtectedView) == Some(reason) {
                sentences.push(format!(
                    "This document is marked for Protected View {because}."
                ));
            }
        }
        (!sentences.is_empty()).then(|| sentences.join(" "))
    }

    const fn union(self, other: Self) -> Self {
        Self {
            total_size: self.total_size | other.total_size,
            long_paragraph: self.long_paragraph | other.long_paragraph,
            density: self.density | other.density,
        }
    }
}

/// Every size- and length-dependent feature decision for one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent feature output of the scale policy"
)]
pub struct DocumentScale {
    /// Whether the document may be opened at all (byte tier).
    pub open_allowed: bool,
    /// Whether undo history is enabled (byte tier).
    pub undo_enabled: bool,
    /// Approximate `GtkTextBuffer` memory multiplier for eviction (byte tier).
    pub estimated_buffer_multiplier: u64,
    /// Local-history availability (byte tier).
    pub local_history: LocalHistoryAvailability,
    /// Syntax highlighting.
    pub syntax_enabled: bool,
    /// Matching-bracket highlighting.
    pub bracket_matching_enabled: bool,
    /// The per-tab minimap.
    pub minimap_allowed: bool,
    /// Invisible-character drawing.
    pub invisibles_allowed: bool,
    /// Automatic search-match highlighting.
    pub search_highlight_allowed: bool,
    /// The document is marked for Protected View (C2 consumes it).
    pub protected_view: bool,
    /// Which features are reduced, and why.
    pub reduced: ReducedFeatures,
}

impl DocumentScale {
    /// The scale of an empty document: every feature on.
    pub const UNRESTRICTED: Self = Self::for_profile(ProfileSummary {
        total_bytes: 0,
        longest_paragraph_chars: 0,
        worst_batch_chars: 0,
    });

    /// Derive every feature decision from a profile.
    #[must_use]
    pub const fn for_profile(profile: ProfileSummary) -> Self {
        use ReductionReason::{Density, LongParagraph, TotalSize};
        use ScaleFeature::{
            BracketMatching, Invisibles, LocalHistory, Minimap, ProtectedView, SearchHighlight,
            Syntax, Undo,
        };
        let bytes = profile.total_bytes;
        let longest = profile.longest_paragraph_chars;
        let syntax_by_size = bytes > S_SYNTAX;
        let syntax_by_length = longest > L_SYNTAX;
        let minimap_by_size = bytes > S_MINIMAP;
        let minimap_by_length = longest > L_MINIMAP;
        let invisibles_by_length = longest > L_INVISIBLES;
        let search_by_length = longest > L_SEARCH_HIGHLIGHT;
        let protect_by_length = longest > L_PROTECT;
        let protect_by_density = profile.worst_batch_chars > V_BATCH;
        let undo_enabled = bytes <= UNDO_TOTAL_BYTES;
        let local_history = if bytes > HISTORY_UNAVAILABLE_BYTES {
            LocalHistoryAvailability::Unavailable
        } else if bytes > HISTORY_SAVE_ONLY_BYTES {
            LocalHistoryAvailability::SaveOnly
        } else {
            LocalHistoryAvailability::Full
        };
        let reduced = ReducedFeatures::NONE
            .with_if(syntax_by_size, Syntax, TotalSize)
            .with_if(syntax_by_length, Syntax, LongParagraph)
            .with_if(syntax_by_size, BracketMatching, TotalSize)
            .with_if(syntax_by_length, BracketMatching, LongParagraph)
            .with_if(minimap_by_size, Minimap, TotalSize)
            .with_if(minimap_by_length, Minimap, LongParagraph)
            .with_if(invisibles_by_length, Invisibles, LongParagraph)
            .with_if(search_by_length, SearchHighlight, LongParagraph)
            .with_if(protect_by_length, ProtectedView, LongParagraph)
            .with_if(protect_by_density, ProtectedView, Density)
            .with_if(!undo_enabled, Undo, TotalSize)
            .with_if(
                !matches!(local_history, LocalHistoryAvailability::Full),
                LocalHistory,
                TotalSize,
            );
        Self {
            open_allowed: bytes <= REFUSE_TO_OPEN,
            undo_enabled,
            estimated_buffer_multiplier: if undo_enabled {
                MULTIPLIER_WITH_UNDO
            } else {
                MULTIPLIER_WITHOUT_UNDO
            },
            local_history,
            syntax_enabled: !(syntax_by_size || syntax_by_length),
            bracket_matching_enabled: !(syntax_by_size || syntax_by_length),
            minimap_allowed: !(minimap_by_size || minimap_by_length),
            invisibles_allowed: !invisibles_by_length,
            search_highlight_allowed: !search_by_length,
            protected_view: protect_by_length || protect_by_density,
            reduced,
        }
    }

    /// The scale of a document known only by its byte size.
    ///
    /// For callers that never see the text — file peek, bookmark excerpts,
    /// draft availability, the local-history journal — and for the save path,
    /// which joins it into the session scale. Its paragraph components are
    /// zero, so it gates only by total size.
    #[must_use]
    pub const fn for_total_bytes(total_bytes: u64) -> Self {
        Self::for_profile(ProfileSummary {
            total_bytes,
            longest_paragraph_chars: 0,
            worst_batch_chars: 0,
        })
    }

    /// The session join: the meet on presentation features, the byte tiers
    /// of `other`.
    ///
    /// A presentation feature is on only when it is on in both; `protected_view`
    /// holds when it holds in either; reduction reasons accumulate. The byte
    /// tiers (open, undo, multiplier, local history) come from `other`, the
    /// newer bytes, so a document saved smaller regains full local history as
    /// before. Kani proves the meet laws and that the join never re-enables a
    /// presentation feature `self` disables (`kani_proofs::scale_join_*`).
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        Self {
            open_allowed: other.open_allowed,
            undo_enabled: other.undo_enabled,
            estimated_buffer_multiplier: other.estimated_buffer_multiplier,
            local_history: other.local_history,
            syntax_enabled: self.syntax_enabled && other.syntax_enabled,
            bracket_matching_enabled: self.bracket_matching_enabled
                && other.bracket_matching_enabled,
            minimap_allowed: self.minimap_allowed && other.minimap_allowed,
            invisibles_allowed: self.invisibles_allowed && other.invisibles_allowed,
            search_highlight_allowed: self.search_highlight_allowed
                && other.search_highlight_allowed,
            protected_view: self.protected_view || other.protected_view,
            // Presentation reasons from both, byte-tier reasons from `other`.
            reduced: self.reduced.presentation_only().union(other.reduced),
        }
    }

    /// Whether `feature` is currently enabled (for `ProtectedView`: whether
    /// the document is **not** marked).
    #[must_use]
    pub const fn allows(self, feature: ScaleFeature) -> bool {
        match feature {
            ScaleFeature::Syntax => self.syntax_enabled,
            ScaleFeature::BracketMatching => self.bracket_matching_enabled,
            ScaleFeature::Minimap => self.minimap_allowed,
            ScaleFeature::Invisibles => self.invisibles_allowed,
            ScaleFeature::SearchHighlight => self.search_highlight_allowed,
            ScaleFeature::ProtectedView => !self.protected_view,
            ScaleFeature::Undo => self.undo_enabled,
            ScaleFeature::LocalHistory => {
                matches!(self.local_history, LocalHistoryAvailability::Full)
            }
        }
    }
}

impl Default for DocumentScale {
    fn default() -> Self {
        Self::UNRESTRICTED
    }
}

/// A frozen transcription of the retired byte-only classification
/// (`services/file_limits.rs` `FileSizeCheck::classify`, its `open_allowed`,
/// `undo_enabled` and `estimated_buffer_multiplier` queries, and
/// `local_history_service::availability_for_size_check`, as of commit
/// 1820f14a). The parity unit tests and the Kani tier harness compare
/// [`DocumentScale::for_total_bytes`] with it; it is not production behaviour.
#[cfg(any(test, kani))]
pub(crate) mod retired_byte_tiers {
    use super::LocalHistoryAvailability;

    const LARGE_FILE_TOAST: u64 = 1_000_000;
    const DISABLE_SYNTAX_HIGHLIGHTING: u64 = 10_000_000;
    const DISABLE_UNDO_HISTORY: u64 = 50_000_000;
    const REFUSE_TO_OPEN: u64 = 500_000_000;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum FileSizeCheck {
        Normal,
        LargeFileToast,
        DisableSyntax,
        DisableUndoAndSyntax,
        TooLarge,
    }

    pub(crate) const fn classify(size: u64) -> FileSizeCheck {
        if size > REFUSE_TO_OPEN {
            FileSizeCheck::TooLarge
        } else if size > DISABLE_UNDO_HISTORY {
            FileSizeCheck::DisableUndoAndSyntax
        } else if size > DISABLE_SYNTAX_HIGHLIGHTING {
            FileSizeCheck::DisableSyntax
        } else if size > LARGE_FILE_TOAST {
            FileSizeCheck::LargeFileToast
        } else {
            FileSizeCheck::Normal
        }
    }

    pub(crate) const fn open_allowed(check: FileSizeCheck) -> bool {
        !matches!(check, FileSizeCheck::TooLarge)
    }

    pub(crate) const fn undo_enabled(check: FileSizeCheck) -> bool {
        matches!(
            check,
            FileSizeCheck::Normal | FileSizeCheck::LargeFileToast | FileSizeCheck::DisableSyntax
        )
    }

    pub(crate) const fn estimated_buffer_multiplier(check: FileSizeCheck) -> u64 {
        if undo_enabled(check) { 3 } else { 2 }
    }

    pub(crate) const fn local_history(check: FileSizeCheck) -> LocalHistoryAvailability {
        match check {
            FileSizeCheck::Normal | FileSizeCheck::LargeFileToast => LocalHistoryAvailability::Full,
            FileSizeCheck::DisableSyntax => LocalHistoryAvailability::SaveOnly,
            FileSizeCheck::DisableUndoAndSyntax | FileSizeCheck::TooLarge => {
                LocalHistoryAvailability::Unavailable
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(total_bytes: u64, longest: u64, worst: u64) -> ProfileSummary {
        ProfileSummary {
            total_bytes,
            longest_paragraph_chars: longest,
            worst_batch_chars: worst,
        }
    }

    fn by_length(longest: u64) -> DocumentScale {
        DocumentScale::for_profile(profile(0, longest, 0))
    }

    #[test]
    fn paragraph_thresholds_keep_the_feature_at_t_and_drop_it_at_t_plus_one() {
        let cases: [(u64, ScaleFeature); 6] = [
            (L_SYNTAX, ScaleFeature::Syntax),
            (L_SYNTAX, ScaleFeature::BracketMatching),
            (L_MINIMAP, ScaleFeature::Minimap),
            (L_INVISIBLES, ScaleFeature::Invisibles),
            (L_SEARCH_HIGHLIGHT, ScaleFeature::SearchHighlight),
            (L_PROTECT, ScaleFeature::ProtectedView),
        ];
        for (threshold, feature) in cases {
            assert!(by_length(threshold).allows(feature), "{feature:?} at t");
            let above = by_length(threshold + 1);
            assert!(!above.allows(feature), "{feature:?} at t+1");
            assert!(
                above
                    .reduced
                    .has_reason(feature, ReductionReason::LongParagraph)
            );
        }
    }

    #[test]
    fn size_thresholds_keep_the_feature_at_t_and_drop_it_at_t_plus_one() {
        for (threshold, feature) in [
            (S_SYNTAX, ScaleFeature::Syntax),
            (S_SYNTAX, ScaleFeature::BracketMatching),
            (S_MINIMAP, ScaleFeature::Minimap),
            (UNDO_TOTAL_BYTES, ScaleFeature::Undo),
            (HISTORY_SAVE_ONLY_BYTES, ScaleFeature::LocalHistory),
        ] {
            assert!(DocumentScale::for_total_bytes(threshold).allows(feature));
            let above = DocumentScale::for_total_bytes(threshold + 1);
            assert!(!above.allows(feature), "{feature:?} at t+1");
            assert!(
                above
                    .reduced
                    .has_reason(feature, ReductionReason::TotalSize)
            );
        }
        assert!(DocumentScale::for_total_bytes(REFUSE_TO_OPEN).open_allowed);
        assert!(!DocumentScale::for_total_bytes(REFUSE_TO_OPEN + 1).open_allowed);
        assert_eq!(
            DocumentScale::for_total_bytes(HISTORY_UNAVAILABLE_BYTES + 1).local_history,
            LocalHistoryAvailability::Unavailable
        );
    }

    #[test]
    fn density_threshold_sets_protected_view_only() {
        let at = DocumentScale::for_profile(profile(0, 0, V_BATCH));
        assert!(!at.protected_view);
        let above = DocumentScale::for_profile(profile(0, 0, V_BATCH + 1));
        assert!(above.protected_view);
        assert!(
            above
                .reduced
                .has_reason(ScaleFeature::ProtectedView, ReductionReason::Density)
        );
        assert!(above.syntax_enabled && above.minimap_allowed && above.invisibles_allowed);
    }

    #[test]
    fn byte_tiers_match_the_retired_classification() {
        let edges = [
            0,
            1_000_000,
            1_000_001,
            S_SYNTAX,
            S_SYNTAX + 1,
            10_000_000,
            10_000_001,
            50_000_000,
            50_000_001,
            500_000_000,
            500_000_001,
            u64::MAX,
        ];
        for bytes in edges {
            let scale = DocumentScale::for_total_bytes(bytes);
            let retired = retired_byte_tiers::classify(bytes);
            assert_eq!(
                scale.open_allowed,
                retired_byte_tiers::open_allowed(retired)
            );
            assert_eq!(
                scale.undo_enabled,
                retired_byte_tiers::undo_enabled(retired)
            );
            assert_eq!(
                scale.estimated_buffer_multiplier,
                retired_byte_tiers::estimated_buffer_multiplier(retired)
            );
            assert_eq!(
                scale.local_history,
                retired_byte_tiers::local_history(retired)
            );
        }
    }

    #[test]
    fn join_never_reenables_and_takes_byte_tiers_from_the_newer_bytes() {
        let giant = DocumentScale::for_profile(profile(60_000_000, L_SYNTAX + 1, 0));
        let saved = DocumentScale::for_total_bytes(100);
        let joined = giant.join(saved);
        assert!(!joined.syntax_enabled && !joined.bracket_matching_enabled);
        assert!(joined.protected_view);
        assert!(joined.undo_enabled);
        assert_eq!(joined.local_history, LocalHistoryAvailability::Full);
        assert!(!joined.reduced.contains(ScaleFeature::Undo));
        assert!(joined.reduced.contains(ScaleFeature::Syntax));
    }

    #[test]
    fn newly_reduced_lists_only_additional_presentation_features() {
        let first = by_length(L_SYNTAX + 1).reduced;
        let second = by_length(L_MINIMAP + 1).reduced;
        let fresh = second.newly_reduced_presentation(first);
        assert!(fresh.contains(ScaleFeature::Minimap));
        assert!(fresh.contains(ScaleFeature::SearchHighlight));
        assert!(!fresh.contains(ScaleFeature::Syntax));
        assert!(
            DocumentScale::for_total_bytes(60_000_000)
                .reduced
                .newly_reduced_presentation(ReducedFeatures::NONE)
                .contains(ScaleFeature::Syntax)
        );
        assert!(!DocumentScale::UNRESTRICTED.reduced.any_presentation());
    }

    #[test]
    fn describe_groups_features_by_reason_and_names_undo() {
        assert_eq!(ReducedFeatures::NONE.describe(), None);
        let long = by_length(L_MINIMAP + 1)
            .reduced
            .describe()
            .expect("reduced features describe themselves");
        assert_eq!(
            long,
            "Syntax highlighting, bracket matching, the minimap and search highlighting are \
             off because of very long lines. This document is marked for Protected View \
             because of very long lines."
        );
        let huge = DocumentScale::for_total_bytes(60_000_000)
            .reduced
            .describe()
            .expect("reduced features describe themselves");
        assert!(huge.contains("undo history"), "{huge}");
        assert!(
            huge.contains("because of the document's total size"),
            "{huge}"
        );
        let dense = DocumentScale::for_profile(profile(0, 0, V_BATCH + 1))
            .reduced
            .describe()
            .expect("reduced features describe themselves");
        assert_eq!(
            dense,
            "This document is marked for Protected View because of dense long lines."
        );
    }

    #[test]
    fn presentation_mask_is_exactly_the_presentation_features() {
        for feature in ScaleFeature::ALL {
            assert_eq!(
                PRESENTATION_MASK & feature.bit() != 0,
                feature.is_presentation(),
                "{feature:?}"
            );
        }
        let presentation: Vec<ScaleFeature> = ScaleFeature::ALL
            .into_iter()
            .filter(|feature| feature.is_presentation())
            .collect();
        assert_eq!(presentation, ScaleFeature::ALL[..6]);
        assert!(!ScaleFeature::Undo.is_presentation());
        assert!(!ScaleFeature::LocalHistory.is_presentation());
    }

    #[test]
    fn a_feature_reduced_for_two_reasons_stays_reduced() {
        assert!(ReducedFeatures::NONE.is_empty());
        assert!(DocumentScale::UNRESTRICTED.reduced.is_empty());
        let both = DocumentScale::for_profile(profile(S_SYNTAX + 1, L_SYNTAX + 1, 0)).reduced;
        assert!(both.has_reason(ScaleFeature::Syntax, ReductionReason::TotalSize));
        assert!(both.has_reason(ScaleFeature::Syntax, ReductionReason::LongParagraph));
        assert!(both.contains(ScaleFeature::Syntax));
        assert!(!both.is_empty());
        assert!(both.any_presentation());
        let protected = DocumentScale::for_profile(profile(0, L_PROTECT + 1, V_BATCH + 1)).reduced;
        assert!(protected.contains(ScaleFeature::ProtectedView));
        assert!(!protected.contains(ScaleFeature::Syntax));
    }

    #[test]
    fn newly_reduced_presentation_drops_byte_tiers_and_earlier_reasons() {
        let earlier = DocumentScale::for_profile(profile(0, 0, V_BATCH + 1)).reduced;
        let now = DocumentScale::for_profile(profile(60_000_000, 0, V_BATCH + 1)).reduced;
        let fresh = now.newly_reduced_presentation(earlier);
        assert!(fresh.contains(ScaleFeature::Syntax));
        assert!(fresh.contains(ScaleFeature::Minimap));
        assert!(!fresh.contains(ScaleFeature::Undo));
        assert!(!fresh.contains(ScaleFeature::LocalHistory));
        assert!(!fresh.contains(ScaleFeature::ProtectedView));
    }

    #[test]
    fn presentation_only_keeps_each_reason_and_drops_byte_tiers() {
        let huge = DocumentScale::for_total_bytes(60_000_000)
            .reduced
            .presentation_only();
        assert!(huge.has_reason(ScaleFeature::Syntax, ReductionReason::TotalSize));
        assert!(!huge.contains(ScaleFeature::Undo));
        assert!(!huge.contains(ScaleFeature::LocalHistory));
        let long = by_length(L_SYNTAX + 1).reduced.presentation_only();
        assert!(long.has_reason(ScaleFeature::Syntax, ReductionReason::LongParagraph));
        assert!(!long.contains(ScaleFeature::Invisibles));
        let dense = DocumentScale::for_profile(profile(0, 0, V_BATCH + 1))
            .reduced
            .presentation_only();
        assert!(dense.has_reason(ScaleFeature::ProtectedView, ReductionReason::Density));
        assert!(!dense.contains(ScaleFeature::Syntax));
    }

    #[test]
    fn join_keeps_presentation_reasons_and_takes_byte_reasons_from_the_newer_bytes() {
        let huge = DocumentScale::for_total_bytes(60_000_000);
        let session = by_length(L_SYNTAX + 1).join(huge);
        let reduced = session.reduced;
        assert!(reduced.has_reason(ScaleFeature::Syntax, ReductionReason::LongParagraph));
        assert!(reduced.has_reason(ScaleFeature::Syntax, ReductionReason::TotalSize));
        assert!(reduced.has_reason(ScaleFeature::Undo, ReductionReason::TotalSize));
        assert!(!reduced.has_reason(ScaleFeature::Undo, ReductionReason::LongParagraph));
        // The same byte reason on both sides survives the join.
        let twice = huge.join(huge).reduced;
        assert!(twice.has_reason(ScaleFeature::Syntax, ReductionReason::TotalSize));
        assert!(twice.has_reason(ScaleFeature::Undo, ReductionReason::TotalSize));
        let shrunk = huge.join(DocumentScale::for_total_bytes(100)).reduced;
        assert!(!shrunk.contains(ScaleFeature::Undo));
        assert!(shrunk.has_reason(ScaleFeature::Syntax, ReductionReason::TotalSize));
        let dense = DocumentScale::for_profile(profile(0, 0, V_BATCH + 1))
            .join(DocumentScale::for_total_bytes(100))
            .reduced;
        assert!(dense.has_reason(ScaleFeature::ProtectedView, ReductionReason::Density));
    }

    #[test]
    fn the_policy_constants_have_their_recorded_values() {
        // Design D7 and `docs/next/long-line-documents.md` record these; a
        // change to one is a policy decision, not a refactor.
        let recorded: [(u64, u64); 13] = std::hint::black_box([
            (S_SYNTAX, 1024 * 1024),
            (L_SYNTAX, 2_000),
            (S_MINIMAP, 10_000_000),
            (L_MINIMAP, 22_000),
            (L_INVISIBLES, 50_000),
            (L_SEARCH_HIGHLIGHT, 10_000),
            (L_PROTECT, 500),
            (N_VISIBLE, 500_000),
            (V_BATCH, 56_000),
            (UNDO_TOTAL_BYTES, 50_000_000),
            (HISTORY_SAVE_ONLY_BYTES, 10_000_000),
            (HISTORY_UNAVAILABLE_BYTES, 50_000_000),
            (REFUSE_TO_OPEN, 500_000_000),
        ]);
        for (constant, value) in recorded {
            assert_eq!(constant, value);
        }
        assert_eq!(std::hint::black_box(R_BATCH), 224);
    }

    #[test]
    fn thresholds_are_ordered_as_the_profile_counts_them() {
        let ascending = std::hint::black_box([
            L_PROTECT,
            L_SYNTAX,
            L_SEARCH_HIGHLIGHT,
            L_MINIMAP,
            L_INVISIBLES,
            N_VISIBLE,
        ]);
        assert!(ascending.windows(2).all(|pair| pair[0] < pair[1]));
        let undo = std::hint::black_box(UNDO_TOTAL_BYTES);
        assert!(std::hint::black_box(HISTORY_SAVE_ONLY_BYTES) < undo);
        assert!(undo < std::hint::black_box(REFUSE_TO_OPEN));
    }
}
