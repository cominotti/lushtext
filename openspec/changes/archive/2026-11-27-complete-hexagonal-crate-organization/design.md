# Design: Complete Hexagonal Crate Organization

## Context
`crates/lushtext-model` and `crates/lushtext-services` were extracted from `crates/lushtext-core` in PRs #125 and #126, leaving `crates/lushtext-core` as a UI/app crate that re-exports `model` and `services` while still directly calling `regex` (`find_execution.rs`) and `pulldown_cmark` (8 files in `ui/markdown_preview/`) and still owning non-GTK tests and Criterion benches. Meanwhile, `lushtext-model/src/` (53 top-level items) and `lushtext-services/src/` (44 top-level items) remain flat directories, and `Cargo.toml` has no automated dependency-direction or file-size ratchet.

## Goals / Non-Goals
**Goals:**
- Rename `crates/lushtext-core` to `crates/lushtext-ui` so the three-crate hexagonal boundary (`lushtext-ui -> lushtext-services -> lushtext-model`) is self-evident in `crates/` and `Cargo.toml`.
- Group `lushtext-model` and `lushtext-services` into explicit bounded-context subdirectories, creating `lushtext-model/src/values/` for domain value types.
- Eliminate direct `regex` and `pulldown-cmark` dependencies from `lushtext-ui` and `regex` from `lushtext-model`.
- Move GTK-free benches and tests into `lushtext-model` / `lushtext-services` so they compile and run without GTK4.
- Enforce hexagonal dependency rules via `cargo metadata` and a ratcheted production file-size ceiling (`<= 1,000` lines for new files) in `make check-policy`.

**Non-Goals:**
- Changing runtime behavior, GTK widget layout, `imp.rs` state structures, `anyhow` vs `thiserror` error types, or primitive field types (handled by Changes 2–5).
- Moving `ui/**/policy.rs` out of `lushtext-ui` (violates `.agents/rules/workflow-convention.md`).
- Refactoring the 30 existing `>= 1,000`-line files in this change.

## Decisions

