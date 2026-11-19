# Design

## Context

See `proposal.md`, "Why". Constraints:

- **Realization contracts stay.** `LushtextRowText` (A108), staged growth and sliced retirement (A110) are unchanged. This change alters what a row model holds and how a row is constructed, not when rows are realized.
- **Locked bindings.** glib 0.22.10, glib-macros 0.22.9 and gtk4-macros 0.11.5 (`Cargo.lock`). Library facts below were read in those versions.
- **Every file is under `ui/`,** so each commit owes the three smokes on its final tree.

## Goals / Non-Goals

**Goals:** invalid row states unrepresentable; children reached through typed fields; zero object data in `lushtext-core`; one straight-line dialog.

**Non-Goals:** `proposal.md`, "Non-goals".

## Decisions

### D1. One entry `enum` per item

- **Palette.** `PaletteEntry::{Header, File, Command, Note, More}` carries exactly the fields each kind uses.
- **Search.** `SearchResultEntry::{File, Match}`. The shared `file_path: Arc<str>` stays beside the entry.
- **File tree.** `FileTreeEntry::{Entry { path, is_dir, is_empty, pending_rename, workspace_folder_id }, Placeholder(DirectoryPlaceholder)}`.
- **Retarget.** `retarget_from` replaces the entry and emits `content-changed` through `ui::signal_connect::emit_void`.
- **Accessors.** Public accessors keep their names, so factories, `evidence.rs` and tests do not change for this reason.
- **Rejected:** one GObject subclass per kind (more `GType`s, and it complicates the mixed `GListStore`).

### D2. The row form comes from the spike

- **Bindings in a row widget.** A row that rebinds the same property bindings to each item may own them through one `glib::BindingGroup` (available under the workspace's `glib` feature `v2_88`), built once at construction and retargeted with `set_source` at bind and unbind, instead of a `BindingBag` rebuilt per bind. The spike's rebind case decides which is used; the retained-site audit lists each binding-group row.
- **Template or Rust subclass** (the spike chose one). Each factory gets a row widget (`SearchResultRow`, `PaletteRow`, `OpenRecentRow`, `SkippedFileRow`, `NotesBrowserRow`, `WorkspaceTreeRow`).
  - **Shape.** Typed children and `bind(&item)` / `unbind()` methods. The factory's `setup` creates the row, `bind` and `unbind` call it.
  - **Bags.** Row-owned `BindingBag` and `SignalBag` live in the row's `imp` and are cleared in `unbind`.
  - **Text.** `LushtextRowText` stays the text child wherever it is today.
  - **Signals.** Row signals connect in Rust through `signal_connect`, never as template callbacks.
- **Factory form kept** (no subclass arm passed). A typed `RowBags<K, B>` registry on the factory's owner holds the bags. Its rules:
  - bind removes any old entry, then inserts;
  - unbind removes the entry, drops the borrow, then clears the bag;
  - the owner's `dispose` drains it as a safety net.
  
  Children are still found by sibling order, recorded as a known limit.
- **Sidebar row.** `WorkspaceTreeRow` keeps the overlay, the expander's gesture handling (`ui.md`, "Disable TreeExpander's gesture for file rows") and the reorder shield exactly. Its code moves out of `row_factory.rs`, so that path key is re-keyed in both visual-policy implementations, with a parity assertion in each self-test.

### D3. `GtkTreeListRow` markers in typed, identity-checked state

- **Expansion watch.** `WATCH_EXPANDED_HOOK_KEY` marks a `TreeListRow` that already has its `notify::expanded` handler. It is installed for rows the watch sees (`watch.rs:117`, `:154`).
  - **Replacement.** The section keeps `watched_rows: RefCell<HashMap<usize, WeakRef<TreeListRow>>>`, keyed by the row's object address. An entry counts only while its `WeakRef` still upgrades to the very row being asked about (pointer equality).
  - **What that covers:**
    - a recreated row for the same item gets its handler;
    - two rows alive at once for one item each get exactly one;
    - a reused address with a dead `WeakRef` is treated as new, and its stale entry is replaced.
  - **Pruning.** Dead entries are pruned when they are met, and once per targeted refresh pass, so the map is bounded by the live rows.
  - **Rejected:**
    - a `WeakRef` field on `FileTreeItem`: it answers "the last row watched for this item", not "this row has a handler", and installs twice when two rows are alive;
    - a strong `HashSet`: it retains collapsed rows;
    - a bare address set: it is fooled by pointer reuse.
- **Reorder marker.** `SUPPRESS_EXPANDED_WATCH_KEY` suppresses one watch restart when a drag collapses a folder. Today the caller marks the row and calls `set_expanded(false)` (`reorder_execution.rs:788-789`). The marker is consumed only by the row's watch handler, so a row without one keeps the marker armed, and its next real expansion is suppressed. That latent bug exists on `main` too.
  - **Replacement.** A scoped guard: `SuppressExpandedWatch::for_row(&section, &row)` stores that row's `WeakRef` in a section slot, and its `Drop` clears the slot. The caller holds it only around `row.set_expanded(false)`, so the slot can never outlive the collapse it was set for, whether or not a handler consumed it.
  - **Identity.** The handler suppresses only when the slot holds its own row. The `expanded=false` that child rows emit during teardown is therefore not suppressed, unlike with a section-wide boolean.

### D4. `open_future` inside `spawn_future_local`, behind a characterization test

- **Guard first.** The guard is acquired before the spawn and moved into the block, because the first poll comes from a later dispatch (glib 0.22.10 `main_context_futures.rs:231-233`).
- **Strong across the `.await`.** `clone!(#[weak])` upgrades inside the block (glib-macros 0.22.9 `clone.rs:545-549`) and holds the window strongly across the `.await`. That matches today's strong `self.clone()` for a modal dialog.
- **Oracle first.** No test drives the Open dialog today. Task 4.1 writes one that activates `win.open-file` (`actions.rs:33`) and observes the guard while the chooser is up and after it is cancelled. If the headless harness cannot present the chooser, the conversion is dropped and the reason recorded.

### D5. The audit

- **Where.** The object-data rule goes into `scripts/check-signal-connections.py` with no allowlist, once group 3 has removed the last site.
- **What it matches.** `.set_data(`, `.steal_data`, `.data::<` and their `qdata` forms.
- **`--self-test` cases:** each form, a test path, an axiom-probe path.

## Risks / Trade-offs

- **The spike's margin is small.** → The bar already limits the slowdown to 1.25×. The per-dispatch budget tests below are the regression proof, and an interleaved `measure.sh` round on the palette and notes browser runs if the spike's margin was under 1.5×.
- **Moving the sidebar row disarms a visual invariant.** → The re-key with parity assertions, and the `workspace-tree-reveal` scenario's `workspace-tree-rendered-row-anchors` in `make visual-geometry-smoke`.
- **Item enums change per-row memory.** No retained-byte budget charges by `size_of` of these items. `size_of` before and after is recorded as a fact.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** the affected cases are the palette publish, the search-result turns, the notes-browser query frame and the sidebar row bind. Evidence: the existing budget tests (`command_palette_category.rs` `test_palette_category_dispatches_stay_within_their_budgets`; `search_panel.rs` `test_search_and_preview_dispatches_fit_the_settle_budget` and `test_search_turn_applies_matches_as_one_splice_per_file`; `notes_browser.rs` `test_browser_dispatches_stay_within_their_measured_bounds`). There is one interleaved `measure.sh` round if the spike's margin was under 1.5×.
- **Axioms:** A108, A110, and the spike's new entry.
