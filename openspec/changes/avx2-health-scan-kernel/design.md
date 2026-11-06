## Context

`analyze_decoded_content_cancellable` (`services/editor_io.rs`) walks each
decoded document in 256 KiB processing slices (one slice up to 1 MiB) and runs
three scans per slice: the line-ending tally (`memchr2`), the paragraph profile,
and the file-health count `text_scans::count_health_characters`. The health
count finds `C2`/`E2`/`EF` with `memchr3` and confirms U+00A0 (`C2 A0`),
U+200B/C/D (`E2 80 8B..8D`), U+2060 (`E2 81 A0`) and U+FEFF (`EF BB BF`) from
the next one or two bytes. Its scalar oracle is `text_scans::reference`.

On ASCII, multibyte and long-line text `memchr3` rarely stops, and the scan is
3–4 GB/s or more. Typographic prose (`’ “ ” — – …`, all `E2 80 xx`) puts a
candidate every few bytes. `memchr3` then stops on each candidate, restarts its
vector loop and confirms in scalar code, and the scan falls to about 1.9 GB/s.
`simd-line-scans` task 4.3 measured it at 19.3 % of `load_text_file` there,
above the 10 % gate of the "Explicit SIMD" policy (`.agents/rules/rust.md`). The
SIMD exploration (H1) had measured a branchy 64-byte AVX2 kernel at 2.8x against
`memchr3`'s 1.7x over the scalar loop on such text.

The load runs on a worker, never on the GTK thread, so this change affects
time-to-open and not frame budgets. B_interact and B_settle are untouched.

## Goals / Non-Goals

**Goals:**
- Make typographic loads measurably faster with exact, unchanged health counts.
- Lose nothing anywhere else: ASCII, multibyte, mixed-EOL and long-line text,
  CPUs without AVX2, and aarch64.
- Meet every condition of the explicit-SIMD policy (D6 of `simd-line-scans`),
  with one allowlisted file.

**Non-Goals:**
- A NEON kernel. There is no aarch64 host to measure one on, and SWAR is not a
  substitute (it regressed to 0.88x in the exploration). aarch64 keeps
  `memchr`'s NEON `memchr3`.
- Vectorizing the line-ending tally or the paragraph profile. They are not
  measured hot under this gate, and the fused form was declined in
  `simd-line-scans` D3.
- AVX-512. It is not available on the measurement host, and a second kernel
  needs its own measured need.

## Decisions

### D1. Kernel shape: two skips, then confirmation in vector registers

Each step covers lead positions `[at, at + 64)` and reads `bytes[at .. at + 66)`:

1. Load the two 32-byte halves `low` and `high`. If `movemask(low | high) == 0`
   the block is ASCII, so skip it.
2. Compute `leads(v) = (v == C2) | (v == E2) | (v == EF)` for both halves. If
   neither half has a candidate lead, skip the block. This covers multibyte
   text without these leads, such as `é` (`C3`), CJK and emoji.
3. Otherwise, per half, load `v1 = bytes[p+1..]` and `v2 = bytes[p+2..]` and
   form
   `nbsp = (v0==C2) & (v1==A0)`,
   `zw = (v0==E2) & (v1==80) & (v2-8B ≤u 2)` |
   `(v0==E2) & (v1==81) & (v2==A0)` |
   `(v0==EF) & (v1==BB) & (v2==BF)`.
   The unsigned range test is `min_epu8(v2 - 8B, 2) == v2 - 8B`. Add
   `popcnt(movemask(·))` of each mask to the counts.
4. When fewer than 66 bytes remain, run the `memchr3` scan from `at` on the
   whole slice. Its `get(at+1)`/`get(at+2)` confirmation then covers leads
   whose continuation bytes are the last in the slice.

Each mask bit stands for one lead position, so a scalar is counted exactly once.
A position's result depends only on the three bytes `b[j], b[j+1], b[j+2]`,
exactly as in the `memchr3` match. The kernel is defined on bytes, and for
every byte string it returns the `memchr3` function's result. On a `&str` that
result equals the oracle's.

