// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain policy for bounded document-sized GTK buffer replacement.
//!
//! GTK adapters own sources, widgets, and terminal callbacks. This module owns
//! the calibrated direct-versus-sliced decision and UTF-8-safe per-turn bounds.
//!
//! ## Two slice budgets (`bound-post-open-settle`, design D6)
//!
//! A slice is bounded by bytes ([`REPLACEMENT_INSERT_SLICE_BYTES`]), which
//! bounds the insert itself, and, for a **dense** document, also by visible
//! characters ([`V_SLICE`]), which bounds the line validation the slice's new
//! lines cause in the frame that lays them out (ledger A31, A70). A document is
//! dense when [`batch_exceeds_slice`] holds for its profile: some validation
//! batch's worth of consecutive paragraphs holds more than `V_SLICE` visible
//! characters. The slice-end decision itself is [`PacedSlice`], over abstract
//! `(bytes, visible characters)` paragraph lengths, which Kani checks
//! (`buffer_replacement/kani_proofs.rs`); the text scanner that produces those
//! lengths is `paragraph_boundary`'s, pinned by C1's proofs.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::paragraph_boundary::{ProfileSummary, next_paragraph_break};

/// Maximum old/new document size that may be replaced synchronously.
///
/// The largest short-line body whose one-turn install fits half of
/// `B_SETTLE_US` (`bound-post-open-settle` task 3.1). Measured on the
/// responsiveness lane (probe build, 2026-10-30, load 13 falling to 5.5): the
/// direct install of the 1,000,000-byte short-line fixture took 11.1 ms
/// (10.9 µs per KiB), and each 256 KiB slice of the 5,000,000-byte fixture
/// 2.3–2.6 ms (9–10 µs per KiB). At the worst rate, 8,000 µs covers 734 KiB;
/// 704 KiB keeps about 4 % of margin. The former value, 1 MiB, took 11.1 ms.
pub const SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES: usize = 704 * 1024;

/// Maximum existing buffer characters deleted in one GTK turn.
pub const REPLACEMENT_CLEAR_SLICE_CHARS: i32 = 64 * 1024;

/// Maximum replacement UTF-8 bytes considered in one GTK insertion turn.
pub const REPLACEMENT_INSERT_SLICE_BYTES: usize = 256 * 1024;

/// The most visible characters one paced slice may install, so that the
/// validation its new lines cause fits half of `B_SETTLE_US` in either wrap
/// mode (`bound-post-open-settle`, design D6; derived by ledger A70).
///
/// "Visible characters" are Unicode scalar values excluding paragraph
/// delimiters, as `paragraph_boundary`'s profile counts them.
///
/// Derived 2026-10-30 by the A70 probe (`gtk-lush-axioms`, bisected over
/// inserts of 710-character lines, two exclusive-lock runs at load 1.3–2.5,
/// identical): 78 lines of 710 characters fit half of `B_SETTLE_US` with
/// wrapping off, which binds; with wrapping on no validation dispatch exceeded
/// 3.64 ms, because GtkTextView caps a batch at 2000 px (A31).
///
/// That bare-view figure (55,380) did not hold in the application: the lane
/// measured ~21 ms validation batches there. The editor-like re-derivation
/// (A70: the editor's font CSS, line numbers, current-line highlight, with and
/// without a `GtkSourceMap`; host load 40–56, so magnitudes only) found a step
/// between 11,360 characters (every repetition at most 4.0 ms) and 22,720
/// (saturated at 8–9 ms). `V_SLICE` is therefore the last size below that
/// step, 16 lines of 710 characters. The responsiveness lane confirmed it on
/// the probe build (2026-10-31, `dense/{120,500,710}` in both wrap modes and
/// all three input variants, load 1.3): every validation dispatch until settle
/// at most 11.6 ms, every post-open dispatch at most 15.0 ms.
pub const V_SLICE: u64 = 11_360;

