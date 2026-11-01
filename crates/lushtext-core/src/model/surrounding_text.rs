// SPDX-License-Identifier: GPL-3.0-or-later

//! The window of text the editor gives its input method as "surrounding
//! text" (`explore-editing-long-paragraphs`, design D6).
//!
//! GTK's own answer spans the cursor's paragraph from its start and computes
//! the whole paragraph's word boundaries on every request; GNOME's text-input
//! path asks on every keystroke and then sends at most 4,000 bytes around the
//! cursor. The editor answers with at most [`SURROUNDING_WINDOW_CHARS`]
//! characters on each side of the cursor (and of the selection's other end),
//! never crossing the cursor's paragraph, so a request costs the same at any
//! paragraph length.
//!
//! Pure: character offsets in, character offsets out. The GTK half is
//! `ui/editor_page/surrounding_text.rs`.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

/// Characters of context on each side of the cursor. A **coverage window**,
/// so a power of two rounded up: the text-input protocol transfers at most
/// 4,000 bytes around the cursor, and an input method that reconverts or
/// completes words sees far more than the three words GTK's own answer
/// guarantees.
pub const SURROUNDING_WINDOW_CHARS: i32 = 1_024;

/// A selection longer than this is not included: the window then spans the
/// cursor alone, with the anchor at the cursor. A **cap**, a power of two
/// rounded down: it bounds the answer's cost.
pub const SURROUNDING_MAX_SELECTION_CHARS: i32 = 8_192;

/// The window, as buffer character offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurroundingWindow {
    /// First character of the window.
    pub start: i32,
    /// One past its last character.
    pub end: i32,
    /// The selection's other end inside the window (the cursor when there is
    /// no selection, or when it is too long or leaves the paragraph).
    pub anchor: i32,
}

/// The window around `cursor` (and `anchor`, the selection's other end) in the
/// paragraph `[line_start, line_end]`, all buffer character offsets.
///
/// The window lies inside the paragraph, contains the cursor, and holds at
/// most `2 × SURROUNDING_WINDOW_CHARS` characters plus the selection when one
/// is included. Offsets outside the paragraph are clamped into it.
#[must_use]
pub const fn surrounding_window(
    line_start: i32,
    cursor: i32,
    anchor: i32,
    line_end: i32,
) -> SurroundingWindow {
    let line_end = if line_end < line_start {
        line_start
    } else {
        line_end
    };
    let cursor = clamp(cursor, line_start, line_end);
    let selection_fits = anchor >= line_start
        && anchor <= line_end
        && anchor.abs_diff(cursor) <= SURROUNDING_MAX_SELECTION_CHARS.unsigned_abs();
    let anchor = if selection_fits { anchor } else { cursor };
    let (low, high) = if anchor < cursor {
        (anchor, cursor)
    } else {
        (cursor, anchor)
    };
    SurroundingWindow {
        start: clamp(
            low.saturating_sub(SURROUNDING_WINDOW_CHARS),
            line_start,
            line_end,
        ),
        end: clamp(
            high.saturating_add(SURROUNDING_WINDOW_CHARS),
            line_start,
            line_end,
        ),
        anchor,
    }
}

const fn clamp(value: i32, low: i32, high: i32) -> i32 {
    if value < low {
        low
    } else if value > high {
        high
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: i32 = SURROUNDING_WINDOW_CHARS;

    #[test]
    fn the_window_is_bounded_and_centred_in_a_long_paragraph() {
        let window = surrounding_window(0, 25_000, 25_000, 50_000);
        assert_eq!(
            window,
            SurroundingWindow {
                start: 25_000 - W,
                end: 25_000 + W,
                anchor: 25_000
            }
        );
    }

    #[test]
    fn the_window_never_leaves_the_paragraph() {
        assert_eq!(
            surrounding_window(100, 150, 150, 400),
            SurroundingWindow {
                start: 100,
                end: 400,
                anchor: 150
            }
        );
        let at_end = surrounding_window(0, 50_000, 50_000, 50_000);
        assert_eq!((at_end.start, at_end.end), (50_000 - W, 50_000));
    }

    #[test]
    fn a_selection_is_included_when_it_fits() {
        let window = surrounding_window(0, 20_000, 18_000, 50_000);
        assert_eq!(
            window,
            SurroundingWindow {
                start: 18_000 - W,
                end: 20_000 + W,
                anchor: 18_000
            }
        );
    }

    #[test]
    fn a_long_or_cross_paragraph_selection_collapses_to_the_cursor() {
        let long = surrounding_window(
            0,
            40_000,
            40_000 - SURROUNDING_MAX_SELECTION_CHARS - 1,
            50_000,
        );
        assert_eq!(long.anchor, 40_000);
        assert_eq!((long.start, long.end), (40_000 - W, 40_000 + W));
        let outside = surrounding_window(1_000, 1_500, 10, 2_000);
        assert_eq!(outside.anchor, 1_500);
    }

    #[test]
    fn extreme_offsets_do_not_overflow() {
        let window = surrounding_window(i32::MAX - 10, i32::MAX, i32::MIN, i32::MAX);
        assert!(window.start <= window.anchor && window.anchor <= window.end);
        let reversed = surrounding_window(10, 5, 5, 0);
        assert_eq!(
            reversed,
            SurroundingWindow {
                start: 10,
                end: 10,
                anchor: 10
            }
        );
    }
}
