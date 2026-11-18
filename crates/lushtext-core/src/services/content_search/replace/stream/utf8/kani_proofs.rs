// SPDX-License-Identifier: GPL-3.0-or-later

//! The Replace All stream's UTF-8 seams (design D11, shard
//! `core-replace-stream`).
//!
//! [`Utf8Carry`] sees every chunk of a file before the engine's line logic,
//! and either stops the file as not UTF-8 or lets the chunk through
//! unchanged. These harnesses prove that where the chunks are cut never
//! changes that verdict:
//!
//! - `utf8_seams_do_not_depend_on_the_chunk_size`: fed in chunks of 1 to
//!   [`MAX_CHUNK`] bytes, the carry rejects exactly the sources of at most
//!   [`CARRY_SOURCE`] arbitrary bytes that one validation of the whole source
//!   rejects;
//! - `the_byte_scan_matches_core_from_utf8`: the `cfg(kani)` byte scan the
//!   carry uses here equals `core::str::from_utf8` on every input of at most
//!   four bytes (one sequence of every length, and each way it can end).
//!
//! **Bound reduction (design D11, "Kani memory").** The engine's line logic
//! is not proved here. Every harness form that ran the engine was stopped in
//! CBMC's symbolic execution: the four-byte forms past the shard table's
//! 12 GiB runner margin, and the concrete-shape ladder, down to one run over
//! 2 symbolic bytes with one fixed edit and no sinks, past 8 GiB and still
//! growing. Its split independence is checked exhaustively over a bounded
//! domain by the unit test
//! `the_engine_does_not_depend_on_its_chunk_size_over_a_bounded_domain`
//! (`replace/tests.rs`) and sampled at scale by the property test
//! `streaming_engine_matches_the_reference_at_every_chunk_size`.

use super::{Utf8Carry, validate_utf8};

/// The longest source the seam harness feeds.
const CARRY_SOURCE: usize = 8;
/// The largest chunk the seam harness compares with the whole source.
const MAX_CHUNK: usize = 4;

/// UTF-8 seams: fed in chunks, the carry rejects exactly the sources a
/// one-shot validation rejects.
#[kani::proof]
#[kani::unwind(10)]
fn utf8_seams_do_not_depend_on_the_chunk_size() {
    let bytes: [u8; CARRY_SOURCE] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= CARRY_SOURCE);
    let source = &bytes[..len];
    let chunk: usize = kani::any();
    kani::assume((1..=MAX_CHUNK).contains(&chunk));

    let whole = validate_utf8(source).is_ok();
    let mut carry = Utf8Carry::default();
    let mut split = true;
    let mut offset = 0;
    while offset < len {
        let end = (offset + chunk).min(len);
        if carry.feed(&source[offset..end]).is_err() {
            split = false;
            break;
        }
        offset = end;
    }
    if split {
        split = carry.finish().is_ok();
    }
    assert_eq!(split, whole);

    let cut = chunk < len && source[chunk] & 0xC0 == 0x80;
    kani::cover!(
        whole && cut,
        "a valid multi-byte character is split by a seam"
    );
    kani::cover!(!whole && split == whole, "an invalid source is rejected");
    kani::cover!(
        len > 0 && validate_utf8(source) == Err((len - 1, true)),
        "a source ends inside a sequence"
    );
}

/// The `cfg(kani)` byte scan equals `core::str::from_utf8` on every input of
/// at most four bytes.
#[kani::proof]
#[kani::unwind(6)]
fn the_byte_scan_matches_core_from_utf8() {
    let bytes: [u8; 4] = kani::any();
    let len: usize = kani::any();
    kani::assume(len <= 4);
    let source = &bytes[..len];
    let core = core::str::from_utf8(source)
        .map(|_| ())
        .map_err(|error| (error.valid_up_to(), error.error_len().is_none()));
    assert_eq!(validate_utf8(source), core);
    kani::cover!(
        core.is_ok() && len == 4 && bytes[0] >= 0xF0,
        "a four-byte character"
    );
    kani::cover!(
        core == Err((0, true)) && len == 3,
        "a cut four-byte sequence"
    );
}
