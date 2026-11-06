// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::model::buffer_replacement::next_slice_boundary;
use crate::model::protected_fold::{FoldThresholds, plan_folds};

/// The characters of every paragraph of `text`, as C2's fold reads them.
fn paragraph_chars(text: &str) -> Vec<u64> {
    let mut lengths = Vec::new();
    let mut start = 0;
    while let Some(found) = next_paragraph_break(text, start) {
        lengths.push(text[start..found.content_end].chars().count() as u64);
        start = found.next_start;
    }
    lengths.push(text[start..].chars().count() as u64);
    lengths
}

/// The byte span `[start, content_end)` of each paragraph.
fn paragraph_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0;
    while let Some(found) = next_paragraph_break(text, start) {
        spans.push((start, found.content_end));
        start = found.next_start;
    }
    spans.push((start, text.len()));
    spans
}

fn folds(text: &str, head_cap: u64) -> Vec<FoldRange> {
    let thresholds = FoldThresholds::new(head_cap, 1, u64::MAX / 2, 1).expect("valid thresholds");
    plan_folds(&paragraph_chars(text), thresholds)
}

fn plan_all(
    text: &str,
    ranges: &[FoldRange],
    budget: SliceBudget,
    limits: ProtectedInstallLimits,
) -> Vec<(usize, ProtectedSlice)> {
    let mut cursor = ProtectedInstallCursor::new();
    let mut slices = Vec::new();
    let mut start = 0;
    while let Some(slice) = plan_protected_install(text, ranges, &mut cursor, budget, limits) {
        assert!(
            slice.end > start || slice.kind == SliceKind::Head,
            "progress at {start}"
        );
        assert!(slices.len() < 10 * text.len() + 10, "the plan terminates");
        slices.push((start, slice));
        start = slice.end;
    }
    assert_eq!(start, text.len(), "the plan covers the text");
    slices
}

/// Every planner property the Kani harnesses state, checked on one input.
fn check(text: &str, ranges: &[FoldRange], budget: SliceBudget, limits: ProtectedInstallLimits) {
    let slices = plan_all(text, ranges, budget, limits);
    let bytes = text.as_bytes();
    let spans = paragraph_spans(text);
    let mut rebuilt = String::new();
    // Hidden tails, as byte ranges, from the plan.
    let tails: Vec<(usize, usize)> = ranges
        .iter()
        .map(|range| {
            let (start, end) = spans[usize::try_from(range.paragraph).expect("fits")];
            (
                start + fold_byte_offset(&text[start..end], range.head_chars),
                end,
            )
        })
        .collect();
    let mut hidden_run = 0usize;
    for (index, (start, slice)) in slices.iter().enumerate() {
        let end = slice.end;
        // K2, K3.
        assert!(
            text.is_char_boundary(end),
            "slice end {end} splits a scalar"
        );
        assert!(
            !(end > 0 && end < bytes.len() && bytes[end - 1] == b'\r' && bytes[end] == b'\n'),
            "slice end {end} splits a CRLF"
        );
        rebuilt.push_str(&text[*start..end]);
        let inside_tail = tails
            .iter()
            .find(|(tail_start, tail_end)| *start >= *tail_start && *start < *tail_end);
        match slice.kind {
            SliceKind::HiddenTail => {
                // K4: wholly inside one tail, which it may end.
                let (_, tail_end) = inside_tail.expect("a hidden slice lies in a tail");
                assert!(end <= *tail_end);
                assert_eq!(slice.last_of_tail, end == *tail_end);
                // K5.
                assert!(end - start <= limits.tail_slice_bytes());
                if !slice.last_of_tail {
                    assert!(end - start + 3 >= limits.tail_slice_bytes());
                }
                hidden_run += end - start;
            }
            SliceKind::Head => {
                // K8: the head ends exactly at the fold point.
                assert!(tails.iter().any(|(tail_start, _)| *tail_start == end));
                assert!(inside_tail.is_none());
            }
            SliceKind::Visible => {
                assert!(inside_tail.is_none(), "a visible slice starts in a tail");
                assert!(
                    !tails
                        .iter()
                        .any(|(tail_start, tail_end)| tail_start < tail_end
                            && *tail_start >= *start
                            && *tail_start < end),
                    "a visible slice holds hidden bytes"
                );
                // A visible slice ends on a paragraph boundary or before a
                // folded paragraph.
                assert!(
                    end == text.len()
                        || spans
                            .iter()
                            .any(|(paragraph_start, _)| *paragraph_start == end),
                    "visible slice ends at {end}"
                );
            }
        }
        // K6 (the executor's marks): a tail slice that is not its tail's last
        // is followed by another tail slice of the same tail.
        if slice.kind == SliceKind::HiddenTail && !slice.last_of_tail {
            assert_eq!(slices[index + 1].1.kind, SliceKind::HiddenTail);
        }
    }
    assert_eq!(rebuilt, text);
    let hidden: usize = tails.iter().map(|(start, end)| end - start).sum();
    assert_eq!(hidden_run, hidden, "every hidden byte is in a hidden slice");
}