**Rationale (measured).** Prototype v1 had only the ASCII skip. Round 1 of its
interleaved run measured typographic 5.3x faster, but **multibyte 0.76x and
long line 0.56x**: in those texts every block is non-ASCII, so v1 paid six loads
and about fifteen vector operations per 32 bytes where `memchr3` skips. Adding
the lead skip (v2) brought them to 0.98x and 1.28x. The lead test costs about
the same as `memchr3`'s own inner loop. Raw logs are in
`.claude/handoffs/avx2-health-scan-kernel/runs-v1/` (v1) and `runs/` (v2).

### D2. One owner: `text_scans/health_avx2.rs`, which also holds the dispatch

The kernel, its `#[target_feature(enable = "avx2,popcnt")]` functions, the
`unsafe` loads and the feature detection all live in
`services/editor_io/text_scans/health_avx2.rs`. The module is declared
`#[cfg(all(target_arch = "x86_64", not(kani)))] mod health_avx2;`. It exposes
one safe function, `count(bytes) -> Option<(usize, usize)>`, which returns
`None` when the CPU lacks AVX2 or POPCNT. `count_health_characters` in
`text_scans.rs` calls it and otherwise runs the fallback. As a result:

- the allowlist row names exactly one file, and the gate keeps `text_scans.rs`
  itself free of SIMD (a row exempts its whole file);
- the `#[expect(clippy::disallowed_macros, reason = "...")]` site is in that file
  and cites the row;
- no `unsafe` leaves the module: its one `unsafe` call (into the
  `target_feature` function, justified by the detection) carries a `SAFETY`
  comment, and every unaligned load carries the bounds argument
  `at + 66 <= len`.

POPCNT is detected together with AVX2. Every AVX2 CPU has it, but Rust's feature
graph does not imply it, and enabling it makes `count_ones` one instruction.
`is_x86_feature_detected!` caches its result in the standard library's atomic
feature cache, which satisfies D6's "cached once" without a second cache. The
cost of the check is measured inside `text_scans/health/*`, which calls it once
per slice.

Tests, fuzzing and benches reach the kernel without a detection macro of
their own (which the gate would reject outside the allowlisted file):
`text_scans.rs` exposes `count_health_characters_kernel(slice: &str) ->
Option<(usize, usize)>` under `cfg(any(test, feature = "test-utils", feature =
"fuzzing"))`, which forwards to `health_avx2::count` on x86-64 outside Kani and
returns `None` everywhere else. The lane test skips through the same `None`, so
the file has exactly one detection site.

Alternatives considered: the `multiversion` crate (a new dependency and an
allowlist dispatch-crate row for one function: rejected), and the kernel inline
in `text_scans.rs` (which would exempt the save rewrites' file from the gate:
rejected).

### D3. The fallback is today's `memchr3` scan, named and measured

The current body becomes `count_health_characters_memchr(bytes, from)` in
`text_scans.rs`, unchanged apart from the starting offset. It is the
baseline-x86-64 path, the aarch64 path, the `kani` path, and the kernel's tail.
A `test-utils` hook (`bench_health_scan_fallback`) and the Criterion cases
`text_scans/health_fallback/*` measure it with the kernel bypassed. That is
D6's "fallback measured with detection forced off", and it needs no environment
switch in production.

### D4. NEON: decided, unmeasured, library path

aarch64 compiles only the fallback. `memchr3` there is `memchr`'s NEON
implementation. The allowlist row records "NEON: unmeasured (no aarch64 host);
aarch64 uses memchr's NEON `memchr3`".

### D5. Verification

- **Exhaustive lane test** (unit, `health_avx2.rs`, x86-64 only, skipped with a
  printed reason when the host lacks AVX2). For every byte triple
  `(b0, b1, b2)` (2^24), place it in an ASCII buffer at lead positions 0, 30,
  31, 32, 62, 63 and 64 of a 130-byte buffer, and at the last three bytes, and
  compare the kernel with `count_health_characters_memchr` on the same bytes.
  Positions 30/62 put only the `+2` byte across a half or block seam and
  31/63 both continuation bytes, so every seam shape is covered. This checks
  the whole lane predicate, both halves, the block seam and the tail seam. To
  keep runtime bounded, triples are batched several to a buffer at
  non-interacting offsets, but only placements whose own `memchr3` count is
  `(0, 0)`: a batch then expects zero, so any false positive shows and none can
  be cancelled by a false negative. Placements with a non-zero count (the
  hit triples, and triples whose second and third bytes start a hit) are
  checked one per buffer. Its budget is measured and recorded, with a target
  under 10 s in debug.
