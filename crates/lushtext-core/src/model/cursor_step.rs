// SPDX-License-Identifier: GPL-3.0-or-later

//! When the editor may answer an arrow-key step logically
//! (`adopt-gnome-51`, lane regression L2).
//!
//! Left and Right ask `GtkTextView` for a *visual* cursor step, which Pango
//! answers by listing every cursor position of the display line, each by its
//! own extents pass: the cost is quadratic in the line's length (ledger
//! A107), 76 ms on a 2,500-character unwrapped line and 1.2 s on a
//! 10,000-character one. A *logical* step moves by the buffer's cursor
//! positions and lays nothing out. On left-to-right text the two land on the
//! same place (A107), so for a paragraph holding no right-to-left text the
//! editor answers the visual step with a logical one.
//!
//! A paragraph is unwrapped, one display line, whenever word wrap is off or
//! the C5 wrap gate unwraps it (longer than `L_WRAP_EDIT`), which is why the
//! decision is about the paragraph and not the display line.
//!
//! Pure: no GTK. The GTK half is `ui/editor_page/cursor_steps.rs`.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

/// Paragraph length, in characters, above which a visual step is answered
/// logically. Below it GTK's own step runs unchanged: A107's quadratic cost
/// puts a 256-character line's visual step well under a millisecond.
///
/// Kind: policy tier; a power of two (it hides nothing: both steps land on the same place).
pub const L_LOGICAL_CURSOR_STEP: usize = 256;

/// Whether `c` makes the paragraph's visual order differ from its logical
/// order: a character of a right-to-left script block, or an explicit
/// right-to-left mark, embedding, override or isolate.
///
/// The blocks are whole Unicode blocks of right-to-left scripts, a superset
/// of the strong `R` and `AL` bidi classes in them: over-matching only keeps
/// GTK's own visual step, which is always correct.
#[must_use]
pub const fn is_right_to_left(c: char) -> bool {
    matches!(
        c as u32,
        // Hebrew, Arabic, Syriac, Arabic Supplement, Thaana, NKo, Samaritan,
        // Mandaic, Syriac Supplement, Arabic Extended-B and -A.
        0x0590..=0x08FF
            // Hebrew and Arabic presentation forms (A).
            | 0xFB1D..=0xFDFF
            // Arabic presentation forms (B).
            | 0xFE70..=0xFEFF
            // Historic right-to-left scripts of the SMP (Cypriot to Old
            // Uyghur, Hanifi Rohingya, Yezidi, Sogdian, ...).
            | 0x10800..=0x10FFF
            // Mende Kikakui, Adlam, Indic and Ottoman Siyaq numbers, Arabic
            // mathematical symbols.
            | 0x1E800..=0x1EFFF
            // RIGHT-TO-LEFT MARK, EMBEDDING, OVERRIDE, ISOLATE.
            | 0x200F
            | 0x202B
            | 0x202E
            | 0x2067
    )
}

/// Whether a visual step inside a paragraph whose text is `paragraph` (its
/// delimiter excluded) may be answered with a logical one: the paragraph is
/// longer than [`L_LOGICAL_CURSOR_STEP`] characters and holds no
/// right-to-left text.
#[must_use]
pub fn logical_step_suffices(paragraph: &str) -> bool {
    let mut chars = 0usize;
    for c in paragraph.chars() {
        if is_right_to_left(c) {
            return false;
        }
        chars = chars.saturating_add(1);
    }
    chars > L_LOGICAL_CURSOR_STEP
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ascii(chars: usize) -> String {
        "abcdefgh ".chars().cycle().take(chars).collect()
    }

    #[test]
    fn a_long_left_to_right_paragraph_takes_the_logical_step() {
        assert!(logical_step_suffices(&ascii(L_LOGICAL_CURSOR_STEP + 1)));
        assert!(logical_step_suffices(&ascii(10_000)));
    }

    #[test]
    fn a_paragraph_at_or_below_the_threshold_keeps_the_visual_step() {
        assert!(!logical_step_suffices(&ascii(L_LOGICAL_CURSOR_STEP)));
        assert!(!logical_step_suffices(""));
    }

    #[test]
    fn the_threshold_counts_characters_not_bytes() {
        // 200 three-byte characters: 600 bytes, under the threshold.
        let cjk: String = std::iter::repeat_n('\u{4e2d}', 200).collect();
        assert!(!logical_step_suffices(&cjk));
        let longer: String = std::iter::repeat_n('\u{4e2d}', L_LOGICAL_CURSOR_STEP + 1).collect();
        assert!(logical_step_suffices(&longer));
    }

    #[test]
    fn any_right_to_left_character_keeps_the_visual_step() {
        for rtl in [
            '\u{05d0}',  // HEBREW LETTER ALEF
            '\u{0627}',  // ARABIC LETTER ALEF
            '\u{0710}',  // SYRIAC LETTER ALAPH
            '\u{0780}',  // THAANA LETTER HAA
            '\u{07ca}',  // NKO LETTER A
            '\u{08a0}',  // ARABIC LETTER BEH WITH SMALL V BELOW
            '\u{fb1d}',  // HEBREW LETTER YOD WITH HIRIQ
            '\u{fdf2}',  // ARABIC LIGATURE ALLAH
            '\u{fe70}',  // ARABIC FATHATAN ISOLATED FORM
            '\u{10900}', // PHOENICIAN LETTER ALF
            '\u{1e900}', // ADLAM CAPITAL LETTER ALIF
            '\u{200f}',  // RIGHT-TO-LEFT MARK
            '\u{202b}',  // RIGHT-TO-LEFT EMBEDDING
            '\u{202e}',  // RIGHT-TO-LEFT OVERRIDE
            '\u{2067}',  // RIGHT-TO-LEFT ISOLATE
        ] {
            assert!(is_right_to_left(rtl), "{:x}", rtl as u32);
            let mut text = ascii(1_000);
            text.insert(500, rtl);
            assert!(!logical_step_suffices(&text), "{:x}", rtl as u32);
            let mut tail = ascii(1_000);
            tail.push(rtl);
            assert!(!logical_step_suffices(&tail), "a trailing {:x}", rtl as u32);
        }
    }

    #[test]
    fn left_to_right_and_neutral_characters_are_not_right_to_left() {
        for ltr in [
            'a',
            ' ',
            '0',
            '{',
            '\u{00e9}',
            '\u{0400}',
            '\u{058f}',
            '\u{0900}',
            '\u{4e2d}',
            '\u{fb1c}',
            '\u{fe00}',
            '\u{ff00}',
            '\u{107ff}',
            '\u{11000}',
            '\u{1e7ff}',
            '\u{1f000}',
            '\u{200e}',
            '\u{202a}',
            '\u{202c}',
            '\u{202d}',
            '\u{2066}',
            '\u{2068}',
            '\u{2069}',
        ] {
            assert!(!is_right_to_left(ltr), "{:x}", ltr as u32);
        }
    }
}
