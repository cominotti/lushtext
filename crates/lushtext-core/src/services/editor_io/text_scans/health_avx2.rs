// SPDX-License-Identifier: GPL-3.0-or-later

//! The AVX2 block kernel of the file-health count
//! ([`super::count_health_characters`]): the one hand-written SIMD kernel the
//! explicit-SIMD allowlist of `.agents/rules/rust.md` admits, and the only
//! file it names.
//!
//! Typographic prose (`’ “ ” — – …`, all `E2 80 xx`) puts a candidate lead byte
//! every few bytes, so the `memchr3` scan stops on each one and confirms it in
//! scalar code. This kernel instead takes 64 lead positions per step and
//! reads `bytes[at .. at + 66]`:
//!
//! 1. an all-ASCII block is skipped;
//! 2. a block with no candidate lead (`C2`, `E2`, `EF`) is skipped, which
//!    covers multibyte text without them (`é`, CJK, emoji);
//! 3. otherwise each 32-byte half is compared with the same bytes loaded at
//!    `+1` and `+2`, every hit pattern becomes one mask bit per lead
//!    position, and `popcnt` adds the bits: no branch per candidate.
//!
//! The last fewer-than-66 bytes go to the `memchr3` scan from the first
//! uncovered lead, which reads continuation bytes from the whole slice. A
//! position's result depends only on `b[j], b[j + 1], b[j + 2]`, exactly as in
//! the `memchr3` match, and each lead position is one mask bit, so for every
//! byte string the kernel returns the `memchr3` scan's counts.
//!
//! Compiled only on x86-64 and outside Kani (`cfg` on the `mod` item). The
//! shipped binary stays baseline x86-64: [`count`] detects AVX2 and POPCNT at
//! run time (the standard library caches the answer) and returns `None`
//! without them, and the caller then runs the `memchr3` scan. Verification:
//! the exhaustive lane test below (every byte triple at every seam shape), the
//! property tests in `tests/properties/text_scans.rs`, the `editor_bytes` fuzz
//! target, and the counting-allocator test in `tests/text_scan_allocations.rs`.
//! Measured in `docs/benchmarks/transient-file-load.md`, "AVX2 file-health
//! kernel".

use std::arch::x86_64::{
    __m256i, _mm256_and_si256, _mm256_cmpeq_epi8, _mm256_loadu_si256, _mm256_min_epu8,
    _mm256_movemask_epi8, _mm256_or_si256, _mm256_set1_epi8, _mm256_sub_epi8,
};

/// Lead positions one step covers.
const BLOCK: usize = 64;

/// Bytes one step reads from its start: the 64 lead positions and the two
/// continuation bytes of the last one.
const BLOCK_READ: usize = BLOCK + 2;

/// Lead positions of one vector.
const HALF: usize = 32;

/// The kernel's counts over `bytes`, or `None` when the CPU lacks AVX2 or
/// POPCNT (the caller then runs the `memchr3` scan).
#[expect(
    clippy::disallowed_macros,
    reason = "the explicit-SIMD allowlist row for text_scans/health_avx2.rs \
              (.agents/rules/rust.md, Explicit-SIMD kernel allowlist) admits \
              this dispatch"
)]
#[must_use]
pub(super) fn count(bytes: &[u8]) -> Option<(usize, usize)> {
    if std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("popcnt") {
        // SAFETY: `count_blocks` enables exactly `avx2` and `popcnt`, and the
        // running CPU reported both just above.
        Some(unsafe { count_blocks(bytes) })
    } else {
        None
    }
}

/// `v` broadcast to every lane.
#[inline]
#[target_feature(enable = "avx2")]
fn splat(byte: u8) -> __m256i {
    _mm256_set1_epi8(i8::from_ne_bytes([byte]))
}

/// The lanes of `v` equal to `byte`.
#[inline]
#[target_feature(enable = "avx2")]
fn lanes_equal(v: __m256i, byte: u8) -> __m256i {
    _mm256_cmpeq_epi8(v, splat(byte))
}

/// One bit per lane: the lane's top bit.
#[inline]
#[target_feature(enable = "avx2")]
fn bits(v: __m256i) -> u32 {
    _mm256_movemask_epi8(v).cast_unsigned()
}

/// The candidate lead bytes (`C2`, `E2`, `EF`) among 32 lead positions.
#[inline]
#[target_feature(enable = "avx2")]
fn leads(v0: __m256i) -> __m256i {
    _mm256_or_si256(
        _mm256_or_si256(lanes_equal(v0, 0xC2), lanes_equal(v0, 0xE2)),
        lanes_equal(v0, 0xEF),
    )
}

