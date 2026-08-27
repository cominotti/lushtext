# Contracts this change must preserve exactly (task 0.5)

Recorded **before** touching anything near them. Each has a before section (the
code as implemented today, quoted) and an after section filled in at
verification.

## 1. Sidecar migration ordering and its retry ledger

**Before** — `ui/window/notes/mod.rs:431` `migrate_note_sidecars_after_rename`,
verbatim call order inside the worker:

```rust
let generation = migration_ledger::record_pending(
    &data_dir, &old_path_for_move, &new_path_for_move,
    &[MigrationKind::Bookmarks, MigrationKind::DocumentNotes, MigrationKind::FolderNotes],
)?;
let bookmark_count = migration_ledger::run_tracked_kind(
    &data_dir, generation, MigrationKind::Bookmarks,
    || bookmark_service::move_path_tree(&data_dir, &old_path_for_move, &new_path_for_move))?;
let document_note_count = migration_ledger::run_tracked_kind(
    &data_dir, generation, MigrationKind::DocumentNotes,
    || document_note_service::move_path_tree(&data_dir, &old_path_for_move, &new_path_for_move))?;
let folder_note_count = migration_ledger::run_tracked_kind(
    &data_dir, generation, MigrationKind::FolderNotes,
    || folder_note_service::move_folder_tree(&data_dir, &old_path_for_move, &new_path_for_move))?;
```

Invariants: pending state for **all three kinds** is recorded before **any**
sidecar move begins; the three kinds run in exactly this order;
`MAX_MIGRATION_ATTEMPTS` bounds retries; anything left is finished by
`reconcile_pending_migrations_on_startup` on a later launch. On error the user
sees `"Rename succeeded, but note sidecars could not be moved"` as a
`MessageKind::Warning` and the rename itself is **not** rolled back.

**After**: unchanged. The worker body moves into the notes `journal`
coordination module verbatim; the call order, the ledger generation, the
`MAX_MIGRATION_ATTEMPTS` bound, the error message, and the
`refresh_command_palette_note_source_debounced` success tail are byte-identical.

## 2. Format-upgrade apply re-scans rather than trusting the dialog snapshot

**Before** — `ui/window/startup_data.rs:176` `run_startup_format_apply`, worker
body:

```rust
// Re-scan in the worker instead of applying the dialog's
// snapshot; app data may have changed while the dialog was
// open, and apply needs fresh file facts.
let data_dir = json_store::data_dir();
let inventory = format_upgrade::scan(&data_dir);
let plan = format_upgrade::build_plan(&inventory);
if !plan.requires_startup_decision() { return StartupFormatApplyWorkerResult::NoDecisionNeeded; }
```

A partial failure re-presents the dialog with `Some(&detail)` — the previous
error — rather than proceeding into `continue_startup_data_flow`. The re-scan is
a **safety property**, not redundancy.

**After**: unchanged. Task 2.2 decides this module cross-cutting, so it is not
restructured at all; only its module doc gains the ownership sentence.

## 3. Workspace persistence latest-generation semantics

**Before** — `model/workspace_persistence.rs` encodes the state machine and
`ui/sidebar/workspaces.rs:715` `start_persist_worker` drives it:

- a 150 ms `Debounce` (`sidebar/imp.rs::persist_debounce`,
  `WORKSPACE_PERSIST_DEBOUNCE_MS` in `sidebar/mod.rs:31`);
- **one active write**, with a newer snapshot waiting behind it;
- bounded retry backoff;
- a current failure awaiting **explicit** retry rather than auto-retrying
  forever;
- close bypassing the debounce (`flush_workspace_persistence`) without falsely
  settling readiness;
- a close-time failure **aborting close**.

The `workspace-persist` readiness blocker documents the same state.

**After**: the module relocates whole, with its co-located tests, into
`ui/sidebar/policy.rs` (task 5.1). No state, transition, threshold, or reason
enum changes; mutation parity is proved in
`mutation-workspace-tree-policy.md`.

## 4. The sidebar's local contracts (`ui/sidebar/AGENTS.md`)

Quoted as they stand today; each survives the migration verbatim:

- **Scan flight**: "Give each materialized child store one
  `model::workspace_scan::WorkspaceScanFlight`: one admitted scan and one
  replaceable latest weak/scalar request. Capture strong store ownership and the
  current mirror only at worker admission, and require lifetime, store,
  target-generation, and scan-generation agreement before reconciliation or
  empty-folder evidence may publish."
- **Watcher mirror**: "Keep workspace watcher targets as an incremental mirror
  of flattened-row splices and expansion state; do not restore full
  `GtkTreeListModel` scans on restart. Watcher creation, registration,
  replacement teardown, and stale-handle disposal stay off the GTK thread, with
  at most one lifecycle worker per section and one latest-generation handoff."