const fn limits(tail: usize) -> ProtectedInstallLimits {
    match ProtectedInstallLimits::new(tail, tail) {
        Some(limits) => limits,
        None => panic!("test limits"),
    }
}

#[test]
fn production_slice_sizes_are_the_capped_derivation() {
    let tail = ProtectedInstallLimits::for_settle_budget(
        B_SETTLE_US,
        PROTECTED_TAIL_COST_NS_PER_KIB,
        PROTECTED_SLICE_MARGIN_PERCENT,
    );
    let clear = ProtectedInstallLimits::for_settle_budget(
        B_SETTLE_US,
        PROTECTED_CLEAR_COST_NS_PER_KIB,
        PROTECTED_SLICE_MARGIN_PERCENT,
    );
    assert_eq!(tail, Ok(MAX_SLICE_BYTES));
    assert_eq!(clear, Ok(MAX_SLICE_BYTES));
    assert_eq!(PROTECTED_TAIL_SLICE_BYTES, MAX_SLICE_BYTES);
    assert_eq!(PROTECTED_CLEAR_SLICE_BYTES, MAX_SLICE_BYTES);
    assert!(PROTECTED_TAIL_SLICE_BYTES.is_power_of_two());
    // Below the cap the derivation is the closed form: 16 ms at 104 µs per
    // KiB with no margin is 157,538 bytes (157,539 would cost 16,000,099 ns).
    assert_eq!(
        ProtectedInstallLimits::for_settle_budget(B_SETTLE_US, 104_000, 0),
        Ok(157_538)
    );
}

#[test]
fn the_derivation_rejects_a_budget_below_one_scalar() {
    assert_eq!(
        ProtectedInstallLimits::for_settle_budget(0, 1, 0),
        Err(SliceBudgetUnattainable)
    );
    assert_eq!(
        ProtectedInstallLimits::for_settle_budget(1, u64::MAX, u64::MAX),
        Err(SliceBudgetUnattainable)
    );
    assert_eq!(
        ProtectedInstallLimits::for_settle_budget(u64::MAX, 0, u64::MAX),
        Ok(MAX_SLICE_BYTES)
    );
    assert!(ProtectedInstallLimits::new(3, 4).is_none());
    assert!(ProtectedInstallLimits::new(4, 3).is_none());
}

#[test]
fn a_document_without_folds_slices_like_the_ordinary_slicer() {
    let text = format!(
        "{}\n{}\r\n{}\u{2029}tail",
        "a".repeat(300_000),
        "b\n".repeat(200_000),
        "c".repeat(10)
    );
    for budget in [SliceBudget::BYTES_ONLY, SliceBudget::PACED] {
        let slices = plan_all(&text, &[], budget, PRODUCTION_LIMITS);
        let mut start = 0;
        for (_, slice) in slices {
            assert_eq!(slice.kind, SliceKind::Visible);
            assert_eq!(slice.end, next_slice_boundary(&text, start, budget));
            start = slice.end;
        }
    }
}