/// The non-breaking-space and zero-width hit masks of the 32 lead positions
/// starting at `bytes[lead]`, whose bytes are `v0`.
///
/// # Safety
///
/// `lead + HALF + 2 <= bytes.len()`: the `+1` and `+2` loads read
/// `bytes[lead + 1 .. lead + 33]` and `bytes[lead + 2 .. lead + 34]`.
#[inline]
#[target_feature(enable = "avx2")]
unsafe fn hit_masks(bytes: &[u8], lead: usize, v0: __m256i) -> (u32, u32) {
    debug_assert!(lead + HALF + 2 <= bytes.len());
    // SAFETY: by this function's contract both 32-byte loads end at or before
    // `bytes.len()`; `loadu` has no alignment requirement.
    let (v1, v2) = unsafe {
        let start = bytes.as_ptr().add(lead);
        (
            _mm256_loadu_si256(start.add(1).cast()),
            _mm256_loadu_si256(start.add(2).cast()),
        )
    };
    let nbsp = _mm256_and_si256(lanes_equal(v0, 0xC2), lanes_equal(v1, 0xA0));
    let e2 = lanes_equal(v0, 0xE2);
    // `v2 - 8B <= 2` unsigned, i.e. `v2` in `8B..=8D` (U+200B/C/D).
    let third_offset = _mm256_sub_epi8(v2, splat(0x8B));
    let third_in_range = _mm256_cmpeq_epi8(_mm256_min_epu8(third_offset, splat(2)), third_offset);
    let zero_width_200x =
        _mm256_and_si256(e2, _mm256_and_si256(lanes_equal(v1, 0x80), third_in_range));
    let zero_width_2060 = _mm256_and_si256(
        e2,
        _mm256_and_si256(lanes_equal(v1, 0x81), lanes_equal(v2, 0xA0)),
    );
    let zero_width_feff = _mm256_and_si256(
        lanes_equal(v0, 0xEF),
        _mm256_and_si256(lanes_equal(v1, 0xBB), lanes_equal(v2, 0xBF)),
    );
    let zero_width = _mm256_or_si256(
        zero_width_200x,
        _mm256_or_si256(zero_width_2060, zero_width_feff),
    );
    (bits(nbsp), bits(zero_width))
}

/// The kernel: whole 64-position steps, then the `memchr3` scan for the rest.
#[target_feature(enable = "avx2,popcnt")]
fn count_blocks(bytes: &[u8]) -> (usize, usize) {
    let mut nbsp_count = 0u64;
    let mut zero_width_count = 0u64;
    let mut at = 0usize;
    while at + BLOCK_READ <= bytes.len() {
        // SAFETY: `at + 66 <= bytes.len()`, so both 32-byte loads
        // (`bytes[at .. at + 64]`) are in bounds; `loadu` has no alignment
        // requirement.
        let (low, high) = unsafe {
            let start = bytes.as_ptr().add(at);
            (
                _mm256_loadu_si256(start.cast()),
                _mm256_loadu_si256(start.add(HALF).cast()),
            )
        };
        let non_ascii = bits(_mm256_or_si256(low, high)) != 0;
        if non_ascii && bits(_mm256_or_si256(leads(low), leads(high))) != 0 {
            // SAFETY: `at + HALF + 2 <= at + 66 <= bytes.len()`, and
            // `at + 2 * HALF + 2 = at + 66 <= bytes.len()`: each call's
            // contract holds.
            let ((nbsp_low, zero_width_low), (nbsp_high, zero_width_high)) =
                unsafe { (hit_masks(bytes, at, low), hit_masks(bytes, at + HALF, high)) };
            nbsp_count += u64::from(nbsp_low.count_ones() + nbsp_high.count_ones());
            zero_width_count +=
                u64::from(zero_width_low.count_ones() + zero_width_high.count_ones());
        }
        at += BLOCK;
    }
    let (nbsp_tail, zero_width_tail) = super::count_health_characters_memchr(bytes, at);
    (
        usize::try_from(nbsp_count).expect("x86-64 usize holds a u64") + nbsp_tail,
        usize::try_from(zero_width_count).expect("x86-64 usize holds a u64") + zero_width_tail,
    )
}

#[cfg(test)]
mod tests {
    use super::{BLOCK, count};
    use crate::services::editor_io::text_scans::count_health_characters_memchr;

    /// ASCII ground the triples are placed on: it never starts or continues a
    /// hit, so placements a byte apart cannot interact.
    const GROUND: u8 = b'a';

    /// The triple's bytes.
    fn triple(index: u32) -> [u8; 3] {
        let [_, b0, b1, b2] = index.to_be_bytes();
        [b0, b1, b2]
    }

    /// The `memchr3` counts of the triple alone on the ASCII ground.
    fn own_counts(bytes: [u8; 3]) -> (usize, usize) {
        count_health_characters_memchr(&[bytes[0], bytes[1], bytes[2], GROUND], 0)
    }

    /// Lead positions of the single placements in a 130-byte buffer: the first
    /// position, the last two of each half and the first of the high half
    /// (30/62 put only the `+2` byte across a half or block seam, 31/63 both
    /// continuation bytes), the next block's first, and the last three bytes
    /// (lead 127 is the last lead the block loop covers; the tail starts at
    /// 128).
    const SINGLE_LEADS: [usize; 8] = [0, 30, 31, 32, 62, 63, 64, 127];
    const SINGLE_LEN: usize = 130;

