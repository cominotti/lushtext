# Design: Idiomatic Rust State Machines and C-ism Removal

## Context
Even with hexagonal crates and zero-cost value types in place, `lushtext-ui` retains imperative C/C++ habits inside widget implementations: 35 `*State`/`*Session`/`*Progress` bags with 4–20 parallel `bool`/`Option` fields, 9 `imp.rs` files over 500 lines (up to 1,395 lines), 49 `Box<dyn Fn...>` fields across 11 `imp.rs` files, 1,661 `.borrow()`/`.borrow_mut()` calls, 5 manual `reset_*_state` routines, 25 `while idx < len` loops, and 323 numeric `as` casts. Meanwhile, `ui/row_text/imp.rs` (488 lines) is pure custom `GtkWidget` drawing (`measure`/`snapshot`/Pango cache) with zero workflow state or callbacks.

## Goals / Non-Goals
**Goals:**
- Replace parallel `bool`/`Option` state bags with algebraic phase enums in a single `RefCell<Phase>` transitioned via `mem::take` / `RefCell::replace`.
- Bring all 9 oversized `imp.rs` files down to `<= 500` production lines without inventing faux-manager traits (`rust-hex-arch`) or off-taxonomy `state.rs` role files (`workflow-convention.md`).
- Consolidate multi-callback farms (`<= 2` `Box<dyn Fn...>` per `imp.rs`) into typed GObject signals or one typed `WidgetEvent` callback.
- Remove manual `reset_*_state`, wrap GTK `-1` sentinels at FFI edges (`Option`), replace non-SIMD `while idx < len` loops with iterators, and eliminate unnecessary/unsafe `as` casts.
- Enforce every target with Clippy (`struct_excessive_bools`, `cast_possible_wrap`) and `scripts/check-idiomatic-state.py` (`--self-test`).

**Non-Goals:**
- Splitting `*Evidence` or `*Snapshot` DTOs (these project independent observed facts rather than mutually exclusive states).
- Modifying `ui/**/policy.rs` pure decision contracts beyond accepting `Option` instead of `-1` in `search_bar/policy.rs`.
- Changing user-visible behavior, GSettings schemas, or accessibility announcements.

## Decisions

### D1. Single-`RefCell<Phase>` state machines + `mem::take` / `replace` transitions
- **Decision**: Convert flat state bags (`TabOverflowState`, `SearchPreviewState`, `LoadState`, `MinimapState`, `DraftState`, `BufferReplacementSession`, `SessionState`, `LanguageAttachState`, `LocalHistoryBrowserState`, `ChunkedSnapshotSession`, `HeldInputState`, `EditorMemoryState`, etc.) into phase enums (e.g., `SaveLifecycle::Idle | Capturing { ... } | Writing { ... }`) held in one `RefCell<Phase>` (plus independent `Cell` counters/generations where orthogonal). Transitions extract owned state with `mem::take` or `replace`, run logic outside the borrow, and store the next phase—reducing `.borrow()`/`.borrow_mut()` density and preventing re-entrant borrow panics.
- **Rationale**: Makes illegal flag combinations unrepresentable and eliminates manual `reset_*_state` functions (a single assignment to `Phase::Idle` cleans all phase-local fields).
- **Alternatives rejected**: Keeping separate `Cell<bool>` + `RefCell<Option<_>>` fields with helper reset methods (every new field still risks partial reset or inconsistent state); introducing an actor framework (overkill for single-threaded GTK main-loop state).

### D2. Thin `imp.rs` (`<= 500` lines) via coordination-owned state + consolidated event channels
- **Decision**:
  1. Move workflow-scoped state structs (`LoadState`, `SaveState`, `MinimapState`, `DraftState`, `SessionState`, `SearchPreviewState`, etc.) out of `imp.rs` into their owning **existing bounded coordination module** (`execution.rs`, `watch.rs`, `journal.rs`, `admission.rs`, or stage-qualified `*_execution.rs` — never creating an unapproved `state.rs` role file in migrated workflow role homes), updating `docs/workflow-readability-matrix.md` and preserving `make check-workflow-boundaries`, leaving `imp.rs` with a single field pointing to each workflow state.
  2. Replace 8–10 separate `RefCell<Option<Box<dyn Fn(...)>>>` slots per widget with typed GObject signals (checked by `make check-signal-connections`) when payloads are GObjects/primitives, or a single `RefCell<Option<Box<dyn Fn(WidgetEvent)>>>` when payloads carry Rust ownership (`DisposalOwned<T>`, `BookmarkRecord`, `InlineActionNotification`). Keep `SignalBag` for external subscriptions.
