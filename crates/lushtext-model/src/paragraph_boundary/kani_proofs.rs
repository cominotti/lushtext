// SPDX-License-Identifier: GPL-3.0-or-later

//! Kani harnesses over the GTK paragraph-boundary functions and the paragraph
//! profile layers.
//!
//! **Boundary harnesses** (`boundary_` prefix, shard `core-document-scale`):
//! symbolic bytes of length at most [`BOUNDARY_BYTES`] (8) with
//! `std::str::from_utf8` assumed to succeed, so every valid UTF-8 string of at
//! most 8 bytes; a symbolic start on a scalar boundary and a symbolic budget.
//! Eight bytes hold every ordered pair of delimiters, a 4-byte scalar
//! straddling a budget edge, and a CRLF beside a U+2029; a longer string only
//! repeats these local situations, which the Pango property test
//! (`tests/properties/paragraph_boundary.rs`) samples on long strings.
//!
//! **Profile harnesses** (`profile_` prefix, shard `core-paragraph-profile`):
//! strings of at most [`PROFILE_SYMBOLS`] symbols and [`PROFILE_BYTES`] (6) bytes
//! (the design's 8 bytes did not fit: a `String`-built text and growing `Vec`s exhausted the 16 GiB lane cap, and with fixed buffers 6 bytes peak at 7 GiB, so 8 would break the shard's 12 GiB budget) over the
//! alphabet {`a`, `é`, `\r`, `\n`, U+2028, U+2029, U+0085} (symbol indices
//! mapped to their UTF-8), cut into at most 3 slices at any scalar boundaries
//! (empty slices included), with a symbolic [`ProfileConfig`] whose thresholds
//! are at most 8, offset cap at most 2 and `r_batch` from 1 to 3.
//!
//! Both groups compare against the **reference rule**, written here as a
//! per-position predicate rather than a scan: position `i` ends a paragraph at
//! `\n`, at U+2029, or at a `\r` not followed by `\n`; `\r\n` is one delimiter
//! ending after its `\n`; U+2028 and U+0085 never end one.
//!
//! Compiled only under `cfg(kani)` (`make kani`).

use super::{
    PROFILE_THRESHOLD_COUNT, ParagraphDelimiter, ParagraphProfileAccumulator, ProfileConfig,
    ProfileThresholds, next_paragraph_break, paragraph_chars_from_line, slice_end_after_paragraph,
};

/// Longest string the boundary harnesses consider.
const BOUNDARY_BYTES: usize = 8;

/// Longest string, in symbols, the profile harnesses consider.
const PROFILE_SYMBOLS: usize = 6;

/// Longest string, in bytes, the profile harnesses consider.
const PROFILE_BYTES: usize = 6;

/// The reference rule: the end (exclusive) of the delimiter that ends a
/// paragraph at byte `i`, if a delimiter starts there. `i` must be a scalar
/// boundary of `bytes`.
fn reference_delimiter_end(bytes: &[u8], i: usize) -> Option<usize> {
    let at = |k: usize| bytes.get(k).copied();
    match at(i) {
        Some(b'\n') => {
            // A `\n` right after `\r` belongs to that CRLF, not a delimiter of
            // its own.
            if i > 0 && bytes[i - 1] == b'\r' {
                None
            } else {
                Some(i + 1)
            }
        }
        Some(b'\r') => Some(if at(i + 1) == Some(b'\n') {
            i + 2
        } else {
            i + 1
        }),
        Some(0xE2) if at(i + 1) == Some(0x80) && at(i + 2) == Some(0xA9) => Some(i + 3),
        _ => None,
    }
}

/// Scalars in `bytes[from..to]` (UTF-8 lead bytes), a per-byte loop: the
/// library's `chars().count()` is an unrolled chunked count that CBMC cannot
/// unwind within the harness bound.
fn scalars(bytes: &[u8], from: usize, to: usize) -> u64 {
    let mut count = 0;
    let mut i = from;
    while i < to {
        if bytes[i] & 0xC0 != 0x80 {
            count += 1;
        }
        i += 1;
    }
    count
}

