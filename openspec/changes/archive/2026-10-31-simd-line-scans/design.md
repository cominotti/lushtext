## Context

**Baseline this design is written against: C1 (`document-scale-policy`)
merged.** At proposal time C1 is on branch `document-scale-policy` (6af792a3)
and lands soon; implementation starts only after it is on `main`.

- **Load analysis** (`editor_io::analyze_decoded_content_cancellable`). For each
  bounded processing slice (the whole text up to 1 MiB, otherwise
  cancellation-checked slices floored to a scalar boundary) it runs, in order:
  `LineEndingTally::scan_slice` (`memchr2_iter` over `\r`/`\n`), C1's
  `ParagraphProfileAccumulator::push_str` (C1: `next_paragraph_break`, a
  `memchr3(\n, \r, 0xE2)` skip that switches to chunked `memchr2` + `memmem`
  after four `0xE2` leads that are not U+2029, then
  `count_scalars` = `str::chars().count()` per paragraph), and the health loop
  `for c in slice.chars() { NBSP? zero-width? }`. The health loop is the only
  per-character pass left. C1's recorded residual: `load_text_file` 642→733 µs
  at 1 MB, 8.06→8.47 ms at 10 MB, 65.5→68.0 ms at 50 MB (interleaved, main
  `1e2556a2`).
- **Minimap analysis.** `MinimapAnalysisAccumulator::inspect_char` (C1 form)
  counts every scalar into `characters_examined`, closes a line on `\n`, `\r`,
  or U+2029 with `\r\n` as one delimiter (`previous_was_cr` carry), and per
  line raises `wrapped_layout_too_large` past `wrapped_line_chars` and records
  one marker past `warning_line_chars` (up to `marker_limit`).
  `analysis_execution.rs` feeds it from `GtkTextIter::char()` /
  `forward_char()` on the GTK thread, 32 Ki characters per turn. The policy
  module carries `#![deny(clippy::float_arithmetic,
  clippy::disallowed_methods)]` and is compiled by its Kani harness
  (`policy/kani_proofs.rs`, minimap fits); the accumulator itself has no
  harness.
- **Save rewrites** run on the save worker:
  `apply_save_formatting_overrides_borrowed` (EditorConfig
  `trim_trailing_whitespace`, `insert_final_newline`) and
  `write_document_to_path` → `normalize_line_endings` (already `Cow`, with
  `line_endings_already_normalized` as its early exit). Trim always allocates
  and copies, char by char.
- **Measured candidates** (REPORT.md, single x86-64 AVX2 host, isolated
  functions, medians of 11; NEON unmeasured): H1 health 28.8→7.1 ms ASCII
  50 MiB, 143.6→5.0 ms multibyte, 1.83→0.15 ms on a 1.9M-char line, 1.7x on
  `0xE2`-dense typographic text (a hand AVX2 kernel 2.8x there, SWAR 0.88x,
  Teddy 0.93x); H3 accumulator 6.8–12.7x on 50 MiB, 33.8x on one long line,
  a hand AVX2 counter **slower** than `chars().count()`; H4 trim 4.3–8.7x
  (no allocation on a clean file), normalize 1.8–4.6x, CRLF check parity on
  dense CRLF and 3.5–18x on long lines; one candidate bug (`CR CR LF`) caught by
  the equivalence harness.
- Constraints: Kani (pinned 0.68.0) runs its own older nightly and cannot model
  the vectorised search, so Kani-checked modules keep a scalar form under
  `cfg(kani)`; no API newer than the workspace MSRV (1.98.0 / toolchain 1.98.1
  at implementation time); no `unsafe` in the library-based rewrites; no new
  dependency; filesystem boundary untouched (pure `&str` code).

## Goals / Non-Goals

**Goals:**

- Remove the last per-character pass from load analysis and close C1's
  residual: `load_text_file` at 1, 10, and 50 MB no slower than main before C1.
- Give the minimap accumulator a slice entry point equal to per-character
  inspection, ready for the follow-up that feeds it slices.
- Make clean saves allocation-free in trim and cut the dirty-save rewrite cost.
- Keep every observable result identical, proven by property tests and fuzzing
  against the current code as the reference.
- Write down when hand-written SIMD is allowed, so the next proposal does not
  re-litigate it.

**Non-Goals:**