- **Rationale**: Brings `ui/window/imp.rs` (1,395L) and `ui/editor_page/imp.rs` (1,230L) under 500 lines while keeping `ui/row_text/imp.rs` (488 lines of pure custom `GtkWidget` drawing) unsplit ("Never split mid-impl block" in `rust.md`), respecting `workflow-convention.md`'s closed role-name taxonomy, and eliminating 67 redundant `connect_*` boilerplate methods.
- **Alternatives rejected**: Adding a new `state.rs` role name (would require amending `gtk-adapter-module-boundaries` and retroactively re-migrating all `WFR-*` rows); forcing `<= 450` lines onto `ui/row_text/imp.rs` (would force an artificial mid-impl split); trait-based presenters (forbidden by `rust.md`).

### D3. Honest FFI sentinel wrapping & iterator/cast cleanup
- **Decision**: Keep GTK `-1` checks strictly at the GTK adapter edge: `ui/search_bar/execution.rs` converts GtkSourceView's `total == -1` into `Option<u32>` before calling `search_bar/policy.rs`; `ui/search_panel/results.rs` keeps its documented `GtkScrolledWindow::max_content_height() == -1` adapter check wrapped in a named helper (`unset_max_height()`). Convert non-SIMD/non-cursor `while idx < len` loops to iterators/`windows`/`array_windows`, audit all 323 `as` casts, and enable `clippy::cast_possible_wrap = "deny"`.
- **Rationale**: Respects real GTK C API conventions without leaking C sentinel values into pure Rust policy.
- **Alternatives rejected**: Blanket regex ban on `-1` without adapter allowlist (would break valid GTK4 `max_content_height()` checks).

### D4. Two-layer gate (`Clippy` + `scripts/check-idiomatic-state.py`)
- **Decision**: Because `clippy::struct_excessive_bools` counts raw `bool` fields only (hitting 53 structs such as `*Evidence`, `*Snapshot`, `DocumentScale`, `*Facts`, `journal_model::Editor`, `kani_proofs`) and does not inspect `Cell<bool>`, split enforcement cleanly:
  1. Enable `struct_excessive_bools = "deny"` (`max-struct-bools = 3`) and `cast_possible_wrap = "deny"` in `Cargo.toml`/`clippy.toml`, permitting reasoned `#[expect]` only on pure observable DTOs (`*Evidence`/`*Snapshot`), domain fact/feature masks (`DocumentScale`, `*Facts`), and Kani model harnesses (`kani_proofs.rs`, `journal_model.rs`).
  2. Use `scripts/check-idiomatic-state.py --self-test` in `make check-policy` as the authoritative GTK/state ratchet: `<= 500` prod lines per `imp.rs`, `<= 2` `Box<dyn Fn...>` fields per `imp.rs`, 0 flat `>= 4` flag+option (`Cell<bool>`/`bool`/`RefCell<Option<_>>`/`Option<_>`) `*State`/`*Session`/`*Progress` bags, 0 manual `reset_*_state` functions, and 0 non-adapter `-1` sentinels.
- **Rationale**: Combines Clippy's zero-cost raw-struct check with a `Cell`/`RefCell`-aware script so GTK widget state bags cannot slip through unnoticed.
- **Alternatives rejected**: Relying on `struct_excessive_bools` alone (blind to `Cell<bool>`, which GTK structs use everywhere); relying on code review alone.

## Budget Exceptions
None.

## GTK Axiom Ledger
Relies on existing GTK4/Libadwaita disposal and GObject `notify` / `dispose()` ordering axioms (`workflow-convention.md`: template children read via `try_get()` on disposed widgets; `Drop` / `dispose()` clears `SignalBag` and callbacks cleanly). No new toolkit assumptions introduced.

## Timing
Verdict class: non-inferiority (touches GTK main-loop coordination and state transitions on timed paths: `load`, `buffer_replacement`, `minimap`, `tab_overflow`, `search_panel`, `sidebar`).
- **Pre-registered protocol** (per `.agents/rules/verification-runs.md`): run `scripts/measure.sh` / `make responsiveness-smoke` and `cargo bench` comparing `main` vs branch on literal smoke cases `baseline/small/steady-populated`, `eviction/detach-minimap`, `minimap-markers/4096`, `save/during-validation`, `reload/250000`, `session/restore-3`, `replace-all/run-512mib`, and Criterion groups `tree_population`, `search_interactive_policies`, `all_tabs_filter_1024`, `save_admission_policy`, `transient_file_load`.
- **Primary metrics**: worst-case and p95 main-loop dispatch µs against `B_INTERACT_US` / `B_SETTLE_US`.
- **Noise band**: 500 µs per dispatch; zero regressions over `B_INTERACT_US` or `B_SETTLE_US` where `main` passes.
