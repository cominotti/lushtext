## ADDED Requirements

### Requirement: The file-health count selects an AVX2 kernel at run time with an exact fallback
On x86-64, the load's file-health count SHALL run a hand-written AVX2 block
kernel when the running CPU reports both AVX2 and POPCNT, and SHALL otherwise
run the `memchr3` lead-byte scan. Non-x86-64 targets and `cfg(kani)` builds
SHALL compile only the `memchr3` scan. For every byte string, the kernel SHALL
return the same non-breaking-space and zero-width counts as the `memchr3` scan,
and for every valid UTF-8 text and every division of it into processing slices,
both SHALL equal the per-character reference. The kernel SHALL use the `memchr3`
scan for the part of a slice too short for a whole block step, so a candidate
whose continuation bytes end the slice is confirmed exactly as before. No build
configuration SHALL require AVX2: the kernel is reached only through run-time
detection.

#### Scenario: Typographic prose is counted exactly by the kernel
- **WHEN** an AVX2 host loads a document whose lines are dense with `’`, `“`, `”`, `—`, `–` and `…` beside a few U+200B, U+2060, U+FEFF and U+00A0 characters
- **THEN** the non-breaking-space and zero-width counts equal the per-character reference counts and the file-health findings are unchanged

#### Scenario: A hit straddles a block seam
- **WHEN** a zero-width character or non-breaking space starts at the last one or two lead positions of a 64-byte block, or at a 32-byte half boundary, so its continuation bytes lie in the next block
- **THEN** it is counted exactly once

#### Scenario: A hit ends the slice
- **WHEN** a processing slice ends with the last byte of U+FEFF or U+00A0, inside the final bytes the block loop does not cover
- **THEN** the tail scan counts it, and the sum over slices equals the count for the whole text

#### Scenario: A CPU without AVX2
- **WHEN** the CPU does not report AVX2 or POPCNT
- **THEN** the `memchr3` scan runs and the counts are identical to those on an AVX2 CPU

#### Scenario: Every byte triple at every lane
- **WHEN** the exhaustive lane test places each of the 2^24 byte triples at the first and last lead positions of both halves of a block and at the tail of a buffer
- **THEN** the kernel's counts equal the `memchr3` scan's counts for every placement

### Requirement: The AVX2 file-health kernel is admitted under the explicit-SIMD policy
The kernel and its feature detection SHALL live in one module file that is
compiled only for x86-64 and not under `cfg(kani)`. That file SHALL be the only
file the explicit-SIMD allowlist names for the kernel, and the allowlist row
SHALL record its function, its measurement record, and its NEON decision:
NEON unmeasured, aarch64 on the library `memchr3` path. The detection-macro site
SHALL carry `#[expect(clippy::disallowed_macros, reason = ...)]` that cites the
row. Every `unsafe` block in the file SHALL carry a `SAFETY` comment stating the
bounds or feature invariant it relies on, and no `unsafe` item SHALL be
reachable from outside the module. The kernel SHALL allocate nothing.
`make check-explicit-simd` SHALL pass, and SIMD added to any other file SHALL
still fail it.

#### Scenario: The allowlist names the kernel file
- **WHEN** `make check-policy` runs on the change
- **THEN** the explicit-SIMD check passes, and its self-test still rejects an unlisted `#[target_feature]` in another file

#### Scenario: The kernel allocates nothing
- **WHEN** the counting-allocator test runs the dispatching health count over a 1 MiB typographic text
- **THEN** it records zero allocations

### Requirement: The AVX2 file-health kernel lands only on its pre-registered interleaved verdict
The kernel SHALL be adopted only when an interleaved Criterion comparison of the
final code, with the bench-first commit as baseline and following the
interleaving, toolchain and statistic rules of this capability, meets every bar
the design states: the typographic health scan at least 2x faster; typographic
`load_text_file` at least 5 % faster at 10 and 50 MB (and no worse than 3 %
slower at 1 MB), with the health scan under 10 % of that load; and no
regression beyond both 3 % and the baseline's round spread on ASCII
`load_text_file` at 1, 10 and 50 MB or on the ASCII, multibyte, mixed-EOL and
long-line health fixtures. The fallback, measured with the kernel bypassed,
SHALL be within 3 % of the baseline. The verdict SHALL be recorded in
`docs/benchmarks/transient-file-load.md` and the programme record, and a missed
bar SHALL leave the `memchr3` scan as the only path.

#### Scenario: The kernel meets its bars
- **WHEN** the final interleaved comparison meets every bar
- **THEN** the allowlist row is added and the verdict table is recorded with load before and after each run

#### Scenario: A non-typographic fixture regresses
- **WHEN** any no-regression fixture is slower than both 3 % and the baseline's round spread allow
- **THEN** the kernel is not dispatched to, and the result is recorded
