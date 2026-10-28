// SPDX-License-Identifier: GPL-3.0-or-later

//! The one GTK-matching paragraph-boundary definition, and the paragraph
//! profile built on it.
//!
//! `GtkTextBuffer` splits its lines with `pango_find_paragraph_boundary`, which
//! ends a paragraph at `\n`, at `\r`, at `\r\n` (one delimiter) and at U+2029
//! PARAGRAPH SEPARATOR, and at nothing else: U+2028 LINE SEPARATOR and U+0085
//! NEXT LINE are ordinary characters to it (ledger A42). Every bounded install
//! or replacement slicer, the load-time [`DocumentProfile`], the edit-time
//! paragraph watch (`model/paragraph_profile_watch.rs`) and the minimap's
//! modified-line marks use this module, so they all see the paragraphs GTK
//! lays out.
//!
//! The profile is two layers, so each can be checked on its own:
//! [`ParagraphProfileAccumulator`] turns text into closed paragraph lengths,
//! and [`ParagraphLengthFold`] folds those lengths into the profile. Both read
//! their thresholds, offset cap and batch size from a [`ProfileConfig`] value,
//! never from constants, and production passes [`ProfileConfig::PRODUCTION`].
//!
//! Verification: Kani proves the boundary functions and the profile layers
//! (`paragraph_boundary/kani_proofs.rs`) on bounded inputs: every valid UTF-8
//! string of at most 8 bytes for the boundary functions, and strings of at
//! most 6 bytes over `a`, `é`, `\r`, `\n`, U+2028, U+2029 and U+0085 cut into
//! at most 3 slices for the profile. The property tests in
//! `crates/lushtext-core/tests/properties/paragraph_boundary.rs` are the
//! evidence beyond that domain: they compare the splits with
//! `pango::find_paragraph_boundary` and the profile's slice independence with
//! the production configuration on long strings.
//!
//! Units are characters (Unicode scalars, which is what `GtkTextIter` counts)
//! and bytes, never pixels.

#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]

#[cfg(kani)]
mod kani_proofs;

use super::document_scale;

/// UTF-8 encoding of U+2029 PARAGRAPH SEPARATOR, the only multibyte delimiter.
const PARAGRAPH_SEPARATOR_UTF8: [u8; 3] = [0xE2, 0x80, 0xA9];

/// One GTK paragraph delimiter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParagraphDelimiter {
    /// `\n` alone.
    Lf,
    /// `\r` not followed by `\n`.
    Cr,
    /// `\r\n`, one delimiter of two characters.
    CrLf,
    /// U+2029 PARAGRAPH SEPARATOR.
    ParagraphSeparator,
}

impl ParagraphDelimiter {
    /// Characters the delimiter occupies in a `GtkTextBuffer` line.
    #[must_use]
    pub const fn chars(self) -> u64 {
        match self {
            Self::CrLf => 2,
            Self::Lf | Self::Cr | Self::ParagraphSeparator => 1,
        }
    }
}

/// Where one paragraph's content ends and the next paragraph starts, as byte
/// offsets into the text that was searched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParagraphBreak {
    /// First byte of the delimiter (the end of the paragraph's content).
    pub content_end: usize,
    /// First byte after the delimiter (the start of the next paragraph).
    pub next_start: usize,
    /// Which delimiter ended the paragraph.
    pub delimiter: ParagraphDelimiter,
}

