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
under `cfg(kani)` `inspect_str` SHALL be the per-character loop.

#### Scenario: CRLF split across slices
- **WHEN** one call ends with `\r` and the next begins with `\n`
- **THEN** the line count and characters examined equal the per-character result for the joined text

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
mode. Saved bytes for every EditorConfig combination SHALL be unchanged.

#### Scenario: Clean file saves without a trim copy
- **WHEN** a document with `trim_trailing_whitespace = true` and no trailing spaces or tabs is saved
- **THEN** the trim step returns the text borrowed and the saved bytes equal the reference output

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
every input that decodes. In a module checked by Kani, the accelerated path
SHALL be compiled only under `cfg(not(kani))` and the scalar path SHALL be the
one the harnesses verify.

#### Scenario: A divergence fails the property suite
- **WHEN** an accelerated scan returns a different result from its reference for some generated input
- **THEN** `make test-prop` fails and records the minimized input in the regression file

#### Scenario: Kani verifies the scalar form
- **WHEN** a Kani harness compiles a module containing an accelerated scan
- **THEN** the harness checks the scalar form and the accelerated form is not compiled

### Requirement: Accelerated scans land only on an interleaved benchmark verdict
Each accelerated scan SHALL be adopted only when an interleaved Criterion
comparison (baseline and branch alternating, at least three rounds per side, on
one machine, with machine load recorded and both sides carrying the same
benchmark IDs) meets the acceptance bar the design states for it. The load
analysis SHALL NOT be slower than before the paragraph-profile pass was added:
`load_text_file` at 1, 10, and 50 MB SHALL be at or below the pre-C1 medians on
the same interleaved run. A scan that misses its bar SHALL keep its scalar
implementation and the measurement SHALL be recorded.

#### Scenario: The C1 load residual is closed
- **WHEN** the interleaved run compares the pre-C1 commit, `main`, and the branch on `editor_file_io/load_text_file`
- **THEN** the branch median is at or below the pre-C1 median at 1, 10, and 50 MB, and the table is recorded in `docs/benchmarks/transient-file-load.md` and `docs/next/long-line-documents.md`

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

#### Scenario: Baseline x86-64 still runs the shipped binary
- **WHEN** the release, Meson, Flatpak, or Snap build is inspected
- **THEN** it sets no `target-cpu` or `target-feature` flag, and a CPU without AVX2 runs the library-based path

#### Scenario: A kernel without a measured need is rejected
- **WHEN** a loop is under 10 % of its workflow's cost after the library-based rewrite
- **THEN** no hand-written kernel is added for it and the measurement is recorded
