## 1. Preconditions and benchmarks first

- [ ] 1.1 Confirm C1 (`document-scale-policy`) is merged on `main`; re-read its merged `model/paragraph_boundary.rs` (`next_paragraph_break`, `count_scalars`, `ParagraphProfileAccumulator::push_str`), the minimap `inspect_char`, and `analyze_decoded_content_cancellable`, and update design D3/D4 if any name or behaviour moved.
- [ ] 1.2 Add the D7 Criterion cases (`text_scans/load_analysis/*`, `text_scans/minimap/*` through `inspect_slice`, `text_scans/save/*`) using only entry points that exist before this change, with seeded synthetic fixtures (ASCII, multibyte, typographic, mixed EOL, one 1.9M-character line, clean and dirty trailing whitespace); commit this alone so it can be cherry-picked onto the baseline worktree.
- [ ] 1.3 Port the H1, H3, and H4 edge cases from `.claude/handoffs/simd-exploration/exp-H*/src/main.rs` as named unit tests against the **current** implementations (they must pass before any rewrite), including `\r\r\n`, delimiters at slice ends, NBSP/zero-width at slice ends, and the look-alike leads `C2 A9`, `E2 80 99`, `EF BB BE`.

## 2. Oracles and failing-first equivalence

- [ ] 2.1 Create `services/editor_io/text_scans.rs`; move the current health loop, `trim_trailing_space_and_tabs`, `normalize_line_endings` body, and `line_endings_already_normalized` verbatim into `text_scans::reference` under `cfg(any(test, feature = "test-utils", feature = "fuzzing"))`; production keeps calling the moved scalar code for now. Add the mutation exclusion for the reference module in `.cargo/mutants.toml` with its reason and confirm with `make mutants-list` that the production scans still generate mutants.
- [ ] 2.2 Add `crates/lushtext-core/tests/properties/text_scans.rs` with generators biased per the spec (delimiters, hit and non-hit lead bytes, U+2028/U+2029, 4-byte scalars) and comparisons for: health counts with every slicing, trim, normalize in LF/CRLF/CR, the already-normalized check, and `inspect_str` vs per-character `inspect_char` over small policies (thresholds 0–8, marker limit 0–3) and every split into consecutive calls.
- [ ] 2.3 Extend `fuzz/fuzz_targets/editor_bytes.rs` with the same fast-vs-reference comparisons on every decodable input; add corpus seeds for the edge cases of 1.3; run `make fuzz-corpus-replay`.
- [ ] 2.4 Failing-first: in a scratch commit (never pushed), point each fast entry point at a deliberately wrong placeholder and confirm 2.2 and 2.3 fail for each one; drop the scratch commit.

## 3. Implementation

- [ ] 3.1 P2: implement the `memchr3(0xC2, 0xE2, 0xEF)` health scan with continuation confirmation (D2) and a unit test that its byte table equals `is_zero_width`'s set; switch `analyze_decoded_content_cancellable` to it (variant A).
- [ ] 3.2 P2: implement variant B (64 KiB sub-block fusion of tally, profile, and health, each with its own carry) behind a private switch used only by the bench, so both variants are measured from one binary as well as interleaved.
- [ ] 3.3 P3: make `count_scalars` `pub(crate)`; implement `MinimapAnalysisAccumulator::inspect_str` (D4) with the per-character loop under `cfg(kani)`; keep `inspect_slice` and `inspect_char` unchanged; confirm the module's whole-pixel lints and `make check-workflow-boundaries` stay green and record the new function in the minimap WFR row if the matrix lists the accumulator's API.
- [ ] 3.4 P4: implement trim returning `Cow<'_, str>`, normalize, and the CRLF check over `memchr2_iter` without `unsafe` (D5); thread the `Cow` through `apply_save_formatting_overrides_borrowed`; add a unit test that a clean text comes back `Cow::Borrowed` (pointer-equal).
- [ ] 3.5 Run `make test-unit`, `make test-prop`, `make test-prop-deep PROPTEST_DEEP_CASES=1024`, `make fuzz-corpus-replay`, a bounded `make fuzz-smoke` for `editor_bytes`, `make kani KANI_SHARD=<the minimap and paragraph-boundary shards>`, and `make check`; all green.

## 4. Interleaved verdict

- [ ] 4.1 In a disposable baseline worktree (main plus the 1.2 commit) and the branch, run the D7 cases alternating baseline/branch, three rounds per side, nothing heavy admitted, load recorded; add the pre-C1 commit `1e2556a2` as a third side for `editor_file_io/load_text_file/{1,10,50}MB`. Query `btrfs-stall-report` for any outlier round.
- [ ] 4.2 Apply the D7 acceptance bars: choose P2 variant A or B per D3; land or revert P3 and each P4 function per its bar; record every median and min–max in `docs/benchmarks/transient-file-load.md` and the C1 residual entry of `docs/next/long-line-documents.md` (closed, or re-recorded with the new figures).
- [ ] 4.3 Explicit-SIMD gate (D6): from the branch run, compute the health scan's share of `load_text_file` on the typographic fixture; record it. If at or above 10 %, open a follow-up proposal for the AVX2 kernel with the D6 conditions; otherwise record that no kernel is admitted.

## 5. Documentation and close-out

- [ ] 5.1 Add the explicit-SIMD policy (D6) to `.agents/rules/rust.md` and point `.agents/skills/gtk-perf-rust-optimize/SKILL.md` at it.
- [ ] 5.2 Update `AGENTS.md` (module layout: `services/editor_io/text_scans.rs`; the large-file and load-analysis design notes), `docs/fuzzing.md` (new `editor_bytes` comparisons), and `docs/end-user-coverage.md` if the property/fuzz lane descriptions list covered scans.
- [ ] 5.3 Record the minimap off-GTK-thread follow-up (per-turn `get_slice` or a worker over a buffer snapshot, feeding `inspect_str`; WFR minimap `analysis_execution`) in `docs/next/long-line-documents.md` as queued work with a pointer to REPORT.md P3.
- [ ] 5.4 Run `make check-agent-docs`, the rustdoc gate from `build.md`, and `openspec validate simd-line-scans --strict`.
