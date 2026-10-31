# Transient File-Load Baseline

This note records the measurements and policy behind LushText's bounded editor
file-load pipeline. The numbers are comparison evidence from one development
host, not release thresholds or RSS guarantees.

## Baseline

Measured on 2026-07-13 with Rust 1.96.0 in the optimized Criterion profile:

```sh
cargo bench -p lushtext-core --features test-utils --bench benchmarks -- \
  'editor_file_io/load_text_file/' \
  --sample-size 10 --warm-up-time 1 --measurement-time 1
```

| UTF-8 fixture | Median estimate | Measured interval |
| --- | ---: | ---: |
| 1 MiB | 737.37 us | 715.68-758.07 us |
| 10 MiB | 17.749 ms | 17.240-18.216 ms |
| 50 MiB | 74.333 ms | 72.785-75.821 ms |

The corresponding direct `GtkTextBuffer::set_text()` diagnostic, run under the
headless widget harness in a debug build, took 16,233 us for 1 MiB and 239,183
us for 16 MiB. Those timings motivated a small synchronous threshold rather
than treating worker-side decoding as the only latency risk.

The dedicated policy benchmark is:

```sh
cargo bench -p lushtext-core --features test-utils --bench benchmarks -- transient_file_load \
  --sample-size 10 --warm-up-time 1 --measurement-time 1
```

Its initial baseline was 97.300 us for 512 small admissions, 207.29 ns for an
eight-request large-load cycle, 32.061 ns for one exclusive near-limit request,
173.96 us for a 1,024-entry stale queue, and 368.66 ns for planning Unicode-safe
slices across 50 MiB. Criterion measures the plain policy and boundary planner;
the headless widget proof measures actual GTK progress.

## Ownership Phases

1. Planning reads compact file facts, rejects an already-unsupported size, and
   computes a scalar transient charge. It retains no document bytes.
2. Admission keeps weak page ownership plus the compact plan. A process-wide
   byte policy grants a `TransientLoadPermit` before any payload worker starts.
3. Ingestion streams at most the admitted plan's byte size into one raw byte
   buffer, probes one non-retained sentinel byte, revalidates path identity and
   file facts, then decodes and classifies the payload. Any growth beyond the
   admitted size stops before it can bypass the plan's transient charge.
4. Installation owns the decoded string and permit while GTK removes old text,
   inserts at most 256 KiB of UTF-8-safe new text, or clears cancelled partial
   text in bounded main-loop turns. Buffer-amplifying projections remain
   suspended until exact finalization or cancellation cleanup.
5. Finalization publishes file metadata, encoding and health state, restore
   position, history seed, monitor state, and one memory-policy update before
   the page becomes `Loaded`. Cancellation clears partial text and drops the
   permit without publishing incomplete state.

The shared charge is `1 MiB + 8 * planned source bytes`, saturating at `u64::MAX`.
The 256 MiB shared budget matches the live-editor upper budget. Its multiplier
covers raw input, a legacy single-byte encoding expanding to three UTF-8 bytes,
the live editor's four-byte-per-character estimate, and allocator slack while
decoded ownership overlaps the growing GTK representation.
Supported requests whose conservative charge exceeds the shared budget run
alone instead of being rejected. The policy also limits admitted jobs to the
generic worker cap, but byte weight, not a fixed concurrency count, is the
normal admission constraint.

Selected tabs may bypass an older request at most twice consecutively. An older
capacity-blocked request prevents later small work from starving it. When
non-evictable editor residency already exceeds the live-memory budget, only one
payload is admitted at a time so restore can still progress without multiplying
pressure. The coordinator is shared across every LushText window in the process.

## Reproducible Evidence

`make performance-smoke` now includes `transient_file_load`. Its summary records:

- active payload weight, queued scalar count, and permit high-water weight from
  deterministic admission fixtures;
- actual installation slice count, main-loop progress, and final editor
  residency from a headless large-Unicode widget load;
- Criterion timings for many-small, concurrent-large, exclusive near-limit,
  stale-queue, and Unicode boundary-planning cases.

These fields are bounded implementation evidence. They deliberately do not
pretend to be exact allocator or resident-set measurements, and the smoke lane
does not impose a brittle hard RSS threshold.

## Paragraph-boundary slicing (document-scale-policy, 2026-10-29)

