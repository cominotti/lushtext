// SPDX-License-Identifier: GPL-3.0-or-later

//! Byte scans over decoded text: the load's file-health count and the save
//! rewrites (trailing-whitespace trim, line-ending normalization, and the
//! already-normalized check).
//!
//! Each scan jumps between the few bytes that matter with the `memchr` crate
//! (SSE2 baseline, AVX2 picked at run time on x86-64, NEON on aarch64) instead
//! of stepping one `char` at a time. Every cut point is an ASCII byte or a
//! UTF-8 lead byte, so every sub-slice is a `&str` on a scalar boundary and no
//! `unsafe` is needed. The scalar forms they replaced live in [`reference`]
//! (tests, benches, and fuzzing only); the property tests
//! (`tests/properties/text_scans.rs`) and the `editor_bytes` fuzz target pin
//! each scan to its reference. No Kani harness compiles this module.
//!
//! Measured on the interleaved bench of `simd-line-scans`; the figures are in
//! `docs/benchmarks/transient-file-load.md`.

use std::borrow::Cow;

use crate::model::encoding::LineEnding;

#[cfg(any(test, feature = "test-utils", feature = "fuzzing"))]
pub mod reference;

/// Whether `character` is one of the zero-width values the load surfaces.
///
/// [`count_health_characters`] matches the same set by its UTF-8 bytes; the
/// unit test `health_byte_table_equals_is_zero_width` pins the two together
/// over every scalar value.
#[must_use]
pub fn is_zero_width(character: char) -> bool {
    matches!(
        character,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
    )
}

/// Count the non-breaking spaces (U+00A0) and zero-width characters
/// ([`is_zero_width`]) of one slice.
///
/// Every counted character starts with `0xC2` (U+00A0 is `C2 A0`), `0xE2`
/// (U+200B/C/D are `E2 80 8B/8C/8D`, U+2060 is `E2 81 A0`), or `0xEF` (U+FEFF
/// is `EF BB BF`). Those bytes are never continuation bytes, so `memchr3`
/// finds exactly the candidate scalars and the next one or two bytes confirm
/// them. `slice` is a `&str`, so a candidate's continuation bytes are inside
/// it and no state carries between slices.
#[must_use]
pub fn count_health_characters(slice: &str) -> (usize, usize) {
    let bytes = slice.as_bytes();
    let mut nbsp_count = 0usize;
    let mut zero_width_count = 0usize;
    for at in memchr::memchr3_iter(0xC2, 0xE2, 0xEF, bytes) {
        let second = bytes.get(at + 1).copied();
        let third = bytes.get(at + 2).copied();
        match (bytes[at], second, third) {
            (0xC2, Some(0xA0), _) => nbsp_count += 1,
            (0xE2, Some(0x80), Some(0x8B..=0x8D))
            | (0xE2, Some(0x81), Some(0xA0))
            | (0xEF, Some(0xBB), Some(0xBF)) => zero_width_count += 1,
            _ => {}
        }
    }
    (nbsp_count, zero_width_count)
}

/// Strip spaces and tabs immediately before each `\n` or `\r` and at the end
/// of the text, borrowing the text when nothing is stripped.
///
/// Walks the line breaks with `memchr2`. Until the first line that loses
/// bytes nothing is allocated; from there the kept part of each line and its
/// break are appended to one `String`.
#[must_use]
pub fn trim_trailing_space_and_tabs(text: &str) -> Cow<'_, str> {
    let mut trimmed: Option<String> = None;
    let mut line_start = 0usize;
    for line_end in memchr::memchr2_iter(b'\n', b'\r', text.as_bytes()) {
        keep_trimmed_line(text, line_start, line_end, line_end + 1, &mut trimmed);
        line_start = line_end + 1;
    }
    keep_trimmed_line(text, line_start, text.len(), text.len(), &mut trimmed);
    trimmed.map_or(Cow::Borrowed(text), Cow::Owned)
}

