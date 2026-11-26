# Proposal: Complete Hexagonal Crate Organization

## Why
Commits `10d57a29` (PR #125) and `b1be5238` (PR #126) extracted `crates/lushtext-model` and `crates/lushtext-services`, leaving `crates/lushtext-core` as a misleadingly named GTK/UI crate that still depends directly on `regex`, `pulldown-cmark`, `serde_json`, and `crossbeam-channel` and hosts non-GTK tests and benchmarks. Meanwhile, `lushtext-model/src` (53 top-level items) and `lushtext-services/src` (44 top-level items) remain flat file dumps, 30 production files exceed the 1,000-line guideline, and no Cargo dependency gate prevents layer regressions.

## Baseline
Measured on `main` (`3c442112`) with a tree/Cargo inspection across `crates/lushtext-{model,services,core}/src`:
- `crates/lushtext-model/src/`: 53 top-level items; `content_search.rs` uses `regex::RegexBuilder`; `automation.rs` depends on `gtk-lush-proof-spine`.
- `crates/lushtext-services/src/`: 44 top-level items mixing driven adapters (`filesystem/`, `json_store.rs`, `workspace_watch.rs`) with application services and test-only instrumentation (`kill_point.rs`, `read_fault.rs`, `conformance/`).
- `crates/lushtext-core/src/`: `ui/editor_page/protected_view/find_execution.rs` calls `regex` directly; 8 files in `ui/markdown_preview/` import `pulldown_cmark` directly; `crates/lushtext-core/benches/benchmarks.rs`, `tests/persistent_json_format.rs`, and `tests/properties.rs` build under the GTK crate.
- Production files >= 1,000 production lines: **30** (led by `scan_execution.rs` at 2,845, `ui/automation.rs` at 2,512, `editor_io.rs` at 1,877, `journal_model.rs` at 1,710, `conformance/draft_service.rs` at 1,692).
- Excluded external consumers & runtime allowlists: `fuzz/Cargo.toml` (+ 3 fuzz targets), `formal/evaluation/stateright/`, `formal/evaluation/verus/drift/`, `scripts/accessibility_warning_allowlist.py` (`lushtext_core::...`), and `scripts/generate-workspace-index-fixture.py` still reference `lushtext-core` / `lushtext_core`.
- `openspec/specs/`: **10** existing capability specs still reference `lushtext-core` or pre-extraction `crates/lushtext-core/src/{model,services}` paths.

## What Changes
- **BREAKING** (internal workspace crate name): Rename `crates/lushtext-core` (`lushtext-core`) to `crates/lushtext-ui` (`lushtext-ui`) across Cargo manifests, Meson/Flatpak/Snap packaging, CI workflows, scripts, Kani/mutants/visual-proof configs, docs, and the 10 existing specs that name `lushtext-core`.
- Organize `crates/lushtext-model/src/` into bounded-context submodules (`values/`, `document/`, `workspace/`, `persistence/`, `budgets/`, `automation/`), keeping `values/` ready as the home for domain value types while preserving `ui/**/policy.rs` co-location in `lushtext-ui` per `.agents/rules/workflow-convention.md`.
- Organize `crates/lushtext-services/src/` into bounded contexts separating driven adapters (`adapters/` or `filesystem/`, `persistence/`, `watch/`) from application services (`notes/`, `search/`, `workspace/`, `editor_io/`, `palette/`, `coordination/`, `instrumentation/`).
- Remove direct infrastructure engine coupling from `lushtext-ui` and `lushtext-model`: route `find_execution.rs` regex search through `lushtext-services`, route `ui/markdown_preview/` behind a `lushtext-services::markdown_render` domain plan model (zero `pulldown-cmark` in `lushtext-ui`), move `regex` compilation/matching out of `lushtext-model`, and document narrow `serde_json` / `crossbeam-channel` UI allowlist entries (`ui/automation.rs`, `ui/plain_disposal.rs`).
- Relocate GTK-free tests (`persistent_json_format`, non-GTK `properties` modules, `text_scan_allocations`) and GTK-free Criterion benchmarks into `lushtext-model` / `lushtext-services` so they build and run without linking GTK4.
- Add `scripts/check-hexagonal-boundaries.py` (`--self-test`, wired into `make check-policy` and CI) enforcing Cargo dependency allowlists via `cargo metadata` and a ratcheted `<= 1,000`-line ceiling on new production files (with a descending ceiling for the 30 existing over-limit files).
- Create programme tracking record `docs/next/idiomatic-rust-hexagonal-programme.md`.

## Non-goals
- Changing runtime algorithms, GTK widget structure, `imp.rs` layout, error types (`anyhow` vs `thiserror`), or primitive field types (covered by the subsequent changes in this programme).
- Relocating single-workflow `ui/**/policy.rs` modules out of `lushtext-ui` (would violate `.agents/rules/workflow-convention.md`).
- Splitting the 30 existing >= 1,000-line files in this change (this change locks the ratchet so no new >= 1,000-line file can be introduced).

## User-visible impact
Zero user-visible or behavioral change compared with `main`. Packaging (`meson.build`, Flatpak, Snap) builds the identical `lushtext` binary with identical features, GSettings schema, and GResources.

## Capabilities

### New Capabilities
- `hexagonal-crate-boundaries`: Enforce LushText's three-crate hexagonal workspace layout (`lushtext-model`, `lushtext-services`, `lushtext-ui`), bounded-context module organization, dependency allowlists, and production file-size ceilings mechanically.

### Modified Capabilities
- `change-delivery-flow`: Update crate/path references from `lushtext-core` and `crates/lushtext-core/src/services/` to `lushtext-ui` and `crates/lushtext-services/src/`.
- `draft-session-recovery`: Update crate references from `lushtext-core` to `lushtext-ui`, `lushtext-services`, and `lushtext-model`.
- `flathub-publication`: Update `crates/lushtext-core/Cargo.toml` version-sync target to `crates/lushtext-ui/Cargo.toml` (plus `lushtext-model` and `lushtext-services`).
- `gtk-lush-proof-harness`: Update `lushtext-core` dependency prohibition reference to `lushtext-ui`.
- `gtk-lush-workspace`: Update `lushtext-core` dependency prohibition references to LushText application crates (`lushtext-model`, `lushtext-services`, `lushtext-ui`).
- `long-line-document-presentation`: Update Kani harness paths from `crates/lushtext-core/src/model/...` and `crates/lushtext-core/src/ui/...` to `crates/lushtext-model/src/...` and `crates/lushtext-ui/src/...`.
- `settle-timer-normalization`: Update crate path references from `crates/lushtext-core` to `crates/lushtext-ui`.
- `text-scan-acceleration`: Update `memchr` dependency-graph target from `lushtext-core` to `lushtext-services` / `lushtext-model` / `lushtext-ui`.
- `visual-geometry-invariants`: Update unrelated-path example from `crates/lushtext-core/src/services/draft_service.rs` to `crates/lushtext-services/src/draft_service.rs` (or its organized path).
- `workflow-readability-boundaries`: Update `_for_test` ratchet scan root from `crates/lushtext-core/src` to `crates/lushtext-ui/src`.

## Impact
- **Workspace crates**: `Cargo.toml`, `crates/lushtext-ui/` (renamed from `crates/lushtext-core/`), `crates/lushtext-model/`, `crates/lushtext-services/`, `crates/lushtext/`, `workspace-hack/`.
- **Excluded crates**: `fuzz/` (`Cargo.toml`, `Cargo.lock`, `fuzz_targets/*`), `formal/evaluation/stateright/` (switching directly to `lushtext-model` + `lushtext-services`), and `formal/evaluation/verus/drift/` (switching directly to `lushtext-model` + `lushtext-services`).
- **Build & Packaging**: `Makefile`, `meson.build`, `build-aux/`, `snap/`, `.github/workflows/`.
- **Kani**: `ui/markdown_preview/policy.rs` (`core-geometry-policies` shard, 6 harnesses on `preview_width`) loses its `pulldown_cmark` import as `InlineFootnoteLowering` moves to `lushtext-services`; `scripts/kani-shards.py` re-keys `lushtext-core` -> `lushtext-ui`.
- **Gates & Tooling**: new `scripts/check-hexagonal-boundaries.py`; path/package updates in `scripts/kani-shards.py`, `scripts/accessibility_warning_allowlist.py`, `scripts/generate-workspace-index-fixture.py`, `scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs`, `scripts/check-workflow-boundaries.py`, `scripts/check-filesystem-boundary.sh`, `scripts/check-explicit-simd.py`, `scripts/check-threshold-kinds.py`, `scripts/check-ui-template-contract.py`, `scripts/check-accessibility-policy.py`, `scripts/check-font-zoom-registration.py`, `scripts/check-settings-writes.py`, `scripts/check-signal-connections.py`, `scripts/check-kill-point-gating.sh`, `.cargo/mutants.toml`.
- **Docs**: `README.md`, `AGENTS.md`, `crates/lushtext-ui/AGENTS.md`, `.agents/rules/*.md`, `docs/workflow-readability-matrix.md`, `docs/thresholds.md`, `docs/next/idiomatic-rust-hexagonal-programme.md`.
