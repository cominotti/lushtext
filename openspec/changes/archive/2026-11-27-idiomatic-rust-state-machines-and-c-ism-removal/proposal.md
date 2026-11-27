# Proposal: Idiomatic Rust State Machines and C-ism Removal

## Why
The highest cognitive load in `lushtext-ui` comes from C/C++-style state management: 35 structs holding parallel `Cell<bool>` and `RefCell<Option<_>>` flags instead of algebraic phase enums, 9 oversized widget `imp.rs` files (up to 1,395 production lines and 83 fields), 49 `Box<dyn Fn...>` callback fields across 11 `imp.rs` files (76 total in UI, 67 `connect_*` methods), 1,661 `.borrow()`/`.borrow_mut()` calls, manual `reset_*_state` functions, raw GTK `-1` sentinel checks leaking into policy, and `while idx < len` / redundant `as` casts.

## Baseline
Measured on `main` (`3c442112`) across production code in `crates/lushtext-{model,services,core}/src`:
- Flat state bags (`*State`/`*Session`/`*Progress` with `>= 4` `bool`/`Option` fields): **35** (led by `TabOverflowState`: 20, `SearchPreviewState`: 18, `LoadState`: 16, `MinimapState`: 13, `DraftState`: 12, `BufferReplacementSession`: 12, `SessionState`: 10, `LanguageAttachState`: 10).
- Oversized `imp.rs` files (`> 500` production lines): **9** (`ui/window/imp.rs`: 1,395, `ui/editor_page/imp.rs`: 1,230, `ui/search_panel/imp.rs`: 939, `ui/command_palette/imp.rs`: 907, `ui/open_popover/imp.rs`: 893, `ui/sidebar/workspace_section/imp.rs`: 734, `ui/markdown_preview/imp.rs`: 694, `ui/window/tab_overflow/all_tabs_popover/imp.rs`: 648, `ui/preferences/imp.rs`: 541). Note: `ui/row_text/imp.rs` (488 lines) is pure custom `GtkWidget` drawing with 0 workflow state and 0 callbacks and stays under `<= 500`.
- Callback farms: **49** `Box<dyn Fn...>` fields across **11** `imp.rs` files (**76** in UI total; **67** `pub connect_*` methods).
- Total `.borrow()` / `.borrow_mut()` calls: **1,661**.
- C/C++-style patterns: **5** manual `reset_*_state`/`clear_*_state` functions, **2** `-1` sentinel comparisons (`ui/search_bar/policy.rs`, `ui/search_panel/results.rs`), **25** `while idx < len` loops, **323** numeric `as` casts.
- Raw `bool` structs (`> 3` raw `bool` fields across workspace, where `clippy::struct_excessive_bools` fires while `Cell<bool>` is ignored): **53** (`*Evidence`, `*Snapshot`, `DocumentScale`, `*Facts`, `journal_model::Editor`, `kani_proofs`, plus runtime state bags).
- Workspace Clippy (`Cargo.toml`): `struct_excessive_bools` and `cast_possible_wrap` are not yet denied.

## What Changes
- Refactor flat parallel `bool`/`Option` state bags (starting with the top 10 bags) into algebraic Rust phase enums stored in a single `RefCell<Phase>` and transitioned atomically via `mem::take` / `RefCell::replace` outside active borrows, preserving all evidence-surface side-effect-freedom and disposal-honesty proofs.
- Shrink all **9** oversized `imp.rs` files to `<= 500` production lines by relocating workflow-owned state structs into their owning **existing bounded coordination module** (`execution.rs`, `watch.rs`, `journal.rs`, `admission.rs`, or `*_execution.rs` — never inventing an off-taxonomy `state.rs` role, updating `docs/workflow-readability-matrix.md` and preserving `make check-workflow-boundaries`), leaving `imp.rs` with GObject subclass boilerplate, template children, and thin delegation.
- Consolidate multi-slot `Box<dyn Fn...>` callback farms in `imp.rs` into typed GObject signals (`make check-signal-connections`) for GObject/primitive payloads or one typed widget event callback (`Box<dyn Fn(WidgetEvent)>`) where payloads are Rust-only (`DisposalOwned<T>`), keeping `SignalBag` for external GObject/GSettings subscriptions.
- Remove C/C++-isms: replace all **5** manual field-by-field `reset_*_state` functions with full-value enum/struct replacement; wrap GTK/GtkSourceView `-1` FFI sentinels into `Option<u32>` at the adapter edge (removing `-1` from `search_bar/policy.rs`); convert non-cursor/non-SIMD `while idx < len` loops to iterators/`windows`/`array_windows`; replace fallible/unnecessary `as` casts with `From`/`TryFrom`/`usize::from`.
- Add mechanical gates: `clippy::struct_excessive_bools = "deny"` (`max-struct-bools = 3` in `clippy.toml`; reasoned `#[expect]` limited to pure `*Evidence`/`*Snapshot` DTOs, domain fact/feature masks like `DocumentScale`/`*Facts`, and Kani model harnesses), `clippy::cast_possible_wrap = "deny"`, and `scripts/check-idiomatic-state.py` (`--self-test`, wired into `make check-policy`) covering `Cell<bool>`/`RefCell<Option<_>>` state bags that Clippy does not see.

## Non-goals
- Changing user-visible widget layouts, GSettings keys, D-Bus automation schemas, or workflow stage ordering.
- Splitting pure `*Evidence` / `*Snapshot` DTO structs solely to satisfy `struct_excessive_bools` (evidence surfaces intentionally flatten independent observable booleans).
- Introducing `Arc<Path>` / `Arc<str>` metadata sharing or boxing cold optional state (addressed in `cold-path-ergonomics-and-shared-metadata`).

## User-visible impact
Zero user-visible behavioral change compared with `main`. Eliminates classes of inconsistent partial state, re-entrant `RefCell` borrow panics, and forgot-to-reset state bugs.

## Capabilities

### New Capabilities
- `idiomatic-rust-state-machines`: Replace flat parallel `bool`/`Option` state bags, oversized `imp.rs` files, multi-slot callback farms, and C/C++-style patterns with algebraic Rust state machines, thin GObject subclasses, and ratcheted policy gates.

### Modified Capabilities
- `gtk-adapter-module-boundaries`: Add requirement keeping `imp.rs` modules thin GObject widget containers and consolidating multi-slot callback farms into typed GObject signals or a single typed widget event enum.

## Impact
- **Code**: `crates/lushtext-ui/src/ui/` (all 9 oversized `imp.rs` widgets and top state-bag workflows), `crates/lushtext-model/`, `crates/lushtext-services/` (`while` / `as` cast cleanups).
- **Gates**: `Cargo.toml`, `clippy.toml`, `scripts/check-idiomatic-state.py`, `scripts/check-workflow-boundaries.py`, `Makefile`, `.github/workflows/ci.yml`.
- **Docs**: `docs/workflow-readability-matrix.md` (re-derived measured cells for restructured rows), `.agents/rules/rust.md`, `.agents/rules/widget-wiring.md`, `AGENTS.md`, `crates/lushtext-ui/src/ui/*/AGENTS.md`, `docs/next/idiomatic-rust-hexagonal-programme.md`.