### D1. Rename `crates/lushtext-core` to `crates/lushtext-ui`
- **Decision**: Rename package and directory `crates/lushtext-core` -> `crates/lushtext-ui`, retire the temporary `pub use lushtext_{model,services}` shim in `lushtext-ui/src/lib.rs` by updating internal imports in `crates/lushtext-ui/src/`, `crates/lushtext/tests/`, `fuzz/`, and `scripts/`, and switch `formal/evaluation/stateright/` and `formal/evaluation/verus/drift/` directly onto `lushtext-model` and `lushtext-services` (removing GTK from those formal-evaluation crates).
- **Rationale**: Keeping the name `lushtext-core` for the GTK/Libadwaita driving-adapter crate misleads contributors into placing domain/service logic inside it, and letting `formal/evaluation/` depend on `lushtext-ui` pulls GTK needlessly. Updating all path-keyed gates (`check-visual-proof-policy.py`, `cargo-gtk-proof`, `kani-shards.py`, `accessibility_warning_allowlist.py`, `docs/workflow-readability-matrix.md`, `docs/thresholds.md`) once here prevents every subsequent change in this programme from re-keying paths twice.
- **Alternatives rejected**: Keeping `lushtext-core` as an alias/UI crate (leaves the name inverse to its contents); splitting `lushtext-ui` further per widget (creates crate proliferation and breaks `workflow-convention.md`'s prohibition on extra crates per workflow).

### D2. Bounded-context module trees in `lushtext-model` and `lushtext-services`
- **Decision**:
  - `lushtext-model/src/` groups into: `values/`, `document/`, `workspace/`, `persistence/`, `budgets/`, `automation/`.
  - `lushtext-services/src/` groups into driven adapters (`filesystem/`, `persistence/` for `json_store`, `json_format`, `durable_write`, `recovery_metadata`, `format_upgrade`, `app_data_layout`, `app_data_leftovers`, and `watch/` for `workspace_watch`) and application services (`notes/`, `search/`, `workspace/`, `editor_io/`, `palette/`, `coordination/` for `single_flight`, `protocol_drive`, `sync`, `notifications`, and `instrumentation/` for `kill_point`, `read_fault`, `smoke_list_budgets`, `responsiveness_log`, `test_isolation`, `conformance`).
- **Rationale**: Replaces flat 53- and 44-item root directories with navigable domain/adapter structure without adding trait boilerplate (`rust-hex-arch` rule: prefer direct concrete ports until a seam pays for itself).
- **Alternatives rejected**: Creating separate Cargo crates per bounded context (excessive workspace/hakari/Flatpak vendoring overhead).

### D3. Plugging infra leaks out of `lushtext-ui` and `lushtext-model`
- **Decision**:
  - Move `ui/editor_page/protected_view/find_execution.rs`'s `regex::RegexBuilder` usage into `lushtext-services::search`.
  - Move `pulldown_cmark` event/tag parsing and `InlineFootnoteLowering` (currently imported in `ui/markdown_preview/policy.rs`, which also houses the Kani-checked `preview_width` policy in shard `core-geometry-policies`) out of `ui/markdown_preview/*` into `lushtext-services::markdown_render` returning LushText-owned `MarkdownRenderPlan` / `MarkdownBlock` / `MarkdownInline` domain types (`lushtext-ui` holds zero `pulldown_cmark` imports while `preview_width` and its 6 Kani harnesses remain untouched in `ui/markdown_preview/policy.rs`).
  - Move `regex::RegexBuilder` replacement expansion out of `lushtext-model/src/content_search.rs` into `lushtext-services::search` (keeping pure preview types in `lushtext-model`), leaving `lushtext-model` free of `regex`.
  - Retain `serde_json` in `lushtext-ui` solely for D-Bus automation serialization (`ui/automation.rs`, `fuzzing.rs`) and `crossbeam-channel` solely for `ui/plain_disposal.rs` / `ui/search_panel/execution.rs` channel plumbing, documented in `scripts/check-hexagonal-boundaries.py`'s explicit UI dependency allowlist.
- **Rationale**: Prevents UI widgets and pure domain types from coupling to parser/regex crates while avoiding contrived wrappers over D-Bus JSON or cross-thread channels.
- **Alternatives rejected**: Wrapping `serde_json::to_string_pretty` or `crossbeam_channel::Receiver` in bespoke traits inside `services/` (pure ceremony with zero decoupling gain).

### D4. Mechanical enforcement (`scripts/check-hexagonal-boundaries.py`)
- **Decision**: Add `scripts/check-hexagonal-boundaries.py` (`--self-test`, wired into `make check-policy` and `.github/workflows/ci.yml`) with two checks:
  1. `cargo metadata` dependency DAG check:
     - `lushtext-model`: no GTK/GLib/Adwaita/SourceView/Pango/GDK, no filesystem/OS/network/async-task crates (`rustix`, `notify*`, `ignore`, `dirs`, `tempfile`, `regex`, `anyhow`, `pulldown-cmark`, `crossbeam-channel`). (`gtk-lush-proof-spine` is explicitly allowlisted only for pure schema DTOs in `automation`, or audited if it stays pure Rust.)
     - `lushtext-services`: no GTK/GLib/Adwaita/SourceView/Pango/GDK (`gtk4`, `libadwaita`, `sourceview5`, `gio`, `glib`, `gdk4`, `pango`, `gtk-lush-widgets`, `gtk-lush-signals`, `gtk-lush-settle`, `gtk-lush-tasks`, `gtk-lush-viewport`).
     - `lushtext-ui`: no raw engine/infra crates (`regex`, `pulldown-cmark`, `rustix`, `notify-debouncer-full`, `ignore`, `simdutf8`, `encoding_rs`, `nucleo-matcher`, `editorconfig-parser`).
  2. Production file-size ratchet: excludes `#[cfg(test)]` sections and dedicated test files (`*/tests.rs`, `*_tests.rs`, `*/tests/**`); fails if any new file exceeds `1,000` production lines or if the count/ceiling of existing over-limit files exceeds the recorded baseline (`30` on `main`).
- **Rationale**: Mirrors existing `make check-policy` ratchets (`check-workflow-boundaries.py`, `check-threshold-kinds.py`).
- **Alternatives rejected**: Running `cargo deny` alone (cannot express LushText's per-layer rules or file-size ratchets as cleanly with `--self-test` fixtures).

## Budget Exceptions
None (no timed or pixel-budget arithmetic changes).

## GTK Axiom Ledger
None (no GTK behavior changes; existing axioms and `check-visual-proof-policy.py` / `cargo-gtk-proof` path prefixes are re-keyed verbatim and proved by running their checks and self-tests).

## Timing
Timing: none (no timed path touched). Pure package/module reorganization, test/bench relocation, and dependency-gate additions; Criterion benchmarks moved to `lushtext-model`/`lushtext-services` keep identical algorithms.
