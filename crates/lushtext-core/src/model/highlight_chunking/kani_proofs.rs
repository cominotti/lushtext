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
    MAX_SYNTAX_PARAGRAPH_BYTES, chunk_gate, highlight_install_order_with, next_chunk_step,
};
use crate::model::buffer_replacement::{PacedSlice, ParagraphLength, SliceBudget};
use crate::model::document_scale::{MODEL_MARKDOWN_CAP, SyntaxLanguage};

/// The longest paragraph sequence the slice harness ranges over.
const MAX_PARAGRAPHS: usize = 6;

fn any_language() -> SyntaxLanguage {
    let index: usize = kani::any();
    kani::assume(index < SyntaxLanguage::ALL.len());
    SyntaxLanguage::ALL[index]
}

/// The shipped table (`None`) or the model cap the chunked tests use: the
/// harnesses prove both.
fn any_markdown_cap() -> Option<u64> {
    if kani::any() {
        Some(MODEL_MARKDOWN_CAP)
    } else {
        None
    }
}

/// The chunked order is taken exactly for an allowed syntax, a
/// chunk-highlighted language, and a size above its attach threshold; a
/// gated document never meets the engine.
#[kani::proof]
fn the_install_order_is_chunked_exactly_where_stated() {
    let language = any_language();
    let total: u64 = kani::any();
    let allowed: bool = kani::any();
    let cap = any_markdown_cap();
    let order = highlight_install_order_with(language, total, allowed, cap);
    assert_eq!(order == HighlightInstallOrder::NoHighlight, !allowed);
    assert_eq!(
        order.is_chunked(),
        allowed
            && language.chunk_highlighted_with(cap)
            && total > language.attach_after_install_threshold()
    );
    if cap.is_none() {
        assert!(!order.is_chunked());
    }
    kani::cover!(order.is_chunked());
    kani::cover!(order == HighlightInstallOrder::AttachAfterInstall);
}

/// The chunked order only ever runs below the scale's syntax gate: a size
/// whose syntax the scale allows by size is never above `syntax_size_threshold`.
#[kani::proof]
fn a_chunked_language_is_gated_above_its_attach_threshold_only_by_the_scale() {
    let language = any_language();
    let cap = any_markdown_cap();
    assert!(language.attach_after_install_threshold() <= language.syntax_size_threshold_with(cap));
    if !language.chunk_highlighted_with(cap) {
        assert_eq!(
            language.attach_after_install_threshold(),
            language.syntax_size_threshold_with(cap)
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

/// Chunks in the liveness model.
const LIVE_CHUNKS: u64 = 4;
/// Steps after which a pending attach stage has resolved at the latest: the
/// stage's `ATTACH_DEADLINE_US` (`language_attach.rs`). An install in the
/// shown, focused editor skips input-quiet and resolves sooner; every other
/// install waits for input-quiet, which the deadline bounds.
const LIVE_DEADLINE: u32 = 3;
/// Identity changes (rename, Save As) that re-arm the stage (review S1).
const LIVE_RENAMES: u32 = 1;

/// Liveness: a chunked install of `LIVE_CHUNKS` chunks finishes within
/// `2·chunks + (deadline + 2)·(1 + renames) + 1` steps, whatever the
/// environment does, as long as a pending attach stage resolves by its
/// deadline: the engine's coverage report may never arrive, the stage may
/// leave no language, and an identity change may re-arm the stage once at
/// any step. Each step is one installer dispatch running `chunk_gate`.
#[kani::proof]
#[kani::unwind(40)]
fn a_chunked_install_finishes_even_without_coverage_or_a_language() {
    let total: u64 = kani::any();
    kani::assume(total >= 1 && total <= LIVE_CHUNKS);
    let bound = 2 * LIVE_CHUNKS as u32 + (LIVE_DEADLINE + 2) * (1 + LIVE_RENAMES) + 1;
    let mut inserted = 0_u64;
    let mut requested = false;
    let mut pending = false;
    let mut pending_age = 0_u32;
    let mut language = false;
    let mut waited = false;
    let mut renames = LIVE_RENAMES;
    let mut steps = 0_u32;
    while inserted < total && steps < bound {
        steps += 1;
        // The environment, before the installer's dispatch.
        if pending {
            pending_age += 1;
            let resolves: bool = kani::any();
            if resolves || pending_age >= LIVE_DEADLINE {
                pending = false;
                language = kani::any();
            }
        }
        if renames > 0 && requested && kani::any() {
            renames -= 1;
            pending = true;
            pending_age = 0;
        }
        let analysed: bool = kani::any();
        let facts = ChunkFacts {
            chunks_inserted: inserted,
            attach_requested: requested,
            attach_pending: pending,
            language_attached: language,
            analysed_to_end: analysed,
            waited,
        };
        match chunk_gate(facts) {
            ChunkGate::Insert | ChunkGate::InsertUnhighlighted => {
                inserted += 1;
                waited = false;
            }
            ChunkGate::RequestAttach => {
                requested = true;
                pending = true;
                pending_age = 0;
            }
            ChunkGate::AwaitAttach => {}
            ChunkGate::AwaitAnalysis => waited = true,
        }
    }
    assert_eq!(inserted, total, "the install finished within the bound");
    kani::cover!(steps > 2 * LIVE_CHUNKS as u32);
}