- **Moving the minimap analysis's per-character `GtkTextIter` reads off the
  GTK thread** (per-turn `get_slice` or a worker over a buffer snapshot). That
  changes the `analysis_execution` coordination role and the minimap WFR row,
  and the cost of `get_slice` per turn is unmeasured. Follow-up change, to be
  proposed after this one lands; its pointer is recorded in
  `docs/next/long-line-documents.md` and REPORT.md P3.
- P1 (paragraph-break search), which C1 already shipped.
- The REPORT's untested ideas (chunk bitmaps, nibble classifiers, prefix-XOR,
  `str_indices`, bit-parallel diff), `stringzilla`, `simdutf`, `std::simd`.
- Any build-time CPU target (`-C target-cpu`, `target-feature` in
  `.cargo/config.toml`, Meson, Flatpak, or Snap flags). Requiring AVX2 at build
  time is a maintainer decision outside this change.

## Decisions

### D1. Scans move to `services/editor_io/text_scans.rs`; the old loops become the oracle

The health scan, trim, normalize, and CRLF check move into a GTK-free child
module of `editor_io` (`editor_io.rs` stays the parent file). The current
scalar implementations move verbatim into `text_scans::reference`, compiled
only under `cfg(any(test, feature = "test-utils", feature = "fuzzing"))`, so
production cannot call them and the property tests and fuzz target share one
oracle. Because `services/**` is in the mutation scope, the reference module is
excluded in `.cargo/mutants.toml` by its module path (an oracle is not code
under test; the exclusion is recorded with that reason, and re-keyed if the
module ever moves, per the path-keyed-gate rule).

*Alternative:* copy the reference into the property test and the fuzz target.
Rejected: two copies of an oracle can drift apart.

### D2. P2: `memchr3(0xC2, 0xE2, 0xEF)` with continuation confirmation

Every surfaced character starts with one of three lead bytes: U+00A0 is
`C2 A0`; U+200B/C/D are `E2 80 8B/8C/8D`; U+2060 is `E2 81 A0`; U+FEFF is
`EF BB BF`. The scan jumps between lead-byte candidates and confirms the next
one or two bytes against those sequences. Slices are `&str` cut on scalar
boundaries, so a candidate's continuation bytes are always inside the slice and
no carry crosses slices. The fallback for typographic text (many `E2` leads
that are not hits) is the same loop, measured at 1.7x over the char loop; no
switch-over is needed (unlike the paragraph search, there is no second needle
to fall back to).

`is_zero_width(char)` stays: `display_character` and the encoding preview use
it per character, and its set must equal the scan's byte table (a unit test
pins the two together).

### D3. P2 fusion with C1's profile pass is decided by measurement

Two variants are built and compared on the interleaved bench (D7):

- **A (default):** three independent passes per processing slice, as today,
  with the health pass now `memchr3`.
- **B (fused):** within each processing slice, the three scanners advance over
  the same 64 KiB sub-block before moving on, so the text is brought into cache
  once per sub-block; each scanner keeps its own carry (the tally's
  `pending_cr`, the profile's open paragraph and `pending_cr`; the health scan
  needs none). Sub-blocks end on scalar boundaries.

The arithmetic predicts A alone offsets the residual (the health pass falls by
about 21 ms on a 50 MiB ASCII file against a 2.5 ms residual). The profile pass's
own cost stays; what closes is `load_text_file` against the pre-C1 medians.