/// Whether a document's densest validation batch holds more than [`V_SLICE`]
/// visible characters (`bound-post-open-settle`, design D6 and D9).
///
/// Such a document is installed in paced slices even below the byte
/// threshold, and its user-started invalidations (word wrap, zoom, font, tab
/// width) are paced. A document of short lines stays direct however many
/// characters it holds: validation cost is per batch (ledger A31), not per
/// document.
#[must_use]
pub const fn batch_exceeds_slice(summary: ProfileSummary) -> bool {
    summary.worst_batch_chars > V_SLICE
}

/// One paragraph as the slicer sees it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParagraphLength {
    /// UTF-8 bytes, its delimiter included.
    pub bytes: u64,
    /// Visible characters: Unicode scalar values, its delimiter excluded.
    pub chars: u64,
}

/// The two budgets of one slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SliceBudget {
    /// The most UTF-8 bytes a slice may hold.
    pub bytes: u64,
    /// The most visible characters a slice may hold.
    pub chars: u64,
}

impl SliceBudget {
    /// The byte budget alone: what every document that is not dense uses.
    pub const BYTES_ONLY: Self = Self {
        bytes: REPLACEMENT_INSERT_SLICE_BYTES as u64,
        chars: u64::MAX,
    };

    /// Both budgets: what a dense document uses.
    pub const PACED: Self = Self {
        bytes: REPLACEMENT_INSERT_SLICE_BYTES as u64,
        chars: V_SLICE,
    };

    /// The budget for a document with `summary`.
    #[must_use]
    pub const fn for_profile(summary: ProfileSummary) -> Self {
        if batch_exceeds_slice(summary) {
            Self::PACED
        } else {
            Self::BYTES_ONLY
        }
    }
}

/// The slice-end decision, one paragraph at a time.
///
/// A slice takes whole paragraphs in order while both budgets hold. The first
/// paragraph is always taken, so a paragraph larger than either budget is
/// installed alone, in one turn: GTK lays a paragraph out whole, so splitting
/// it would re-lay-out the growing line on every later slice (the existing
/// giant-paragraph exception, owned by C2 and C3). Allocation-free, so Kani
/// checks it over symbolic paragraph sequences.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacedSlice {
    budget: SliceBudget,
    bytes: u64,
    chars: u64,
    paragraphs: u32,
}

impl PacedSlice {
    /// An empty slice under `budget`.
    #[must_use]
    pub const fn new(budget: SliceBudget) -> Self {
        Self {
            budget,
            bytes: 0,
            chars: 0,
            paragraphs: 0,
        }
    }

    /// Whether `paragraph` joins this slice; when it does not, the slice ends
    /// before it.
    pub const fn admit(&mut self, paragraph: ParagraphLength) -> bool {
        let bytes = self.bytes.saturating_add(paragraph.bytes);
        let chars = self.chars.saturating_add(paragraph.chars);
        if self.paragraphs > 0 && (bytes > self.budget.bytes || chars > self.budget.chars) {
            return false;
        }
        self.bytes = bytes;
        self.chars = chars;
        self.paragraphs = self.paragraphs.saturating_add(1);
        true
    }

    /// The bytes the slice holds.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The visible characters the slice holds.
    #[must_use]
    pub const fn chars(&self) -> u64 {
        self.chars
    }

    /// The paragraphs the slice holds.
    #[must_use]
    pub const fn paragraphs(&self) -> u32 {
        self.paragraphs
    }
}

/// Execution shape for one whole-buffer replacement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferReplacementMode {
    /// Clear and install in the current GTK turn.
    Direct,
    /// Clear and install through bounded scheduled turns.
    Sliced,
}

/// Immutable policy decision for one clear-only or clear-and-insert request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferReplacementPlan {
    /// Direct or scheduled execution selected from both old and new sizes.
    pub mode: BufferReplacementMode,
    /// Whether the terminal buffer must remain empty.
    pub clear_only: bool,
}