/// Find the first GTK paragraph break at or after byte `from`.
///
/// `from` must lie on a scalar boundary of `text` (a `from` past the end
/// returns `None`). A `\r` followed by `\n` is one [`ParagraphDelimiter::CrLf`]
/// break; a `\r` at the very end of `text` is a [`ParagraphDelimiter::Cr`]
/// break, because no further text is known. U+2028 and U+0085 are never
/// breaks.
///
/// Kani proves, for every valid UTF-8 string of at most 8 bytes and every
/// scalar-boundary `from`, that the returned offsets are scalar boundaries,
/// that exactly one delimiter lies between them, and that the sequence of
/// breaks equals the reference rule's (`kani_proofs::boundary_*`). The Pango
/// property test (`tests/properties/paragraph_boundary.rs`) is the evidence
/// for longer strings.
#[must_use]
pub fn next_paragraph_break(text: &str, from: usize) -> Option<ParagraphBreak> {
    let bytes = text.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => {
                return Some(ParagraphBreak {
                    content_end: index,
                    next_start: index + 1,
                    delimiter: ParagraphDelimiter::Lf,
                });
            }
            b'\r' => {
                let crlf = bytes.get(index + 1) == Some(&b'\n');
                return Some(ParagraphBreak {
                    content_end: index,
                    next_start: index + if crlf { 2 } else { 1 },
                    delimiter: if crlf {
                        ParagraphDelimiter::CrLf
                    } else {
                        ParagraphDelimiter::Cr
                    },
                });
            }
            0xE2 if bytes.get(index + 1) == Some(&PARAGRAPH_SEPARATOR_UTF8[1])
                && bytes.get(index + 2) == Some(&PARAGRAPH_SEPARATOR_UTF8[2]) =>
            {
                return Some(ParagraphBreak {
                    content_end: index,
                    next_start: index + PARAGRAPH_SEPARATOR_UTF8.len(),
                    delimiter: ParagraphDelimiter::ParagraphSeparator,
                });
            }
            _ => index += 1,
        }
    }
    None
}

/// Choose where one bounded slice of `text` starting at byte `start` ends.
///
/// Let `b` be `start + budget`, floored to a scalar boundary and capped at
/// the text end. If `b` is the text end, the slice takes the rest. Otherwise
/// it ends after the **last** paragraph break that ends at or before `b`, if
/// one ends after `start`; otherwise after the **first** break beyond `b`;
/// otherwise (no further delimiter) at the text end. So a slice ends at the
/// text end or just after a delimiter, never between `\r` and `\n`, never
/// inside a scalar, and always advances; a paragraph longer than the budget
/// is taken whole (GTK lays out whole paragraphs, so splitting one would make
/// every later slice re-lay-out it), and a multi-paragraph remainder is never
/// taken in one slice.
///
/// `start` is the text start or a previous slice end, so it never lies
/// between the `\r` and `\n` of a CRLF pair (a `\n` found at `start` would
/// otherwise be read as a delimiter of its own).
///
/// Kani proves those guarantees and that the result is exactly this choice,
/// for every valid UTF-8 string of at most 8 bytes, every scalar-boundary
/// `start` that does not split a CRLF pair, and every budget (`kani_proofs::boundary_slice_end_*`); the Pango
/// property test is the evidence for longer strings.
#[must_use]
pub fn slice_end_after_paragraph(text: &str, start: usize, budget: usize) -> usize {
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
    let mut cursor = start;
    while let Some(paragraph_break) = next_paragraph_break(text, cursor) {
        if paragraph_break.next_start > budget_end {
            return last_within.unwrap_or(paragraph_break.next_start);
        }
        last_within = Some(paragraph_break.next_start);
        cursor = paragraph_break.next_start;
    }
    last_within.unwrap_or(text.len())
}

/// Turn `gtk_text_iter_get_chars_in_line` into a paragraph length.
///
/// GTK counts a line's delimiter in its characters (one for `\n`, `\r` or
/// U+2029, two for `\r\n`; the last line has none, `ending` `None`), while a
/// paragraph's length here excludes it, so the load profile and the edit
/// watch compare one quantity. Saturates instead of underflowing.
///
/// Kani proves it equals the reference paragraph length for every ending and
/// never underflows (`kani_proofs::boundary_line_length_excludes_its_delimiter`,
/// strings of at most 8 bytes); the Pango property test covers longer lines.
#[must_use]
pub const fn paragraph_chars_from_line(
    chars_in_line: u64,
    ending: Option<ParagraphDelimiter>,
) -> u64 {
    match ending {
        Some(delimiter) => chars_in_line.saturating_sub(delimiter.chars()),
        None => chars_in_line,
    }
}

/// Number of per-threshold long-paragraph counters a profile keeps.
pub const PROFILE_THRESHOLD_COUNT: usize = 6;

/// Most long-paragraph start offsets a production profile keeps.
#[cfg(not(kani))]
pub const MAX_PROFILED_OFFSETS: usize = 256;

/// Largest density window the fold keeps (`R_BATCH` fits, checked below).
#[cfg(not(kani))]
pub const MAX_DENSITY_WINDOW: usize = 256;

#[cfg(not(kani))]
const _: () = assert!(document_scale::R_BATCH <= MAX_DENSITY_WINDOW);