**C1's paragraph-granular profile makes B's premise weaker (review R1).** C1's
`push_str` calls `next_paragraph_break` and then `count_scalars` (the standard
library's `chars().count()`) once per paragraph. On short-line text that pass is
bound by per-paragraph call overhead (a paragraph of about 40 bytes per
`memchr3` call and per count), not by memory bandwidth. The processing slices
are `LOAD_PROCESSING_CHUNK_BYTES` (256 KiB), so the three passes already re-read
a slice that sits in L2. B's 64 KiB sub-blocks gain at most the step from L2 to
L1, and do nothing for call overhead. B is therefore expected to be declined. The
rule still decides it, so a surprise is measured rather than assumed.

B is adopted only if, on the interleaved run, it improves `load_text_file` over A
by at least 3 % at some size, and no size is worse than A by more than A's own
round-to-round spread (D7's noise tolerance). Otherwise A ships and B is recorded as
measured and declined. A deeper single-scanner fusion (one classifier for
`\n`, `\r`, `C2`, `E2`, `EF`) is out of scope: `memchr` searches at most three
needles, and a five-byte classifier is a hand-written kernel subject to D6.

### D4. P3: `inspect_str` built on C1's paragraph search and `chars().count()`

`inspect_str(&mut self, text: &str)` is defined as equal, field for field, to
`for c in text.chars() { self.inspect_char(c) }`, for every text, every split of
a document into consecutive `inspect_str` calls, and every policy. Outside Kani:

1. `characters_examined += count_scalars(text)` once (delimiters count as
   characters, `\r\n` as two, exactly as per character).
2. A leading `\n` that completes a CRLF carried in `previous_was_cr` is consumed
   without closing a line.
3. Paragraphs come from `next_paragraph_break` (`\n`, `\r`, `\r\n`, U+2029, the
   delimiters `inspect_char` uses since C1). For a paragraph closed inside
   this call, the exact character count is skipped when the line's running
   count plus the content's **byte** length cannot cross either threshold
   still relevant to the line (a scalar is at least one byte, and the closing
   break resets the count, so no field depends on the skipped value);
   otherwise, and always for the paragraph still open at the end of the call,
   the running count advances by `count_scalars` of the content. Crossing `wrapped_line_chars` sets the flag; crossing
   `warning_line_chars` records the marker for the current line once, within
   `marker_limit`. Each break closes the line as `inspect_char` does, and
   `previous_was_cr` is set when the slice ends on `\r`.

`count_scalars` and `next_paragraph_break` are C1's (`model/paragraph_boundary.rs`);
`count_scalars` becomes `pub(crate)`. The module's whole-pixel lint
(`deny(float_arithmetic, disallowed_methods)`) is unaffected: this is integer code.

**What the `cfg(kani)` split does and does not give (review R2).** C1 compiles
`count_scalars` only under `cfg(not(kani))`, and the minimap policy module is
compiled by its Kani harness (`policy/kani_proofs.rs`, the minimap fits). So under
`cfg(kani)` `inspect_str` must be the per-character loop, or the harness build
would not compile. That is the whole reason for the split. **No harness checks the
accumulator**, in either form, so nothing is "verified by Kani" here. Equivalence
rests on the property tests and the fuzz target (D1).

The one new piece of logic, the byte-length shortcut, does get a harness. It is
written as a pure function over abstract lengths:

```text
paragraph_may_cross(running: usize, content_bytes: usize, thresholds) -> bool
```

A Kani harness in the minimap policy shard proves that, for every running count,
every content of `b` bytes holding `c ≤ b` scalars (`c ≥ 1` when `b ≥ 1`), and
every threshold pair, a `false` result implies that per-character inspection of
those `c` scalars crosses neither threshold. The harness covers the decision, not the
SIMD search, so it needs no `memchr` model. The equivalence property test then
covers the composition.

The state compared field for field is the struct's full state:
- `current_line` (`u32`, saturating);
- `current_line_chars` (`usize`, saturating);
- `current_line_marked`, `wrapped_layout_too_large`, `long_line_lines`,
  `characters_examined` and `previous_was_cr`.

Saturation is part of the reference behaviour and is covered by a unit test with
the counters near their maxima.

`inspect_slice` (the `take(limit)` iterator form used by the GTK caller and the
bench) is unchanged. `inspect_str` gets no production caller in this change; it
is reached by the public re-export (`ui::editor_page::MinimapAnalysisAccumulator`)
from the bench and tests, as `inspect_slice` is today, and the follow-up wires
it into `analysis_execution`.

### D5. P4: save rewrites over `memchr2_iter`, borrowing when unchanged

The save pipeline, as it stands on C1, applies the rewrites in this order (review R3):
1. `trim_trailing_space_and_tabs` (EditorConfig `trim_trailing_whitespace`);
2. `insert_final_newline` (`push('\n')`, or truncating trailing `\n`/`\r`), through
   `Cow::to_mut`;
3. `write_document_to_path` → `normalize_line_endings` (already `Cow`), then encoding.

Trim treats `\r` and `\n` alike as line breaks, so trimming before normalization is
correct for CRLF, CR-only and mixed text. Mixed text is refused at step 3 as before.
Trim returning `Cow` therefore changes no output.

Two callers depend on what trim returns:
- **`save/execution.rs` computes `formatting_changed`** as
  `formatted_text.as_ref() != text.as_str()`. That comparison stays. Today trim
  always returns `Owned`, so the variant says nothing. After this change `Owned`
  would imply "changed", but only by an invariant that one later edit could
  silently break. Deriving the flag from the `Cow` variant is declined. The
  comparison is one `memcmp` and is outside every bar.
- **The public `apply_save_formatting_overrides` (`-> String`)** stays, with its
  idempotence property test and fuzz check (`fuzzing.rs`).

Replace All, drafts and local-history capture do not call these rewrites. They are
unaffected.

- **Trim** walks `\n`/`\r` positions; for each segment it computes the kept end
  (trailing `' '`/`'\t'` stripped). Until the first segment that loses bytes it
  allocates nothing; at that point it starts a `String` with the untouched
  prefix and from then appends `&text[kept range]` and the delimiter. Every
  cut point is an ASCII byte, so every sub-slice is a `&str` on a scalar
  boundary and no `unsafe` is needed. It returns `Cow<'_, str>`;
  `apply_save_formatting_overrides_borrowed` keeps `Cow::Borrowed` for a clean
  file, so `insert_final_newline` and normalization operate on the borrowed
  text. The public `apply_save_formatting_overrides` (`-> String`) is
  unchanged.
- **Normalize** copies runs between delimiters with `push_str` and writes the
  separator, treating `\r\n` as one delimiter (the `CR CR LF` case the H4
  harness caught is a named regression seed).
- **CRLF check** walks `\n`/`\r` positions: every `\r` must be followed by
  `\n`, and every `\n` must be the one just after a `\r`. The `Lf` and `Cr` modes are
  already `str::contains(char)`, which is a `memchr` search, so they stay as they are.
  Only the `Crlf` arm changes.

### D6. Explicit-SIMD policy (recorded in `.agents/rules/rust.md`)

A hand-written SIMD kernel is admitted only when all hold:

1. The loop is ours and stays measured hot **after** its library-based
   rewrite: on the interleaved bench it is at least 10 % of the enclosing
   workflow's cost (for example `load_text_file`) on a realistic fixture.
2. It is `#[target_feature(enable = "avx2")]` (or the relevant x86-64 level)
   behind `is_x86_feature_detected!` dispatch, cached once; the fallback is the
   library-based path, and both are measured on the interleaved bench (the
   fallback with detection forced off).
3. NEON parity is decided explicitly: a real NEON kernel measured on aarch64
   hardware, or the aarch64 build stays on the library path and the record
   says NEON is unmeasured. SWAR `u64` is not a NEON substitute (H1: 0.88x).
4. Its `unsafe` call site carries a `SAFETY` comment, it is pinned to the scalar
   reference by property tests and fuzzing, and in a Kani-checked module it sits
   behind `cfg(not(kani))`.
5. No build-time CPU requirement: the shipped binary runs on baseline x86-64.

**How the policy is enforced (review R4).** Prose alone cannot keep a later change
from adding a kernel. Two mechanical gates hold it:

- **A check script, `scripts/check-explicit-simd.py`, in `make check-policy`.** It
  fails when:
  - a Rust file under `crates/` uses `#[target_feature`, `std::arch`, `core::arch`,
    `is_x86_feature_detected!` or `is_aarch64_feature_detected!` and is not listed
    in its allowlist. The allowlist is a table in `.agents/rules/rust.md`, one row
    per kernel, giving the file, the function, the measurement record and the NEON
    decision. Today the table is empty, and so is the tree;
  - `.cargo/config.toml`, `rust-toolchain.toml`, `meson.build`, `build-aux/`,
    `snap/snapcraft.yaml` or the Flatpak manifest sets `target-cpu` or
    `target-feature` (condition 5);
  - a dependency that exists to dispatch SIMD (`multiversion`, `pulp`,
    `fearless_simd`) appears in `Cargo.lock` without an allowlist row;
  - `memchr` is resolved without its `std` feature for `lushtext-core`. It reads
    `cargo metadata`, because without `std` `memchr` cannot detect AVX2 at run time
    and silently falls back to SSE2, which would void every measurement here.
- **Clippy:** `disallowed-macros` in the root `clippy.toml` names
  `std::is_x86_feature_detected` and `std::is_aarch64_feature_detected`. An admitted
  kernel's dispatch site carries `#[expect(clippy::disallowed_macros, reason = …)]`
  that cites its allowlist row. Unlike `disallowed-methods`, which is allowed
  workspace-wide and raised only in the geometry modules, this lint stays at its
  default `warn`, which `-D warnings` makes blocking everywhere.

The check script has a self-test in the style of the other `check-*` scripts, with a
planted violation of each kind.

The only candidate known today is the typographic health scan (H1: 1.7x with
`memchr3`, 2.8x with an AVX2 block kernel). Task 4.3 evaluates the gate after P2;
if the health scan is under 10 % of `load_text_file` on the typographic
fixture, no kernel is written and the measurement is recorded.

### D7. Verdict by interleaved bench, landed bench-first

The first commit adds Criterion cases, and the bench hooks they need, so a disposable
baseline worktree (`main` plus that commit) and the branch carry identical IDs. The scans
are private functions of `editor_io`. That commit therefore adds `test-utils`-gated hooks
that both sides compile (review R5):
- `bench_load_analysis(&str)` (the whole analysis);
- `bench_health_scan(&str)` (the health loop alone);
- `bench_trim(&str)`, `bench_normalize(&str, LineEnding)` and
  `bench_crlf_check(&str)`.

On the baseline they call today's code, and on the branch the rewrite. Without the
health-only hook, D7's health bar could not be measured: the load-analysis case also
contains the tally and profile passes. The cases are:

- `text_scans/load_analysis/{ascii,multibyte,typographic,mixed_eol,long_line}_50MiB`
  and the existing `editor_file_io/load_text_file/{1,10,50}MB`;
- `text_scans/minimap/{many_short_lines,long_line}` through `inspect_slice` on
  the baseline and `inspect_slice` vs `inspect_str` on the branch;
- `text_scans/save/{trim_clean,trim_dirty,trim_single_line,normalize_crlf,normalize_mixed,crlf_check_dense}`
  through `apply_save_formatting_overrides_borrowed` and
  `write_document_to_path`'s normalize step (via a bench-only `test-utils`
  hook if it is not otherwise reachable).

Protocol, as C1 and C4c ran it: baseline and branch alternate, three rounds per
side, same machine, nothing heavy admitted, load recorded, under the measurement lock
(`flock -x INTENT flock -x LOCK`). The C1 pre-merge commit (`1e2556a2`) is added as a
third side for `load_text_file` to judge the residual.

**One toolchain for every side (review R6).** `1e2556a2` pins 1.96.0 in
`rust-toolchain.toml`, while `main` moves to 1.98.1 (PR #68). Codegen differences
between compilers would be read as scan speed-ups or regressions. Every side is built
with the branch's toolchain (`RUSTUP_TOOLCHAIN` set explicitly) and the same profile.
The record names the toolchain. A side that does not build on it is reported, not
silently built with another.

**The statistic (R5).**
- Each round yields one Criterion median per case and side.
- A side's figure is the median of its three round medians.
- Its spread is the max − min of the round medians.
- A ratio bar (≥ 3x, ≥ 5x, ≥ 1.5x) compares the two sides' figures.
- A no-regression bar is met when the branch figure is at most the reference figure
  plus the reference side's spread. The no-regression bars are `≤ pre-C1`, typographic
  ≥ 1.0x and CRLF check ≥ 0.95x. A bar written as ≥ 1.0x reads as "branch ≤ baseline +
  spread".

A bar that holds only inside the noise is reported as such, with the three round
medians of each side.

**Acceptance per item:**
- P2: `bench_health_scan` ≥ 3x on ASCII and no regression on typographic;
  `load_text_file` no regression against the pre-C1 side at 1, 10 and 50 MB.
- P3: `inspect_str` ≥ 5x `inspect_slice` on 50 MiB.
- P4: `bench_trim` ≥ 3x dirty, `bench_normalize` ≥ 1.5x, and `bench_crlf_check` no
  regression on dense CRLF. "Allocation-free clean" is **not** a bench bar: it is the
  unit test that a clean text comes back `Cow::Borrowed` and pointer-equal (task 3.4).
  A counting-allocator test asserts zero allocations in trim on a clean 1 MiB text.

An item that misses its bar is not landed and the result is recorded.

## Risks / Trade-offs

- [A fast path diverges from the reference on an input class the generators
  miss] → generators bias toward every lead byte that is *not* a hit (`C2 A9`,
  `E2 80 99`, `E2 80 94`, `EF BB BE`), U+2028/U+2029, `\r\r\n`, delimiters at
  slice ends, and 4-byte scalars; cuts at every scalar boundary; fuzzing runs
  the same comparison on arbitrary bytes that decode; H1–H4 seeds are ported.
- [C1 changes before it lands and invalidates D3/D4] → tasks start by re-reading
  C1's merged `paragraph_boundary.rs` and accumulator; the design names the
  functions it reuses, not their line numbers.
- [`inspect_str` without a production caller reads as dead weight] → it is the
  slice API the follow-up needs, benchmarked and proven now; the follow-up is
  recorded in the planned-work record.
- [Single-host numbers] → the interleaved verdict is the acceptance, not
  REPORT.md; aarch64 stays on `memchr`'s own NEON paths, unmeasured, and is
  recorded as such.
- [Mutation exclusion of the reference module hides a mutant] → the reference
  is never called by production; its correctness is the old code's, which this
  change does not edit.

## Migration Plan

Pure code, no data or format change. Each item lands behind its own commit and
bench verdict, so any one can be reverted alone; reverting restores the scalar
path, which the tests already pin.

## Open Questions

- The workspace MSRV moves to 1.98.0 with toolchain 1.98.1 (PR #68). Nothing here
  needs an API newer than 1.96. Every API used (`memchr` 2, `str::char_indices`,
  `Cow`, `is_x86_feature_detected!` if a kernel is ever admitted) is older than
  1.98. The verdict builds every side with one toolchain (D7).
- Whether a hand AVX2 health kernel is wanted at all depends on task 4.3's
  measurement; a build-time AVX2 baseline stays a maintainer decision.

## Adversarial review (2026-10-30)

A review against C1 (`document-scale-policy`, 6af792a3), the SIMD report and
experiments H1–H4 tried to refute each decision. The decisions above are already
revised; this records why.

- **R1. P2 fusion against C1's paragraph-granular profile: holds, with a sharper
  prediction.** C1's profile pass now counts characters per paragraph with the
  standard library, so it is bound by call overhead on short lines. The processing
  slices are 256 KiB and already sit in L2. Fusing into 64 KiB sub-blocks addresses
  neither. The decision rule is still correct, and D3 now predicts B is declined and
  gives "no size worse" a tolerance. The goal is restated: the profile pass's cost
  stays, and what closes is `load_text_file` against the pre-C1 medians.
- **R2. P3's `cfg(kani)` split is needed to compile, not to verify.** C1's
  `count_scalars` exists only under `cfg(not(kani))`, so the split is required.
  But no Kani harness checks the accumulator, so the spec's "the scalar path is the
  one the harnesses verify" was vacuous for `inspect_str`. The spec now says what
  holds. The byte-length shortcut, the only new decision, gets its own harness
  over abstract lengths (`paragraph_may_cross`). The `inspect_str` equivalence itself
  is sound as designed. The skip is safe because a scalar is at least one byte and the
  closing break resets every field the skipped count could reach (`current_line_chars`,
  `current_line_marked`). The open paragraph and `characters_examined` are always
  counted exactly. Saturation of `current_line` (u32) and `current_line_chars`
  (usize) is now named.
- **R3. P4's `Cow` trim is safe on every save path, with one explicit non-change.**
  The order is trim, then `insert_final_newline` via `to_mut`, then normalization in
  `write_document_to_path`. Trim treats `\r` and `\n` alike, so output is unchanged
  in every line-ending mode. No other writer (Replace All, drafts, local history)
  calls it. `save/execution.rs` computes `formatting_changed` by content comparison;
  switching it to the `Cow` variant would rest on an unchecked invariant and is
  declined. The `Lf` and `Cr` checks are already `memchr`, so only the `Crlf` arm
  changes.
- **R4. The explicit-SIMD policy was not enforceable.** It is now held by a
  `check-policy` script with an allowlist table, a build-flag scan, a
  dispatch-crate scan and a check of `memchr`'s `std` feature. The last matters:
  without `std`, `memchr` drops runtime AVX2 detection and every measurement here
  becomes SSE2's. Clippy `disallowed-macros` on the feature-detection macros backs the
  script up.
- **R5. The bench bars were not well defined.** Four gaps are closed:
  - The scans are private, so "entry points that exist before this change" could
    not reach them. The health bar also could not be separated from the whole
    analysis. The bench-first commit now adds `test-utils` hooks on both sides.
  - The statistic and the noise tolerance are now stated, so that the
    no-regression bars (≥ 1.0x, ≥ 0.95x, ≤ pre-C1) are not coin flips.
  - "Allocation-free" moved from a bench to a unit test and a counting-allocator
    test.
  - The run takes the measurement lock.
- **R6. A cross-toolchain comparison.** The pre-C1 side pins 1.96.0 and `main` moves
  to 1.98.1. Every side is now built with one explicit toolchain, and the record names
  it.