- **Property tests** (`tests/properties/text_scans.rs`): the existing
  every-slicing test now runs through the dispatching entry. A new test
  compares the kernel (when available), the fallback and the reference on
  texts of at least 66 bytes with hits and look-alikes placed at offsets
  around 32/64-byte seams, with slices cut so that every tail length 0–65 occurs
  (chunk-boundary splits).
- **Fuzz** (`fuzzing.rs` → `reference::assert_scans_match`): also compare the
  fallback and the kernel separately with the reference on every decodable
  input. New seeds `text_scans_health_block_seams.txt` (hits straddling bytes
  31–33 and 63–65, and a 65-byte tail) and
  `text_scans_health_typographic_run.txt` (long `E2 80 xx` runs with sparse
  real hits). `make fuzz-corpus-replay` plus a bounded `make fuzz-smoke`.
- **Allocation**: `tests/text_scan_allocations.rs` asserts that
  `count_health_characters` allocates nothing on a 1 MiB typographic text.
- **Kani**: no new harness. Kani does not model x86 SIMD intrinsics, the kernel
  is behind `cfg(not(kani))`, and the kernel's only non-library logic, the
  per-lane predicate, is checked exhaustively by the lane test over its finite
  domain, which is a complete check. The scalar half is `memchr` internals plus
  a `match` that the existing byte-table unit test already checks over every
  scalar value. No Kani build reaches `text_scans` today, so no shard needs a
  rerun. `make check-kani-shards` confirms that the shard table is unchanged.