    /// Batched layouts: per 64-byte block, two non-overlapping lead offsets
    /// that together cover every seam shape of `SINGLE_LEADS` (64 is the next
    /// block's 0). A triple at 62 or 63 spills into the next block's first
    /// bytes, which no layout with a 62 or 63 uses.
    const BATCH_LAYOUTS: [[usize; 2]; 3] = [[0, 31], [30, 62], [32, 63]];

    /// Blocks per batched buffer.
    const BATCH_BLOCKS: usize = 4096;

    fn kernel(bytes: &[u8]) -> (usize, usize) {
        count(bytes).expect("checked by the caller")
    }

    /// Every triple whose own count is zero, at both offsets of `layout`, in
    /// consecutive blocks; each buffer must count zero. Returns how many
    /// triples were checked.
    fn check_batched(layout: [usize; 2], triples: std::ops::Range<u32>) -> usize {
        let len = BATCH_BLOCKS * BLOCK + 2;
        let mut buffer = vec![GROUND; len];
        let mut placed: Vec<u32> = Vec::with_capacity(BATCH_BLOCKS);
        let mut checked = 0usize;
        let flush = |buffer: &mut Vec<u8>, placed: &mut Vec<u32>| {
            assert_eq!(
                kernel(buffer),
                (0, 0),
                "layout {layout:?}: a false positive among triples {:06X}..={:06X}",
                placed.first().copied().unwrap_or(0),
                placed.last().copied().unwrap_or(0),
            );
            buffer.fill(GROUND);
            placed.clear();
        };
        for index in triples {
            let bytes = triple(index);
            if own_counts(bytes) != (0, 0) {
                continue;
            }
            let block = placed.len() * BLOCK;
            for offset in layout {
                buffer[block + offset..block + offset + 3].copy_from_slice(&bytes);
            }
            placed.push(index);
            checked += 1;
            if placed.len() == BATCH_BLOCKS {
                flush(&mut buffer, &mut placed);
            }
        }
        if !placed.is_empty() {
            flush(&mut buffer, &mut placed);
        }
        checked
    }

    /// Every byte triple at every seam shape: the kernel equals the `memchr3`
    /// scan. Triples with a zero own count are batched (a batch expects zero,
    /// so a false positive shows and no false negative can cancel it); the
    /// others, the hits and the triples ending in `C2 A0`, are checked one
    /// per buffer at every single-placement lead.
    #[test]
    fn kernel_equals_memchr_for_every_triple_at_every_seam() {
        if count(&[]).is_none() {
            eprintln!("skipped: this CPU lacks AVX2 or POPCNT, so the kernel never runs here");
            return;
        }
        const TRIPLES: u32 = 1 << 24;
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let chunk = TRIPLES.div_ceil(u32::try_from(threads).expect("few threads"));
        let mut batched = 0usize;
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..TRIPLES)
                .step_by(usize::try_from(chunk).expect("u32 fits usize"))
                .map(|start| {
                    let range = start..start.saturating_add(chunk).min(TRIPLES);
                    scope.spawn(move || {
                        BATCH_LAYOUTS
                            .into_iter()
                            .map(|layout| check_batched(layout, range.clone()))
                            .sum::<usize>()
                    })
                })
                .collect();
            for worker in workers {
                batched += worker.join().expect("a lane worker panicked");
            }
        });

        let mut single = 0usize;
        for index in 0..TRIPLES {
            let bytes = triple(index);
            if own_counts(bytes) == (0, 0) {
                continue;
            }
            for lead in SINGLE_LEADS {
                let mut buffer = [GROUND; SINGLE_LEN];
                buffer[lead..lead + 3].copy_from_slice(&bytes);
                assert_eq!(
                    kernel(&buffer),
                    count_health_characters_memchr(&buffer, 0),
                    "triple {bytes:02X?} at lead {lead}"
                );
            }
            single += 1;
        }
        // Every triple is checked one way or the other, each zero-count
        // triple under all three layouts.
        assert_eq!(batched + 3 * single, 3 * (1 << 24));
        assert!(single > 0 && single < 1024, "{single} non-zero triples");
    }

    /// The kernel on a few whole texts, beside the shared reference check.
    #[test]
    fn kernel_counts_typographic_and_seam_texts() {
        if count(&[]).is_none() {
            eprintln!("skipped: this CPU lacks AVX2 or POPCNT");
            return;
        }
        let typographic =
            "\u{201C}It\u{2019}s\u{2026}\u{201D} \u{2014} \u{200B}\u{00A0}\u{FEFF}".repeat(40);
        assert_eq!(
            kernel(typographic.as_bytes()),
            count_health_characters_memchr(typographic.as_bytes(), 0)
        );
        assert_eq!(kernel(typographic.as_bytes()), (40, 80));
        // A hit whose lead is the last byte the block loop covers is counted
        // once, and one whose lead starts the tail is counted by the tail.
        for len in 66..200 {
            let mut text = vec![GROUND; len];
            text[len - 3..].copy_from_slice("\u{FEFF}".as_bytes());
            assert_eq!(kernel(&text), (0, 1), "len {len}");
            let mut text = vec![GROUND; len];
            text[len - 2..].copy_from_slice("\u{00A0}".as_bytes());
            assert_eq!(kernel(&text), (1, 0), "len {len}");
        }
    }
}
