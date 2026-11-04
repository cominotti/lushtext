// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the chunked highlighted install
//! (`chunked-highlight-attach`, C6b).
//!
//! Domain: every language class, every `u64` total size, both syntax
//! verdicts; every pair of coverage and wait flags; and every sequence of at
//! most [`MAX_PARAGRAPHS`] paragraphs whose byte length is at most
//! [`MAX_SYNTAX_PARAGRAPH_BYTES`] (what the scale lets a highlighted
//! document hold), under every base budget the installers cut with. The
//! paragraph scanner is `paragraph_boundary`'s, pinned elsewhere (C1).
//!
//! Compiled only under `cfg(kani)` (`make kani`, shard
//! `core-post-open-settle`).

use super::{
    ChunkFacts, ChunkGate, ChunkStep, HIGHLIGHT_CHUNK_BYTES, HighlightInstallOrder,
    MAX_SYNTAX_PARAGRAPH_BYTES, chunk_gate, highlight_install_order, next_chunk_step,
};
use crate::model::buffer_replacement::{PacedSlice, ParagraphLength, SliceBudget};
use crate::model::document_scale::SyntaxLanguage;

/// The longest paragraph sequence the slice harness ranges over.
const MAX_PARAGRAPHS: usize = 6;

fn any_language() -> SyntaxLanguage {
    let index: usize = kani::any();
    kani::assume(index < SyntaxLanguage::ALL.len());
    SyntaxLanguage::ALL[index]
}

/// The chunked order is taken exactly for an allowed syntax, a
/// chunk-highlighted language, and a size above its attach threshold; a
/// gated document never meets the engine.
#[kani::proof]
fn the_install_order_is_chunked_exactly_where_stated() {
    let language = any_language();
    let total: u64 = kani::any();
    let allowed: bool = kani::any();
    let order = highlight_install_order(language, total, allowed);
    assert_eq!(order == HighlightInstallOrder::NoHighlight, !allowed);
    assert_eq!(
        order.is_chunked(),
        allowed
            && language.chunk_highlighted()
            && total > language.attach_after_install_threshold()
    );
    kani::cover!(order.is_chunked());
    kani::cover!(order == HighlightInstallOrder::AttachAfterInstall);
}

/// The chunked order only ever runs below the scale's syntax gate: a size
/// whose syntax the scale allows by size is never above `syntax_size_threshold`.
#[kani::proof]
fn a_chunked_language_is_gated_above_its_attach_threshold_only_by_the_scale() {
    let language = any_language();
    assert!(language.attach_after_install_threshold() <= language.syntax_size_threshold());
    if !language.chunk_highlighted() {
        assert_eq!(
            language.attach_after_install_threshold(),
            language.syntax_size_threshold()
        );
    }
}

/// A chunk waits at most once, and never waits once analysed.
#[kani::proof]
fn a_chunk_waits_at_most_once_and_never_when_analysed() {
    let analysed: bool = kani::any();
    let waited: bool = kani::any();
    let step = next_chunk_step(analysed, waited);
    if analysed || waited {
        assert_eq!(step, ChunkStep::Insert);
    }
    // After one wait the same chunk always inserts.
    assert_eq!(next_chunk_step(analysed, true), ChunkStep::Insert);
    kani::cover!(step == ChunkStep::AwaitAnalysis);
}

/// Under the highlight cut, every slice of a highlighted document holds at
/// most [`HIGHLIGHT_CHUNK_BYTES`], whatever the base budget.
#[kani::proof]
#[kani::unwind(8)]
fn highlighted_slices_never_exceed_the_cut() {
    let paragraphs: [ParagraphLength; MAX_PARAGRAPHS] = core::array::from_fn(|_| {
        let paragraph = ParagraphLength {
            bytes: kani::any(),
            chars: kani::any(),
        };
        kani::assume(paragraph.bytes >= 1 && paragraph.bytes <= MAX_SYNTAX_PARAGRAPH_BYTES);
        paragraph
    });
    let count: usize = kani::any();
    kani::assume(count <= MAX_PARAGRAPHS);
    let base = SliceBudget {
        bytes: kani::any(),
        chars: kani::any(),
    };
    let budget = base.with_highlight_cut();
    assert!(budget.bytes <= HIGHLIGHT_CHUNK_BYTES);
    assert!(budget.bytes <= base.bytes && budget.chars == base.chars);
    let mut first = 0;
    while first < count {
        let mut slice = PacedSlice::new(budget);
        let mut end = first;
        while end < count && slice.admit(paragraphs[end]) {
            end += 1;
        }
        assert!(end > first);
        assert!(slice.bytes() <= HIGHLIGHT_CHUNK_BYTES);
        kani::cover!(end - first > 1);
        first = end;
    }
}

/// Without the syntax gate's paragraph bound, the cut cannot hold: a single
/// paragraph larger than it goes in alone.
#[kani::proof]
#[kani::should_panic]
fn without_the_paragraph_bound_a_slice_exceeds_the_cut() {
    let paragraph = ParagraphLength {
        bytes: kani::any(),
        chars: kani::any(),
    };
    let budget = SliceBudget::BYTES_ONLY.with_highlight_cut();
    let mut slice = PacedSlice::new(budget);
    assert!(slice.admit(paragraph));
    assert!(slice.bytes() <= HIGHLIGHT_CHUNK_BYTES);
}

/// After the first chunk, no chunk goes in with a language attached until
/// the attach resolved and the engine reported the previous one analysed or
/// it waited once; only the first chunk is ever in before the attach request.
#[kani::proof]
fn a_highlighted_chunk_follows_the_attach_and_the_analysis() {
    let facts = ChunkFacts {
        chunks_inserted: kani::any(),
        attach_requested: kani::any(),
        attach_pending: kani::any(),
        language_attached: kani::any(),
        analysed_to_end: kani::any(),
        waited: kani::any(),
    };
    let gate = chunk_gate(facts);
    if facts.chunks_inserted > 0 {
        if gate == ChunkGate::Insert {
            assert!(facts.attach_requested && !facts.attach_pending);
            assert!(facts.language_attached);
            assert!(facts.analysed_to_end || facts.waited);
        }
        if !facts.attach_requested {
            assert_eq!(gate, ChunkGate::RequestAttach);
        }
        if gate == ChunkGate::InsertUnhighlighted {
            assert!(!facts.language_attached && !facts.attach_pending);
        }
    } else {
        assert_eq!(gate, ChunkGate::Insert);
    }
    kani::cover!(gate == ChunkGate::AwaitAttach);
    kani::cover!(gate == ChunkGate::AwaitAnalysis);
    kani::cover!(gate == ChunkGate::InsertUnhighlighted);
}
