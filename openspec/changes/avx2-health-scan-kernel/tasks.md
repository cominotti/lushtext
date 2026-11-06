## 1. Preconditions and benchmarks first

- [x] 1.1 Rebase on `origin/main` and re-read `services/editor_io/text_scans.rs`, `text_scans/reference.rs`, `analyze_decoded_content_cancellable`, `scripts/check-explicit-simd.py` and the "Explicit SIMD" section of `.agents/rules/rust.md`. If a name, the processing-slice size (256 KiB, 1 MiB direct) or the gate's allowlist parsing moved, update design D2/D3. Do not depend on PR #84 (`power-of-two-thresholds`).
- [x] 1.2 Bench-first commit, alone, so it can serve as baseline A. Apply `.claude/handoffs/avx2-health-scan-kernel/bench-first.patch` (`editor_file_io/load_text_file_typographic/{1,10,50}MB`). Add the `test-utils` hook `bench_health_scan_fallback`, which calls today's scan, and the Criterion cases `text_scans/health_fallback/*`, so both sides carry the same IDs.

## 2. Failing-first tests

- [x] 2.1 Rename today's body to `count_health_characters_memchr(bytes: &[u8], from: usize)` (D3). Visibility is `pub` under `cfg(any(test, feature = "test-utils", feature = "fuzzing"))` and private otherwise. `count_health_characters` keeps calling it. Add `count_health_characters_kernel(slice) -> Option<(usize, usize)>` under the same cfg (D2), returning `None` until 3.1 lands and on non-x86-64 / `cfg(kani)`; tests and fuzzing reach the kernel only through it, never with a detection macro of their own. Existing unit and property tests stay green.
- [x] 2.2 Add the D5 property test to `tests/properties/text_scans.rs`: texts of at least 66 bytes with hits and look-alikes at offsets around the 32/64-byte seams, with slices cut so that every tail length 0–65 occurs, comparing the dispatching entry, the fallback and the reference (and the kernel through `count_health_characters_kernel` when it returns `Some`).
- [x] 2.3 Extend `fuzzing.rs`/`reference::assert_scans_match` to compare the fallback, and the kernel when available, with the reference separately. Add the seeds `fuzz/corpus/editor_bytes/text_scans_health_block_seams.txt` and `text_scans_health_typographic_run.txt`.
- [x] 2.4 Add the D5 counting-allocator case to `tests/text_scan_allocations.rs` (1 MiB typographic text, zero allocations).
- [x] 2.5 Add the exhaustive lane test (D5) against a stub kernel entry that deliberately returns the wrong count, and confirm that 2.2, 2.3 (`make fuzz-corpus-replay`) and the lane test fail. Keep the stub in a scratch commit that is never pushed, and drop it.

## 3. Implementation

- [x] 3.1 Add `services/editor_io/text_scans/health_avx2.rs`, declared `#[cfg(all(target_arch = "x86_64", not(kani)))]`, with the D1 kernel (starting from `.claude/handoffs/avx2-health-scan-kernel/prototype-v2.patch`), the safe `count(bytes) -> Option<(usize, usize)>` with `is_x86_feature_detected!("avx2")` and `("popcnt")` under `#[expect(clippy::disallowed_macros, reason = "...allowlist row...")]`, and a `SAFETY` comment on every `unsafe` block. Write integer conversions without `as` where the lint policy asks for that.
- [x] 3.2 Dispatch in `count_health_characters`: `health_avx2::count(bytes)`, or the fallback when it returns `None`. The module docs of `text_scans.rs` drop "no `unsafe`" for the dispatching entry and point to the kernel file.
- [x] 3.3 Add the allowlist row to `.agents/rules/rust.md` (D7) and run `make check-explicit-simd`, including its `--self-test`, and `make check-policy`.
- [ ] 3.4 Run the lane test and record its debug and release wall time (target under 10 s in debug; batch triples per D5 if it is slower). Attempt `cargo miri test` on the lane test with a reduced triple range, passing `-Ctarget-feature=+avx2,+popcnt` on the command line only (D5; without it Miri's detection returns false and the kernel is skipped). Record whether Miri supports the intrinsics used, and the result.
- [ ] 3.5 `cargo mutants` on `text_scans.rs` and `health_avx2.rs` (`make mutants-list` first). Add narrow `exclude_re` entries for the equivalent mutants D5 names (availability returning `None`, the detection conjunction, skip predicates weakened toward confirming more blocks), each with the D5 reason; a mutant that skips a block it should confirm must be killed. Fix, or justify in `.cargo/mutants.toml`, every other survivor.
- [ ] 3.6 Kani: confirm that no harness compiles `text_scans` (`make check-kani-shards` passes and the shard table is unchanged). Build the cheapest shard that compiles `lushtext-core` under `cfg(kani)` (`make kani KANI_SHARD=core-second-writer`, 6.6 CI minutes, under `scripts/with-memory-cap.sh`, one CBMC at a time; every shard compiles the whole crate, so the cheapest proves the gate as well as any) to prove the `cfg(not(kani))` gate keeps the intrinsics out of the Kani build.
- [ ] 3.7 Run `make test-unit`, `make test-prop`, `make test-prop-deep PROPTEST_DEEP_CASES=1024`, `make fuzz-corpus-replay`, a bounded `make fuzz-smoke` for `editor_bytes`, `cargo check -p lushtext --bins --locked` (default features) and `make check`. All must be green, every heavy command through `scripts/heavy.sh`.

## 4. Interleaved verdict

- [ ] 4.1 Build both bench binaries with toolchain 1.98.1 (`RUSTUP_TOOLCHAIN` explicit): A = the 1.2 commit, B = the branch tip. Run the D6 filter, alternating A and B, three rounds a side. Use `taskset -c 8-23 scripts/measure.sh` for each run, start each only at 1-min load < 4, and record the load before and after (`.claude/handoffs/avx2-health-scan-kernel/interleave.sh` is the template). Query `btrfs-stall-report` for any outlier round.
- [ ] 4.2 Apply bars B1–B5 (design D6) with the median of round medians and the spread of round medians. If any bar is missed, try D6's fallbacks in order and remeasure. If it is still missed, remove the dispatch and the allowlist row, record the numbers and stop.

## 5. Lanes, axioms and documentation

- [ ] 5.1 GTK axioms and samples: none. The change relies on no toolkit behaviour (worker-side byte scan). Record that in the PR. Responsiveness lane: no new case, because the load analysis runs off the GTK thread and its install path is unchanged. `make end-user-smoke` is not required.
- [ ] 5.2 `docs/benchmarks/transient-file-load.md`: a new "AVX2 file-health kernel" section with the final table and the load record, and the 4.3 gate paragraph pointing at it.
- [ ] 5.3 `docs/next/long-line-documents.md`: rewrite only the "AVX2 file-health kernel" bullet under "Queued follow-ups from `simd-line-scans`" as closed, with the headline figures. Touch no other line (PR #84 edits that file).
- [ ] 5.4 `AGENTS.md` (the `editor_io/text_scans.rs` layout line and "allowlist empty" in "Large file handling"), `docs/fuzzing.md` (the new comparisons and seeds) and `docs/end-user-coverage.md` (the text-scan row: lane test). README unchanged (D7). `.agents/rules/rust.md` is changed only by the allowlist row from 3.3.
- [ ] 5.5 Run `make check-agent-docs`, the rustdoc gate from `build.md`, and `openspec validate avx2-health-scan-kernel --strict`.
