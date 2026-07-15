# Transient File-Load Baseline

This note records the measurements and policy behind LushText's bounded editor
file-load pipeline. The numbers are comparison evidence from one development
host, not release thresholds or RSS guarantees.

## Baseline

Measured on 2026-07-13 with Rust 1.96.0 in the optimized Criterion profile:

```sh
cargo bench -p lushtext-core --bench benchmarks -- \
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
cargo bench -p lushtext-core --bench benchmarks -- transient_file_load \
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