- **Mutation** (`cargo mutants`): kernel mutants are killed by the lane and
  property tests on AVX2 hosts (CI's x86-64 runners have AVX2). A mutant of the
  availability check (returning `None`) gives results identical to the
  original by construction, because both paths return the same counts, but it
  silently drops the speed-up, and every kernel test would skip. So it is not
  excluded: `kernel_is_selected_on_every_cpu_with_avx2_and_popcnt` asserts that
  `count` returns `Some` exactly when the CPU reports AVX2 and POPCNT, and
  kills it (review decision R2 below). Two further mutant classes are equivalent by construction and are expected
  to survive: a detection conjunction turned into a disjunction (identical on
  any host that has both features, as every AVX2 host has POPCNT), and a skip
  predicate weakened so that more blocks are confirmed (`&&` to `||` between
  the two skips, or a skip removed): the skips only save work, and confirming
  a block that would have been skipped adds zero. Each gets a narrow
  `exclude_re` with that reason. A mutant that skips a block it should
  confirm changes counts and must be killed. Any other survivor is fixed or
  justified in `.cargo/mutants.toml`.
- **Unsafe hygiene**: Miri is attempted on the lane test (Miri implements most
  AVX2 intrinsics). Miri's `is_x86_feature_detected!` reports only the target
  features the crate was compiled with, so that local run passes
  `-Ctarget-feature=+avx2,+popcnt` through `MIRIFLAGS`/`RUSTFLAGS` on the
  command line only (no configuration file changes, so the build-flag gate is
  untouched); without it the kernel would be skipped and Miri would check
  nothing. If it is unsupported, that is recorded, and the bounds
  argument rests on the `SAFETY` comments, the tail tests and the property
  tests with lengths 0–200.

### D6. Pre-registered bar and measured baseline

The bar was written down before measuring
(`.claude/handoffs/avx2-health-scan-kernel/bar.md`, 2026-11-05):

- **B1** `text_scans/health/typographic_50MiB` at most 0.50x of A;
- **B2** `text_scans/load_text_file/typographic_50MiB` at most 0.95x of A, with
  the health share under 10 %;
- **B3** `editor_file_io/load_text_file_typographic/{10,50}MB` at most 0.95x of A,
  1 MB no worse than 0.97x;
- **B4 (no loss)** ASCII `editor_file_io/load_text_file/{1,10,50}MB` and
  `text_scans/health/{ascii,multibyte,mixed_eol,long_line}` at most A plus 3 %,
  and also at most A's figure plus A's round spread (the spec's no-regression
  rule);
- **B5** the fallback within 3 % of A's health figures.

This refines the task's wording on purpose. On ASCII input the bar asks for
**no regression, not improvement**: on bytes below `0x80`, `memchr3` already
runs `memchr`'s AVX2 loop, and the kernel takes the same all-ASCII skip, so no
kernel can make the ASCII load measurably faster. (It did measure 1.005–1.04x,
inside noise.)

**Measured** (spec stage, prototype v2). A = `origin/main` `fbf0f875` plus the
bench-first patch; B = A plus the kernel. Toolchain 1.98.1, `bench` profile,
Intel Core Ultra 9 285K, `taskset -c 8-23`, each run under
`scripts/measure.sh` (exclusive locks). Rounds alternated A, B three times. Load
at each start was 3.96, 3.56, 3.71, 2.64, 2.22, 1.77 (1-min). Round 1 A ended at
4.91 because of outside load, and its figures are in line with rounds 2–3.
Median of round medians (min–max):

| Benchmark | A | B | A/B | Bar |
|---|---|---|---|---|
| `text_scans/health/typographic_50MiB` | 27.80 ms (27.79–28.19) | 6.10 ms (6.04–6.34) | 4.56x | B1 met |
| `text_scans/load_text_file/typographic_50MiB` | 149.6 ms (149.5–149.7) | 127.3 ms (126.5–127.8) | 1.175x; share 4.8 % | B2 met |
| `editor_file_io/load_text_file_typographic/1MB` | 2.195 ms (2.178–2.224) | 1.778 ms (1.766–1.782) | 1.235x | B3 met |
| `editor_file_io/load_text_file_typographic/10MB` | 23.84 ms (23.82–24.10) | 19.64 ms (19.63–19.89) | 1.214x | B3 met |
| `editor_file_io/load_text_file_typographic/50MB` | 140.3 ms (139.0–143.9) | 120.4 ms (120.2–123.6) | 1.166x | B3 met |
| `editor_file_io/load_text_file/1MB` (ASCII) | 299.2 µs (290.0–301.1) | 287.6 µs (280.5–291.1) | 1.040x | B4 met |
| `editor_file_io/load_text_file/10MB` (ASCII) | 4.537 ms (4.525–4.719) | 4.373 ms (4.202–4.582) | 1.037x | B4 met |
| `editor_file_io/load_text_file/50MB` (ASCII) | 48.28 ms (47.74–48.75) | 48.05 ms (47.94–48.40) | 1.005x | B4 met |
| `text_scans/health/ascii_50MiB` | 4.068 ms (3.872–4.124) | 2.912 ms (2.850–3.000) | 1.397x | B4 met |
| `text_scans/health/multibyte_50MiB` | 4.039 ms (3.925–4.127) | 4.129 ms (3.801–4.180) | 0.978x | B4 met (+2.2 %, under A + spread 4.241) |
| `text_scans/health/mixed_eol_50MiB` | 4.009 ms (3.847–4.096) | 2.806 ms (2.771–2.876) | 1.429x | B4 met |
| `text_scans/health/long_line` | 93.57 µs (92.98–95.38) | 73.22 µs (72.93–75.11) | 1.278x | B4 met |
| `text_scans/health_fallback/*` (B) vs `health/*` (A) | | | 0.994–1.029x | B5 met |

Verdict: **viable**. The implementation stage reruns the same interleaved
comparison on the final code (bench-first commit as A) and must meet the same
bars again before the allowlist row lands. Multibyte is the case with the
least margin. If the final code loses it, the implementation tries, in order,
a lead-only skip that replaces the ASCII-then-lead pair, or processing 128
bytes per step, and stops at the first that keeps every bar.

### D7. Documentation

- `.agents/rules/rust.md`, "Explicit-SIMD kernel allowlist": replace "Empty: no
  kernel is admitted…" with one row (file `crates/lushtext-core/src/services/editor_io/text_scans/health_avx2.rs`,
  function `count` / the 64-byte block kernel, measurement
  `docs/benchmarks/transient-file-load.md#avx2-file-health-kernel`, NEON
  unmeasured on the memchr path, dispatch crate none). The policy text itself
  does not change.
- `docs/benchmarks/transient-file-load.md`: a new section with the final
  verdict table, and the 4.3 gate paragraph pointing at it.