impl BufferReplacementPlan {
    /// Classify one replacement from the conservative existing character charge
    /// and exact incoming UTF-8 byte length, for a text that is not dense.
    #[must_use]
    pub fn for_sizes(existing_chars: i32, replacement_bytes: usize) -> Self {
        Self::for_text(existing_chars, replacement_bytes, false)
    }

    /// Classify one replacement from the conservative existing character charge,
    /// the exact incoming UTF-8 byte length, and whether the incoming text is
    /// dense ([`batch_exceeds_slice`]): a dense text is sliced at any size, so
    /// each slice's validation stays bounded.
    #[must_use]
    pub fn for_text(existing_chars: i32, replacement_bytes: usize, dense: bool) -> Self {
        let existing_bytes = usize::try_from(existing_chars.max(0))
            .unwrap_or(usize::MAX)
            .saturating_mul(4);
        let mode = if !dense
            && existing_bytes <= SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
            && replacement_bytes <= SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
        {
            BufferReplacementMode::Direct
        } else {
            BufferReplacementMode::Sliced
        };
        Self {
            mode,
            clear_only: replacement_bytes == 0,
        }
    }
}

/// Return the number of leading characters one clear turn may delete.
#[must_use]
pub fn next_clear_char_count(remaining_chars: i32) -> i32 {
    remaining_chars.clamp(0, REPLACEMENT_CLEAR_SLICE_CHARS)
}

/// Return the next paragraph-aligned boundary near the insertion byte budget.
///
/// GTK text layout validates whole paragraphs, so a slice that stops inside a
/// paragraph forces the next slice to re-lay-out everything installed so far —
/// quadratic total work that freezes recovery of single-line documents. Every
/// slice therefore ends just after a **GTK paragraph delimiter** (`\n`, `\r`,
/// `\r\n` kept whole, or U+2029, the set `pango_find_paragraph_boundary` and
/// `GtkTextBuffer` use; ledger A42): paragraphs installed by earlier turns are
/// never touched again. It ends after the last delimiter within
/// [`REPLACEMENT_INSERT_SLICE_BYTES`], or, when none is, after the first one
/// beyond it, so a single paragraph longer than the budget is installed in one
/// turn because GTK cannot lay it out incrementally anyway.
///
/// The choice itself is
/// [`slice_end_after_paragraph`](super::paragraph_boundary::slice_end_after_paragraph),
/// whose guarantees Kani proves on strings of at most 8 bytes and the Pango
/// property test samples beyond; this function is its named entry point for
/// the `WFR-BUFFER-REPLACEMENT` workflow.
#[must_use]
pub fn next_replacement_boundary(text: &str, start: usize) -> usize {
    super::paragraph_boundary::slice_end_after_paragraph(
        text,
        start,
        REPLACEMENT_INSERT_SLICE_BYTES,
    )
}

