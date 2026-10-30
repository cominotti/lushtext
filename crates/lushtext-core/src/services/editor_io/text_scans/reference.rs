// SPDX-License-Identifier: GPL-3.0-or-later

//! The scalar scans the accelerated ones in [`super`] replaced, kept verbatim
//! as their oracle.
//!
//! Compiled only for tests, `test-utils` (benches and property tests), and the
//! `fuzzing` feature, so production cannot call them; the property tests and
//! the `editor_bytes` fuzz target compare every accelerated scan with these.
//! `.cargo/mutants.toml` excludes this file: an oracle is not code under test.

use std::borrow::Cow;

use crate::model::encoding::LineEnding;

/// Count the non-breaking spaces and zero-width characters of one slice, one
/// character at a time.
#[must_use]
pub fn count_health_characters(slice: &str) -> (usize, usize) {
    let mut nbsp_count = 0usize;
    let mut zero_width_count = 0usize;
    for character in slice.chars() {
        if character == '\u{00A0}' {
            nbsp_count += 1;
        } else if super::is_zero_width(character) {
            zero_width_count += 1;
        }
    }
    (nbsp_count, zero_width_count)
}

/// Normalize to `line_ending` one character at a time (`None` for `Mixed`).
#[must_use]
pub fn normalize_line_endings(text: &str, line_ending: LineEnding) -> Option<Cow<'_, str>> {
    let separator = line_ending.separator()?;
    if line_endings_already_normalized(text, line_ending) {
        return Some(Cow::Borrowed(text));
    }

    let mut normalized = String::with_capacity(text.len() + text.len() / 16);
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\r' {
            let _ = chars.next_if_eq(&'\n');
            normalized.push_str(separator);
            continue;
        }
        if character == '\n' {
            normalized.push_str(separator);
        } else {
            normalized.push(character);
        }
    }
    Some(Cow::Owned(normalized))
}

/// Whether `text` is already in `line_ending` form, one byte at a time.
#[must_use]
pub fn line_endings_already_normalized(text: &str, line_ending: LineEnding) -> bool {
    match line_ending {
        LineEnding::Lf => !text.contains('\r'),
        LineEnding::Cr => !text.contains('\n'),
        LineEnding::Crlf => {
            let bytes = text.as_bytes();
            let mut index = 0usize;
            while index < bytes.len() {
                match bytes[index] {
                    b'\r' if bytes.get(index + 1) == Some(&b'\n') => index += 2,
                    b'\r' | b'\n' => return false,
                    _ => index += 1,
                }
            }
            true
        }
        LineEnding::Mixed => false,
    }
}

/// Strip spaces and tabs before line endings and at end of text, one
/// character at a time, always copying.
#[must_use]
pub fn trim_trailing_space_and_tabs(text: &str) -> String {
    let mut trimmed = String::with_capacity(text.len());
    let mut pending_spaces = String::new();

    for character in text.chars() {
        if matches!(character, ' ' | '\t') {
            pending_spaces.push(character);
            continue;
        }

        if matches!(character, '\n' | '\r') {
            pending_spaces.clear();
            trimmed.push(character);
            continue;
        }

        trimmed.push_str(&pending_spaces);
        pending_spaces.clear();
        trimmed.push(character);
    }

    trimmed
}

/// Every line-ending mode, `Mixed` (which has no separator) included.
pub const ENDINGS: [LineEnding; 4] = [
    LineEnding::Lf,
    LineEnding::Crlf,
    LineEnding::Cr,
    LineEnding::Mixed,
];

/// Panic unless every accelerated scan of [`super`] equals its reference on
/// `text`: the health count summed over `pieces` (consecutive scalar-boundary
/// parts of `text`), the trim, and the already-normalized check and the
/// normalization in every line-ending mode. Shared by the unit tests and the
/// `editor_bytes` fuzz target.
///
/// # Panics
///
/// On the first scan that differs from its reference: that is its purpose.
pub fn assert_scans_match(text: &str, pieces: &[&str]) {
    let (mut nbsp, mut zero_width) = (0, 0);
    for piece in pieces {
        let (piece_nbsp, piece_zero_width) = super::count_health_characters(piece);
        nbsp += piece_nbsp;
        zero_width += piece_zero_width;
    }
    assert_eq!(
        (nbsp, zero_width),
        count_health_characters(text),
        "health {text:?}"
    );
    assert_eq!(
        super::trim_trailing_space_and_tabs(text).as_ref(),
        trim_trailing_space_and_tabs(text),
        "trim {text:?}"
    );
    for ending in ENDINGS {
        assert_eq!(
            super::line_endings_already_normalized(text, ending),
            line_endings_already_normalized(text, ending),
            "already {ending:?} {text:?}"
        );
        assert_eq!(
            super::normalize_line_endings(text, ending).as_deref(),
            normalize_line_endings(text, ending).as_deref(),
            "normalize {ending:?} {text:?}"
        );
    }
}
