// SPDX-License-Identifier: GPL-3.0-or-later

//! Which paragraphs the editor shows unwrapped: the per-paragraph wrap gate
//! (`explore-editing-long-paragraphs`, design D3).
//!
//! Every keystroke makes GTK lay the edited paragraph out once, whole, inside
//! the key press (ledger A53's companion profile: `gtk_text_view_commit_text`
//! places the virtual cursor). A wrapped layout of a long paragraph costs
//! about twice an unwrapped one, so a paragraph longer than
//! [`L_WRAP_EDIT`] is shown unwrapped
//! while every other paragraph keeps the user's wrap setting.
//!
//! The decision has **hysteresis**: a paragraph unwraps above the threshold
//! and wraps again only once it is [`REWRAP_MARGIN`] characters shorter, so
//! typing and deleting around the threshold does not flip its layout (and
//! relayout it) on every keystroke.
//!
//! Pure: no GTK. The GTK half (`ui/editor_page/paragraph_wrap.rs`) tags the
//! paragraph, whose first character decides its wrap mode (ledger A53); the
//! tag is also the `current` state this decision keeps.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

use super::document_scale::L_WRAP_EDIT;
use super::paragraph_boundary::next_paragraph_break;

/// Characters a paragraph unwrapped by the gate must lose, below
/// [`L_WRAP_EDIT`], before it wraps again. A hysteresis width, a power of two
/// like the cap it is subtracted from (wraps again at 7,168).
pub const REWRAP_MARGIN: u64 = 1_024;

/// How one paragraph is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParagraphWrap {
    /// It follows the view's wrap setting.
    FollowsView,
    /// It is shown unwrapped whatever the view's setting.
    Unwrapped,
}

/// The wrap state of a paragraph of `chars` characters (its delimiter
/// excluded) that is currently in state `current`.
///
/// Above [`L_WRAP_EDIT`] it is unwrapped; at or below
/// `L_WRAP_EDIT - REWRAP_MARGIN` it follows the view; in between it keeps
/// `current`.
#[must_use]
pub const fn paragraph_wrap(chars: u64, current: ParagraphWrap) -> ParagraphWrap {
    if chars > L_WRAP_EDIT {
        ParagraphWrap::Unwrapped
    } else if chars <= L_WRAP_EDIT.saturating_sub(REWRAP_MARGIN) {
        ParagraphWrap::FollowsView
    } else {
        current
    }
}

/// The indices, counted from an inserted run's first paragraph (0), of the
/// paragraphs that lie wholly inside `run` (between two of its GTK paragraph
/// breaks) and are longer than [`L_WRAP_EDIT`] characters.
///
/// Such paragraphs are new and untagged, so these are the only interior ones
/// the gate must look at; the run's first and last paragraphs join existing
/// text and are always read from the buffer. An installation of many short
/// lines therefore reads no line from the buffer.
#[must_use]
pub fn long_interior_paragraphs(run: &str) -> Vec<i32> {
    let threshold = usize::try_from(L_WRAP_EDIT).unwrap_or(usize::MAX);
    let mut long = Vec::new();
    let Some(mut previous) = next_paragraph_break(run, 0) else {
        return long;
    };
    let mut index: i32 = 1;
    while let Some(next) = next_paragraph_break(run, previous.next_start) {
        let content = &run[previous.next_start..next.content_end];
        // A byte count bounds the character count, so most paragraphs are
        // ruled out without counting.
        if content.len() > threshold && content.chars().count() > threshold {
            long.push(index);
        }
        previous = next;
        index = index.saturating_add(1);
    }
    long
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paragraph_unwraps_only_above_the_threshold() {
        for current in [ParagraphWrap::FollowsView, ParagraphWrap::Unwrapped] {
            assert_eq!(
                paragraph_wrap(L_WRAP_EDIT + 1, current),
                ParagraphWrap::Unwrapped
            );
            assert_eq!(paragraph_wrap(u64::MAX, current), ParagraphWrap::Unwrapped);
        }
        assert_eq!(
            paragraph_wrap(L_WRAP_EDIT, ParagraphWrap::FollowsView),
            ParagraphWrap::FollowsView
        );
    }

    #[test]
    fn an_unwrapped_paragraph_rewraps_only_below_the_margin() {
        let low = L_WRAP_EDIT - REWRAP_MARGIN;
        assert_eq!(
            paragraph_wrap(L_WRAP_EDIT, ParagraphWrap::Unwrapped),
            ParagraphWrap::Unwrapped
        );
        assert_eq!(
            paragraph_wrap(low + 1, ParagraphWrap::Unwrapped),
            ParagraphWrap::Unwrapped
        );
        assert_eq!(
            paragraph_wrap(low, ParagraphWrap::Unwrapped),
            ParagraphWrap::FollowsView
        );
        assert_eq!(
            paragraph_wrap(0, ParagraphWrap::Unwrapped),
            ParagraphWrap::FollowsView
        );
    }

    #[test]
    fn only_long_paragraphs_between_two_breaks_are_named() {
        let long = "w".repeat(usize::try_from(L_WRAP_EDIT).expect("fits") + 1);
        let exact = "w".repeat(usize::try_from(L_WRAP_EDIT).expect("fits"));
        assert!(
            long_interior_paragraphs(&long).is_empty(),
            "no break: no interior paragraph"
        );
        assert!(
            long_interior_paragraphs(&format!("{long}\n")).is_empty(),
            "the first paragraph is not interior"
        );
        assert!(
            long_interior_paragraphs(&format!("\n{long}")).is_empty(),
            "the last paragraph is not interior"
        );
        assert_eq!(long_interior_paragraphs(&format!("a\n{long}\nb")), vec![1]);
        assert_eq!(
            long_interior_paragraphs(&format!("a\r\nb\r{exact}\u{2029}{long}\n")),
            vec![3]
        );
        // Multibyte text longer than the threshold in bytes but not in characters.
        let wide = "é".repeat(usize::try_from(L_WRAP_EDIT).expect("fits"));
        assert!(long_interior_paragraphs(&format!("a\n{wide}\nb")).is_empty());
    }

    #[test]
    fn typing_back_and_forth_at_the_threshold_flips_at_most_once() {
        let mut state = ParagraphWrap::FollowsView;
        let mut flips = 0;
        for chars in [
            L_WRAP_EDIT,
            L_WRAP_EDIT + 1,
            L_WRAP_EDIT,
            L_WRAP_EDIT + 1,
            L_WRAP_EDIT - 1,
        ] {
            let next = paragraph_wrap(chars, state);
            flips += usize::from(next != state);
            state = next;
        }
        assert_eq!(flips, 1);
        assert_eq!(state, ParagraphWrap::Unwrapped);
    }
}
