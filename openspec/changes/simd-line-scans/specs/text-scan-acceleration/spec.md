## ADDED Requirements

### Requirement: The load file-health scan counts exactly what the per-character scan counts
The load analysis SHALL count non-breaking spaces (U+00A0) and zero-width
characters (U+200B, U+200C, U+200D, U+2060, U+FEFF) by searching the decoded
bytes for the lead bytes `0xC2`, `0xE2`, and `0xEF` and confirming the
continuation bytes, and the resulting counts, line-ending classification,
paragraph profile, and file-health findings SHALL equal those of the
per-character reference scan for every valid UTF-8 document and every division
of it into processing slices. The set of zero-width characters the scan counts
SHALL equal the set `is_zero_width` reports. Whether the health scan runs as
its own pass or fused with the paragraph-profile pass SHALL be decided by the
interleaved benchmark verdict, and either form SHALL meet this equality.

#### Scenario: Typographic text is counted exactly
- **WHEN** a document contains many `0xE2`-led characters that are not zero-width (such as `’`, `—`, `…`, U+2028, U+2029) beside real U+200B and U+2060 characters
- **THEN** the zero-width count equals the per-character reference count

#### Scenario: Look-alike lead bytes are not counted
- **WHEN** a document contains U+00A9 (`C2 A9`) and U+FEFE (`EF BB BE`) but no U+00A0 or U+FEFF
- **THEN** the non-breaking-space and zero-width counts are zero

#### Scenario: Slicing does not change the result
- **WHEN** a document larger than the direct-processing threshold is analysed in cancellation-checked slices
- **THEN** every count and finding equals the result of analysing it as one slice with the reference scan

### Requirement: The minimap accumulator accepts text slices with per-character equivalence
`MinimapAnalysisAccumulator` SHALL provide `inspect_str(&str)` whose resulting
state (characters examined, current line and its character count, the
marked-line flag, the CRLF carry, the wrapped-layout verdict, and the marker
list) SHALL equal the state reached by calling `inspect_char` for each
character of the same text in order, for every policy, every text, and every
division of a text into consecutive `inspect_str` calls, including a `\r\n`
split across two calls. `inspect_char` SHALL remain as the reference form, and
under `cfg(kani)` `inspect_str` SHALL be the per-character loop, so that the
module's Kani build compiles without the accelerated path. The decision that lets
`inspect_str` skip the exact character count of a closed line SHALL be a pure
function of the line's running count, the content's byte length and the
thresholds. A Kani harness SHALL prove it sound: whenever it permits the skip,
per-character inspection of any content of that byte length crosses neither
threshold. The counters' saturation SHALL behave as in `inspect_char`.

#### Scenario: CRLF split across slices
- **WHEN** one call ends with `\r` and the next begins with `\n`
- **THEN** the line count and characters examined equal the per-character result for the joined text

#### Scenario: The byte-length shortcut never hides a crossing
- **WHEN** the Kani harness explores every running count, content byte length, scalar count no larger than it, and threshold pair
- **THEN** a line whose count the shortcut skips crosses neither threshold under per-character inspection

#### Scenario: Thresholds crossed inside a long line
- **WHEN** one line exceeds `warning_line_chars` and `wrapped_line_chars` partway through a slice of multibyte text
- **THEN** the marker for that line is recorded once and the wrapped-layout verdict is set, exactly as per character

### Requirement: Save rewrites are byte-identical and borrow unchanged text
Trailing space-and-tab trimming, line-ending normalization, and the
already-normalized check SHALL produce output byte-identical to the
per-character reference implementations for every valid UTF-8 text and every
line-ending mode. Trimming SHALL return the input borrowed, without allocating,
when no space or tab precedes a line break or the end of the text, and
normalization SHALL return the input borrowed when it is already in the target
mode. Saved bytes for every EditorConfig combination SHALL be unchanged, with the
rewrites applied in the existing order: trim, final-newline insertion or removal, then
line-ending normalization. Whether saving changed the text SHALL still be decided
by comparing contents, never by whether a rewrite returned borrowed or owned text.

#### Scenario: Clean file saves without a trim copy
- **WHEN** a document with `trim_trailing_whitespace = true` and no trailing spaces or tabs is saved
- **THEN** the trim step returns the text borrowed and the saved bytes equal the reference output

#### Scenario: A trimmed save is still mirrored into the buffer
- **WHEN** a document with trailing spaces and `trim_trailing_whitespace = true` is saved
- **THEN** the save reports that formatting changed the text, and the buffer receives the trimmed text exactly as before this change

#### Scenario: Carriage-return runs normalize like the reference
- **WHEN** a document containing `\r\r\n`, a lone `\r` at the end, and `\n\r` is normalized to each of LF, CRLF, and CR
- **THEN** each output equals the reference normalization byte for byte