/// Symbolic bytes of length at most [`BOUNDARY_BYTES`] that are valid UTF-8.
fn any_text() -> ([u8; BOUNDARY_BYTES], usize) {
    let bytes: [u8; BOUNDARY_BYTES] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= BOUNDARY_BYTES);
    kani::assume(std::str::from_utf8(&bytes[..len]).is_ok());
    (bytes, len)
}

/// A symbolic scalar-boundary offset of `text` (the end included).
fn any_boundary(text: &str) -> usize {
    let offset: usize = kani::any();
    kani::assume(offset <= text.len());
    kani::assume(text.is_char_boundary(offset));
    offset
}

/// A symbolic slice start: a scalar boundary that does not split a CRLF pair.
/// Every production start is the text start or a previous slice end, which
/// is just after a delimiter and so never between `\r` and `\n`.
fn any_slice_start(text: &str) -> usize {
    let start = any_boundary(text);
    let bytes = text.as_bytes();
    kani::assume(
        !(start > 0 && start < bytes.len() && bytes[start - 1] == b'\r' && bytes[start] == b'\n'),
    );
    start
}

/// The reference slice choice of design D2, from the reference rule's breaks.
fn reference_slice_end(text: &str, start: usize, budget: usize) -> usize {
    let bytes = text.as_bytes();
    if start >= text.len() {
        return text.len();
    }
    let mut budget_end = start.saturating_add(budget).min(text.len());
    while budget_end > start && !text.is_char_boundary(budget_end) {
        budget_end -= 1;
    }
    if budget_end == text.len() {
        return text.len();
    }
    let mut last_within = None;
    let mut first_beyond = None;
    let mut i = start;
    while i < text.len() {
        if text.is_char_boundary(i)
            && let Some(end) = reference_delimiter_end(bytes, i)
        {
            if end <= budget_end {
                last_within = Some(end);
            } else if first_beyond.is_none() {
                first_beyond = Some(end);
            }
        }
        i += 1;
    }
    last_within.or(first_beyond).unwrap_or(text.len())
}

#[kani::proof]
#[kani::unwind(10)]
fn boundary_break_is_a_scalar_boundary_after_a_delimiter() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    let from = any_boundary(text);
    if let Some(found) = next_paragraph_break(text, from) {
        assert!(from <= found.content_end && found.content_end < found.next_start);
        assert!(found.next_start <= text.len());
        assert!(text.is_char_boundary(found.content_end));
        assert!(text.is_char_boundary(found.next_start));
        let delimiter = &text[found.content_end..found.next_start];
        let expected = match found.delimiter {
            ParagraphDelimiter::Lf => "\n",
            ParagraphDelimiter::Cr => "\r",
            ParagraphDelimiter::CrLf => "\r\n",
            ParagraphDelimiter::ParagraphSeparator => "\u{2029}",
        };
        assert!(delimiter == expected);
        // A lone `\r` is never followed by `\n`.
        if found.delimiter == ParagraphDelimiter::Cr {
            assert!(bytes.get(found.next_start).copied() != Some(b'\n') || found.next_start == len);
        }
        kani::cover!(found.delimiter == ParagraphDelimiter::Lf, "break at \\n");
        kani::cover!(found.delimiter == ParagraphDelimiter::Cr, "break at \\r");
        kani::cover!(
            found.delimiter == ParagraphDelimiter::CrLf,
            "break at \\r\\n"
        );
        kani::cover!(
            found.delimiter == ParagraphDelimiter::ParagraphSeparator,
            "break at U+2029"
        );
    } else {
        kani::cover!(len > 0 && from == 0, "text with no delimiter");
    }
}

