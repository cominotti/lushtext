# Proposal

## Why

The refactor-1 audit (disposition in `docs/next/refactor-1-disposition.md`) found three places where LushText's GTK code holds state in shapes modern gtk-rs code avoids. Measured on `origin/main` `761866ff`; task 0.1 re-measures:

- **List row models are tagged unions.**
  - `PaletteItem` stores a `kind: Cell<u8>` discriminant (five values) beside eight parallel fields.
  - `SearchResultItem` does the same with two kinds.
  - `FileTreeItem` encodes "placeholder or entry" as two `Option`s plus flags.
  - Nothing stops a command row from carrying a file path.
- **Rows are hand-built and navigated by position.**
  - The six factories (15 `SignalListItemFactory` sites) build rows in `setup` and find children in `bind` by sibling order (34 `next_sibling` calls in `ui/`).
  - Row-owned lifetime state hangs on four `unsafe` object-data sites: a `BindingBag` (`search_panel/list_factory.rs:287`), a `SignalBag` (`workspace_section/row_accessibility.rs:145`), and two `bool` markers on `GtkTreeListRow` (`watch.rs:169`, `reorder_execution.rs:437`).
- **The Open dialog is continuation-passing** (`ui/window/dialogs.rs:76-92`).

The maintainer decided on 2026-11-19 to adopt typed rows fully, without weighing churn. `spike-list-row-construction` decides the row form first.

## What Changes

- **Typed entries.** `PaletteItem`, `SearchResultItem` and `FileTreeItem` each hold one typed entry `enum` in a single `RefCell`. `retarget_from` becomes one replacement. `match_count` stays a GObject property.
- **Typed rows, in the form the spike chose.** Every factory row becomes a row widget whose children are typed fields: a `CompositeTemplate` row, or a Rust-built subclass. Row-owned bindings and signal bags live in the row's `imp` and are cleared in its unbind.
  - If the spike keeps the factory form, the bags move instead to a typed registry on the factory's owner, filled at bind and emptied at unbind.
- **No untyped object data.**
  - The expansion-watch marker becomes a section map of `WeakRef<TreeListRow>` checked by object identity, and the one-shot reorder marker becomes a scoped guard held only around the drag's collapse (design D3).
  - A policy audit then forbids `set_data`, `steal_data`, `data` and their `qdata` forms anywhere in `crates/lushtext-core/src`.
- **The Open dialog becomes an `async` block** with `FileDialog::open_future` inside `glib::spawn_future_local`, behind a characterization test.

## Non-goals

- **`#[gtk4::template_callbacks]` (declined).**
  - In the locked glib 0.22.10 (`closure.rs:254`), a closure that answers `None` where the signal expects a value panics inside the C trampoline, which aborts the process: the class of ledger A122.
  - Template signal handlers would also connect by runtime name, outside `ui::signal_connect` and `make check-signal-connections`.
  - Template rows connect their signals in Rust, through `signal_connect`.
- **The `glib::clone!` rewrite.** `adopt-glib-clone-weak-captures` does it after this change, so closures this change deletes are not rewritten first.
- **Futures for worker completions.** `spawn_future_local` runs at `G_PRIORITY_DEFAULT`, while completions keep `completion_priority()` (ledger A80).
- **Save As and the close-confirmation dialogs.** `dialogs.rs` owns the confirmed-close contract (`ui/window/AGENTS.md`).

## User-visible impact

None intended. Every row renders, activates and announces as on `main`. The one difference is that the Open file chooser is requested one main-loop dispatch later, with the modal-surface guard already held. Nothing is lost compared with `main`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `gtk-lush-signals`: LushText keeps no state as untyped GObject data, and row-owned bags have a typed owner.

## Impact

- **Code:**
  - the six factory files above;
  - `ui/command_palette/item.rs`, `ui/search_panel/item.rs`, `ui/sidebar/file_tree_item.rs`;
  - `ui/sidebar/workspace_section/{row_accessibility,reorder_execution,watch,imp}.rs`;
  - `ui/window/dialogs.rs`, new row-widget modules, `.blp` templates if the spike chose them;
  - `scripts/check-signal-connections.py`.
- **Path-keyed gates.** `ui/sidebar/workspace_section/row_factory.rs` is a literal key of the visual-proof policy in both implementations (`scripts/check-visual-proof-policy.py`, `crates/cargo-gtk-proof/src/policy.rs:86`). Moving its code re-keys both, with parity tests (`testing.md`, "Path-keyed gates").
- **Order:** the Start line in `tasks.md`, after `spike-list-row-construction` records its decision.
- **Shared with the other refactor-1 changes:** `crates/cargo-gtk-proof/src/policy.rs` (this change re-keys `row_factory.rs`; `type-cargo-gtk-proof-errors` converts its errors), and `scripts/check-signal-connections.py` (this change adds the object-data rule; `adopt-glib-clone-weak-captures` may add its audit there). The later change in the ledger starts after the earlier merges.