Installation slices now end at GTK paragraph delimiters
(`model/paragraph_boundary.rs`), and the load analysis builds a paragraph
profile on the worker. Interleaved against main (`1e2556a2`, three rounds per
side): `install_boundaries/unicode_50_mib` 2.26 µs on main against 3.77 µs;
the new single-line cases `single_line_ascii_50_mib` 4.83 against 4.09 ms and
`single_line_typographic_50_mib` 4.90 against 5.13 ms (one slice of a 50 MiB
paragraph, the slicer's worst call on the GTK thread, under B_interact).
`editor_file_io/load_text_file` carries a recorded residual of +4–5 % at
10–50 MB and +14 % (about 90 µs) at 1 MB: worker-side, off the GTK thread,
from the extra profile pass, to be removed by fusing the profile with the
file-health scan in the queued `simd-line-scans` change. The full table and
its history are in `docs/next/long-line-documents.md` (task 2.5). **Closed by
`simd-line-scans`** (below): the residual is gone, and the branch loads faster
than pre-C1 at 1, 10 and 50 MB.

## Text scans (`simd-line-scans`, 2026-10-30)

The load's file-health count, the save rewrites, and the minimap
accumulator's slice entry point moved from per-character loops to `memchr`
skips (`services/editor_io/text_scans.rs`, `MinimapAnalysisAccumulator::inspect_str`).
Interleaved Criterion verdict: pre-C1 `1e2556a2`, main `0439d4c4` plus the
bench-first commit, and the branch, alternating, three rounds per side, every
side built with toolchain 1.98.1 (`RUSTUP_TOOLCHAIN=1.98.1`, `bench` profile),
each run under the exclusive measurement lock, CPUs 8–23, load average
1.2–3.0 (measured under maintainer-accepted load; `btrfs-stall-report`:
storage was not a bottleneck, no stall episode). Figures are the median of the
three round medians, then the min–max of those medians.

| Benchmark | pre-C1 `1e2556a2` | main `0439d4c4` | branch | branch vs main | Bar | Result |
|---|---|---|---|---|---|---|
| `editor_file_io/load_text_file/1MB` | 655 µs (642–656) | 698 µs (689–710) | 295 µs (293–296) | 2.36× | ≤ pre-C1 | met |
| `editor_file_io/load_text_file/10MB` | 8.00 ms (7.96–8.07) | 8.41 ms (8.08–8.58) | 3.99 ms (3.95–4.11) | 2.11× | ≤ pre-C1 | met |
| `editor_file_io/load_text_file/50MB` | 64.0 ms (61.4–64.2) | 64.1 ms (62.2–67.1) | 42.4 ms (40.8–44.4) | 1.51× | ≤ pre-C1 | met |
| `text_scans/health/ascii_50MiB` | | 22.90 ms (22.87–22.99) | 4.06 ms (3.75–4.12) | 5.64× | ≥ 3× | met |
| `text_scans/health/multibyte_50MiB` | | 139.4 ms (139.3–139.4) | 4.14 ms (4.05–4.19) | 33.7× | | |
| `text_scans/health/mixed_eol_50MiB` | | 22.88 ms (22.86–22.90) | 3.90 ms (3.87–3.92) | 5.87× | | |
| `text_scans/health/long_line` (1.9M chars) | | 1.78 ms (1.78–1.79) | 93.8 µs (92.9–94.0) | 19.0× | | |
| `text_scans/health/typographic_50MiB` | | 52.06 ms (52.06–52.23) | 27.78 ms (27.77–27.82) | 1.87× | no regression | met |
| `text_scans/load_analysis/ascii_50MiB` | | 46.85 ms (46.79–47.31) | 25.35 ms (25.29–25.38) | 1.85× | | |
| `text_scans/load_analysis/multibyte_50MiB` | | 153.5 ms (153.5–153.9) | 17.54 ms (17.38–17.71) | 8.75× | | |
| `text_scans/load_analysis/mixed_eol_50MiB` | | 102.2 ms (102.1–102.5) | 80.51 ms (80.34–80.66) | 1.27× | | |
| `text_scans/load_analysis/typographic_50MiB` | | 120.9 ms (120.7–121.1) | 97.75 ms (97.56–97.95) | 1.24× | | |
| `text_scans/load_analysis/long_line` | | 1.92 ms (1.92–1.93) | 240.1 µs (239.9–240.3) | 8.02× | | |
| `text_scans/minimap/many_short_lines` (`inspect_slice`) | | 105.9 ms (105.9–106.7) | 106.0 ms (105.7–106.5) | 1.00× | | |
| `text_scans/minimap/many_short_lines_str` (`inspect_str`) | | | 10.97 ms (10.86–10.98) | 9.67× over `inspect_slice` | ≥ 5× | met |
| `text_scans/minimap/long_line` (`inspect_slice`) | | 3.60 ms (3.11–3.69) | 3.12 ms (3.12–3.21) | 1.15× | | |
| `text_scans/minimap/long_line_str` (`inspect_str`) | | | 170.1 µs (170.0–170.2) | 18.3× over `inspect_slice` | | |
| `text_scans/save/trim_dirty` | | 175.3 ms (175.0–175.4) | 41.82 ms (40.69–42.04) | 4.19× | ≥ 3× | met |
| `text_scans/save/trim_clean` | | 174.6 ms (174.0–174.9) | 21.01 ms (20.89–21.36) | 8.31× | (allocation-free by test) | |
| `text_scans/save/trim_single_line` | | 3.26 ms (2.26–3.30) | 59.4 µs (58.9–60.0) | 54.9× | | |
| `text_scans/save/normalize_crlf` | | 92.56 ms (92.46–94.54) | 37.92 ms (37.23–37.99) | 2.44× | ≥ 1.5× | met |
| `text_scans/save/normalize_mixed` | | 96.32 ms (95.93–97.46) | 44.20 ms (43.92–45.00) | 2.18× | ≥ 1.5× | met |
| `text_scans/save/crlf_check_dense` | | 25.51 ms (25.50–25.51) | 25.59 ms (25.57–25.64) | 0.997× | ≥ 0.95× | met (parity, outside main's 0.01 ms spread) |

Variant B of the load analysis (the three scanners fused over 64 KiB
sub-blocks, `load_analysis_fused/*`, measured from the same branch binary and
removed after the verdict) was **declined**: it was no
faster than variant A at any fixture (ascii 25.46 against 25.35 ms, multibyte
17.62 against 17.54, typographic 97.93 against 97.75, mixed EOL 80.76 against
80.51, long line 241.2 against 240.1 µs), short of D3's 3 % bar. Every item
met its bar and landed. C1's load residual is closed: `load_text_file` is now
below the pre-C1 medians at every size.

Explicit-SIMD gate (D6, task 4.3): on the typographic fixture the health scan
is 27.79 ms of a 143.78 ms `load_text_file` (`text_scans/load_text_file/typographic_50MiB`,
three rounds, branch only), **19.3 %**, above the 10 % threshold. The
allowlist stays empty in this change; the AVX2 kernel is queued as a follow-up
proposal (see `docs/next/long-line-documents.md`).
