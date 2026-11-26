# Tasks: Idiomatic Rust State Machines and C-ism Removal

Kani: Check `scripts/kani-shards.py oracle` for `ui::search_bar::policy` (none) and any touched `model/` or `policy.rs` files; expected `Kani: none (no proven module touched)` unless `editor_memory` or `load/policy.rs` state types change — if any proven module's inputs change, run the affected shard(s) per `.agents/rules/verification-runs.md`.
Timing: `Verdict class: non-inferiority` on smoke cases `baseline/small/steady-populated`, `eviction/detach-minimap`, `minimap-markers/4096`, `save/during-validation`, `reload/250000`, `session/restore-3`, `replace-all/run-512mib` and Criterion groups `tree_population`, `search_interactive_policies`, `all_tabs_filter_1024`, `save_admission_policy`, `transient_file_load` via `scripts/measure.sh`.

## 1. Failing-first state/C-ism gate

- [ ] 1.1 Write `scripts/check-idiomatic-state.py` with `--self-test` proving detection of (a) `imp.rs` > 500 prod lines, (b) `imp.rs` with > 2 `Box<dyn Fn...>` fields, (c) `*State`/`*Session`/`*Progress` with >= 4 `Cell<bool>`/`bool`/`RefCell<Option<_>>`/`Option<_>` fields, (d) `fn reset_*_state`, and (e) `-1` sentinel outside an FFI wrapper; verify `--self-test` passes and the real scan fails on the `main` baseline (35 bags, 9 oversized `imp.rs`, 49 `Box<dyn Fn>` across 11 `imp.rs`, 5 `reset_*_state`, 2 `-1` sentinels).
- [ ] 1.2 Wire `check-idiomatic-state` into `Makefile` (`check-policy`) and `.github/workflows/ci.yml`, create a signed commit (`git commit -S`), and push the branch.

## 2. Algebraic state machines & `imp.rs` slimming (Batch A: editor_page, window, tab_overflow)

- [ ] 2.1 Refactor `LoadState`, `MinimapState`, `BufferReplacementSession`, `LanguageAttachState`, `ChunkedSnapshotSession`, `HeldInputState`, `TabOverflowState`, `DraftState`, `SessionState`, and `EditorMemoryState` into single-`RefCell<Phase>` algebraic state machines housed inside their owning bounded coordination modules (`execution.rs`, `watch.rs`, `journal.rs`, `*_execution.rs` — no new `state.rs` role), preserving all evidence-surface side-effect-freedom and disposal-honesty tests.
- [ ] 2.2 Consolidate `ui/editor_page/imp.rs` (1,230L -> <= 500L) and `ui/window/imp.rs` (1,395L -> <= 500L) and their callback farms into typed GObject signals / single typed event callbacks; update `docs/workflow-readability-matrix.md` re-derived cells, run `make check-workflow-boundaries`, `make check-signal-connections`, and `make test-widget-shard` for editor/window shards.
- [ ] 2.3 Create a signed commit (`git commit -S`) and push the branch.

## 3. Algebraic state machines & `imp.rs` slimming (Batch B: remaining widgets & state bags)

- [ ] 3.1 Refactor `SearchPreviewState`, `SearchRuntimeState`, `LocalHistoryBrowserState`, `WatchRuntimeState`, `TransientSurfaceState`, and remaining `>= 4`-field `bool`/`Option` state bags (all 35 down to 0) into phase enums with `mem::take` / `RefCell::replace`.
- [ ] 3.2 Slim the remaining 7 oversized `imp.rs` (`search_panel`, `command_palette`, `open_popover`, `workspace_section`, `markdown_preview`, `all_tabs_popover`, `preferences`) to `<= 500` lines (keeping `ui/row_text/imp.rs` at 488L unsplit) and `<= 2` `Box<dyn Fn...>` callback slots each; update `docs/workflow-readability-matrix.md` and run `make check-workflow-boundaries`, `make check-signal-connections`, and affected widget shards.
- [ ] 3.3 Create a signed commit (`git commit -S`) and push the branch.

## 4. C/C++-ism cleanup, Clippy hardening & full verification

- [ ] 4.1 Remove all 5 `reset_*_state` functions, convert `ui/search_bar/policy.rs` and `ui/search_panel/results.rs` `-1` sentinels to FFI-edge `Option` wrappers, refactor non-SIMD `while idx < len` loops (25 sites) to iterators/`array_windows`, and replace fallible/unneeded `as` casts (323 sites) with `From`/`TryFrom`.
- [ ] 4.2 Enable `max-struct-bools = 3` in `clippy.toml` and `struct_excessive_bools = "deny"` + `cast_possible_wrap = "deny"` in `Cargo.toml` (adding reasoned `#[expect]` only on pure `*Evidence`/`*Snapshot` DTOs, domain feature/fact masks such as `DocumentScale`/`*Facts`, and Kani model harnesses); verify `make check`, `make check-policy`, `make test-unit`, and widget tests pass.
- [ ] 4.3 Run the pre-registered non-inferiority timing verification via `scripts/measure.sh` / `make responsiveness-smoke`, update `.agents/rules/rust.md`, `.agents/rules/widget-wiring.md`, `AGENTS.md`, and `docs/next/idiomatic-rust-hexagonal-programme.md`, run `openspec validate idiomatic-rust-state-machines-and-c-ism-removal --strict`, create a signed commit (`git commit -S`), and push the branch.