/// Under Kani the fold's two fixed buffers shrink to the harness domains
/// (`r_batch` at most 3, offset caps at most 2), which never reach these
/// bounds: the same code with buffers small enough that CBMC does not encode
/// two 256-element arrays per fold (the watch harnesses ran out of memory with
/// them). Production builds never see these values.
#[cfg(kani)]
pub const MAX_PROFILED_OFFSETS: usize = 4;

/// The Kani value of the density-window bound; see [`MAX_PROFILED_OFFSETS`].
#[cfg(kani)]
pub const MAX_DENSITY_WINDOW: usize = 4;

/// The paragraph lengths a profile counts paragraphs above.
///
/// Index 0 is also the "long paragraph" threshold whose paragraphs have their
/// start offsets recorded. Production orders them ascending
/// ([`ProfileConfig::PRODUCTION`]); nothing depends on the order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileThresholds(pub [u64; PROFILE_THRESHOLD_COUNT]);

/// Every value the profile layers and the edit-time watch read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProfileConfig {
    /// Lengths whose exceeding paragraphs are counted.
    pub thresholds: ProfileThresholds,
    /// Most long-paragraph offsets kept before `truncated` is set.
    pub offset_cap: usize,
    /// Consecutive paragraphs in one density window (at least one is used).
    pub r_batch: usize,
    /// The density limit the edit-time watch's fast path compares against;
    /// the fold itself ignores it.
    pub v_batch: u64,
}

impl ProfileConfig {
    /// The shipped configuration, built from `document_scale`'s named
    /// thresholds (ascending) and [`MAX_PROFILED_OFFSETS`].
    pub const PRODUCTION: Self = Self {
        thresholds: ProfileThresholds([
            document_scale::L_PROTECT,
            document_scale::L_SYNTAX,
            document_scale::L_SEARCH_HIGHLIGHT,
            document_scale::L_MINIMAP,
            document_scale::L_INVISIBLES,
            document_scale::N_VISIBLE,
        ]),
        offset_cap: MAX_PROFILED_OFFSETS,
        r_batch: document_scale::R_BATCH,
        v_batch: document_scale::V_BATCH,
    };

    /// The density window size, never zero.
    #[must_use]
    pub const fn window(self) -> usize {
        if self.r_batch == 0 {
            1
        } else if self.r_batch > MAX_DENSITY_WINDOW {
            MAX_DENSITY_WINDOW
        } else {
            self.r_batch
        }
    }
}

/// A document's paragraph profile: what the scale policy and the
/// reduced-features evidence know about its paragraphs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentProfile {
    /// Size of the document in UTF-8 bytes.
    pub total_bytes: u64,
    /// Paragraphs in the document (GTK lines; an empty document has one).
    pub paragraph_count: u64,
    /// Length of the longest paragraph in characters, delimiter excluded.
    pub longest_paragraph_chars: u64,
    /// Paragraphs longer than each [`ProfileThresholds`] entry.
    pub long_paragraph_counts: [u64; PROFILE_THRESHOLD_COUNT],
    /// Character offsets where paragraphs longer than threshold 0 start,
    /// at most `offset_cap` of them.
    pub long_paragraph_offsets: Vec<u64>,
    /// Whether more long paragraphs exist than `long_paragraph_offsets` holds.
    pub offsets_truncated: bool,
    /// Largest character sum of any `r_batch` consecutive paragraphs (the sum
    /// of all of them when there are fewer).
    pub worst_batch_chars: u64,
}

impl DocumentProfile {
    /// The profile of an empty buffer (an untitled tab).
    pub const EMPTY: Self = Self {
        total_bytes: 0,
        paragraph_count: 1,
        longest_paragraph_chars: 0,
        long_paragraph_counts: [0; PROFILE_THRESHOLD_COUNT],
        long_paragraph_offsets: Vec::new(),
        offsets_truncated: false,
        worst_batch_chars: 0,
    };

    /// Profile `text` in one slice with the production configuration.
    ///
    /// Workers that already traverse their body feed a
    /// [`ParagraphProfileAccumulator`] from that traversal instead.
    #[must_use]
    pub fn of_text(text: &str) -> Self {
        let mut accumulator = ParagraphProfileAccumulator::new(ProfileConfig::PRODUCTION);
        accumulator.push_str(text);
        accumulator.finish()
    }