#[test]
fn a_giant_paragraph_has_one_head_and_bounded_tail_slices() {
    let tail_bytes = 4096;
    let text = format!("{}\nafter\n", "x".repeat(40_000));
    let ranges = folds(&text, 500);
    assert_eq!(ranges.len(), 1);
    let slices = plan_all(&text, &ranges, SliceBudget::BYTES_ONLY, limits(tail_bytes));
    let kinds: Vec<SliceKind> = slices.iter().map(|(_, slice)| slice.kind).collect();
    assert_eq!(kinds[0], SliceKind::Head);
    assert_eq!(slices[0].1.end, 500);
    let tail_slices = kinds
        .iter()
        .filter(|kind| **kind == SliceKind::HiddenTail)
        .count();
    // ⌈39,500 / 4,096⌉ = 10, and the delimiter starts the visible remainder.
    assert_eq!(tail_slices, 10);
    assert_eq!(kinds.last(), Some(&SliceKind::Visible));
    check(&text, &ranges, SliceBudget::BYTES_ONLY, limits(tail_bytes));
}

#[test]
fn edges_hold_every_property() {
    let multibyte = "é€𝄞a".repeat(3_000);
    let cases = [
        // The fold point exactly at a slice budget, and a tail of exactly
        // one budget.
        format!("{}\n", "f".repeat(64 + 16)),
        "f".repeat(64 + 32),
        // CR-only, CRLF straddling a budget, U+2029.
        format!("{}\r{}\rz", "r".repeat(100), "s".repeat(90)),
        format!("{}\r\n{}", "q".repeat(64 + 31), "w".repeat(70)),
        format!("{}\u{2029}{}\u{2029}", "u".repeat(99), "v".repeat(5)),
        // Multibyte scalars at every byte phase.
        multibyte.clone(),
        format!("a{multibyte}\nb"),
        format!("ab{multibyte}\r\n"),
        format!("abc{multibyte}\u{2029}"),
        // Several protected paragraphs mixed with short ones.
        format!(
            "short\n{}\nmid\n{}\r\n\nend",
            "1".repeat(300),
            "2é".repeat(200)
        ),
        // An empty document and a document of short lines only.
        String::new(),
        "a\nb\r\nc\u{2029}d".to_string(),
    ];
    for text in &cases {
        let ranges = folds(text, 64);
        for tail in [4, 5, 6, 7, 32, 64, 1_000] {
            for budget in [
                SliceBudget::BYTES_ONLY,
                SliceBudget {
                    bytes: 7,
                    chars: u64::MAX,
                },
                SliceBudget {
                    bytes: 50,
                    chars: 3,
                },
            ] {
                check(text, &ranges, budget, limits(tail));
            }
        }
    }
}

#[test]
fn the_slice_count_bound_uses_the_rounded_step() {
    // Four-byte scalars with a five-byte budget: one scalar per slice, so the
    // bound needs `step = tail_slice_bytes − 3`, not the budget itself.
    let text = "𝄞".repeat(10);
    let ranges = folds(&text, 1);
    let slices = plan_all(&text, &ranges, SliceBudget::BYTES_ONLY, limits(5));
    let tail = slices
        .iter()
        .filter(|(_, slice)| slice.kind == SliceKind::HiddenTail)
        .count();
    assert_eq!(tail, 9);
    let len = text.len() - 4;
    assert!(tail > len.div_ceil(5), "the naive bound fails here");
    assert!(tail <= len.div_ceil(5 - 3));
}