- `docs/next/long-line-documents.md`: only the "AVX2 file-health kernel" bullet
  under "Queued follow-ups from `simd-line-scans`" is rewritten to a closed
  record with the headline numbers. PR #84 (`power-of-two-thresholds`, held)
  edits the same file, so no other line is touched.
- `AGENTS.md`: the `editor_io/text_scans.rs` layout line and the "Large file
  handling" sentence ("allowlist empty") are updated. `docs/fuzzing.md` gets the
  new comparisons and seeds, and `docs/end-user-coverage.md` the row for the
  lane test.
- README: no change. Nothing is visible to the user apart from speed, and the
  README does not list scan internals.

## Risks / Trade-offs

- **Unsafe code in a load path.** It is confined to one module, every load is
  under a bounds invariant checked by the loop condition, and the kernel is
  checked exhaustively per lane and by property tests and fuzzing. If the
  kernel were wrong, the result would be a wrong count in the file-health
  findings (a cosmetic warning), never memory corruption, provided the bounds
  invariant holds. That invariant is what the review must check.
- **Fixture realism.** The typographic fixture puts a mark in one of every three
  tokens. Real prose sits between it and ASCII, so real gains fall between the
  1.17x and 1.0x measured.
- **CI hosts without AVX2** would run only the fallback, and the lane test
  prints that it was skipped. GitHub's x86-64 runners have AVX2. The property
  and fuzz comparisons of the fallback run everywhere.
- **Detection cost per slice.** It is an atomic load, and the 1 MB load (four
  slices) measured 1.04x, so it does not show.

## Migration Plan

No data or format changes. Rolling back is reverting the commits. If the kernel
misbehaved in the field, removing the dispatch call (one line) restores the
fallback.

## Review decisions (stage 4, 2026-11-05)

- **R1. B4's 1 MB ASCII load: accepted on reruns, not on a changed bar.** The
  implementation's three-round run put B at 1.048 of A, over B4's 3 % limit
  (inside A's round spread), so read literally the pre-registered bar missed
  there. The kernel cannot plausibly cause it: the health scan is about 80 µs
  of that load and 1.42x faster on the same ASCII bytes, and both sides' round
  medians spread about 25 % (bimodal). Rerunning that one fixture is a
  deviation from the pre-registered protocol, so it gets independent
  confirmation instead of one rerun by the same agent: the implementation's
  five-round rerun gave 1.006, and the review's own five-round interleaved
  rerun of the same binaries (load 0.28–1.35, before and after every run in
  `.claude/handoffs/avx2-health-scan-kernel/review/runs-1mb/`) gave
  **0.955** (A 292.9 µs, B 279.6 µs; health/ascii in the same run 0.690).
  User experience first: holding back a 1.17–1.23x faster typographic load
  over measurement noise on a fixture the kernel makes faster would cost users
  speed for no safety gain. B4 is met. A later kernel verdict should
  pre-register a rerun rule (for example five rounds for a fixture whose
  round spread exceeds its limit) so this decision isn't needed again.
- **R2. The dispatch itself is tested.** Every kernel test skips when `count`
  returns `None`, so with the `count -> None` mutant excluded nothing failed
  if the dispatch stopped selecting the kernel: users would just lose the
  speed-up. `kernel_is_selected_on_every_cpu_with_avx2_and_popcnt` asserts
  `count` returns `Some` exactly when the CPU reports both features. It failed
  first with the dispatch disabled (the two other kernel tests still passed,
  by skipping). The exclusion is removed from `.cargo/mutants.toml`.
- **R3. Out-of-scope mutation survivors stay out of this change.** cargo-mutants
  27 ignores `--re` and `--exclude-re` for struct-field-deletion mutants, so
  every focused run also tests about 40 of them from untouched files; 14
  survive on `main` as well (`draft_service.rs`, `file_tree.rs`,
  `editor_io.rs:231`). They aren't a failing gate (mutation CI is disabled,
  mutation lanes are local), they don't block verifying this change, and none
  of them touches the health scan. They're left to the maintainer as a
  separate coverage change.

## Open Questions

None. NEON is decided as unmeasured (D4), Kani as not meaningful (D5), the
bar was fixed before measuring (D6), and the review decisions are R1–R3.