/// Return the next paragraph-aligned boundary under `budget`: the end of the
/// slice [`PacedSlice`] chooses from the paragraphs starting at `start`.
///
/// With [`SliceBudget::BYTES_ONLY`] this is [`next_replacement_boundary`],
/// which it delegates to (the backwards search finds the same end without
/// walking every paragraph); with a visible-character budget it walks the
/// paragraphs forwards, at most one budget of characters plus one paragraph.
#[must_use]
pub fn next_slice_boundary(text: &str, start: usize, budget: SliceBudget) -> usize {
    if budget.chars == u64::MAX && budget.bytes == SliceBudget::BYTES_ONLY.bytes {
        return next_replacement_boundary(text, start);
    }
    let mut slice = PacedSlice::new(budget);
    let mut end = start;
    while end < text.len() {
        let (content_end, next_start) = next_paragraph_break(text, end)
            .map_or((text.len(), text.len()), |found| {
                (found.content_end, found.next_start)
            });
        let paragraph = ParagraphLength {
            bytes: u64::try_from(next_start - end).unwrap_or(u64::MAX),
            chars: u64::try_from(text[end..content_end].chars().count()).unwrap_or(u64::MAX),
        };
        if !slice.admit(paragraph) {
            break;
        }
        end = next_start;
    }
    end.max(start.min(text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::paragraph_boundary::DocumentProfile;

    /// `lines` lines of `width` visible characters each, `\n`-ended.
    fn lines_of(width: usize, lines: usize) -> String {
        format!("{}\n", "d".repeat(width)).repeat(lines)
    }

    #[test]
    fn a_dense_document_below_the_byte_threshold_is_sliced() {
        // 160 lines of 710 characters: 113 kB, far below the byte threshold,
        // but one validation batch holds far more than V_SLICE characters.
        let text = lines_of(710, 160);
        assert!(text.len() < SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES);
        let summary = DocumentProfile::of_text(&text).summary();
        assert!(batch_exceeds_slice(summary), "{summary:?}");
        assert_eq!(
            BufferReplacementPlan::for_text(0, text.len(), batch_exceeds_slice(summary)).mode,
            BufferReplacementMode::Sliced
        );
        let mut start = 0;
        let mut slices = 0;
        while start < text.len() {
            let end = next_slice_boundary(&text, start, SliceBudget::for_profile(summary));
            let chars = u64::try_from(text[start..end].chars().filter(|c| *c != '\n').count())
                .expect("fits");
            assert!(chars <= V_SLICE, "slice of {chars} visible characters");
            assert!(text[..end].ends_with('\n'));
            start = end;
            slices += 1;
        }
        assert!(slices > 1, "a dense document must take several slices");
    }

    #[test]
    fn a_short_line_document_with_many_characters_stays_direct() {
        // More than V_SLICE characters in total, but 40-character lines keep
        // every validation batch far below it (review R13).
        let text = lines_of(40, 2_000);
        let summary = DocumentProfile::of_text(&text).summary();
        assert!(u64::try_from(text.len()).expect("fits") > V_SLICE);
        assert!(!batch_exceeds_slice(summary), "{summary:?}");
        assert_eq!(SliceBudget::for_profile(summary), SliceBudget::BYTES_ONLY);
        assert_eq!(
            BufferReplacementPlan::for_text(0, text.len(), false).mode,
            BufferReplacementMode::Direct
        );
    }

    #[test]
    fn the_byte_budget_alone_is_the_existing_slicer() {
        for text in [
            lines_of(999, 600),
            format!(
                "head\n{}\ntail",
                "b".repeat(3 * REPLACEMENT_INSERT_SLICE_BYTES)
            ),
            format!(
                "{}\r\n{}",
                "a".repeat(REPLACEMENT_INSERT_SLICE_BYTES - 1),
                "b\r\n".repeat(8)
            ),
        ] {
            let mut start = 0;
            while start < text.len() {
                let fast = next_replacement_boundary(&text, start);
                // A walker under the same byte budget (any chars budget it can
                // never reach) must cut exactly where the backwards search does.
                let walked = next_slice_boundary(
                    &text,
                    start,
                    SliceBudget {
                        bytes: SliceBudget::BYTES_ONLY.bytes,
                        chars: u64::MAX - 1,
                    },
                );
                assert_eq!(walked, fast, "at {start}");
                assert_eq!(
                    next_slice_boundary(&text, start, SliceBudget::BYTES_ONLY),
                    fast
                );
                start = fast;
            }
        }
    }

    #[test]
    fn a_paragraph_over_the_visible_budget_goes_alone() {
        let giant = "g".repeat(usize::try_from(V_SLICE).expect("fits") * 2);
        let text = format!("a\n{giant}\nb\n");
        let first = next_slice_boundary(&text, 0, SliceBudget::PACED);
        assert_eq!(&text[..first], "a\n");
        let second = next_slice_boundary(&text, first, SliceBudget::PACED);
        assert_eq!(&text[first..second], format!("{giant}\n"));
        assert_eq!(
            next_slice_boundary(&text, second, SliceBudget::PACED),
            text.len()
        );
        assert_eq!(
            next_slice_boundary(&text, text.len(), SliceBudget::PACED),
            text.len()
        );
    }

    #[test]
    fn a_paced_slice_admits_the_first_paragraph_and_stops_at_either_budget() {
        let budget = SliceBudget {
            bytes: 10,
            chars: 6,
        };
        let mut slice = PacedSlice::new(budget);
        assert!(slice.admit(ParagraphLength {
            bytes: 40,
            chars: 39
        }));
        assert!(!slice.admit(ParagraphLength { bytes: 1, chars: 0 }));
        let mut slice = PacedSlice::new(budget);
        assert!(slice.admit(ParagraphLength { bytes: 4, chars: 3 }));
        assert!(slice.admit(ParagraphLength { bytes: 4, chars: 3 }));
        assert!(
            !slice.admit(ParagraphLength { bytes: 1, chars: 1 }),
            "chars"
        );
        let mut slice = PacedSlice::new(budget);
        assert!(slice.admit(ParagraphLength { bytes: 8, chars: 1 }));
        assert!(
            !slice.admit(ParagraphLength { bytes: 3, chars: 1 }),
            "bytes"
        );
        assert_eq!(
            (slice.bytes(), slice.chars(), slice.paragraphs()),
            (8, 1, 1)
        );
    }

    #[test]
    fn plan_uses_both_existing_and_incoming_sizes() {
        let direct = BufferReplacementPlan::for_sizes(12, 12);
        assert_eq!(
            BufferReplacementPlan::for_text(12, 12, true).mode,
            BufferReplacementMode::Sliced
        );
        assert_eq!(direct.mode, BufferReplacementMode::Direct);
        assert!(!direct.clear_only);

        let large_old = i32::try_from(SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES / 4 + 1)
            .expect("threshold fits i32");
        assert_eq!(
            BufferReplacementPlan::for_sizes(large_old, 0),
            BufferReplacementPlan {
                mode: BufferReplacementMode::Sliced,
                clear_only: true,
            }
        );
        assert_eq!(
            BufferReplacementPlan::for_sizes(0, SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES + 1).mode,
            BufferReplacementMode::Sliced
        );
    }

    #[test]
    fn clear_policy_handles_empty_and_exact_boundaries() {
        assert_eq!(next_clear_char_count(-1), 0);
        assert_eq!(next_clear_char_count(0), 0);
        assert_eq!(
            next_clear_char_count(REPLACEMENT_CLEAR_SLICE_CHARS),
            REPLACEMENT_CLEAR_SLICE_CHARS
        );
        assert_eq!(
            next_clear_char_count(REPLACEMENT_CLEAR_SLICE_CHARS + 1),
            REPLACEMENT_CLEAR_SLICE_CHARS
        );
    }

    #[test]
    fn insertion_policy_ends_bounded_slices_after_paragraphs() {
        let line = format!("{}🙂e\u{301}\n", "a".repeat(1_000));
        let text = line.repeat(2 * REPLACEMENT_INSERT_SLICE_BYTES / line.len());
        let mut start = 0;
        let mut rebuilt = String::new();
        while start < text.len() {
            let end = next_replacement_boundary(&text, start);
            assert!(end > start);
            assert!(end - start <= REPLACEMENT_INSERT_SLICE_BYTES);
            assert!(text.is_char_boundary(end));
            assert!(text[..end].ends_with('\n') || end == text.len());
            rebuilt.push_str(&text[start..end]);
            start = end;
        }
        assert_eq!(rebuilt, text);
    }

    #[test]
    fn insertion_policy_installs_oversized_paragraphs_atomically() {
        // A paragraph longer than the byte budget must land in one turn: GTK
        // lays out whole paragraphs, so splitting it re-validates the same
        // growing line on every later slice (quadratic recovery installs).
        let giant = "b".repeat(3 * REPLACEMENT_INSERT_SLICE_BYTES);
        let text = format!("head\n{giant}\ntail");
        let first = next_replacement_boundary(&text, 0);
        assert_eq!(&text[..first], "head\n");
        let second = next_replacement_boundary(&text, first);
        assert_eq!(second, text.len() - "tail".len());
        assert!(text[..second].ends_with('\n'));
        assert_eq!(next_replacement_boundary(&text, second), text.len());
    }

    #[test]
    fn insertion_policy_takes_newline_free_tail_in_one_turn() {
        let text = format!("x{}", "y".repeat(2 * REPLACEMENT_INSERT_SLICE_BYTES));
        assert_eq!(next_replacement_boundary(&text, 0), text.len());
        assert_eq!(next_replacement_boundary(&text, text.len()), text.len());
    }

    /// Collect the slices `next_replacement_boundary` cuts `text` into, asserting
    /// the per-slice safety contract on the way.
    fn replacement_slices(text: &str) -> Vec<&str> {
        let mut slices = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let end = next_replacement_boundary(text, start);
            assert!(end > start, "a slice must advance");
            assert!(text.is_char_boundary(end));
            assert!(
                !(text.as_bytes().get(end.wrapping_sub(1)) == Some(&b'\r')
                    && text.as_bytes().get(end) == Some(&b'\n')),
                "a slice must never separate CR from LF"
            );
            slices.push(&text[start..end]);
            start = end;
        }
        slices
    }

    #[test]
    fn insertion_policy_slices_cr_only_documents() {
        // GTK ends a paragraph at a lone `\r` (pango_find_paragraph_boundary), so
        // a CR-only document above the synchronous threshold must be sliced
        // like a `\n` one instead of landing in one GTK turn.
        let line = format!("{}\r", "c".repeat(999));
        let text = line.repeat(SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES / line.len() + 1);
        assert!(text.len() > SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES);
        let slices = replacement_slices(&text);
        assert!(slices.len() > 1, "CR-only text installed in one turn");
        for slice in &slices {
            assert!(slice.len() <= REPLACEMENT_INSERT_SLICE_BYTES);
            assert!(slice.ends_with('\r'));
        }
        assert_eq!(slices.concat(), text);
    }

    #[test]
    fn insertion_policy_slices_paragraph_separator_documents() {
        let line = format!("{}\u{2029}", "p".repeat(997));
        let text = line.repeat(SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES / line.len() + 1);
        assert!(text.len() > SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES);
        let slices = replacement_slices(&text);
        assert!(slices.len() > 1, "U+2029 text installed in one turn");
        for slice in &slices {
            assert!(slice.len() <= REPLACEMENT_INSERT_SLICE_BYTES);
            assert!(slice.ends_with('\u{2029}'));
        }
        assert_eq!(slices.concat(), text);
    }

    #[test]
    fn insertion_policy_never_splits_crlf_at_the_budget_edge() {
        // The budget ends immediately after the `\r` of a CRLF pair: the slice
        // must end after the `\n`, keeping the pair one GTK delimiter.
        let text = format!(
            "{}\r\n{}",
            "a".repeat(REPLACEMENT_INSERT_SLICE_BYTES - 1),
            "b\r\n".repeat(8)
        );
        let end = next_replacement_boundary(&text, 0);
        assert_eq!(end, REPLACEMENT_INSERT_SLICE_BYTES + 1);
        assert_eq!(replacement_slices(&text).concat(), text);
    }

    #[test]
    fn insertion_policy_never_splits_multibyte_chars_at_the_budget_edge() {
        // A multibyte char straddling the byte budget must not panic the
        // newline search on either side of the budget edge.
        let text = format!(
            "{}🙂e\u{301}\nnext line\n",
            "a".repeat(REPLACEMENT_INSERT_SLICE_BYTES - 1)
        );
        let end = next_replacement_boundary(&text, 0);
        assert!(text.is_char_boundary(end));
        assert!(text[..end].ends_with('\n'));
        assert_eq!(next_replacement_boundary(&text, end), text.len());
    }
}