#[test]
fn a_mismatched_plan_never_moves_past_a_delimiter() {
    // A range whose head is longer than its paragraph (not what C2 plans).
    let text = "abc\ndefghij\n";
    let ranges = [FoldRange {
        paragraph: 0,
        head_chars: 10,
        tail_chars: 1,
    }];
    let slices = plan_all(text, &ranges, SliceBudget::BYTES_ONLY, limits(4));
    assert_eq!(slices[0].1.end, 3);
    let rebuilt: String = slices
        .iter()
        .map(|(start, slice)| &text[*start..slice.end])
        .collect();
    assert_eq!(rebuilt, text);
}

#[test]
fn clear_turns_take_ranges_from_the_start_within_the_budget() {
    let limits = limits(10);
    assert_eq!(clear_ranges_for_turn(&[], limits), 0);
    assert_eq!(clear_ranges_for_turn(&[50], limits), 1);
    assert_eq!(clear_ranges_for_turn(&[4, 4, 4], limits), 2);
    assert_eq!(clear_ranges_for_turn(&[1, 30, 3, 7], limits), 1);
    assert_eq!(clear_ranges_for_turn(&[3, 7, 30], limits), 2);
    assert_eq!(clear_ranges_for_turn(&[11, 9], limits), 1);
}

#[test]
fn the_content_end_scan_finds_the_first_delimiter_within_its_window() {
    // Every delimiter ends the content where it starts.
    assert_eq!(paragraph_content_end("ab\ncd", 0, 64), Ok(2));
    assert_eq!(paragraph_content_end("ab\r\ncd", 0, 64), Ok(2));
    assert_eq!(paragraph_content_end("ab\rcd", 0, 64), Ok(2));
    assert_eq!(paragraph_content_end("ab\u{2029}cd", 0, 64), Ok(2));
    // From inside the content, and at a delimiter.
    assert_eq!(paragraph_content_end("abc\nd\ne", 1, 64), Ok(3));
    assert_eq!(paragraph_content_end("abc\nd\ne", 3, 64), Ok(3));
    assert_eq!(paragraph_content_end("abc\nd\ne", 4, 64), Ok(5));
    // An undelimited last paragraph ends at the text's end, also when the
    // window reaches it exactly.
    assert_eq!(paragraph_content_end("abcdef", 0, 64), Ok(6));
    assert_eq!(paragraph_content_end("abcdef", 2, 4), Ok(6));
    // A delimiter that starts on the window's last byte is found.
    assert_eq!(paragraph_content_end("abc\ndef", 0, 4), Ok(3));
}

#[test]
fn the_content_end_scan_resumes_without_losing_a_straddling_separator() {
    // No end within the window: resume two bytes before its end, so a U+2029
    // whose lead byte is past the window is found by the next scan.
    assert_eq!(paragraph_content_end("abcdefgh\n", 0, 4), Err(2));
    assert_eq!(paragraph_content_end("abcdefgh\n", 2, 4), Err(4));
    // Never before `from + 1`, so a scan always advances.
    assert_eq!(paragraph_content_end("abcdefgh\n", 2, 1), Err(3));
    assert_eq!(paragraph_content_end("abcdefgh\n", 5, 0), Err(6));
    // A separator straddling the window's end is found by the resumed scan.
    let text = "abcd\u{2029}ef";
    let mut from = 0;
    let end = loop {
        match paragraph_content_end(text, from, 5) {
            Ok(end) => break end,
            Err(resume) => {
                assert!(resume > from);
                from = resume;
            }
        }
    };
    assert_eq!(end, 4);
    // Resuming walks a long undelimited tail to its delimiter.
    let long = format!("{}\n", "a".repeat(100));
    let mut from = 0;
    let mut scans = 0;
    let end = loop {
        scans += 1;
        match paragraph_content_end(&long, from, 16) {
            Ok(end) => break end,
            Err(resume) => from = resume,
        }
    };
    assert_eq!((end, scans), (100, 8));
}

