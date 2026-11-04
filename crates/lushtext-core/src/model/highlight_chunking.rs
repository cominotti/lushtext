// SPDX-License-Identifier: GPL-3.0-or-later

//! The chunked highlighted install (`chunked-highlight-attach`, C6b): the
//! pure decisions behind installing a document whose language is attached
//! while the text goes in.
//!
//! GtkSourceView's context engine analyses in a 10 ms first update and then
//! in 30 ms worker slices the application cannot shorten (ledger A69), so a
//! language attached to a whole document of more than a few tens of KiB
//! stalls for a slice per 30 ms of analysis. Attaching it after the first
//! chunk and inserting the rest in paragraph-aligned chunks of at most
//! [`HIGHLIGHT_CHUNK_BYTES`], each next chunk only once the engine has
//! reported the previous one analysed (A84), keeps the analysis in first
//! updates and every dispatch within `B_SETTLE_US` at every size measured.
//!
//! Three decisions live here, GTK-free and Kani-checked
//! (`highlight_chunking/kani_proofs.rs`):
//!
//! - [`highlight_install_order`]: which order an install takes;
//! - [`chunk_gate`] and [`next_chunk_step`]: whether the next chunk may go
//!   in now, and what the install waits for otherwise;
//! - the chunk size, through
//!   [`SliceBudget::with_highlight_cut`](super::buffer_replacement::SliceBudget::with_highlight_cut),
//!   and the fact that a document the scale highlights has no paragraph the
//!   cut cannot hold ([`MAX_SYNTAX_PARAGRAPH_BYTES`]).

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::document_scale::{L_SYNTAX, SyntaxLanguage};

/// The most bytes one chunk of a highlighted install holds.
///
/// Measured by C6 (ledger A69, `--options-s-bg-highlight`): chunks of at most
/// 8 KiB, each followed by its first update, kept every dispatch within
/// `B_SETTLE_US` at every size tried (html worst 7.7–11.9 ms, no worker slice);
/// 16 KiB chunks pushed a first update to 16.1–26.4 ms. Re-measured by C6b
/// (A84, the paced install at 1 MiB in twelve languages): worst iteration
/// 2.9–11.2 ms, none over `B_SETTLE_US`.
///
/// Kind: cap. Rounding: a power of two, as measured.
pub const HIGHLIGHT_CHUNK_BYTES: u64 = 8 * 1024;

/// The most UTF-8 bytes one paragraph of a document the scale highlights can
/// hold: [`L_SYNTAX`] characters of at most four bytes each plus the longest
/// paragraph delimiter (U+2029, three bytes). Above `L_SYNTAX` characters the
/// scale turns syntax off, so a highlighted install never meets a paragraph
/// larger than one chunk.
///
/// Kind: external bound, derived from `L_SYNTAX` and UTF-8; not a threshold,
/// so not rounded to a power of two.
pub const MAX_SYNTAX_PARAGRAPH_BYTES: u64 = 4 * L_SYNTAX + 3;

const _: () = assert!(MAX_SYNTAX_PARAGRAPH_BYTES <= HIGHLIGHT_CHUNK_BYTES);

/// The order in which one install meets the context engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightInstallOrder {
    /// The scale keeps syntax off: no language during or after the install.
    NoHighlight,
    /// The text goes in first and the language is attached afterwards, by
    /// the deferred attach stage (`bound-post-open-settle`, D2): the
    /// document is small enough that the analysis of the whole stays within
    /// `B_SETTLE_US` per dispatch.
    AttachAfterInstall,
    /// The language is attached after the first chunk and the rest goes in
    /// chunk by chunk, each analysed before the next.
    AttachBeforeChunks,
}

impl HighlightInstallOrder {
    /// Whether the install is chunked for the engine.
    #[must_use]
    pub const fn is_chunked(self) -> bool {
        matches!(self, Self::AttachBeforeChunks)
    }
}

/// The order for a document of `total_bytes` in `language`, given whether
/// the incoming scale allows syntax.
///
/// Chunked exactly when syntax is allowed, the language has a chunked cap
/// (`document_scale::chunked_threshold`) ([`SyntaxLanguage::chunk_highlighted`]) and the
/// document is above that language's attach-after-install threshold: below
/// it the C6 order is measured within budget and shows the whole text in the
/// first frame.
#[must_use]
pub const fn highlight_install_order(
    language: SyntaxLanguage,
    total_bytes: u64,
    syntax_allowed: bool,
) -> HighlightInstallOrder {
    if !syntax_allowed {
        HighlightInstallOrder::NoHighlight
    } else if language.chunk_highlighted()
        && total_bytes > language.attach_after_install_threshold()
    {
        HighlightInstallOrder::AttachBeforeChunks
    } else {
        HighlightInstallOrder::AttachAfterInstall
    }
}

/// What a chunked install's next step does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkStep {
    /// Insert the next chunk now.
    Insert,
    /// Wait once at `G_PRIORITY_LOW + 1`, which runs only after the engine's
    /// worker has finished (A69), then insert.
    AwaitAnalysis,
}