#[kani::proof]
#[kani::unwind(10)]
fn boundary_slice_end_is_safe_and_advances() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    let start = any_slice_start(text);
    let budget: usize = kani::any();
    let end = slice_end_after_paragraph(text, start, budget);
    assert!(end <= text.len());
    assert!(text.is_char_boundary(end));
    if start < text.len() {
        assert!(end > start);
    }
    // At the text end or just after a delimiter.
    if end < text.len() {
        let last = bytes[end - 1];
        let after_separator = end >= 3 && bytes[end - 3..end] == [0xE2, 0x80, 0xA9];
        assert!(last == b'\n' || last == b'\r' || after_separator);
        // Never between `\r` and `\n`.
        assert!(!(last == b'\r' && bytes[end] == b'\n'));
    }
    let budget_end = start.saturating_add(budget);
    kani::cover!(
        budget_end < len
            && budget_end > 0
            && bytes[budget_end - 1] == b'\r'
            && bytes[budget_end] == b'\n',
        "budget ends between \\r and \\n"
    );
    kani::cover!(
        budget_end < len && !text.is_char_boundary(budget_end),
        "budget ends inside a multibyte scalar"
    );
}

#[kani::proof]
#[kani::unwind(10)]
fn boundary_slice_end_is_the_reference_choice() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    let start = any_slice_start(text);
    let budget: usize = kani::any();
    let end = slice_end_after_paragraph(text, start, budget);
    assert!(end == reference_slice_end(text, start, budget));
    kani::cover!(
        end < len && end <= start.saturating_add(budget),
        "slice ends at the last break within the budget"
    );
    kani::cover!(
        end < len && end > start.saturating_add(budget),
        "slice ends at the first break beyond the budget"
    );
}

#[kani::proof]
#[kani::unwind(10)]
fn boundary_agrees_with_the_reference_rule() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    // Walk the production breaks and the reference positions together: every
    // reference delimiter is found in order, and nothing else is.
    let mut cursor = 0;
    let mut i = 0;
    while i < len {
        if text.is_char_boundary(i)
            && let Some(end) = reference_delimiter_end(&bytes[..len], i)
        {
            let found = next_paragraph_break(text, cursor);
            assert!(found.is_some());
            let found = found.unwrap();
            assert!(found.content_end == i && found.next_start == end);
            cursor = found.next_start;
        }
        i += 1;
    }
    assert!(next_paragraph_break(text, cursor).is_none());
    kani::cover!(
        len >= 3 && bytes[..3] == [0xE2, 0x80, 0xA8],
        "U+2028 is not a delimiter"
    );
    kani::cover!(
        len >= 2 && bytes[..2] == [0xC2, 0x85],
        "U+0085 is not a delimiter"
    );
}

#[kani::proof]
#[kani::unwind(10)]
fn boundary_line_length_excludes_its_delimiter() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    let from = any_boundary(text);
    // A GTK line starting at `from`: its content runs to the next break, and
    // `chars_in_line` counts the delimiter too.
    let (content_end, line_end, ending) = match next_paragraph_break(text, from) {
        Some(found) => (found.content_end, found.next_start, Some(found.delimiter)),
        None => (len, len, None),
    };
    let content_chars = scalars(&bytes, from, content_end);
    let line_chars = scalars(&bytes, from, line_end);
    assert!(paragraph_chars_from_line(line_chars, ending) == content_chars);
    // Never underflows, whatever GTK reports.
    let arbitrary: u64 = kani::any();
    let delimiter: Option<ParagraphDelimiter> = match kani::any::<u8>() % 5 {
        0 => None,
        1 => Some(ParagraphDelimiter::Lf),
        2 => Some(ParagraphDelimiter::Cr),
        3 => Some(ParagraphDelimiter::CrLf),
        _ => Some(ParagraphDelimiter::ParagraphSeparator),
    };
    assert!(paragraph_chars_from_line(arbitrary, delimiter) <= arbitrary);
    kani::cover!(ending == Some(ParagraphDelimiter::CrLf), "CRLF line");
    kani::cover!(ending.is_none() && from < len, "last line");
}

