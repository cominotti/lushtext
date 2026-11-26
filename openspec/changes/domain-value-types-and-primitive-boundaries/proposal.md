# Proposal: Domain Value Types and Primitive Boundaries

## Why
LushText currently passes raw integer primitives (`u64`, `usize`, `u32`, `i32`) and multiple positional `bool` parameters across domain, service, and UI workflow boundaries—making unit confusion (`Bytes` vs `CharCount`, `Sp` vs Pango units), generation swaps, and boolean argument transpositions invisible to the type checker.

## Baseline
Measured on `main` (`3c442112`) across `crates/lushtext-{model,services,core}/src` (excluding test files and `#[cfg(test)]`):
- Raw integer `*generation*` fields (`u64` / `u32` / `usize`): **204** (`Cell<u64>`, `u64`, `Option<u64>`, `Cell<u32>`, etc.).
- Raw `editor_id: usize` fields: **5** (including `EditorResidency::editor_id` in `lushtext-model/src/editor_memory.rs`).
- Production functions with `>= 2` positional `bool` parameters: **48** in `lushtext-{model,services,core}` (up to 5 positional `bool`s in `ui/window/drafts/policy.rs::draft_candidate_is_eligible` and 4 in `model/content_search.rs::ContentSearchOptions::new`) plus **1** private probe helper (`content_window`) in `crates/gtk-lush/axioms/src/probes/a18.rs`.
- Workspace Clippy lints (`Cargo.toml`): `fn_params_excessive_bools` is not yet denied.
- `scripts/check-threshold-kinds.py` (`docs/thresholds.md`): currently expects bare integer `const` / `static` declarations and does not yet parse zero-cost typed unit constants (`Bytes`, `Sp`, `Micros`, `CharCount`).

## What Changes
- Introduce zero-cost, layout-verified (`#[repr(transparent)]`, `Copy`, `const fn`, `const assert!(size_of == ...)`, `#[cfg_attr(kani, derive(kani::Arbitrary))]`) domain value types in `lushtext-model::values`:
  - `define_generation!` and `define_id!` internal macros generating distinct typed generations (`LoadGeneration`, `EditGeneration`, `SaveGeneration`, `PathGeneration`, `MirrorGeneration`, `AccessGeneration`, `PolicyGeneration`, `LossyAnalysisGeneration`, `OverscrollGeneration`, etc.) with `.next()`, plus `EditorId` and sibling domain IDs.
  - Typed unit wrappers with saturating arithmetic and explicit boundary constructors: `Bytes(u64)`, `Sp(i32)` (integrating with the whole-pixel geometry policy), `Micros(u64)`, `CharCount(u64)`, and `LineIndex(u32)`.
- Migrate generation, `editor_id`, and unit boundaries across `lushtext-model`, `lushtext-services`, and `lushtext-ui` seams/policies/states, while leaving internal loop indices and single-scope counters as primitives ("no newtype soup").
- Replace all **48** functions accepting `>= 2` positional `bool` parameters with 1-byte `Copy` two-variant enums or named `Copy` fact structs.
- Extend `scripts/check-threshold-kinds.py` (`--self-test`) and `docs/thresholds.md` to recognize typed unit constants (`Bytes`, `Sp`, `Micros`, `CharCount`) without relaxing power-of-two, presentation-cap, or `Kind:` rules.
- Add mechanical gates: `clippy::fn_params_excessive_bools = "deny"` (`max-fn-params-bools = 1` in `clippy.toml`) and `scripts/check-domain-value-types.py` (`--self-test`, wired into `make check-policy`).

## Non-goals
- Wrapping every local loop counter or private arithmetic temporary in a newtype.
- Replacing flat `bool` + `Option` state bags inside `imp.rs` / workflow structs (addressed in `idiomatic-rust-state-machines-and-c-ism-removal`).
- Introducing `Arc` / `Box` metadata wrappers (addressed in `cold-path-ergonomics-and-shared-metadata`).

## User-visible impact
Zero user-visible or behavioral change compared with `main`. All runtime layouts, thresholds, and responsiveness budgets (`B_INTERACT_US`, `B_SETTLE_US`, `EDITOR_MEMORY_UPPER_BUDGET_BYTES`) remain identical.

## Capabilities

### New Capabilities
- `domain-value-types`: Replace ambiguous primitive integers and multi-boolean parameter lists at LushText domain and workflow boundaries with zero-cost, layout-verified Rust value types and enums.

### Modified Capabilities
- `threshold-constants`: Recognize zero-cost typed unit constants (`Bytes`, `Sp`, `Micros`, `CharCount`) in the committed threshold inventory and `make check-thresholds` while keeping all power-of-two and `Kind:` checks unchanged.

## Impact
- **Code**: `crates/lushtext-model/src/values/`, `lushtext-model`, `lushtext-services`, `crates/lushtext-ui/src/ui/` (`seams.rs`, `policy.rs`, state structs), and `crates/gtk-lush/axioms/src/probes/a18.rs` (private `content_window` helper).
- **Kani & Thresholds**: `scripts/check-threshold-kinds.py`, `docs/thresholds.md`, and touched Kani policy/model shards (`core-memory-policy`, `core-geometry-policies`, `core-second-writer`, `core-protected-fold`, `core-protected-install`, `core-protected-view`, `core-document-scale`, `core-post-open-settle`).
- **Gates**: `Cargo.toml`, `clippy.toml`, `scripts/check-domain-value-types.py`, `Makefile`.
- **Docs**: `.agents/rules/rust.md`, `.agents/rules/workflow-convention.md`, `AGENTS.md`, `docs/next/idiomatic-rust-hexagonal-programme.md`.
