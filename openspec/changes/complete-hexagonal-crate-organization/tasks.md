# Tasks: Complete Hexagonal Crate Organization

Kani: `core-geometry-policies` (pull-request shard — `ui/markdown_preview/policy.rs` is edited when relocating `InlineFootnoteLowering` to `lushtext-services`, while `preview_width` and its 6 Kani harnesses stay intact), plus `make check-kani-shards` and `scripts/kani-shards.py check --self-test` for the `lushtext-core` -> `lushtext-ui` package rename.
Timing: none (no timed path touched).

## 1. Programme record & failing-first boundary gate

- [x] 1.1 Create `docs/next/idiomatic-rust-hexagonal-programme.md` recording the 5-change programme scope, `main` (`3c442112`) baselines, ratchet policy, and ordering rationale; verify `scripts/check-agent-docs.sh` passes.
- [x] 1.2 Write `scripts/check-hexagonal-boundaries.py` with `--self-test` (testing forbidden cross-layer Cargo dependencies in `lushtext-model`, `lushtext-services`, and `lushtext-ui`, plus a new `> 1,000`-line production file rejection) and verify `--self-test` fails on the intentional violation fixtures before passing on valid fixtures, and that running the real check on the current pre-refactor tree fails on the known `lushtext-core` leaks (`regex`, `pulldown-cmark`, missing `lushtext-ui`).
- [ ] 1.3 Wire `check-hexagonal-boundaries` into `Makefile` (`check-policy`) and `.github/workflows/ci.yml`; create a signed commit (`git commit -S`) and push the branch.

## 2. Rename `crates/lushtext-core` to `crates/lushtext-ui` & re-key all gates

- [x] 2.1 Rename `crates/lushtext-core` directory and package name to `crates/lushtext-ui` across `Cargo.toml`, `crates/lushtext/`, `workspace-hack` (`cargo hakari generate`), `meson.build`, `build-aux/`, `snap/`, `.github/workflows/`, `fuzz/` (`Cargo.toml`, `Cargo.lock`, `fuzz_targets/*`), and `formal/evaluation/{stateright,verus/drift}/` (pointing both formal crates directly to `lushtext-model` + `lushtext-services`); retire the temporary `pub use` shim in `lushtext-ui/src/lib.rs` and update internal imports in `crates/lushtext-ui/src/` and `crates/lushtext/tests/`. Verify `cargo check --workspace --all-targets` and `fuzz/` + `formal/evaluation/stateright` compile.
- [x] 2.2 Re-key every path-keyed gate, config, and script from `lushtext-core`/`lushtext_core` to `lushtext-ui`/`lushtext_ui` (including `scripts/kani-shards.py`, `scripts/accessibility_warning_allowlist.py` line 35 log prefix, `scripts/generate-workspace-index-fixture.py`, `scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs`, `scripts/check-workflow-boundaries.py`, `scripts/check-filesystem-boundary.sh`, `scripts/check-explicit-simd.py`, `scripts/check-threshold-kinds.py`, `scripts/check-ui-template-contract.py`, `scripts/check-accessibility-policy.py`, `scripts/check-font-zoom-registration.py`, `scripts/check-settings-writes.py`, `scripts/check-signal-connections.py`, `scripts/check-kill-point-gating.sh`, `.cargo/mutants.toml`, `docs/workflow-readability-matrix.md`, `docs/thresholds.md`); verify every re-keyed check script and `--self-test` passes against the renamed tree.
- [x] 2.3 Update `README.md`, `AGENTS.md`, `crates/lushtext-ui/AGENTS.md`, and `.agents/rules/*.md` for `lushtext-ui`, run `make check-agent-docs`, create a signed commit (`git commit -S`), and push the branch.

## 3. Bounded-context module layout & infra leak removal

- [ ] 3.1 Reorganize `crates/lushtext-model/src/` into `values/`, `document/`, `workspace/`, `persistence/`, `budgets/`, and `automation/`, and move `regex` expansion logic out of `lushtext-model` into `lushtext-services`; verify `cargo test -p lushtext-model` and `make check-kani-shards` pass.
- [ ] 3.2 Reorganize `crates/lushtext-services/src/` into separated driven adapters (`filesystem/`, `persistence/`, `watch/`) and application services (`notes/`, `search/`, `workspace/`, `editor_io/`, `palette/`, `coordination/`, `instrumentation/`); verify `cargo test -p lushtext-services` passes.
- [ ] 3.3 Move `regex` usage from `ui/editor_page/protected_view/find_execution.rs` and `pulldown_cmark` usage (including `InlineFootnoteLowering` in `ui/markdown_preview/policy.rs`) from `ui/markdown_preview/*` behind `lushtext-services` domain APIs, keeping `preview_width` and its 6 Kani harnesses intact in `ui/markdown_preview/policy.rs`, and remove `regex` / `pulldown-cmark` from `crates/lushtext-ui/Cargo.toml`; verify `cargo test -p lushtext-ui`, `make test-unit`, and `core-geometry-policies` Kani shard pass.
- [ ] 3.4 Update `AGENTS.md`, `README.md`, `docs/workflow-readability-matrix.md`, and `docs/thresholds.md` with the organized module paths, run `make check-policy`, create a signed commit (`git commit -S`), and push the branch.

## 4. Relocate GTK-free tests/benches & green full validation

- [ ] 4.1 Move GTK-free tests (`persistent_json_format`, `text_scan_allocations`, non-GTK `properties` modules) and GTK-free Criterion benchmarks from `crates/lushtext-ui` into `crates/lushtext-model` and `crates/lushtext-services`; verify `cargo test -p lushtext-model --all-features`, `cargo test -p lushtext-services --all-features`, and `cargo bench --no-run` build and pass without GTK.
- [ ] 4.2 Run `python3 scripts/check-hexagonal-boundaries.py` (confirming 0 violations and file-size ratchet at `<= 30` legacy files), `make check`, `make check-policy`, `make test-unit`, and `openspec validate complete-hexagonal-crate-organization --strict`.
- [ ] 4.3 Update `docs/next/idiomatic-rust-hexagonal-programme.md` with Change 1 completion metrics, create a signed commit (`git commit -S`), and push the branch.
