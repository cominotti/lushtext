# Tasks: Domain Value Types and Primitive Boundaries

Kani: Affected shards (because `Bytes`, `Sp`, `Micros`, `CharCount`, typed generations, and enum inputs touch proven pure modules/policies) — `core-memory-policy`, `core-geometry-policies`, `core-minimap-fit`, `core-minimap-binning`, `core-shell-geometry`, `core-post-open-settle`, `core-list-budget`, `core-document-scale`, `core-protected-fold`, `core-protected-install`, `core-protected-install-step`, `core-protected-install-step-visible`, `core-protected-fold-shape`, `core-protected-fold-mapping`, `core-protected-view`, `core-protected-view-navigation`, `core-second-writer`. Run affected pull-request shards via CI/local verification and each touched scheduled shard once detached via `scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=<shard>`, recording runner budgets per `.agents/rules/verification-runs.md`.
Timing: `Verdict class: non-inferiority` on smoke cases `eviction/reselect`, `eviction/switch-hidden-dense`, `recovery/protected-32mib`, `save/during-validation`, `baseline/small/steady-populated` and Criterion groups `editor_memory_policy`, `transient_file_load`, `document_scale`, `save_admission_policy` via `scripts/measure.sh`.

## 1. Failing-first value-type gate & threshold parser extension

- [x] 1.1 Write `scripts/check-domain-value-types.py` with `--self-test` (proving failure on a raw `*generation*` / `editor_id` field in `seams.rs`/`policy.rs`/`*State`, and failure on a unit type in `lushtext-model/src/values/` lacking a `const assert!(size_of...)`); verify `--self-test` fails before passing and the real check fails on the current 204 raw `*generation*` / 5 `editor_id` baseline.
- [x] 1.2 Add failing-first tests to `scripts/check-threshold-kinds.py --self-test` for typed unit constants (`Bytes`, `Sp`, `Micros`, `CharCount`), then implement typed-constant parsing in `scripts/check-threshold-kinds.py`; verify `make check-thresholds` and `--self-test` pass.
- [x] 1.3 Wire `check-domain-value-types` into `Makefile` (`check-policy`) and `.github/workflows/ci.yml`; create a signed commit (`git commit -S`) and push the branch.

## 2. Core value types in `lushtext-model::values`

- [x] 2.1 Implement `define_generation!`, `define_id!`, `Bytes`, `Sp`, `Micros`, `CharCount`, and `LineIndex` in `crates/lushtext-model/src/values/` with `#[repr(transparent)]`, `Copy`, `const fn`, saturating ops, `#[cfg_attr(kani, derive(kani::Arbitrary))]`, and `const _: () = assert!(size_of...)`.
- [x] 2.2 Add unit tests and property tests verifying zero-cost layout, `.next()` wrap behavior, saturating arithmetic, and `size_units` IEC formatting integration; verify `cargo test -p lushtext-model --all-features` passes.
- [x] 2.3 Update `.agents/rules/rust.md` and `AGENTS.md` documenting `lushtext-model::values` and the "no newtype soup" boundary criterion, create a signed commit (`git commit -S`), and push the branch.

## 3. Migrate generations, IDs, units, and multi-bool signatures

- [x] 3.1 Migrate `EditorId` and typed generations (`AccessGeneration`, `PolicyGeneration`, `LoadGeneration`, `EditGeneration`, `SaveGeneration`, `PathGeneration`, `MirrorGeneration`, etc.) across `lushtext-model`, `lushtext-services`, and `lushtext-ui` seams/policies/states until `scripts/check-domain-value-types.py` passes with ceiling 0.
- [x] 3.2 Adopt `Bytes`, `Sp`, `Micros`, `CharCount`, and `LineIndex` in `responsiveness_budget`, `editor_memory`, `protected_fold`, `protected_install`, `ui/window/geometry/policy.rs`, `ui/window/focus_mode/policy.rs`, `ui/sidebar/width_preset.rs`, and `ui/window/tab_overflow/policy.rs`, updating `docs/thresholds.md` rows to match; verify `make check-thresholds` and `make check-workflow-boundaries` pass.
- [x] 3.3 Replace all 48 multi-boolean signatures (`>= 2` positional `bool` parameters) in LushText plus `content_window` in `crates/gtk-lush/axioms/src/probes/a18.rs` with 1-byte `Copy` two-variant enums or named `Copy` fact structs, then enable `max-fn-params-bools = 1` in `clippy.toml` and `fn_params_excessive_bools = "deny"` in `Cargo.toml`; verify `make check` and `make test-unit` pass.
- [x] 3.4 Create a signed commit (`git commit -S`) and push the branch.

## 4. Kani, timing, documentation & full gate verification

- [ ] 4.1 Run all affected scheduled Kani shards via `scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=<shard>` and dispatch PR/scheduled shard budget runs per `.agents/rules/verification-runs.md`.
- [ ] 4.2 Run the pre-registered non-inferiority `scripts/measure.sh` / benchmark campaign and confirm all touched metrics stay within the 5% / 500 µs noise band and under `B_INTERACT_US` / `B_SETTLE_US`.
- [ ] 4.3 Run `make check`, `make check-policy`, `make test-unit`, `make check-agent-docs`, and `openspec validate domain-value-types-and-primitive-boundaries --strict`; update `docs/next/idiomatic-rust-hexagonal-programme.md`, create a signed commit (`git commit -S`), and push the branch.