- **Mailbox cap**: "mailbox and refresh planning share the 1,024-path cap, raw
  normalization stops when overflow becomes one full refresh, and each
  non-blocking GTK poll consumes at most one notice. A pending full refresh
  releases and dominates targeted paths until it runs".
- **DnD shield**: "workspace-folder reorder DnD hover belongs to the transparent
  row-level shield. `GtkTreeExpander` owns normal disclosure behavior, and any
  idle collapse after drag-hover child-model creation is defensive only, not the
  intended reorder path."
- **Row-factory ownership**: "Keep recycled row setup/bind/unbind ownership in
  `workspace_section/row_factory.rs`, row metadata and expanded-hook symmetry in
  `row_accessibility.rs`, and file/header popup construction plus targeting in
  `context_menus.rs`."

**After**: all five preserved. `AGENTS.md` gains the role classification of each
module (task 4.2) without weakening any contract sentence.

## 5. Expansion-state authority

**Before** — `.agents/rules/ui.md` states it and the code implements it:
`expanded_paths` is authoritative live per-section state, kept current by row
`notify::expanded` transitions, accepted reconciliation retirement
(`clear_dir_states`), and rename prefix rewrites. The full derivation
(`derive_expanded_paths_from_model`) is reserved for bootstrap, pre-replacement
capture, and the test oracle. **Deferred restore callbacks
(`schedule_child_state_restore`, `restore_materialized_state`) read the set at
apply time, not at schedule time.**

**After**: unchanged, and the evidence surface is explicitly forbidden from
calling `derive_expanded_paths_from_model` because that derivation advances the
capture counters the surface reports (task 4.6).

## 6. File-operation semantics

**Before**:

- create uses a unique-name policy (`New File`, `New File 2`, …) rather than
  overwriting;
- rename cancels on an empty or unchanged name;
- the inline-rename focus-out double-fire guard is `entry.parent().is_none()`;
- directory operations match open tabs by `Path::starts_with` **prefix**, not by
  equality.

**After**: unchanged; the unique-name policy and the rename validation move into
`policy.rs` as pure functions with the same literals, and the prefix-matching
call sites keep `starts_with`.

## 7. The `GtkTreeExpander` internal-gesture disable for file rows

**Before** — `ui/sidebar/workspace_section/row_factory.rs:325-343`, verbatim,
inside `connect_bind`:

```rust
// GtkTreeExpander installs a BUBBLE-phase gesture that intercepts
// clicks for ALL rows — even non-expandable files — preventing
// GtkListView's built-in double-click activation from firing.
// Setting phase=None disables it for files while preserving
// expand/collapse for directories. Must run on every bind
// (row recycling resets state).
let phase = if file_item.is_dir() && !file_item.is_placeholder() {
    gtk4::PropagationPhase::Bubble
} else {
    gtk4::PropagationPhase::None
};
let controllers = expander.observe_controllers();
for i in 0..controllers.n_items() {
    if let Some(obj) = controllers.item(i)
        && let Ok(gesture) = obj.downcast::<gtk4::GestureClick>()
    {
        gesture.set_propagation_phase(phase);
    }
}
```

`.agents/rules/ui.md` records this as a **three-iteration lesson** with two
rejected fixes: `single-click-activate=true` changed the UX, and a CAPTURE-phase
gesture was fragile and failed for the first file due to
`SingleSelection::selected()` timing. It runs on **every** bind, including
recycling.

**After**: byte-identical and in the same position within `connect_bind`.
`row_factory.rs` is classified as a **called presentation surface** precisely so
that no role move touches this block. Verified by diffing the function.

## 8. The peek key controller's phase and gating

**Before** — `ui/sidebar/workspace_section/peek.rs:321-330`, verbatim:

```rust
fn setup_peek_key_controller(&self) {
    let controller = gtk4::EventControllerKey::new();
    // Real keyboard focus lands on realized row widgets inside GtkListView,
    // not on the ListView wrapper itself. Capture-phase delivery lets the
    // section observe Space/Escape/Enter before row-local widgets consume
    // them, while still preserving default behavior for inline rename
    // entries and other focused controls that should own their keys.
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
```

followed by the `focus_allows_peek_shortcuts()` gate before any key match. A
handler that assumes `list_view.has_focus()`, or that only works when a test
emits the key directly on the list widget, passes synthetically and does nothing
for a real user.

**After**: phase and gate unchanged; the controller stays attached to the list
view in `Capture` and the gate remains the first statement in the handler.