    /// The three components the scale policy reads.
    #[must_use]
    pub const fn summary(&self) -> ProfileSummary {
        ProfileSummary {
            total_bytes: self.total_bytes,
            longest_paragraph_chars: self.longest_paragraph_chars,
            worst_batch_chars: self.worst_batch_chars,
        }
    }
}

/// The policy view of a profile: the only components `DocumentScale` reads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProfileSummary {
    /// Size of the document in UTF-8 bytes.
    pub total_bytes: u64,
    /// Length of the longest paragraph in characters, delimiter excluded.
    pub longest_paragraph_chars: u64,
    /// Largest character sum of any `r_batch` consecutive paragraphs.
    pub worst_batch_chars: u64,
}

impl ProfileSummary {
    /// Componentwise maximum (the high-water of two profiles).
    #[must_use]
    pub const fn max(self, other: Self) -> Self {
        Self {
            total_bytes: if self.total_bytes > other.total_bytes {
                self.total_bytes
            } else {
                other.total_bytes
            },
            longest_paragraph_chars: if self.longest_paragraph_chars > other.longest_paragraph_chars
            {
                self.longest_paragraph_chars
            } else {
                other.longest_paragraph_chars
            },
            worst_batch_chars: if self.worst_batch_chars > other.worst_batch_chars {
                self.worst_batch_chars
            } else {
                other.worst_batch_chars
            },
        }
    }
}

/// Folds closed paragraph lengths, in document order, into a profile.
///
/// It owns everything that depends only on the sequence of lengths: the
/// longest paragraph, the per-threshold counts, the capped offset list and
/// its `truncated` flag, and a ring of the last `r_batch` lengths whose sum's
/// maximum is `worst_batch_chars`. It never sees text, which is what lets the
/// edit-time watch reuse it for window sums.
///
/// Kani proves (`kani_proofs::profile_matches_the_specification`), with
/// thresholds of at most 8, an offset cap of at most 2 and `r_batch` from 1 to
/// 3, that the profile equals the one specified from the paragraph lengths;
/// the production configuration differs only in constant values, compared by
/// the same code, and the property test runs it on long strings.
#[derive(Clone, Debug)]
pub struct ParagraphLengthFold {
    config: ProfileConfig,
    paragraphs: u64,
    longest: u64,
    counts: [u64; PROFILE_THRESHOLD_COUNT],
    // Fixed buffers at their bound, never reallocated: a `Vec` grown per
    // paragraph made the Kani models explore the allocator's growth path and
    // exhaust the 16 GiB lane cap.
    offsets: [u64; MAX_PROFILED_OFFSETS],
    offsets_len: usize,
    truncated: bool,
    ring: [u64; MAX_DENSITY_WINDOW],
    ring_len: usize,
    ring_next: usize,
    ring_sum: u64,
    worst: u64,
}

impl ParagraphLengthFold {
    /// An empty fold (no paragraph closed yet).
    #[must_use]
    pub fn new(config: ProfileConfig) -> Self {
        Self {
            config,
            paragraphs: 0,
            longest: 0,
            counts: [0; PROFILE_THRESHOLD_COUNT],
            offsets: [0; MAX_PROFILED_OFFSETS],
            offsets_len: 0,
            truncated: false,
            ring: [0; MAX_DENSITY_WINDOW],
            ring_len: 0,
            ring_next: 0,
            ring_sum: 0,
            worst: 0,
        }
    }

    /// Fold one closed paragraph of `length` characters starting at character
    /// offset `start`.
    pub fn push(&mut self, start: u64, length: u64) {
        self.paragraphs = self.paragraphs.saturating_add(1);
        if length > self.longest {
            self.longest = length;
        }
        let thresholds = self.config.thresholds.0;
        let mut index = 0;
        while index < PROFILE_THRESHOLD_COUNT {
            if length > thresholds[index] {
                self.counts[index] = self.counts[index].saturating_add(1);
            }
            index += 1;
        }
        if length > thresholds[0] {
            if self.offsets_len < self.config.offset_cap.min(MAX_PROFILED_OFFSETS) {
                self.offsets[self.offsets_len] = start;
                self.offsets_len += 1;
            } else {
                self.truncated = true;
            }
        }
        let window = self.config.window();
        if self.ring_len < window {
            self.ring[self.ring_len] = length;
            self.ring_len += 1;
        } else {
            self.ring_sum = self.ring_sum.saturating_sub(self.ring[self.ring_next]);
            self.ring[self.ring_next] = length;
            self.ring_next += 1;
            if self.ring_next == window {
                self.ring_next = 0;
            }
        }
        self.ring_sum = self.ring_sum.saturating_add(length);
        if self.ring_sum > self.worst {
            self.worst = self.ring_sum;
        }
    }