#[test]
fn a_body_needs_slicing_exactly_when_its_hidden_bytes_could_exceed_one_tail_slice() {
    let per_slice = (PROTECTED_TAIL_SLICE_BYTES / MAX_UTF8_SCALAR_BYTES) as u64;
    assert_eq!(per_slice, 1 << 16);
    assert!(!hidden_tail_requires_slicing(0));
    assert!(!hidden_tail_requires_slicing(per_slice));
    assert!(hidden_tail_requires_slicing(per_slice + 1));
    // Saturating: a count whose bytes overflow `u64` still needs slicing.
    assert!(hidden_tail_requires_slicing(u64::MAX / 2));
    assert!(hidden_tail_requires_slicing(u64::MAX));
}

#[test]
fn the_tail_scan_window_is_32_mib() {
    assert_eq!(PROTECTED_TAIL_SCAN_BYTES, 1 << 25);
    assert!(PROTECTED_TAIL_SCAN_BYTES.is_power_of_two());
}

#[test]
fn the_cursor_reports_its_offset_and_whether_it_is_inside_a_tail() {
    // "ab" visible, then a paragraph folded after one character with a
    // six-byte tail cut at four bytes.
    let text = "ab\ncdefghi";
    let ranges = [FoldRange {
        paragraph: 1,
        head_chars: 1,
        tail_chars: 6,
    }];
    let limits = ProtectedInstallLimits::new(4, 4).expect("4 bytes is one scalar");
    let mut cursor = ProtectedInstallCursor::default();
    assert_eq!((cursor.offset(), cursor.in_tail()), (0, false));
    let mut seen = Vec::new();
    while let Some(slice) =
        plan_protected_install(text, &ranges, &mut cursor, SliceBudget::BYTES_ONLY, limits)
    {
        assert_eq!(cursor.offset(), slice.end);
        seen.push((slice.kind, slice.end, cursor.in_tail()));
    }
    assert_eq!(
        seen,
        [
            (SliceKind::Visible, 3, false),
            (SliceKind::Head, 4, true),
            (SliceKind::HiddenTail, 8, true),
            (SliceKind::HiddenTail, 10, false),
        ]
    );
}

#[test]
fn the_tail_rounding_stops_at_its_floor_and_never_splits_a_scalar_or_a_crlf() {
    // Down to a scalar boundary.
    let euro = "a\u{20ac}b".as_bytes();
    assert_eq!(round_down_to_safe_boundary(euro, 0, 2), 1);
    assert_eq!(round_down_to_safe_boundary(euro, 0, 3), 1);
    assert_eq!(round_down_to_safe_boundary(euro, 0, 4), 4);
    // Never below the floor, even inside a scalar.
    assert_eq!(round_down_to_safe_boundary(euro, 2, 2), 2);
    assert_eq!(round_down_to_safe_boundary(euro, 2, 3), 2);
    // Never between `\r` and `\n`.
    assert_eq!(round_down_to_safe_boundary(b"\r\n", 0, 1), 0);
    assert_eq!(round_down_to_safe_boundary(b"a\r\n", 0, 2), 1);
    // A lone `\r` before another byte is not a CRLF.
    assert_eq!(round_down_to_safe_boundary(b"a\rb", 0, 2), 2);
    // At and past the text's end: the end, read nowhere beyond it.
    assert_eq!(round_down_to_safe_boundary(b"ab", 0, 2), 2);
    assert_eq!(round_down_to_safe_boundary(b"ab", 0, 9), 2);
}

#[test]
fn only_u2029_among_the_e2_80_scalars_is_a_paragraph_delimiter() {
    // U+2028 (E2 80 A8) shares U+2029's first two bytes and is not a GTK
    // paragraph delimiter (ledger A42); the proofs' alphabet has no such scalar.
    let text = "a\u{2028}b\u{2029}c\n".as_bytes();
    assert_eq!(first_delimiter_in(text, 0, text.len()), Some(5));
    // A third byte A9 after a different second byte is not U+2029 either.
    let text = "\u{2069}\n".as_bytes();
    assert_eq!(first_delimiter_in(text, 0, text.len()), Some(3));
    assert_eq!(first_delimiter_in(b"\xe2\x80", 0, 2), None);
}