### Requirement: Accelerated scans are pinned to their scalar references
Every accelerated scan SHALL keep its scalar reference implementation,
reachable only from tests, property tests, benchmarks, and the fuzz target. The
property tests SHALL compare each accelerated scan with its reference over
generated texts biased toward delimiter bytes, every lead byte that is and is
not a hit, U+2028 and U+2029, four-byte scalars, and cuts at every scalar
boundary, and the `editor_bytes` fuzz target SHALL make the same comparisons on
every input that decodes. In a module that a Kani harness compiles, the
accelerated path SHALL be compiled only under `cfg(not(kani))`. Equivalence of an
accelerated scan with its reference SHALL NOT be claimed as Kani-verified unless a
harness actually checks it.

#### Scenario: A divergence fails the property suite
- **WHEN** an accelerated scan returns a different result from its reference for some generated input
- **THEN** `make test-prop` fails and records the minimized input in the regression file

#### Scenario: Kani verifies the scalar form
- **WHEN** a Kani harness compiles a module containing an accelerated scan
- **THEN** only the scalar form is compiled, and any harness that reaches the scan checks that form
- **AND** the scan's equivalence with its reference is pinned by the property tests and the fuzz target

### Requirement: Accelerated scans land only on an interleaved benchmark verdict
Each accelerated scan SHALL be adopted only when an interleaved Criterion
comparison meets the acceptance bar the design states for it. The comparison runs
under the measurement lock: baseline and branch alternate, at least three rounds
per side, on one machine, with machine load recorded. Both sides carry the same
benchmark IDs and the same `test-utils` bench hooks, and every side is built with
the same explicitly named toolchain and profile. A side's figure SHALL be the median
of its round medians, and its noise tolerance the spread of those round medians. A
ratio bar compares figures. A no-regression bar SHALL be met when the branch's figure
is at most the reference figure plus the reference's spread. Allocation-free
behaviour SHALL be shown by tests, not by a benchmark. The load
analysis SHALL NOT be slower than before the paragraph-profile pass was added:
`load_text_file` at 1, 10, and 50 MB SHALL be at or below the pre-C1 medians on
the same interleaved run. A scan that misses its bar SHALL keep its scalar
implementation and the measurement SHALL be recorded.

#### Scenario: The C1 load residual is closed
- **WHEN** the interleaved run compares the pre-C1 commit, `main`, and the branch on `editor_file_io/load_text_file`
- **THEN** the branch median is at or below the pre-C1 median at 1, 10, and 50 MB, and the table is recorded in `docs/benchmarks/transient-file-load.md` and `docs/next/long-line-documents.md`

#### Scenario: Sides built with different toolchains
- **WHEN** a side of the comparison, such as the pre-C1 commit, pins an older toolchain than the branch
- **THEN** every side is built with the one named toolchain, and a side that does not build with it is reported instead of being built with another

#### Scenario: A scan that does not pay is not landed
- **WHEN** an accelerated scan misses its acceptance bar on the interleaved run
- **THEN** the scalar implementation stays in production and the result is recorded with the run

### Requirement: Hand-written SIMD kernels are admitted only by measured need
A hand-written SIMD kernel SHALL be added only for a loop that remains at least
10 % of its enclosing workflow's measured cost after its library-based
rewrite. Such a kernel SHALL be selected at run time
(`#[target_feature(enable = "avx2")]` with cached `is_x86_feature_detected!`
dispatch), SHALL fall back to the library-based path, and both paths SHALL be
measured on the interleaved benchmark. NEON parity SHALL be decided for each
kernel, either with a NEON kernel measured on aarch64 or by keeping aarch64 on
the library path with that recorded as unmeasured. No build SHALL require a
CPU feature beyond the target's baseline at build time.

The policy SHALL be enforced mechanically by a policy check in `make check-policy`.
The check SHALL fail on any of these:
- a use of `#[target_feature]`, `std::arch`, `core::arch` or a runtime
  feature-detection macro under `crates/` outside a file the allowlist names;
- a `target-cpu` or `target-feature` flag in the Cargo, toolchain, Meson, Flatpak or
  Snap build configuration;
- a SIMD-dispatch dependency without an allowlist entry;
- `memchr` resolved without its `std` feature, which carries its runtime AVX2
  detection.

The allowlist SHALL record, for each kernel, its file and function, its measurement
record and its NEON decision. The workspace's Clippy configuration SHALL also
disallow the feature-detection macros except where an admitted kernel's site
expects the lint with a reason.

#### Scenario: Baseline x86-64 still runs the shipped binary
- **WHEN** the release, Meson, Flatpak, or Snap build is inspected
- **THEN** it sets no `target-cpu` or `target-feature` flag, and a CPU without AVX2 runs the library-based path

#### Scenario: An unlisted kernel fails the policy check
- **WHEN** a change adds `#[target_feature(enable = "avx2")]` or `is_x86_feature_detected!` in a file the allowlist does not name
- **THEN** `make check-policy` fails and names the file

#### Scenario: memchr loses its runtime dispatch
- **WHEN** the dependency graph resolves `memchr` for `lushtext-core` without its `std` feature
- **THEN** `make check-policy` fails, because the accelerated scans would silently fall back to SSE2

#### Scenario: A kernel without a measured need is rejected
- **WHEN** a loop is under 10 % of its workflow's cost after the library-based rewrite
- **THEN** no hand-written kernel is added for it and the measurement is recorded