    /// Paragraphs folded so far.
    #[must_use]
    pub const fn paragraphs(&self) -> u64 {
        self.paragraphs
    }

    /// Longest paragraph folded so far.
    #[must_use]
    pub const fn longest_paragraph_chars(&self) -> u64 {
        self.longest
    }

    /// Largest window sum folded so far.
    #[must_use]
    pub const fn worst_batch_chars(&self) -> u64 {
        self.worst
    }

    /// The profile of the folded paragraphs, for a document of `total_bytes`.
    #[must_use]
    pub fn into_profile(self, total_bytes: u64) -> DocumentProfile {
        DocumentProfile {
            total_bytes,
            paragraph_count: self.paragraphs,
            longest_paragraph_chars: self.longest,
            long_paragraph_counts: self.counts,
            long_paragraph_offsets: self.offsets[..self.offsets_len].to_vec(),
            offsets_truncated: self.truncated,
            worst_batch_chars: self.worst,
        }
    }
}

/// Turns text, fed slice by slice, into closed paragraph lengths for a
/// [`ParagraphLengthFold`].
///
/// It carries a pending-`\r` bit and the length of the paragraph still open
/// at a slice end, so the result does not depend on where the slices are cut
/// (a CRLF pair split across two slices is still one delimiter). Slices are
/// `&str`, so a U+2029 is never split and needs no carry. A paragraph's
/// length excludes its delimiter; [`finish`](Self::finish) closes the last
/// paragraph, the empty one after a trailing delimiter included, as
/// `GtkTextBuffer` has that last line too.
///
/// Kani proves (`kani_proofs::profile_is_independent_of_slicing`), for
/// strings of at most 6 bytes over `a`, `é`, `\r`, `\n`, U+2028, U+2029 and
/// U+0085 cut into at most 3 slices at any scalar boundaries, and any
/// configuration with thresholds of at most 8, an offset cap of at most 2 and
/// `r_batch` from 1 to 3, that every field of the profile equals the
/// whole-text profile; the property test is the evidence for longer strings
/// and the production configuration.
#[derive(Clone, Debug)]
pub struct ParagraphProfileAccumulator {
    fold: ParagraphLengthFold,
    total_bytes: u64,
    position: u64,
    open_start: u64,
    open_chars: u64,
    pending_cr: bool,
}

impl ParagraphProfileAccumulator {
    /// An accumulator that has seen no text.
    #[must_use]
    pub fn new(config: ProfileConfig) -> Self {
        Self {
            fold: ParagraphLengthFold::new(config),
            total_bytes: 0,
            position: 0,
            open_start: 0,
            open_chars: 0,
            pending_cr: false,
        }
    }

    /// Feed one slice.
    pub fn push_str(&mut self, slice: &str) {
        for character in slice.chars() {
            self.push_char(character);
        }
    }

    /// Feed one character (the decode analysis calls this from its own
    /// per-character loop, so profiling adds no traversal).
    #[inline]
    pub fn push_char(&mut self, character: char) {
        // `len_utf8` is at most 4, so the cast is lossless.
        self.total_bytes = self.total_bytes.saturating_add(character.len_utf8() as u64);
        let after = self.position.saturating_add(1);
        match character {
            '\n' if self.pending_cr => {
                // The `\n` of a CRLF: the paragraph closed at the `\r`, and
                // the next one starts after this character.
                self.open_start = after;
            }
            '\n' | '\r' | '\u{2029}' => {
                self.fold.push(self.open_start, self.open_chars);
                self.open_start = after;
                self.open_chars = 0;
            }
            _ => self.open_chars = self.open_chars.saturating_add(1),
        }
        self.pending_cr = character == '\r';
        self.position = after;
    }

