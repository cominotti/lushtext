## Why

Three byte-scanning loops still walk text one `char` at a time where a
SIMD-backed byte search (the `memchr` crate already in the tree, which picks
AVX2 at run time on x86-64 and NEON on aarch64) finds the few bytes that
matter:

- the **file-health scan** of every load (non-breaking spaces and zero-width
  characters, `services/editor_io.rs` `analyze_decoded_content_cancellable`),
  which is about 44 % of `load_text_file` on a 50 MB ASCII file;
- the **minimap long-line analysis** accumulator
  (`ui/editor_page/minimap/policy.rs`, `MinimapAnalysisAccumulator`), which
  has only a per-character entry point;
- the **save rewrites** (`trim_trailing_space_and_tabs`,
  `normalize_line_endings`, `line_endings_already_normalized`), which copy the
  whole document a character at a time even when nothing changes.

The SIMD exploration (`.claude/handoffs/simd-exploration/REPORT.md`,
experiments H1, H3, H4, 2026-10-29) measured library-based rewrites of each at
4–29x (health), 7–34x (minimap policy), and 2–9x (save), with exact output
equality on large inputs, edge cases, and 5k–20k random strings. Now is the
time because C1 (`document-scale-policy`) closed its load-analysis bench with an
accepted residual (`load_text_file` +4–5 % at 10–50 MB, +14 % at 1 MB, from
the paragraph-profile pass) whose recorded remedy is this change.

## What Changes

- **P2 – file-health scan.** Replace the per-`char` loop with
  `memchr3(0xC2, 0xE2, 0xEF)` over the slice's bytes plus confirmation of the
  continuation bytes (U+00A0, U+200B/C/D, U+2060, U+FEFF). Fuse it with C1's
  paragraph-profile pass in the same load-analysis traversal **only if** the
  interleaved bench shows the fusion is what removes C1's residual; otherwise
  keep the passes separate.
- **P3 – minimap analysis.** Add a pure
  `MinimapAnalysisAccumulator::inspect_str(&str)` that reaches exactly the
  state per-character `inspect_char` reaches: paragraph breaks by C1's
  `next_paragraph_break`, a byte-length shortcut for lines that cannot cross a
  threshold, and the standard library's `chars().count()`. `inspect_char`
  stays as the reference. The GTK-side caller is unchanged (non-goal below).
- **P4 – save rewrites.** Rewrite trim, normalize, and the CRLF check with
  `memchr2_iter(b'\n', b'\r')` and block copies of `&str` sub-slices; trim
  returns `Cow<str>` and borrows when nothing is trimmed.
- **Explicit-SIMD policy.** Hand-written AVX2 code is admitted only for one of
  our own loops that is still measured hot **after** its library-based rewrite,
  behind `#[target_feature(enable = "avx2")]` with `is_x86_feature_detected!`
  dispatch and a measured fallback; NEON parity is decided for each such
  kernel. No build-time CPU target is required. This change adds no such
  kernel unless the gate in the design is met.
- **Verification.** Every fast path is pinned to its scalar reference by
  property tests and the `editor_bytes` fuzz target; fast paths in Kani-checked
  modules sit behind `cfg(not(kani))` with the scalar form kept for the
  harnesses; each item lands only on an interleaved main/branch Criterion
  verdict.

Non-goal, recorded for a follow-up: moving the minimap analysis's per-character
`GtkTextIter` reads (`minimap/analysis_execution.rs`) off the GTK thread, or to
`get_slice` per turn, so production actually feeds `inspect_str`. That changes
the `analysis_execution` coordination role (WFR minimap row) and gets its own
change; see design "Non-Goals".

## Capabilities

### New Capabilities

- `text-scan-acceleration`: equivalence, verification, and adoption rules for
  accelerated byte scans over decoded text (load file-health scan, minimap
  line analysis, save rewrites) and the policy for any explicit SIMD kernel.

### Modified Capabilities

None. Every user-visible result (file-health findings, minimap markers and
wrapped-layout verdict, saved bytes) stays byte-for-byte identical; only cost
changes.

## Impact

- Code: `crates/lushtext-core/src/services/editor_io.rs` (scans move into a new
  child module `services/editor_io/text_scans.rs`, shrinking a 3,000-line
  file), `crates/lushtext-core/src/ui/editor_page/minimap/policy.rs`
  (`inspect_str`), `crates/lushtext-core/src/model/paragraph_boundary.rs` (C1;
  `count_scalars` becomes `pub(crate)` for reuse).
- Tests: `crates/lushtext-core/tests/properties/` (new `text_scans.rs`),
  `fuzz/fuzz_targets/editor_bytes.rs` plus corpus seeds, unit tests beside
  each scan.
- Benches: new cases in `crates/lushtext-core/benches/benchmarks.rs`
  (`text_scans/*`), landed first so both sides of the interleaved comparison
  carry them.
- Dependencies: none new (`memchr` 2 is already a workspace dependency).
- Docs: `docs/benchmarks/transient-file-load.md`,
  `docs/next/long-line-documents.md` (C1 residual closed or re-recorded),
  `AGENTS.md` (module layout, editor_io), `.agents/rules/rust.md` (the
  explicit-SIMD policy), `docs/fuzzing.md`, `.cargo/mutants.toml` if reference
  oracles need an exclusion.
- Depends on C1 (`document-scale-policy`, branch `document-scale-policy`)
  being merged: P2's fusion and P3 build on its `model/paragraph_boundary.rs`
  and its GTK-paragraph line identities in the minimap accumulator.
