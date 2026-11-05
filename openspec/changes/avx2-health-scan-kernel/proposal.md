## Why

`simd-line-scans` (PR #71) rewrote the load's file-health scan (the count of
non-breaking spaces and zero-width characters) with `memchr3` over the lead
bytes `C2`/`E2`/`EF`. Its explicit-SIMD gate (task 4.3, design D6) then found
the scan still at **19.3 %** of `load_text_file` on typographic prose (27.8 of
143.8 ms): curly quotes, dashes and ellipses all start with `0xE2`, so
`memchr3` stops on every one of them and confirms it in scalar code. The gate's
threshold is 10 %, so a hand-written kernel became admissible and was queued in
`docs/next/long-line-documents.md`.

This change's spec stage measured whether a kernel pays before anything was
proposed (bar written down first:
`.claude/handoffs/avx2-health-scan-kernel/bar.md`). A prototype AVX2 kernel,
interleaved against `main` plus the same benches (three rounds a side, CPUs
8–23, load 1.8–3.96 at every start), measured:

- the health scan on typographic text **4.56x** faster (27.8 → 6.10 ms on
  50 MiB), its share of the load **4.8 %**;
- `load_text_file` on typographic text **1.17–1.24x** faster at 1, 10 and 50 MB
  (2.20 → 1.78 ms, 23.8 → 19.6 ms, 140.3 → 120.4 ms);
- `load_text_file` on ASCII at parity (1.005–1.04x), and every other health
  fixture faster or within the noise bar (multibyte 0.978x, within main's
  spread).

Prose with typographic punctuation is ordinary user content (Markdown notes,
exported documents, books), so opening such a file gets 14–19 % faster on the
worker, ahead of installation. Nothing changes on the GTK thread.

## What Changes

- Add a 64-byte AVX2 block kernel for the file-health count in its own module,
  `services/editor_io/text_scans/health_avx2.rs`, the only file the
  explicit-SIMD allowlist will name. It skips all-ASCII blocks and blocks with
  no candidate lead byte, and otherwise confirms the two continuation bytes in
  vector registers (loads at +1 and +2), counting hits with `popcnt`, with no
  per-candidate branch.
- `text_scans::count_health_characters` dispatches to it at run time when the
  CPU has AVX2 and POPCNT (`is_x86_feature_detected!`, cached by the standard
  library). Otherwise, and on every non-x86-64 target, it runs the current
  `memchr3` scan, which stays as the fallback and as the kernel's tail. The
  shipped binary stays baseline x86-64, and no build sets a CPU flag.
- The kernel is pinned to the `reference` oracle and to the fallback by an
  exhaustive unit test over every byte triple at every lane, block and tail
  position, by property tests (including slicing at chunk and block
  boundaries), by the `editor_bytes` fuzz target with new seeds, and by a
  counting-allocator test. It sits behind `cfg(not(kani))`.
- The rule, programme and benchmark records are updated: an allowlist row in
  `.agents/rules/rust.md` with NEON recorded as unmeasured (aarch64 keeps
  `memchr`'s NEON path), the interleaved verdict in
  `docs/benchmarks/transient-file-load.md`, the queued item in
  `docs/next/long-line-documents.md` closed, and `AGENTS.md`/`docs/fuzzing.md`
  brought in line.
- A bench-first commit adds `editor_file_io/load_text_file_typographic/{1,10,50}MB`
  and the `text_scans/health_fallback/*` cases, so later changes can compare
  against the same benchmark IDs.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `text-scan-acceleration`: adds the requirements for the run-time-selected
  AVX2 file-health kernel (exact equivalence with the fallback and the
  reference, dispatch and fallback, its allowlist admission and verification,
  and its landing verdict). The existing requirements are unchanged.

## Impact

- Code: `crates/lushtext-core/src/services/editor_io/text_scans.rs` (dispatch,
  fallback naming), new `text_scans/health_avx2.rs`, bench hooks in
  `services/editor_io.rs`, `benches/benchmarks.rs`,
  `tests/properties/text_scans.rs`, `tests/text_scan_allocations.rs`,
  `src/fuzzing.rs` and the `editor_bytes` corpus.
- Policy: one allowlist row (`.agents/rules/rust.md`), one
  `#[expect(clippy::disallowed_macros)]` site, possibly a `.cargo/mutants.toml`
  exclusion for the equivalent dispatch mutant.
- No GTK, persistence, D-Bus or settings change, no new dependency, no
  user-facing text. File-health findings are byte-identical.
