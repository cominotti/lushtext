// SPDX-License-Identifier: GPL-3.0-or-later

//! Split independence of the Replace All streaming engine (design D11, shard
//! `core-replace-stream`).
//!
//! Domain, stated here and in the `ReplaceStream` rustdoc: a source of at most
//! [`MAX_SOURCE`] arbitrary bytes, every chunk size from 1 to [`MAX_CHUNK`]
//! compared with the whole source fed at once, at most two edits whose
//! recorded lines come from [`LINES`] (which includes a two-byte character, a
//! lone `\r` and the empty line), any output limit up to 16 bytes, and the
//! `cfg(kani)` byte-loop UTF-8 and newline paths with a recording digest sink,
//! never SHA-256, `simdutf8` or `memchr`.
//!
//! Properties:
//!
//! 1. whether a file is replaced does not depend on the chunk size, and a
//!    replaced file has the same output, replacement count, lengths and digests
//!    at every chunk size;
//! 2. on valid UTF-8 the outcome itself does not depend on the chunk size, and
//!    is never `NotUtf8`; a replaced source is valid UTF-8;
//! 3. no chunking writes past the output limit, and the output digest records
//!    exactly the bytes written.

use super::{RecordingSink, ReplacementTextOutcome, StreamEdit, stream_whole};

const MAX_SOURCE: usize = 8;
const MAX_CHUNK: usize = 4;
const LINES: [&str; 5] = ["", "a", "ab", "\r", "\u{e9}"];

fn any_line() -> &'static str {
    let index: usize = kani::any();
    kani::assume(index < LINES.len());
    LINES[index]
}

/// A char-boundary range of `line` that starts at or after `from`.
fn any_range(line: &str, from: usize) -> (usize, usize) {
    let start: usize = kani::any();
    let end: usize = kani::any();
    kani::assume(from <= start && start <= end && end <= line.len());
    kani::assume(line.is_char_boundary(start) && line.is_char_boundary(end));
    (start, end)
}

fn any_replacement() -> &'static str {
    let index: u8 = kani::any();
    kani::assume(index < 3);
    ["", "x", "\u{e9}\n"][usize::from(index)]
}

/// At most two edits in the order `plan_edits` produces: by line, and without
/// overlap on one line.
fn any_edits() -> ([StreamEdit<'static>; 2], usize) {
    let count: usize = kani::any();
    kani::assume(count <= 2);
    let first_line: u64 = kani::any();
    kani::assume((1..=3).contains(&first_line));
    let first_text = any_line();
    let (start, end) = any_range(first_text, 0);
    let first = StreamEdit {
        line_number: first_line,
        original_line: first_text,
        start,
        end,
        replacement: any_replacement(),
    };
    let same_line: bool = kani::any();
    let second = if same_line {
        let (start, end) = any_range(first_text, end);
        StreamEdit {
            line_number: first_line,
            original_line: first_text,
            start,
            end,
            replacement: any_replacement(),
        }
    } else {
        let second_line: u64 = kani::any();
        kani::assume(first_line < second_line && second_line <= 4);
        let text = any_line();
        let (start, end) = any_range(text, 0);
        StreamEdit {
            line_number: second_line,
            original_line: text,
            start,
            end,
            replacement: any_replacement(),
        }
    };
    ([first, second], count)
}

#[kani::proof]
#[kani::unwind(10)]
fn the_stream_does_not_depend_on_its_chunk_size() {
    let bytes: [u8; MAX_SOURCE] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= MAX_SOURCE);
    let source = &bytes[..len];
    let chunk: usize = kani::any();
    kani::assume((1..=MAX_CHUNK).contains(&chunk));
    let limit: u64 = kani::any();
    kani::assume(limit <= 16);
    let (edits, count) = any_edits();
    let edits = &edits[..count];

    let (whole, whole_out) = stream_whole(
        source,
        edits,
        MAX_SOURCE,
        limit,
        RecordingSink::default(),
        RecordingSink::default(),
    );
    let (split, split_out) = stream_whole(
        source,
        edits,
        chunk,
        limit,
        RecordingSink::default(),
        RecordingSink::default(),
    );

    // Property 3, for both feeds.
    for (summary, out) in [(&whole, &whole_out), (&split, &split_out)] {
        assert!(summary.output_len <= limit);
        assert_eq!(summary.output_len, out.len() as u64);
        assert_eq!(&summary.output_digest.0, out);
    }

    let whole_replaced = matches!(whole.outcome, ReplacementTextOutcome::Replaced { .. });
    let split_replaced = matches!(split.outcome, ReplacementTextOutcome::Replaced { .. });
    // Property 1.
    assert_eq!(whole_replaced, split_replaced);
    if whole_replaced {
        assert_eq!(whole.outcome, split.outcome);
        assert_eq!(whole_out, split_out);
        assert_eq!(whole.source_len, split.source_len);
        assert_eq!(whole.source_len, len as u64);
        assert_eq!(whole.source_digest, split.source_digest);
        assert_eq!(whole.source_digest.0.as_slice(), source);
        kani::cover!(count == 2, "two edits are applied");
        kani::cover!(
            source.contains(&b'\r'),
            "a replaced source holds a carriage return"
        );
        kani::cover!(
            source.iter().any(|&byte| byte >= 0x80),
            "a replaced source holds a multi-byte character"
        );
    }

    // Property 2.
    let valid = core::str::from_utf8(source).is_ok();
    if valid {
        assert_eq!(whole.outcome, split.outcome);
        assert_ne!(whole.outcome, ReplacementTextOutcome::NotUtf8);
        kani::cover!(
            whole.outcome == ReplacementTextOutcome::OutputLimitExceeded,
            "the output limit stops a valid source"
        );
        kani::cover!(
            matches!(whole.outcome, ReplacementTextOutcome::StaleLine { .. }),
            "a stale line stops a valid source"
        );
    } else {
        kani::cover!(
            split.outcome == ReplacementTextOutcome::NotUtf8,
            "an invalid source is not UTF-8"
        );
    }
    if whole_replaced {
        assert!(valid);
    }
}
