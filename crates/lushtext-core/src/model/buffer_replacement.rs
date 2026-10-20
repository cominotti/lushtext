// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain policy for bounded document-sized GTK buffer replacement.
//!
//! GTK adapters own sources, widgets, and terminal callbacks. This module owns
//! the calibrated direct-versus-sliced decision and UTF-8-safe per-turn bounds.

/// Maximum old/new document size that may be replaced synchronously.
pub const SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES: usize = 1024 * 1024;

/// Maximum existing buffer characters deleted in one GTK turn.
pub const REPLACEMENT_CLEAR_SLICE_CHARS: i32 = 64 * 1024;

/// Maximum replacement UTF-8 bytes considered in one GTK insertion turn.
pub const REPLACEMENT_INSERT_SLICE_BYTES: usize = 256 * 1024;

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
    /// and exact incoming UTF-8 byte length.
    #[must_use]
    pub fn for_sizes(existing_chars: i32, replacement_bytes: usize) -> Self {
        let existing_bytes = usize::try_from(existing_chars.max(0))
            .unwrap_or(usize::MAX)
            .saturating_mul(4);
        let mode = if existing_bytes <= SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_uses_both_existing_and_incoming_sizes() {
        let direct = BufferReplacementPlan::for_sizes(12, 12);
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