/// The retired `'\n'`-only slicer, transcribed from
/// `model/buffer_replacement.rs::next_replacement_boundary` at commit 1820f14a
/// with its `REPLACEMENT_INSERT_SLICE_BYTES` constant replaced by `budget`.
/// With the 256 KiB constant every string of at most 8 bytes fits the budget
/// and the comparison would be vacuous. It is not production code.
///
/// The retired body's `str::rfind('\n')` and `str::find('\n')` are written
/// as byte loops: `'\n'` is one ASCII byte that never occurs inside a
/// multibyte scalar, so the byte search returns the same index, and CBMC
/// verifies the loops in minutes where the library searchers took 26.
fn retired_newline_only_slicer(text: &str, start: usize, budget: usize) -> usize {
    if start >= text.len() {
        return text.len();
    }
    let mut budget_end = start.saturating_add(budget).min(text.len());
    while budget_end > start && !text.is_char_boundary(budget_end) {
        budget_end -= 1;
    }
    if budget_end == text.len() {
        return text.len();
    }
    let bytes = text.as_bytes();
    // `text[start..budget_end].rfind('\n')`
    let mut index = budget_end;
    while index > start {
        index -= 1;
        if bytes[index] == b'\n' {
            return index.saturating_add(1);
        }
    }
    // `text[budget_end..].find('\n')`
    let mut index = budget_end;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            return index.saturating_add(1);
        }
        index += 1;
    }
    text.len()
}

/// The retired slicer violates the reference slice choice: Kani finds a
/// CR-only or U+2029 text whose retired slice is the text end (the one-turn
/// install this change removes). It satisfies the four safety guarantees, so
/// the blocker is pinned through the choice property.
#[kani::proof]
#[kani::unwind(10)]
#[kani::should_panic]
fn boundary_newline_only_slicer_disagrees() {
    let (bytes, len) = any_text();
    let text = std::str::from_utf8(&bytes[..len]).unwrap();
    let start = any_slice_start(text);
    let budget: usize = kani::any();
    assert!(
        retired_newline_only_slicer(text, start, budget)
            == reference_slice_end(text, start, budget)
    );
}