/// Keep `text[line_start..line_end]` without its trailing spaces and tabs,
/// then the break `text[line_end..break_end]`, starting the owned copy at the
/// first line that loses bytes.
fn keep_trimmed_line(
    text: &str,
    line_start: usize,
    line_end: usize,
    break_end: usize,
    trimmed: &mut Option<String>,
) {
    let kept = text[line_start..line_end].trim_end_matches([' ', '\t']);
    let kept_end = line_start + kept.len();
    if kept_end != line_end && trimmed.is_none() {
        let mut owned = String::with_capacity(text.len());
        owned.push_str(&text[..line_start]);
        *trimmed = Some(owned);
    }
    if let Some(owned) = trimmed {
        owned.push_str(kept);
        owned.push_str(&text[line_end..break_end]);
    }
}

/// Normalize every line break (`\r\n`, `\r`, or `\n`) to `line_ending`,
/// borrowing the text when it is already in that form; `None` for
/// [`LineEnding::Mixed`], which has no separator.
///
/// Copies the runs between breaks with `push_str`; a `\r\n` is one break.
#[must_use]
pub fn normalize_line_endings(text: &str, line_ending: LineEnding) -> Option<Cow<'_, str>> {
    let separator = line_ending.separator()?;
    if line_endings_already_normalized(text, line_ending) {
        return Some(Cow::Borrowed(text));
    }
    let bytes = text.as_bytes();
    let mut normalized = String::with_capacity(text.len() + text.len() / 16);
    let mut run_start = 0usize;
    let mut crlf_lf_at = None;
    for at in memchr::memchr2_iter(b'\n', b'\r', bytes) {
        if crlf_lf_at == Some(at) && bytes[at] == b'\n' {
            // The `\n` of a `\r\n` already written as one separator.
            run_start = at + 1;
            continue;
        }
        normalized.push_str(&text[run_start..at]);
        normalized.push_str(separator);
        crlf_lf_at = (bytes[at] == b'\r').then_some(at + 1);
        run_start = at + 1;
    }
    normalized.push_str(&text[run_start..]);
    Some(Cow::Owned(normalized))
}