/// The next step, from whether the engine has reported the buffer analysed
/// to its end (A84) and whether this chunk already waited.
///
/// A chunk waits at most once: the low-priority wait runs after the worker,
/// so it finds the analysis done unless something outside the engine holds
/// it back, and the install must make progress regardless.
#[must_use]
pub const fn next_chunk_step(analysed_to_end: bool, waited: bool) -> ChunkStep {
    if analysed_to_end || waited {
        ChunkStep::Insert
    } else {
        ChunkStep::AwaitAnalysis
    }
}

/// What one chunked install knows when its next chunk could go in.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChunkFacts {
    /// Chunks already in the buffer.
    pub chunks_inserted: u64,
    /// Whether this install has asked for the language attach.
    pub attach_requested: bool,
    /// Whether that attach stage is still pending.
    pub attach_pending: bool,
    /// Whether the buffer has a language now.
    pub language_attached: bool,
    /// Whether the engine has reported the buffer analysed to its end (A84).
    pub analysed_to_end: bool,
    /// Whether this chunk already waited once for the analysis.
    pub waited: bool,
}

/// What a chunked install does next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChunkGate {
    /// Insert the next chunk under the highlight cut.
    Insert,
    /// Ask for the language attach (once, after the first chunk), then decide
    /// again.
    RequestAttach,
    /// Park until the attach stage has run or been withdrawn.
    AwaitAttach,
    /// Wait once at `G_PRIORITY_LOW + 1` for the engine's worker.
    AwaitAnalysis,
    /// The attach ran but left no language (the authoritative guess found
    /// none, or the scale withdrew it): insert the rest without the cut.
    InsertUnhighlighted,
}

/// The next step of a chunked install (design D3 of
/// `chunked-highlight-attach`): the first chunk goes in before the attach,
/// so the first frame shows text; then the attach is requested and no
/// further chunk goes in until it has resolved; afterwards each chunk waits
/// for the analysis of the previous one ([`next_chunk_step`]).
#[must_use]
pub const fn chunk_gate(facts: ChunkFacts) -> ChunkGate {
    if facts.chunks_inserted == 0 {
        return ChunkGate::Insert;
    }
    if !facts.attach_requested {
        return ChunkGate::RequestAttach;
    }
    if facts.attach_pending {
        return ChunkGate::AwaitAttach;
    }
    if !facts.language_attached {
        return ChunkGate::InsertUnhighlighted;
    }
    match next_chunk_step(facts.analysed_to_end, facts.waited) {
        ChunkStep::Insert => ChunkGate::Insert,
        ChunkStep::AwaitAnalysis => ChunkGate::AwaitAnalysis,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::document_scale::S_SYNTAX;

    #[test]
    fn only_a_language_with_a_chunked_cap_takes_the_chunked_order_above_its_threshold() {
        for language in SyntaxLanguage::ALL {
            let threshold = language.attach_after_install_threshold();
            assert_eq!(
                highlight_install_order(language, threshold, true),
                HighlightInstallOrder::AttachAfterInstall,
                "{language:?} at its threshold"
            );
            let above = highlight_install_order(language, threshold + 1, true);
            assert_eq!(
                above.is_chunked(),
                language.chunk_highlighted(),
                "{language:?}"
            );
            assert_eq!(
                highlight_install_order(language, S_SYNTAX, false),
                HighlightInstallOrder::NoHighlight
            );
        }
    }

    #[test]
    fn the_first_chunk_goes_in_before_the_attach_and_no_second_one_until_it_resolves() {
        let first = ChunkFacts::default();
        assert_eq!(chunk_gate(first), ChunkGate::Insert);
        let after_first = ChunkFacts {
            chunks_inserted: 1,
            ..first
        };
        assert_eq!(chunk_gate(after_first), ChunkGate::RequestAttach);
        let requested = ChunkFacts {
            attach_requested: true,
            attach_pending: true,
            ..after_first
        };
        assert_eq!(chunk_gate(requested), ChunkGate::AwaitAttach);
        let attached = ChunkFacts {
            attach_pending: false,
            language_attached: true,
            ..requested
        };
        assert_eq!(chunk_gate(attached), ChunkGate::AwaitAnalysis);
        assert_eq!(
            chunk_gate(ChunkFacts {
                analysed_to_end: true,
                ..attached
            }),
            ChunkGate::Insert
        );
        assert_eq!(
            chunk_gate(ChunkFacts {
                language_attached: false,
                ..attached
            }),
            ChunkGate::InsertUnhighlighted
        );
    }

    #[test]
    fn a_chunk_waits_at_most_once() {
        assert_eq!(next_chunk_step(true, false), ChunkStep::Insert);
        assert_eq!(next_chunk_step(false, false), ChunkStep::AwaitAnalysis);
        assert_eq!(next_chunk_step(false, true), ChunkStep::Insert);
        assert_eq!(next_chunk_step(true, true), ChunkStep::Insert);
    }
}
