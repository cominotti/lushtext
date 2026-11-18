// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental UTF-8 validation for the Replace All stream (design D2).
//!
//! [`Utf8Carry`] sees every chunk before the engine's line logic does, and
//! either stops the file as not UTF-8 or lets the chunk through unchanged. An
//! incomplete sequence at the end of a chunk (at most three bytes) is carried
//! into the next one, so where the chunks are cut never changes the verdict.
//! Kani proves that (`kani_proofs.rs`, shard `core-replace-stream`, design D11).

/// Incremental UTF-8 validation that carries an incomplete trailing sequence.
#[derive(Debug, Default)]
pub(super) struct Utf8Carry {
    pending: [u8; 4],
    len: usize,
}

impl Utf8Carry {
    /// Validate the next chunk; `Err` means the stream is not UTF-8.
    pub(super) fn feed(&mut self, mut chunk: &[u8]) -> Result<(), ()> {
        if self.len > 0 {
            let expected = utf8_sequence_len(self.pending[0]);
            let take = (expected - self.len).min(chunk.len());
            self.pending[self.len..self.len + take].copy_from_slice(&chunk[..take]);
            self.len += take;
            chunk = &chunk[take..];
            if self.len < expected {
                return Ok(());
            }
            validate_utf8(&self.pending[..expected]).map_err(|_| ())?;
            self.len = 0;
        }
        match validate_utf8(chunk) {
            Ok(()) => Ok(()),
            Err((valid_up_to, true)) => {
                let tail = &chunk[valid_up_to..];
                self.pending[..tail.len()].copy_from_slice(tail);
                self.len = tail.len();
                Ok(())
            }
            Err((_, false)) => Err(()),
        }
    }

    /// The stream ended; an unfinished sequence is invalid.
    pub(super) fn finish(&self) -> Result<(), ()> {
        if self.len == 0 { Ok(()) } else { Err(()) }
    }
}

/// Length of the sequence a valid UTF-8 lead byte starts.
fn utf8_sequence_len(lead: u8) -> usize {
    match lead {
        0xF0..=0xF7 => 4,
        0xE0..=0xEF => 3,
        0xC0..=0xDF => 2,
        _ => 1,
    }
}

/// `Ok`, or the valid prefix length and whether the error is only an
/// incomplete sequence at the end of the input.
#[cfg(not(kani))]
fn validate_utf8(bytes: &[u8]) -> Result<(), (usize, bool)> {
    simdutf8::compat::from_utf8(bytes)
        .map(|_| ())
        .map_err(|error| (error.valid_up_to(), error.error_len().is_none()))
}

/// The byte-loop path for the Kani harness: `simdutf8`'s runtime-detected SIMD
/// is out of CBMC's reach, and so is `core::str::from_utf8`'s word-at-a-time
/// fast path, whose unwinding ran CBMC out of memory (design D11).
#[cfg(kani)]
fn validate_utf8(bytes: &[u8]) -> Result<(), (usize, bool)> {
    validate_utf8_bytewise(bytes)
}

/// UTF-8 validation one byte at a time, with the result `simdutf8::compat`
/// and `core::str::from_utf8` give: `Ok`, or the length of the valid prefix
/// and whether the error is only a sequence cut short by the end of `bytes`.
///
/// It follows the well-formed byte sequences of the Unicode standard (table
/// 3-7): no overlong forms, no surrogates, nothing above U+10FFFF. The Kani
/// harness `the_byte_scan_matches_core_from_utf8` proves it equal to
/// `core::str::from_utf8` on every input of at most four bytes, and the unit
/// test `the_byte_scan_matches_simdutf8` compares it with the production
/// validator on every sequence of class-boundary bytes up to five long.
#[cfg(any(test, kani))]
fn validate_utf8_bytewise(bytes: &[u8]) -> Result<(), (usize, bool)> {
    // One pass, one byte per step: `need` continuation bytes are still owed
    // to the sequence that starts at `start`, and the next one must lie in
    // `low..=high`. No index jumps by a sequence's width, which keeps CBMC's
    // unwinding linear in the input.
    let mut need = 0u8;
    let mut start = 0usize;
    let (mut low, mut high) = (0x80u8, 0xBFu8);
    for (index, &byte) in bytes.iter().enumerate() {
        if need == 0 {
            start = index;
            (need, low, high) = match byte {
                0x00..=0x7F => continue,
                0xC2..=0xDF => (1, 0x80, 0xBF),
                0xE0 => (2, 0xA0, 0xBF),
                0xE1..=0xEC | 0xEE..=0xEF => (2, 0x80, 0xBF),
                0xED => (2, 0x80, 0x9F),
                0xF0 => (3, 0x90, 0xBF),
                0xF1..=0xF3 => (3, 0x80, 0xBF),
                0xF4 => (3, 0x80, 0x8F),
                _ => return Err((index, false)),
            };
        } else {
            if !(low..=high).contains(&byte) {
                return Err((start, false));
            }
            need -= 1;
            (low, high) = (0x80, 0xBF);
        }
    }
    if need == 0 {
        Ok(())
    } else {
        Err((start, true))
    }
}

#[cfg(test)]
mod tests {
    use super::{Utf8Carry, validate_utf8, validate_utf8_bytewise};

    /// Every byte at an edge of a UTF-8 byte class (Unicode table 3-7).
    const CLASS_EDGES: [u8; 24] = [
        0x00, 0x7F, 0x80, 0x8F, 0x90, 0x9F, 0xA0, 0xBF, 0xC0, 0xC1, 0xC2, 0xDF, 0xE0, 0xE1, 0xEC,
        0xED, 0xEE, 0xEF, 0xF0, 0xF1, 0xF3, 0xF4, 0xF5, 0xFF,
    ];

    /// Every sequence of `CLASS_EDGES` bytes of each length up to `max`.
    fn each_sequence(max: usize, mut check: impl FnMut(&[u8])) {
        let mut bytes = Vec::with_capacity(max);
        check(&bytes);
        for len in 1..=max {
            let mut digits = vec![0usize; len];
            loop {
                bytes.clear();
                bytes.extend(digits.iter().map(|&digit| CLASS_EDGES[digit]));
                check(&bytes);
                let Some(position) = digits
                    .iter()
                    .rposition(|&digit| digit + 1 < CLASS_EDGES.len())
                else {
                    break;
                };
                digits[position] += 1;
                for digit in &mut digits[position + 1..] {
                    *digit = 0;
                }
            }
        }
    }

    #[test]
    fn the_byte_scan_matches_simdutf8() {
        each_sequence(5, |bytes| {
            assert_eq!(
                validate_utf8_bytewise(bytes),
                validate_utf8(bytes),
                "{bytes:02x?}"
            );
            let core = core::str::from_utf8(bytes)
                .map(|_| ())
                .map_err(|error| (error.valid_up_to(), error.error_len().is_none()));
            assert_eq!(validate_utf8_bytewise(bytes), core, "{bytes:02x?}");
        });
    }

    #[test]
    fn the_carry_rejects_exactly_what_a_one_shot_validation_rejects() {
        each_sequence(4, |bytes| {
            let whole = validate_utf8(bytes).is_ok();
            for chunk in 1..=bytes.len().max(1) {
                let mut carry = Utf8Carry::default();
                let split = bytes.chunks(chunk).all(|piece| carry.feed(piece).is_ok())
                    && carry.finish().is_ok();
                assert_eq!(split, whole, "{bytes:02x?} chunk {chunk}");
            }
        });
    }
}

#[cfg(kani)]
mod kani_proofs;