/// Element-wise equality: slice `==` lowers to a byte `memcmp` whose loop
/// outruns the harness unwind bound.
fn same_u64s(left: &[u64], right: &[u64]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut i = 0;
    while i < left.len() {
        if left[i] != right[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The profile alphabet {`a`, `é`, `\r`, `\n`, U+2028, U+2029, U+0085} as
/// fixed UTF-8 rows (length, bytes).
const ALPHABET_BYTES: [(usize, [u8; 3]); 7] = [
    (1, [b'a', 0, 0]),
    (2, [0xC3, 0xA9, 0]),
    (1, [b'\r', 0, 0]),
    (1, [b'\n', 0, 0]),
    (3, [0xE2, 0x80, 0xA8]),
    (3, [0xE2, 0x80, 0xA9]),
    (2, [0xC2, 0x85, 0]),
];

/// A symbolic profile text: at most [`PROFILE_SYMBOLS`] symbols and [`PROFILE_BYTES`] bytes
/// over {`a`, `é`, `\r`, `\n`, U+2028, U+2029, U+0085}, written into a fixed buffer (a `String` grown by
/// `push_str` made CBMC model every reallocation and exhausted the 16 GiB
/// lane cap), so no symbolic `from_utf8` or allocation is needed.
struct ProfileText {
    bytes: [u8; PROFILE_BYTES],
    len: usize,
}

impl ProfileText {
    fn as_str(&self) -> &str {
        // SAFETY: the bytes are a concatenation of whole UTF-8 encodings
        // from `ALPHABET_BYTES`, so they are valid UTF-8 by construction.
        unsafe { std::str::from_utf8_unchecked(&self.bytes[..self.len]) }
    }
}

fn any_profile_text() -> ProfileText {
    let count: usize = kani::any();
    kani::assume(count <= PROFILE_SYMBOLS);
    let mut bytes = [0_u8; PROFILE_BYTES];
    let mut len = 0;
    let mut i = 0;
    while i < count {
        let symbol: usize = kani::any();
        kani::assume(symbol < ALPHABET_BYTES.len());
        let (width, encoded) = ALPHABET_BYTES[symbol];
        kani::assume(len + width <= PROFILE_BYTES);
        let mut k = 0;
        while k < width {
            bytes[len + k] = encoded[k];
            k += 1;
        }
        len += width;
        i += 1;
    }
    ProfileText { bytes, len }
}

/// A symbolic configuration in the harness domain.
fn any_config() -> ProfileConfig {
    let mut thresholds = [0_u64; PROFILE_THRESHOLD_COUNT];
    let mut i = 0;
    while i < PROFILE_THRESHOLD_COUNT {
        thresholds[i] = kani::any();
        kani::assume(thresholds[i] <= 8);
        i += 1;
    }
    let offset_cap: usize = kani::any();
    kani::assume(offset_cap <= 2);
    let r_batch: usize = kani::any();
    kani::assume((1..=3).contains(&r_batch));
    ProfileConfig {
        thresholds: ProfileThresholds(thresholds),
        offset_cap,
        r_batch,
        v_batch: kani::any(),
    }
}

/// Two symbolic cuts on scalar boundaries, `0 ≤ first ≤ second ≤ len`.
fn any_cuts(text: &str) -> (usize, usize) {
    let first = any_boundary(text);
    let second = any_boundary(text);
    kani::assume(first <= second);
    (first, second)
}

fn profile_whole(text: &str, config: ProfileConfig) -> super::DocumentProfile {
    let mut accumulator = ParagraphProfileAccumulator::new(config);
    accumulator.push_str(text);
    accumulator.finish()
}

#[kani::proof]
#[kani::unwind(10)]
fn profile_is_independent_of_slicing() {
    let owned = any_profile_text();
    let text = owned.as_str();
    let config = any_config();
    let (first, second) = any_cuts(text);
    let mut sliced = ParagraphProfileAccumulator::new(config);
    sliced.push_str(&text[..first]);
    sliced.push_str(&text[first..second]);
    sliced.push_str(&text[second..]);
    let sliced = sliced.finish();
    let whole = profile_whole(text, config);
    assert!(sliced.total_bytes == whole.total_bytes);
    assert!(sliced.paragraph_count == whole.paragraph_count);
    assert!(sliced.longest_paragraph_chars == whole.longest_paragraph_chars);
    assert!(same_u64s(
        &sliced.long_paragraph_counts,
        &whole.long_paragraph_counts
    ));
    assert!(same_u64s(
        &sliced.long_paragraph_offsets,
        &whole.long_paragraph_offsets
    ));
    assert!(sliced.offsets_truncated == whole.offsets_truncated);
    assert!(sliced.worst_batch_chars == whole.worst_batch_chars);
    let bytes = text.as_bytes();
    let cut_in_crlf =
        |cut: usize| cut > 0 && cut < bytes.len() && bytes[cut - 1] == b'\r' && bytes[cut] == b'\n';
    kani::cover!(
        cut_in_crlf(first) || cut_in_crlf(second),
        "CRLF split across a cut"
    );
    kani::cover!(
        second + 3 <= bytes.len() && bytes[second..second + 3] == [0xE2, 0x80, 0xA9],
        "cut immediately before U+2029"
    );
    kani::cover!(
        first >= 3 && bytes[first - 3..first] == [0xE2, 0x80, 0xA9],
        "cut immediately after U+2029"
    );
    kani::cover!(first == second && !text.is_empty(), "an empty slice");
}

#[kani::proof]
#[kani::unwind(10)]
fn profile_matches_the_specification() {
    let owned = any_profile_text();
    let text = owned.as_str();
    let config = any_config();
    let whole = profile_whole(text, config);
    // The specification, from the reference rule's paragraph lengths.
    let bytes = text.as_bytes();
    let mut lengths = [0_u64; PROFILE_SYMBOLS + 1];
    let mut starts = [0_u64; PROFILE_SYMBOLS + 1];
    let mut paragraphs = 0;
    let mut open_chars = 0_u64;
    let mut position = 0_u64;
    let mut open_start = 0_u64;
    let mut i = 0;
    while i < bytes.len() {
        if text.is_char_boundary(i) {
            if let Some(end) = reference_delimiter_end(bytes, i) {
                lengths[paragraphs] = open_chars;
                starts[paragraphs] = open_start;
                paragraphs += 1;
                position += scalars(bytes, i, end);
                open_start = position;
                open_chars = 0;
            } else if !(bytes[i] == b'\n' && i > 0 && bytes[i - 1] == b'\r') {
                open_chars += 1;
                position += 1;
            }
        }
        i += 1;
    }
    lengths[paragraphs] = open_chars;
    starts[paragraphs] = open_start;
    paragraphs += 1;

    assert!(whole.total_bytes == bytes.len() as u64);
    assert!(whole.paragraph_count == paragraphs as u64);
    let mut longest = 0;
    let mut counts = [0_u64; PROFILE_THRESHOLD_COUNT];
    let mut long_seen = 0;
    let window = config.r_batch;
    let mut worst = 0;
    let mut p = 0;
    while p < paragraphs {
        if lengths[p] > longest {
            longest = lengths[p];
        }
        let mut t = 0;
        while t < PROFILE_THRESHOLD_COUNT {
            if lengths[p] > config.thresholds.0[t] {
                counts[t] += 1;
            }
            t += 1;
        }
        if lengths[p] > config.thresholds.0[0] {
            if long_seen < config.offset_cap {
                assert!(whole.long_paragraph_offsets[long_seen] == starts[p]);
            }
            long_seen += 1;
        }
        // The window of `min(window, paragraphs)` paragraphs ending at `p`
        // (partial windows at the start never exceed a full one).
        let mut sum = 0;
        let mut k = 0;
        while k < window && k <= p {
            sum += lengths[p - k];
            k += 1;
        }
        if sum > worst {
            worst = sum;
        }
        p += 1;
    }
    assert!(whole.longest_paragraph_chars == longest);
    assert!(same_u64s(&whole.long_paragraph_counts, &counts));
    assert!(whole.long_paragraph_offsets.len() == long_seen.min(config.offset_cap));
    assert!(whole.offsets_truncated == (long_seen > config.offset_cap));
    assert!(whole.worst_batch_chars == worst);
    kani::cover!(whole.offsets_truncated, "offset truncation");
    kani::cover!(paragraphs < window, "fewer paragraphs than r_batch");
}

/// Profiling each slice with a fresh accumulator and combining the slice
/// profiles componentwise is wrong: Kani finds a CRLF pair or a paragraph
/// split across a cut.
#[kani::proof]
#[kani::unwind(10)]
#[kani::should_panic]
fn profile_slices_without_carry_disagree() {
    // An existence claim, so one text suffices: "aa" cut in its middle splits
    // a paragraph. A symbolic text exhausted the 16 GiB lane cap (four
    // accumulators over symbolic slices); the cuts stay symbolic.
    let text = "aa";
    let config = ProfileConfig {
        thresholds: ProfileThresholds([0; PROFILE_THRESHOLD_COUNT]),
        offset_cap: 0,
        r_batch: 1,
        v_batch: 0,
    };
    let (first, second) = any_cuts(text);
    let parts = [
        profile_whole(&text[..first], config),
        profile_whole(&text[first..second], config),
        profile_whole(&text[second..], config),
    ];
    let whole = profile_whole(text, config);
    let mut longest = 0;
    let mut worst = 0;
    let mut counts = [0_u64; PROFILE_THRESHOLD_COUNT];
    for part in &parts {
        longest = longest.max(part.longest_paragraph_chars);
        worst = worst.max(part.worst_batch_chars);
        let mut t = 0;
        while t < PROFILE_THRESHOLD_COUNT {
            counts[t] += part.long_paragraph_counts[t];
            t += 1;
        }
    }
    assert!(longest == whole.longest_paragraph_chars);
    assert!(worst == whole.worst_batch_chars);
    assert!(same_u64s(&counts, &whole.long_paragraph_counts));
}