/// Whether `text` is already in `line_ending` form.
///
/// `Lf` and `Cr` are one `str::contains` (itself a `memchr` search). `Crlf`
/// walks the `\n`/`\r` positions: every `\r` must be followed by `\n`, and
/// every `\n` must be the one just after a `\r`.
#[must_use]
pub fn line_endings_already_normalized(text: &str, line_ending: LineEnding) -> bool {
    match line_ending {
        LineEnding::Lf => !text.contains('\r'),
        LineEnding::Cr => !text.contains('\n'),
        LineEnding::Crlf => {
            let bytes = text.as_bytes();
            let mut crlf_lf_at = None;
            for at in memchr::memchr2_iter(b'\n', b'\r', bytes) {
                if bytes[at] == b'\r' {
                    if bytes.get(at + 1) != Some(&b'\n') {
                        return false;
                    }
                    crlf_lf_at = Some(at + 1);
                } else if crlf_lf_at != Some(at) {
                    return false;
                }
            }
            true
        }
        LineEnding::Mixed => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every scan equals its reference on `text`, taken whole.
    fn assert_scans_match_reference(text: &str) {
        reference::assert_scans_match(text, &[text]);
    }

    #[test]
    fn health_byte_table_equals_is_zero_width() {
        let mut buffer = [0u8; 4];
        for character in (0..=u32::from(char::MAX)).filter_map(char::from_u32) {
            let encoded = character.encode_utf8(&mut buffer);
            let expected = (
                usize::from(character == '\u{00A0}'),
                usize::from(is_zero_width(character)),
            );
            assert_eq!(
                count_health_characters(encoded),
                expected,
                "U+{:04X}",
                u32::from(character)
            );
        }
    }

    /// H1 edge cases: look-alike leads (`C2 A9`, `E2 80 99`, `E2 80 94`,
    /// `EF BB BE`), hits at the slice ends, and typographic text.
    #[test]
    fn health_counts_hits_and_ignores_look_alike_leads() {
        assert_eq!(
            count_health_characters("\u{00A9}\u{2019}\u{2014}\u{FEFE}"),
            (0, 0)
        );
        assert_eq!(
            count_health_characters("\u{2028}\u{2029}\u{200A}\u{2061}"),
            (0, 0)
        );
        assert_eq!(count_health_characters("\u{00A0}"), (1, 0));
        assert_eq!(count_health_characters("\u{FEFF}"), (0, 1));
        assert_eq!(
            count_health_characters("\u{200B}a\u{200C}\u{200D}\u{2060}\u{FEFF}\u{00A0}"),
            (1, 5)
        );
        assert_eq!(
            count_health_characters("“quoted” — it’s… \u{200B} and \u{2060}"),
            (0, 2)
        );
        for text in [
            "\u{00A9}\u{2019}\u{2014}\u{FEFE}",
            "\u{00A0}x\u{00A0}",
            "\u{2060}",
            "a\u{FEFF}",
            "日本語😀\u{200D}",
        ] {
            assert_scans_match_reference(text);
        }
    }

    /// Every split of a text on scalar boundaries sums to the whole count.
    #[test]
    fn health_counts_are_additive_over_scalar_boundary_cuts() {
        let text = "a\u{00A0}\u{200B}é\u{2019}\u{FEFF}\r\n\u{2060}😀\u{00A9}";
        let whole = reference::count_health_characters(text);
        for cut in (0..=text.len()).filter(|&cut| text.is_char_boundary(cut)) {
            let (head, tail) = text.split_at(cut);
            let (head_nbsp, head_zero) = count_health_characters(head);
            let (tail_nbsp, tail_zero) = count_health_characters(tail);
            assert_eq!((head_nbsp + tail_nbsp, head_zero + tail_zero), whole);
        }
    }

    /// H4 edge cases: carriage-return runs, breaks at both ends, and trailing
    /// blanks before every break at offsets around a 64-byte block.
    #[test]
    fn save_rewrites_match_reference_on_edge_cases() {
        let mut cases: Vec<String> = [
            "",
            " ",
            "\t",
            "\r",
            "\n",
            "\r\n",
            " \r",
            "a \r\n",
            "a  ",
            " a",
            "\r\r",
            "\n\n",
            "\n\r",
            "\r\r\n",
            "a\r\r\nb\r",
            "x\n\ry \t",
            "é \t\r\n日 ",
            "\u{00A0} \n",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        for offset in 60..70 {
            for tail in ["\r\n", " \r\n", "\t\r", " \n", "\r", " "] {
                cases.push(format!("{}{tail}y \t", "x".repeat(offset)));
                cases.push(format!("{}  {tail}", "x".repeat(offset - 2)));
            }
        }
        for case in &cases {
            assert_scans_match_reference(case);
        }
    }

    #[test]
    fn carriage_return_runs_normalize_like_the_reference() {
        let text = "a\r\r\nb\n\rc\r";
        assert_eq!(
            normalize_line_endings(text, LineEnding::Lf).as_deref(),
            Some("a\n\nb\n\nc\n")
        );
        assert_eq!(
            normalize_line_endings(text, LineEnding::Crlf).as_deref(),
            Some("a\r\n\r\nb\r\n\r\nc\r\n")
        );
        assert_eq!(
            normalize_line_endings(text, LineEnding::Cr).as_deref(),
            Some("a\r\rb\r\rc\r")
        );
        assert_eq!(normalize_line_endings(text, LineEnding::Mixed), None);
    }

    #[test]
    fn trim_strips_blanks_before_every_break_and_the_end() {
        assert_eq!(
            trim_trailing_space_and_tabs("a \t\r\nb  \rc\t\nd \u{00A0} "),
            "a\r\nb\rc\nd \u{00A0}"
        );
    }

    #[test]
    fn clean_text_is_returned_borrowed() {
        let text = "fn main() {\r\n    body();\n}\rlast";
        let trimmed = trim_trailing_space_and_tabs(text);
        assert!(matches!(trimmed, Cow::Borrowed(_)));
        assert!(std::ptr::eq(trimmed.as_ref(), text));

        let lf = "a\nb\n";
        let normalized = normalize_line_endings(lf, LineEnding::Lf).expect("LF has a separator");
        assert!(matches!(normalized, Cow::Borrowed(_)));
        assert!(std::ptr::eq(normalized.as_ref(), lf));
    }

    #[test]
    fn crlf_check_rejects_every_lone_break() {
        assert!(line_endings_already_normalized(
            "a\r\nb\r\n",
            LineEnding::Crlf
        ));
        assert!(line_endings_already_normalized("", LineEnding::Crlf));
        for text in ["\n", "\r", "a\r\n\n", "a\r\r\n", "\n\r\n", "a\r"] {
            assert!(
                !line_endings_already_normalized(text, LineEnding::Crlf),
                "{text:?}"
            );
        }
    }
}