    /// Close the last paragraph and return the profile.
    #[must_use]
    pub fn finish(mut self) -> DocumentProfile {
        self.fold.push(self.open_start, self.open_chars);
        self.fold.into_profile(self.total_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaks(text: &str) -> Vec<(usize, usize, ParagraphDelimiter)> {
        let mut out = Vec::new();
        let mut cursor = 0;
        while let Some(found) = next_paragraph_break(text, cursor) {
            // A break that does not advance would grow `out` without bound
            // (a mutant that did so took one test process to GiBs); fail at
            // once instead.
            assert!(found.next_start > cursor, "a paragraph break must advance");
            out.push((found.content_end, found.next_start, found.delimiter));
            cursor = found.next_start;
        }
        out
    }

    #[test]
    fn breaks_follow_the_gtk_delimiter_set() {
        use ParagraphDelimiter::{Cr, CrLf, Lf, ParagraphSeparator};
        assert_eq!(
            breaks("a\nb\rc\r\nd\u{2029}e"),
            vec![
                (1, 2, Lf),
                (3, 4, Cr),
                (5, 7, CrLf),
                (8, 11, ParagraphSeparator)
            ]
        );
        // U+2028 and U+0085 are ordinary characters to Pango.
        assert!(breaks("a\u{2028}b\u{85}c").is_empty());
        assert_eq!(breaks("\r"), vec![(0, 1, Cr)]);
        assert_eq!(
            breaks("\r\r\n\n"),
            vec![(0, 1, Cr), (1, 3, CrLf), (3, 4, Lf)]
        );
    }

    #[test]
    fn slice_end_takes_the_last_break_within_the_budget() {
        let text = "ab\ncd\ref\u{2029}gh";
        assert_eq!(slice_end_after_paragraph(text, 0, 7), 6);
        assert_eq!(slice_end_after_paragraph(text, 0, 6), 6);
        assert_eq!(slice_end_after_paragraph(text, 0, 5), 3);
        // No break within: the first beyond.
        assert_eq!(slice_end_after_paragraph(text, 0, 1), 3);
        // Budget reaches the end: the rest.
        assert_eq!(slice_end_after_paragraph(text, 6, 100), text.len());
        // A budget inside U+2029 floors to its start and keeps the earlier break.
        assert_eq!(slice_end_after_paragraph(text, 0, 9), 6);
        assert_eq!(slice_end_after_paragraph(text, text.len(), 4), text.len());
    }

    #[test]
    fn slice_end_takes_a_break_exactly_at_the_budget() {
        // The second break ends exactly at the budget: it is within it.
        assert_eq!(slice_end_after_paragraph("a\nb\nc", 0, 4), 4);
        assert_eq!(slice_end_after_paragraph("a\nb\nc", 0, 3), 2);
    }

    #[test]
    fn density_window_is_clamped_to_one_and_the_ring_capacity() {
        let window = |r_batch| small_config(r_batch, 1).window();
        assert_eq!(window(0), 1);
        assert_eq!(window(3), 3);
        assert_eq!(window(MAX_DENSITY_WINDOW), MAX_DENSITY_WINDOW);
        assert_eq!(window(MAX_DENSITY_WINDOW + 1), MAX_DENSITY_WINDOW);
    }

    #[test]
    fn summary_max_is_componentwise() {
        let a = ProfileSummary {
            total_bytes: 1,
            longest_paragraph_chars: 5,
            worst_batch_chars: 3,
        };
        let b = ProfileSummary {
            total_bytes: 2,
            longest_paragraph_chars: 4,
            worst_batch_chars: 6,
        };
        let both = ProfileSummary {
            total_bytes: 2,
            longest_paragraph_chars: 5,
            worst_batch_chars: 6,
        };
        assert_eq!(a.max(b), both);
        assert_eq!(b.max(a), both);
        assert_eq!(a.max(a), a);
    }

    #[test]
    fn fold_reports_its_paragraph_count_and_longest_length() {
        let mut fold = ParagraphLengthFold::new(small_config(2, 2));
        for length in [3, 7, 2] {
            fold.push(0, length);
        }
        assert_eq!(fold.paragraphs(), 3);
        assert_eq!(fold.longest_paragraph_chars(), 7);
    }

    #[test]
    fn slice_end_never_separates_cr_from_lf() {
        let text = "aaa\r\nb";
        // The budget ends between `\r` and `\n`: after the `\n`.
        assert_eq!(slice_end_after_paragraph(text, 0, 4), 5);
        // A delimiter-free remainder is taken whole.
        assert_eq!(slice_end_after_paragraph("abcdef", 0, 2), 6);
    }

    #[test]
    fn line_lengths_exclude_their_delimiter() {
        use ParagraphDelimiter::{Cr, CrLf, Lf, ParagraphSeparator};
        assert_eq!(paragraph_chars_from_line(4, Some(Lf)), 3);
        assert_eq!(paragraph_chars_from_line(4, Some(Cr)), 3);
        assert_eq!(paragraph_chars_from_line(4, Some(ParagraphSeparator)), 3);
        assert_eq!(paragraph_chars_from_line(4, Some(CrLf)), 2);
        assert_eq!(paragraph_chars_from_line(4, None), 4);
        assert_eq!(paragraph_chars_from_line(0, Some(CrLf)), 0);
        assert_eq!(paragraph_chars_from_line(1, Some(CrLf)), 0);
    }

    fn small_config(r_batch: usize, offset_cap: usize) -> ProfileConfig {
        ProfileConfig {
            thresholds: ProfileThresholds([2, 3, 4, 5, 6, 7]),
            offset_cap,
            r_batch,
            v_batch: 10,
        }
    }

    fn profile(text: &str, config: ProfileConfig) -> DocumentProfile {
        let mut accumulator = ParagraphProfileAccumulator::new(config);
        accumulator.push_str(text);
        accumulator.finish()
    }

    #[test]
    fn profile_measures_paragraphs_without_their_delimiters() {
        let found = profile("abc\r\nde\u{2029}fghij\r", small_config(2, 8));
        assert_eq!(found.total_bytes, 16);
        // Paragraphs: 3, 2, 5, and the empty last line.
        assert_eq!(found.paragraph_count, 4);
        assert_eq!(found.longest_paragraph_chars, 5);
        assert_eq!(found.long_paragraph_counts, [2, 1, 1, 0, 0, 0]);
        // Character offsets of the paragraphs over 2: "abc" at 0, "fghij" at 8.
        assert_eq!(found.long_paragraph_offsets, vec![0, 8]);
        assert!(!found.offsets_truncated);
        assert_eq!(found.worst_batch_chars, 7);
    }

    #[test]
    fn offset_list_truncates_but_counts_everything() {
        let found = profile("aaa\nbbb\nccc\n", small_config(1, 2));
        assert_eq!(found.long_paragraph_counts[0], 3);
        assert_eq!(found.long_paragraph_offsets, vec![0, 4]);
        assert!(found.offsets_truncated);
    }

    #[test]
    fn density_is_one_window_when_paragraphs_are_fewer_than_r_batch() {
        let found = profile("ab\ncd", small_config(5, 2));
        assert_eq!(found.worst_batch_chars, 4);
        let found = profile("a\nbbb\ncc\nd", small_config(2, 2));
        assert_eq!(found.worst_batch_chars, 5);
    }

    #[test]
    fn empty_text_is_the_empty_profile() {
        assert_eq!(DocumentProfile::of_text(""), DocumentProfile::EMPTY);
    }

    #[test]
    fn crlf_split_across_slices_is_one_delimiter() {
        let config = small_config(2, 4);
        let whole = profile("ab\r\ncd", config);
        let mut accumulator = ParagraphProfileAccumulator::new(config);
        accumulator.push_str("ab\r");
        accumulator.push_str("\ncd");
        assert_eq!(accumulator.finish(), whole);
    }

    #[test]
    fn production_configuration_reads_the_scale_constants() {
        let production = ProfileConfig::PRODUCTION;
        assert_eq!(
            production.thresholds.0,
            [
                document_scale::L_PROTECT,
                document_scale::L_SYNTAX,
                document_scale::L_SEARCH_HIGHLIGHT,
                document_scale::L_MINIMAP,
                document_scale::L_INVISIBLES,
                document_scale::N_VISIBLE,
            ]
        );
        assert_eq!(production.offset_cap, MAX_PROFILED_OFFSETS);
        assert_eq!(production.r_batch, document_scale::R_BATCH);
        assert_eq!(production.v_batch, document_scale::V_BATCH);
        assert!(
            production
                .thresholds
                .0
                .windows(2)
                .all(|pair| pair[0] < pair[1])
        );
    }
}
