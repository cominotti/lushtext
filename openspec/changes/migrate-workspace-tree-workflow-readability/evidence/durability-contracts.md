# Workspace-tree preservation contracts, as implemented today (task 0.6)

Recorded **before any move** of `crates/lushtext-core/src/ui/sidebar/**`. Every
item quotes the current code with exact `file:line` ranges so the same behavior
is quotable after the structural migration. Paths are relative to
`crates/lushtext-core/src/ui/sidebar/` unless a fuller path is given.

Line numbers are as of this snapshot (branch `main`, working tree clean of
sidebar edits). Three deviations from the task's stated premises were found and
are called out inline as **FINDING**.

---

## 1. The five Local Contracts in `AGENTS.md`

Quoted verbatim from `crates/lushtext-core/src/ui/sidebar/AGENTS.md`.

### 1a. Scan flight — `AGENTS.md:19`

> - Give each materialized child store one `model::workspace_scan::WorkspaceScanFlight`: one admitted scan and one replaceable latest weak/scalar request. Capture strong store ownership and the current mirror only at worker admission, and require lifetime, store, target-generation, and scan-generation agreement before reconciliation or empty-folder evidence may publish.

Implementation anchors. The flight type, `crates/lushtext-core/src/model/workspace_scan.rs:66-100`:

```rust
/// One-active plus one-latest policy for a materialized child store.
#[derive(Debug, Default)]
pub struct WorkspaceScanFlight {
    next_scan_generation: u64,
    next_target_generation: u64,
    active: Option<WorkspaceScanTicket>,
    pending: Option<WorkspaceScanTicket>,
    metrics: WorkspaceScanFlightMetrics,
}

impl WorkspaceScanFlight {
    /// Submit a request and either admit it or replace the sole pending ticket.
    pub fn submit(&mut self, lifetime: u64) -> WorkspaceScanSubmission {
        self.next_scan_generation = self.next_scan_generation.wrapping_add(1);
        self.next_target_generation = self.next_target_generation.wrapping_add(1);
        let ticket = WorkspaceScanTicket {
            lifetime,
            target_generation: self.next_target_generation,
            scan_generation: self.next_scan_generation,
        };

        let Some(active) = self.active else {
            self.active = Some(ticket);
            self.metrics.starts = self.metrics.starts.saturating_add(1);
            self.metrics.active_high_water = 1;
            return WorkspaceScanSubmission::Start(ticket);
        };

        let replaced_pending = self.pending.replace(ticket).is_some();
```

One flight per store key, `workspace_section/imp.rs:297`:

```rust
    pub(super) child_scan_flights: RefCell<HashMap<usize, WorkspaceScanFlight>>,
```

The four-way agreement gate, `workspace_section/tree_loading.rs:704-726`:

```rust
fn child_scan_is_active(
    section: &LushtextWorkspaceSection,
    store_key: usize,
    ticket: ChildScanTicket,
    store: &gio::ListStore,
    token: &Arc<AtomicBool>,
) -> bool {
    if token.load(Ordering::Acquire) {
        return false;
    }
    if child_store_key(store) != store_key
        || !child_store_identity_matches(section, store_key, store)
    {
        return false;
    }
    let lifetime = section.imp().child_scan_lifetime.get();
    let owns_current_flight = section
        .imp()
        .child_scan_flights
        .borrow()
        .get(&store_key)
        .is_some_and(|flight| flight.is_current(ticket, lifetime));
```

The publication side calls it before *and* returns early otherwise,
`workspace_section/tree_loading.rs:1174-1199`:

```rust
    let store_key = child_store_key(store);
    if !child_scan_is_active(section, store_key, ticket, store, token) {
        finish_child_scan(section, store_key, ticket);
        return;
    }
    let recached = {
        let mirrors = section.imp().child_row_mirrors.borrow();
        mirrors.get(&child_store_key(store)).is_some_and(|mirror| {
            section.recache_child_rows_from_mirror(dir_path, mirror);
            true
        })
    };
    if !recached {
        finish_child_scan(section, store_key, ticket);
        return;
    }
    schedule_child_state_restore(section);
```

**Preservation obligation:** every materialized child store must keep exactly one
`WorkspaceScanFlight` with one admitted scan plus one replaceable latest ticket,
and no reconciliation or empty-folder evidence may publish unless cancellation
token, store key, store identity, lifetime, target generation, and scan
generation all still agree.

### 1b. Watcher mirror — `AGENTS.md:20`

> - Keep workspace watcher targets as an incremental mirror of flattened-row splices and expansion state; do not restore full `GtkTreeListModel` scans on restart. Watcher creation, registration, replacement teardown, and stale-handle disposal stay off the GTK thread, with at most one lifecycle worker per section and one latest-generation handoff.

Module intent, `workspace_section/watch_targets.rs:1-30`:

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! Incremental materialized watch-target bookkeeping for a flattened tree.

use std::collections::{BTreeMap, BTreeSet};

use crate::services::workspace_watch::WorkspaceWatchTarget;

/// Monotonic identity for one effective materialized target set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct WatchTargetGeneration(u64);
```

Off-GTK lifecycle worker at `workspace_section/watch.rs:268`
(`gtk_lush_tasks::spawn_blocking_then(`) and off-GTK stale-handle disposal at
`workspace_section/watch.rs:591`:

```rust
    gtk_lush_tasks::spawn_blocking_then((), move || drop(watcher), |(), ()| {});
```

Latest-generation acceptance predicates, `workspace_section/watch.rs:450-458`:

```rust
    /// Whether the installed backend belongs to the latest effective targets.
    ...
    /// Whether terminal unavailability belongs to the latest effective targets.
```

**Preservation obligation:** watch targets must stay an incremental mirror keyed
by `WatchTargetGeneration` / `WatchLifetimeGeneration` — never re-derived by a
full `GtkTreeListModel` walk on watcher restart — and watcher creation,
registration, replacement teardown, and stale-handle `drop` must stay on a
worker with at most one lifecycle worker per section and one latest-generation
handoff.

### 1c. Mailbox cap — `AGENTS.md:21`

> - Keep watcher event delivery in the per-handle GTK-free coalescing mailbox. Backend callbacks own tree-change filtering and bounded path deduplication; mailbox and refresh planning share the 1,024-path cap, raw normalization stops when overflow becomes one full refresh, and each non-blocking GTK poll consumes at most one notice. A pending full refresh releases and dominates targeted paths until it runs; targeted planning indexes expanded stores once per pass.

The shared cap, `crates/lushtext-core/src/services/workspace_watch.rs:19-23`:

```rust
/// Maximum unique changed paths retained before targeted work becomes a full refresh.
///
/// The same cap is enforced by the GTK refresh planner. It is intentionally
/// large enough to preserve precise handling for ordinary save and rename
/// bursts while bounding retained paths and per-poll work during bulk changes.
pub const WORKSPACE_WATCH_PATH_CAP: usize = 1_024;
```

Overflow becomes one full refresh, `services/workspace_watch.rs:138-141` and
`:160-163`:

```rust
                    if retained.len() > WORKSPACE_WATCH_PATH_CAP {
                        self.change = PendingChange::FullRefresh;
```

Full refresh dominates, `services/workspace_watch.rs:134-136`:

```rust
            (_, PendingChange::Empty) | (PendingChange::FullRefresh, _) => {}
            (current, PendingChange::FullRefresh) => *current = PendingChange::FullRefresh,
```

The same cap is re-imported by the GTK planner at
`workspace_section/refresh.rs:19` and enforced at `:102`
(`if pending_paths.len() > WORKSPACE_WATCH_PATH_CAP {`).

One notice per non-blocking poll, `workspace_section/watch.rs:370-388`:

```rust
    fn poll_workspace_watch(&self) -> glib::ControlFlow {
        let notice = {
            let watcher = self.imp().watch_runtime.watcher.borrow();
            watcher.as_ref().and_then(WorkspaceWatcher::try_poll)
        };
        ...
        let Some(notice) = notice else {
            return glib::ControlFlow::Continue;
        };

        match notice.change {
            Some(WorkspaceWatchChange::Paths(paths)) => self.queue_auto_refresh(paths),
            Some(WorkspaceWatchChange::FullRefresh) => self.queue_auto_full_refresh(),
            None => {}
        }
```

**Preservation obligation:** `WORKSPACE_WATCH_PATH_CAP = 1_024` must remain one
constant shared by the GTK-free mailbox and the GTK refresh planner, overflow
must collapse into a single dominating `FullRefresh`, and each GTK poll must
consume at most one notice through the non-blocking `try_poll`.

### 1d. DnD shield — `AGENTS.md:22`

> - In workspace-section rows, workspace-folder reorder DnD hover belongs to the transparent row-level shield. `GtkTreeExpander` owns normal disclosure behavior, and any idle collapse after drag-hover child-model creation is defensive only, not the intended reorder path.

See item 7 for the full quoted implementation.

**Preservation obligation:** reorder hover must stay owned by the transparent
`.workspace-folder-dnd-shield` overlay child, `GtkTreeExpander` must keep its
normal disclosure behavior, and the idle collapse in
`empty_children_model_for_drag_hover` must stay a defensive fallback rather than
becoming the reorder path.

### 1e. Row-factory ownership — `AGENTS.md:23-27`

> - Keep recycled row setup/bind/unbind ownership in
>   `workspace_section/row_factory.rs`, row metadata and expanded-hook symmetry in
>   `row_accessibility.rs`, and file/header popup construction plus targeting in
>   `context_menus.rs`. `workspace_section/imp.rs` retains subclass state,
>   template children, construction, and disposal glue.

All four files exist today at
`workspace_section/{row_factory.rs, row_accessibility.rs, context_menus.rs, imp.rs}`.
`row_factory.rs` owns `connect_setup`/`connect_bind`/`connect_unbind`;
`row_accessibility.rs` owns `apply_file_tree_row_accessibility`,
`install_expanded_accessibility_hook`, and `clear_expanded_accessibility_hook`,
all three called from the factory at `row_factory.rs:345-368`.

**Preservation obligation:** after the move, recycled-row setup/bind/unbind must
still live in one row-factory module, row metadata plus the expanded-hook
apply/clear pair must still live in one row-accessibility module, popup
construction and targeting in one context-menus module, and subclass
state/template children/construction/disposal glue in the section's `imp.rs`.

---

## 2. Expansion-state authority: apply-time reads

`.agents/rules/ui.md:213-224` requires that `expanded_paths` is authoritative
live state and that deferred restore callbacks read it **at apply time**.

`schedule_child_state_restore` proves the contract,
`workspace_section/tree_loading.rs:1240-1269`:

```rust
/// Restore expansion and pending selection after a refresh may replace rows.
///
/// Shared by watcher reconciliation and targeted in-place refresh; both defer
/// one main-loop tick so `GtkTreeListModel` row recycling settles first.
pub(super) fn schedule_child_state_restore(section: &LushtextWorkspaceSection) {
    if section.imp().expanded_paths.borrow().is_empty()
        && section.imp().pending_selection.borrow().is_none()
    {
        return;
    }

    let section_weak = section.downgrade();
    glib::timeout_add_local_once(Duration::from_millis(1), move || {
        if let Some(section) = section_weak.upgrade() {
            // Read expansion intent at apply time: the authoritative set is
            // live, so a collapse between scheduling and this callback must
            // not be resurrected by a stale snapshot.
            let expanded_paths = section.imp().expanded_paths.borrow().clone();
            for path in expanded_paths {
                if let Some(row) = section.find_dir_row(&path) {
                    row.set_expanded(true);
                }
            }
            let pending_selection = section.imp().pending_selection.borrow().clone();
            if let Some(path) = pending_selection {
                section.select_and_scroll_to(&path);
            }
        }
    });
}
```

The clone happens **inside** the timeout closure, not in the enclosing scope:
apply-time read confirmed. Callers are
`workspace_section/tree_loading.rs:1190` and `workspace_section/refresh.rs:527`.

The set stays live through per-row transitions,
`workspace_section/tree_index.rs:65-98`:

```rust
    /// Mirror one live row expansion transition into the authoritative set.
    pub(super) fn record_row_expansion_transition(&self, row: &gtk4::TreeListRow) {
        // Rows being destroyed by a splice or an ancestor collapse can still
        // emit property notifications; only rows still present in the flattened
        // model carry user expansion intent.
        if row.position() == gtk4::INVALID_LIST_POSITION {
            return;
        }
        let Some(path) = row
            .item()
            .and_downcast::<FileTreeItem>()
            .filter(FileTreeItem::is_dir)
            .and_then(|item| item.path())
        else {
            return;
        };
        let mut expanded = self.imp().expanded_paths.borrow_mut();
        if row.is_expanded() {
            expanded.insert(path);
            return;
        }
```

Full derivation stays reserved, `workspace_section/tree_index.rs:21-28`:

```rust
    /// Derive the complete expanded-path set by walking the flattened model.
    ///
    /// This full scan is reserved for bootstrap, pre-replacement capture, and
    /// the test oracle. Targeted in-place refresh relies on the live
    /// `expanded_paths` set maintained by row expansion transitions and
    /// accepted reconciliation instead.
    pub(super) fn derive_expanded_paths_from_model(&self) -> Option<HashSet<PathBuf>> {
```

**FINDING (stale rule citation, not a code defect):** the second function the
rule names, **`restore_materialized_state`, does not exist anywhere in the
codebase.** `grep -rn restore_materialized_state` over tracked sources returns
only three prose hits: `.agents/rules/ui.md:221`, slot 5a's archived
`durability-contracts.md:123`, and this change's `tasks.md:864`. The nearest
real name is `refresh_materialized_view` (`workspace_section/refresh.rs:303`),
which is **synchronous** and does not read `expanded_paths` at all — it reads
`expanded_store_index()` and queues directory refreshes. So the apply-time
obligation today has exactly **one** implementing site,
`schedule_child_state_restore`, and it satisfies it.

**Preservation obligation:** any deferred expansion-restore callback must clone
`expanded_paths` inside the callback body at apply time, never in the scheduling
scope, so a user collapse between schedule and apply cannot be resurrected; and
the full flattened-model derivation must stay confined to bootstrap,
pre-replacement capture, and the test oracle.

---

## 3. `GtkTreeExpander` internal-gesture disable for file rows

`workspace_section/row_factory.rs:324-343`, inside `connect_bind`:

```rust
            // Disable the TreeExpander's internal GestureClick for file rows.
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

Runs on **every** bind: the block is unconditional inside the
`factory.connect_bind` closure's `if let` arm for a bound `FileTreeItem` (arm
opens well above `:270`, closes at `:364`), with no generation guard or
run-once flag. Directory rows (`is_dir() && !is_placeholder()`) get
`PropagationPhase::Bubble`; files and placeholders get `PropagationPhase::None`.
The controller is located through `expander.observe_controllers()` and
downcast to `gtk4::GestureClick`.

Note on line numbers: the task cites `:325-343` and the rules cite `:336-341`.
The comment actually starts at `:324` and the `observe_controllers()` loop spans
`:336-343`. Content matches.

**Preservation obligation:** the phase computation and the
`observe_controllers()` loop must remain in the row-factory bind path, run
unconditionally on every bind including recycled rows, and keep `Bubble` for
non-placeholder directory rows and `None` for everything else.

---

## 4. The two row-recycling cleanup loops (duplicated)

**FINDING (task premise correction):** the task describes both copies as being
in `connect_bind`. They are not. The **first** copy is in `connect_bind`; the
**second** is in `connect_unbind`, whose closure opens at
`workspace_section/row_factory.rs:373` (`factory.connect_unbind(move |_factory, list_item| {`).
The duplication and the regression risk are real; only the location of the
second copy differs from the task text.

### Copy A — `connect_bind`, `row_factory.rs:296-305`

```rust
            // GTK recycles ListItem widgets: a row previously used for
            // inline rename may still have a GtkEntry appended.
            let mut child = label.next_sibling();
            while let Some(sibling) = child {
                child = sibling.next_sibling();
                if sibling.downcast_ref::<gtk4::Entry>().is_some() {
                    content_box.remove(&sibling);
                }
            }
            label.set_visible(true);
```

### Copy B — `connect_unbind`, `row_factory.rs:391-406`

```rust
                // Recycled ListItem widgets must leave no row-local editing
                // controls or markup mode behind for the next bound item.
                let mut child = label.next_sibling();
                while let Some(sibling) = child {
                    child = sibling.next_sibling();
                    if sibling.downcast_ref::<gtk4::Entry>().is_some() {
                        content_box.remove(&sibling);
                    }
                }
                label.set_visible(true);
                label.set_use_markup(false);
                drag_handle.set_visible(false);
                drag_handle.set_sensitive(false);
                accessibility::set_hidden(&drag_handle, true);
                accessibility::set_disabled(&drag_handle, true);
                content_box.set_margin_end(0);
```

### Explicit diff — what Copy B does that Copy A does not

| # | Statement only in Copy B | Line |
| - | ------------------------ | ---- |
| 1 | `label.set_use_markup(false);` | `:401` |
| 2 | `drag_handle.set_visible(false);` | `:402` |
| 3 | `drag_handle.set_sensitive(false);` | `:403` |
| 4 | `accessibility::set_hidden(&drag_handle, true);` | `:404` |
| 5 | `accessibility::set_disabled(&drag_handle, true);` | `:405` |
| 6 | `content_box.set_margin_end(0);` | `:406` |

The shared part is byte-identical apart from indentation: the
`label.next_sibling()` walk, the `next_sibling()` advance **before** the removal
(so removing a sibling cannot break iteration), the `Entry` downcast test, the
`content_box.remove(&sibling)`, and the trailing `label.set_visible(true)`.

Two further structural differences worth recording, because a move can silently
change them:

- **Widget acquisition.** Copy A uses locals captured by the bind closure. Copy B
  re-navigates the row from scratch through a chained `if let` at `:384-390`
  (`overlay.child()` → `TreeExpander` → `expander.child()` → `Box` →
  `first_child()` → `drag_handle: Button` → `next_sibling()` → `open_indicator` →
  `next_sibling()` → `icon: Image` → `next_sibling()` → `label: Label`). If any
  link of that chain fails, **the whole of Copy B is skipped**, including the
  `Entry` removal.
- **Where Copy B's six extras are set positively.** They are not missing from the
  bind path; bind sets the same state affirmatively elsewhere —
  `drag_handle.set_visible(show_reorder_handle)` / `set_sensitive(...)` at
  `:281-282`, and `content_box.set_margin_end(36)` / `set_margin_end(0)` at
  `:271` / `:274`. Copy B is the unbind-side reset of that state.

**Preservation obligation:** both loops must survive the move with the
`next_sibling()`-before-`remove()` walk, the `Entry` downcast test, and
`label.set_visible(true)` intact, and Copy B must keep all six extra resets plus
its full re-navigation chain — a change applied to one copy and not the other is
a row-recycling regression, not a cleanup.

---

## 5. The `pending_rename` one-shot handoff

State, `file_tree_item.rs:33-35`:

```rust
        /// Flag set on freshly created items (New File/Folder) to trigger
        /// inline rename in `connect_bind`. Cleared after rename begins.
        pub pending_rename: Cell<bool>,
```

Accessors, `file_tree_item.rs:130-137`:

```rust
    #[must_use]
    pub fn is_pending_rename(&self) -> bool {
        self.imp().pending_rename.get()
    }

    pub fn set_pending_rename(&self, pending: bool) {
        self.imp().pending_rename.set(pending);
    }
```

Producer — the New File / New Folder action, `workspace_section/actions.rs:74-76`:

```rust
                let new_item = FileTreeItem::new(temp_path, is_dir, None);
                new_item.set_pending_rename(true);
                section.imp().is_new_item.set(true);
```

Consumer — one-shot clear then deferred begin, `row_factory.rs:307-322`:

```rust
            // New file/folder rows carry a one-shot flag so rename starts
            // only after GTK has bound the recycled row widget.
            if file_item.is_pending_rename() {
                file_item.set_pending_rename(false);
                if let Some(section) = section_weak.upgrade() {
                    let imp = section.imp();
                    *imp.context_target.borrow_mut() =
                        FileContextTarget::from_item(&expander, &file_item);
                    let sw = section.downgrade();
                    glib::idle_add_local_once(move || {
                        if let Some(s) = sw.upgrade() {
                            s.begin_rename();
                        }
                    });
                }
            }
```

Ordering that matters: the flag is cleared **before** the `context_target` write
and **before** the idle is queued, so a re-bind of the same recycled row cannot
queue `begin_rename` twice. `set_pending_rename(false)` also runs even if the
section weak ref has already died.

**Preservation obligation:** the flag must be cleared unconditionally at the top
of the bind-side branch, before `context_target` is written and before
`begin_rename` is deferred through `idle_add_local_once`, so exactly one rename
starts per created item regardless of row recycling.

---

## 6. Peek key controller: `Capture` phase and its focus gate

Controller setup, `workspace_section/peek.rs:321-359`:

```rust
    /// Keep peek keyboard interactions local to the sidebar list.
    fn setup_peek_key_controller(&self) {
        let controller = gtk4::EventControllerKey::new();
        // Real keyboard focus lands on realized row widgets inside GtkListView,
        // not on the ListView wrapper itself. Capture-phase delivery lets the
        // section observe Space/Escape/Enter before row-local widgets consume
        // them, while still preserving default behavior for inline rename
        // entries and other focused controls that should own their keys.
        controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let section_weak = self.downgrade();
        controller.connect_key_pressed(move |_, key, _, _| {
            let Some(section) = section_weak.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if !section.focus_allows_peek_shortcuts() {
                return glib::Propagation::Proceed;
            }

            match key {
                gdk::Key::space => {
                    if section.toggle_peek_for_selection() {
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    }
                }
                gdk::Key::Escape if section.peek_visible() => {
                    section.dismiss_peek(true);
                    glib::Propagation::Stop
                }
                gdk::Key::Return | gdk::Key::KP_Enter if section.peek_visible() => {
                    let _ = section.promote_peeked_file();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        self.imp().file_tree_view.add_controller(controller);
    }
```

The gate, `workspace_section/peek.rs:604-626`:

```rust
    /// Return whether the currently focused widget should participate in the
    /// sidebar peek shortcut flow.
    fn focus_allows_peek_shortcuts(&self) -> bool {
        let Some(root) = self.root() else {
            return true;
        };
        let Some(window) = root.downcast_ref::<gtk4::Window>() else {
            return true;
        };
        let Some(focus) = gtk4::prelude::GtkWindowExt::focus(window) else {
            return true;
        };

        if focus.is::<gtk4::Entry>() || focus.is::<gtk4::Button>() {
            return false;
        }

        widget_is_within(
            &focus,
            self.imp().file_tree_view.upcast_ref::<gtk4::Widget>(),
        )
    }
```

The controller is attached to `file_tree_view` (the `GtkListView`), not to a row.
The gate returns `true` on the three unrooted/unfocused early exits, `false` for
a focused `Entry` (inline rename) or `Button` (row buttons), and otherwise
requires focus to be inside the list view.

**Preservation obligation:** the peek key controller must stay on the section's
`GtkListView` in `PropagationPhase::Capture`, and every key branch must remain
behind `focus_allows_peek_shortcuts()` with its `Entry`/`Button` exclusion and
its containment check against `file_tree_view`.

---

## 7. DnD inert-hover rules

### Transparent target and its fixed-height insertion-line child

**Note on location:** the widgets live in `row_factory.rs` (setup), the
controllers in `dnd.rs`. `row_factory.rs:121-141`:

```rust
        let drop_target = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        drop_target.add_css_class("workspace-folder-drop-target");
        drop_target.set_can_target(false);
        drop_target.set_focusable(false);
        drop_target.set_halign(gtk4::Align::Fill);
        drop_target.set_valign(gtk4::Align::Start);
        drop_target.set_height_request(2);
        drop_target.set_visible(false);
        accessibility::set_hidden(&drop_target, true);
        accessibility::set_disabled(&drop_target, true);

        let drop_shield = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        drop_shield.add_css_class("workspace-folder-dnd-shield");
        drop_shield.set_can_target(false);
        drop_shield.set_focusable(false);
        drop_shield.set_halign(gtk4::Align::Fill);
        drop_shield.set_valign(gtk4::Align::Fill);
        drop_shield.set_hexpand(true);
        drop_shield.set_vexpand(true);
        accessibility::set_hidden(&drop_shield, true);
        accessibility::set_disabled(&drop_shield, true);
```

`row_factory.rs:143-158`:

```rust
        let drop_indicator = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        drop_indicator.add_css_class("workspace-folder-drop-indicator");
        drop_indicator.set_can_target(false);
        drop_indicator.set_focusable(false);
        drop_indicator.set_halign(gtk4::Align::Fill);
        drop_indicator.set_valign(gtk4::Align::Center);
        drop_indicator.set_hexpand(true);
        drop_indicator.set_height_request(2);
        accessibility::set_hidden(&drop_indicator, true);
        accessibility::set_disabled(&drop_indicator, true);
        drop_target.append(&drop_indicator);

        overlay.set_child(Some(&expander));
        // Reorder DnD hover belongs to the transparent full-row shield;
        // the separate 2px indicator surface only paints the insertion line.
        overlay.add_overlay(&drop_shield);
        overlay.set_measure_overlay(&drop_shield, false);
        overlay.add_overlay(&drop_target);
```

One transparent 2px target box holding exactly one fixed-height (2px) insertion
line child; the shield is excluded from overlay measurement.

### Drop-target setup and hover ownership

`workspace_section/dnd.rs:95-99`:

```rust
        // DropTarget lives on the transparent row shield. Capture-phase hover
        // is owned before GtkTreeExpander can treat the drag as disclosure hover.
        let drop_target = gtk4::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
        drop_target.set_propagation_phase(gtk4::PropagationPhase::Capture);
        drop_target.connect_accept(move |_, _| folder_reorder_drag_should_own_row_hover());
```

`connect_accept` consults only whether a folder-reorder drag is active — not
whether *this* row is a valid target — so hover is accepted for **every**
file-tree row, `dnd.rs:431-433`:

```rust
/// Return whether the active drag should be consumed by row shields.
fn folder_reorder_drag_should_own_row_hover() -> bool {
    folder_reorder_drag_is_active()
}
```

Motion separates "owns hover" from "shows indicator", `dnd.rs:107-131`:

```rust
        drop_target.connect_motion(move |_, _, y| {
            ...
            let position = drop_position_for_y(overlay.height(), y);
            let decision = active_drag_hover_decision_for_list_item(&section, &list_item, position);
            if decision.shows_indicator {
                show_drop_indicator(&drop_surface, &shown_position_for_motion, position);
            } else {
                hide_drop_indicator(&drop_surface, &shown_position_for_motion);
            }
            if decision.owns_hover {
                gdk::DragAction::MOVE
            } else {
                gdk::DragAction::empty()
            }
        });
```

The validity ladder — every rejection keeps `owns_hover` while clearing
`shows_indicator` and `accepts_drop`, `dnd.rs:449-491`:

```rust
/// Validate workspace identity, target identity, and no-op moves for live hover feedback.
fn active_drag_hover_decision_for_target(
    section: &LushtextWorkspaceSection,
    target_folder_id: Option<&WorkspaceFolderId>,
    position: DropPosition,
) -> FolderReorderHoverDecision {
    let owns_hover = folder_reorder_drag_should_own_row_hover();
    let Some(payload) = active_drag_payload().filter(|_| owns_hover) else {
        return FolderReorderHoverDecision {
            owns_hover,
            shows_indicator: false,
            accepts_drop: false,
        };
    };
    if payload.workspace_id != section.workspace_id() {
        return FolderReorderHoverDecision { owns_hover, shows_indicator: false, accepts_drop: false };
    }
    let Some(target_folder_id) = target_folder_id else {
        return FolderReorderHoverDecision { owns_hover, shows_indicator: false, accepts_drop: false };
    };
    let Some((source_index, new_index)) =
        drop_source_and_new_index(section, &payload.folder_id, target_folder_id, position)
    else {
        return FolderReorderHoverDecision { owns_hover, shows_indicator: false, accepts_drop: false };
    };
    FolderReorderHoverDecision {
        owns_hover,
        shows_indicator: source_index != new_index,
        accepts_drop: true,
    }
}
```

(The three middle `return`s are line-wrapped here for width; the file spells each
struct literal across five lines.) Non-top-level rows yield
`target_folder_id == None` (`workspace_folder_id_for_list_item`), cross-workspace
drags fail the `workspace_id` test, and a no-op move fails `source_index != new_index`.

Indicator show/hide, `dnd.rs:504-530`:

```rust
fn show_drop_indicator(
    drop_target_surface: &gtk4::Box,
    shown_position: &Cell<Option<DropPosition>>,
    position: DropPosition,
) {
    accessibility::set_hidden(drop_target_surface, true);
    accessibility::set_disabled(drop_target_surface, true);
    if shown_position.get() != Some(position) {
        drop_target_surface.set_valign(match position {
            DropPosition::Before => gtk4::Align::Start,
            DropPosition::After => gtk4::Align::End,
        });
        shown_position.set(Some(position));
    }
    if !drop_target_surface.is_visible() {
        drop_target_surface.set_visible(true);
    }
}

fn hide_drop_indicator(
    drop_target_surface: &gtk4::Box,
    shown_position: &Cell<Option<DropPosition>>,
) {
    shown_position.set(None);
    drop_target_surface.set_visible(false);
    accessibility::set_hidden(drop_target_surface, true);
    accessibility::set_disabled(drop_target_surface, true);
}
```

Leave and drop both hide it first, `dnd.rs:136-152`. Row recycling resets it,
`dnd.rs:397-410`:

```rust
/// Reset one recycled row so no targetability or insertion line leaks.
pub(super) fn reset_reorder_row_for_unbind(overlay: &gtk4::Overlay) {
    set_reorder_shield_targetable(overlay, false);
    hide_reorder_indicator(overlay);
}

/// Prepare one newly-bound row for the current drag state.
pub(super) fn reset_reorder_row_for_bind(overlay: &gtk4::Overlay) {
    set_reorder_shield_targetable(overlay, folder_reorder_drag_is_active());
    hide_reorder_indicator(overlay);
    if folder_reorder_drag_is_active() {
        hide_focus_folder_button(overlay);
    }
}
```

### Never expand, never materialize descendants, never restart a watch

`workspace_section/tree_loading.rs:130-153`:

```rust
fn empty_children_model_for_drag_hover(
    section: &LushtextWorkspaceSection,
    dir_path: &Path,
) -> gio::ListStore {
    #[cfg(feature = "test-utils")]
    DRAG_HOVER_EMPTY_CHILD_MODEL_COUNT.with(|count| count.set(count.get() + 1));

    let store = gio::ListStore::new::<FileTreeItem>();
    let path = dir_path.to_path_buf();
    let section_weak = section.downgrade();
    // GTK can ask TreeListModel for children if a row auto-expands during DnD
    // hover. Return an empty temporary model and collapse the row back without
    // scanning or restarting watches; reorder hover must only move the line cue.
    glib::idle_add_local_once(move || {
        if let Some(section) = section_weak.upgrade()
            && let Some(row) = section.find_dir_row(&path)
            && row.is_expanded()
        {
            super::dnd::suppress_next_expanded_watch_for_drag(&row);
            row.set_expanded(false);
        }
    });
    store
}
```

The one-shot watch suppression, `dnd.rs:412-430`:

```rust
pub(super) fn suppress_next_expanded_watch_for_drag(row: &gtk4::TreeListRow) {
    // SAFETY: the private key stores a single row-local marker consumed by the
    // same widget factory's notify::expanded handler. No external code reads it.
    unsafe {
        row.set_data(SUPPRESS_EXPANDED_WATCH_KEY, true);
    }
}

pub(super) fn expanded_watch_should_be_suppressed(row: &gtk4::TreeListRow) -> bool {
    // SAFETY: mirrors set_data(SUPPRESS_EXPANDED_WATCH_KEY) above. Taking the
    // marker makes the suppression one-shot, so later user expansions still restart watching.
    let marker_present = unsafe {
        row.steal_data::<bool>(SUPPRESS_EXPANDED_WATCH_KEY)
            .is_some()
    };
    marker_present || folder_reorder_drag_is_active()
}
```

Consumed by the sole `notify::expanded` handler, `workspace_section/watch.rs:162-182`:

```rust
        row.connect_notify_local(Some("expanded"), move |row, _| {
            // The authoritative expansion set follows every live transition,
            // even during a reorder drag, matching what a whole-model snapshot
            // would capture at the next refresh.
            if let Some(section) = section_weak.upgrade() {
                section.record_row_expansion_transition(row);
            }
            if super::dnd::expanded_watch_should_be_suppressed(row) {
                return;
            }
            ...
                section.refresh_workspace_watch_row(&row);
```

Note the ordering: `record_row_expansion_transition` runs **before** the
suppression check, so `expanded_paths` stays truthful even for a
drag-induced transition; only the watch refresh is skipped.

### No filled drop rectangle — the `:drop(active)` neutralizer

`resources/style/style.css:332-364`:

```css
/* Transparent full-row shield that owns reorder hover before GtkTreeExpander can react. */
.workspace-folder-dnd-shield {
  min-height: 0;
  padding: 0;
  background: transparent;
}

/* Transparent overlay that positions the one painted workspace-folder reorder line. */
.workspace-folder-drop-target {
  min-height: 2px;
  margin-left: 10px;
  margin-right: 10px;
  padding: 0;
  background: transparent;
}

.workspace-folder-dnd-surface:drop(active),
.workspace-folder-dnd-shield:drop(active),
.workspace-folder-drop-target:drop(active) {
  background: none;
  background-color: transparent;
  box-shadow: none;
  outline-width: 0;
  outline-color: transparent;
  border-width: 0;
  border-color: transparent;
}

/* Workspace-folder reorder target line shown only for valid same-workspace drops. */
.workspace-folder-drop-indicator {
  min-height: 2px;
  background-color: @accent_bg_color;
  border-radius: 999px;
}
```

Three classes are neutralized (`...-dnd-surface`, `...-dnd-shield`,
`...-drop-target`); only `.workspace-folder-drop-indicator` paints, at 2px with
`@accent_bg_color`.

**Preservation obligation:** hover must keep being accepted for every file-tree
row while indicator and drop acceptance stay gated on same-workspace, top-level,
non-no-op reorder positions; the drag-hover child model must stay empty with its
one-shot watch suppression so no folder expands, no descendant materializes, and
no watch restarts; and the three `:drop(active)` neutralizer selectors plus the
2px indicator must survive any CSS or class rename so no filled drop rectangle
appears.

---

## 8. Inner `ScrolledWindow` and the no-horizontal-scrollbar contract

Inner (per section), `resources/ui/workspace-section.blp:93-100`:

```blueprint
  // Inner ScrolledWindow provides a vadjustment for GtkListView and prevents
  // deep tree indentation from expanding the outer sidebar width.
  ScrolledWindow inner_scrolled_window {
    propagate-natural-height: true;
    propagate-natural-width: false;
    vscrollbar-policy: never;
    hscrollbar-policy: never;
```

Outer (whole sidebar), `resources/ui/sidebar.blp:44-53`:

```blueprint
    // Scrollable area for all workspace sections. Natural width propagation
    // stays disabled so long file names cannot widen the sidebar split.
    child: ScrolledWindow outer_scrolled_window {
      vexpand: true;
      propagate-natural-width: false;
      hscrollbar-policy: never;
      // Workspace sections are appended programmatically.
      child: Box sections_box {
        orientation: vertical;
      };
    };
```

The cooperating label contract, `workspace_section/row_factory.rs:70-74`:

```rust
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_wrap(false);
        label.set_hexpand(true);
```

**Preservation obligation:** `propagate-natural-width: false` and
`hscrollbar-policy: never` must stay on both the inner per-section scroller and
the outer sidebar scroller, `propagate-natural-height: true` and
`vscrollbar-policy: never` must stay on the inner one, and the row label must
keep `EllipsizeMode::End` with `wrap = false` so deep indentation is clipped
rather than widening the sidebar or exposing a horizontal scrollbar.

---

## 9. Workspace persistence: latest-generation semantics and debounce window

### The debounce constant, with its value

`ui/sidebar/mod.rs:41`:

```rust
pub(super) const PERSIST_DEBOUNCE_MS: u64 = 150;
```

Used at `workspaces.rs:704-718`:

```rust
    /// Save the current workspace state to disk on a background thread.
    pub(super) fn persist(&self) {
        let should_schedule = {
            let imp = self.imp();
            let mut state = imp.persistence.borrow_mut();
            state.request_mutation();
            state.in_flight_generation().is_none()
        };
        if !should_schedule {
            return;
        }

        self.schedule_persist(
            Duration::from_millis(super::PERSIST_DEBOUNCE_MS),
            WorkspacePersistenceStartReason::Debounce,
        );
    }
```

Bounded retry delays, `crates/lushtext-core/src/model/workspace_persistence.rs:7-12`:

```rust
const RETRY_DELAYS: [Duration; 4] = [
    Duration::from_millis(250),
    Duration::from_millis(500),
    Duration::from_secs(1),
    Duration::from_secs(2),
];
```

### The generation comparisons

Admission, `model/workspace_persistence.rs:101-117`:

```rust
    /// Start the newest pending generation while preserving non-durable state.
    pub fn start(
        &mut self,
        reason: WorkspacePersistenceStartReason,
    ) -> Option<WorkspacePersistenceGeneration> {
        if self.in_flight.is_some() || self.requested == self.durable {
            return None;
        }
        if self.failed.is_some() && reason == WorkspacePersistenceStartReason::Debounce {
            return None;
        }

        let generation = self.requested;
        self.in_flight = Some(generation);
        Some(generation)
    }
```

Successful terminal, `model/workspace_persistence.rs:119-134`:

```rust
    /// Apply one successful terminal only when it owns the current worker slot.
    pub fn apply_success(
        &mut self,
        generation: WorkspacePersistenceGeneration,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        self.durable = generation;
        self.failed = None;
        if self.requested == self.durable {
            WorkspacePersistenceTerminalEffect::Settled
        } else {
            WorkspacePersistenceTerminalEffect::StartNewest
        }
    }
```

Failed terminal, `model/workspace_persistence.rs:136-168`:

```rust
    /// Apply one failed terminal and choose bounded retry or newest-state progress.
    pub fn apply_failure(
        &mut self,
        generation: WorkspacePersistenceGeneration,
        summary: impl Into<String>,
    ) -> WorkspacePersistenceTerminalEffect {
        if self.in_flight != Some(generation) {
            return WorkspacePersistenceTerminalEffect::IgnoredStale;
        }
        self.in_flight = None;
        if self.requested != generation {
            self.failed = None;
            return WorkspacePersistenceTerminalEffect::StartNewest;
        }

        let attempts = self
            .failed
            .as_ref()
            .filter(|failure| failure.generation == generation)
            .map_or(1, |failure| failure.attempts.saturating_add(1));
        self.failed = Some(WorkspacePersistenceFailure {
            generation,
            attempts,
            summary: summary.into(),
        });
        RETRY_DELAYS
            .get(attempts.saturating_sub(1))
            .copied()
            .map_or(
                WorkspacePersistenceTerminalEffect::AwaitExplicitRetry,
                WorkspacePersistenceTerminalEffect::RetryAfter,
            )
    }
```

### The load-adoption guard (slot 5a's M-4 fix)

`workspaces.rs:35-79`:

```rust
    /// Load workspaces from disk and build sections.
    pub fn load_workspaces(&self) {
        let data_dir = json_store::data_dir();
        // Capture the newest requested mutation *before* dispatching the load.
        // "New Workspace" is reachable from window present, so a user can create
        // a workspace while this load is in flight — and `build_sections_from_file`
        // unconditionally overwrites `workspaces_file`, which would discard that
        // workspace from memory while `persist()` has already scheduled it for
        // disk. The mismatch is what makes it data loss rather than a stale view.
        let requested_at_dispatch = self.imp().persistence.borrow().requested_generation();
        spawn_blocking_then(
            self.clone(),
            move || workspace_manager::load_recovering(&data_dir),
            move |sidebar, load| {
                ...
                // A mutation arrived while the load was running: the in-memory
                // state is newer than what came off disk, and the pending write
                // will make it durable. Adopting the loaded file here would
                // silently revert it.
                if sidebar.imp().persistence.borrow().requested_generation()
                    != requested_at_dispatch
                {
                    tracing::info!(
                        "Skipping workspace load adoption: a workspace mutation superseded it"
                    );
                    sidebar.notify_workspace_structure_changed();
                    sidebar.notify_workspace_scope_changed();
                    return;
                }
                let workspaces_file = load.value;
                sidebar.build_sections_from_file(workspaces_file);
```

Close-time flush bypasses debounce, `workspaces.rs:814` +
`workspaces.rs:838` (`let _ = self.imp().persist_debounce.invalidate();`), using
`WorkspacePersistenceStartReason::Close`.

**Preservation obligation:** `PERSIST_DEBOUNCE_MS = 150` and the four-step
`RETRY_DELAYS` ladder must keep their exact numeric values; `start`,
`apply_success`, and `apply_failure` must keep rejecting any terminal whose
generation is not the current `in_flight` (`IgnoredStale`) and must keep
progressing to `StartNewest` when `requested != durable`; and
`load_workspaces` must keep comparing `requested_generation()` against the value
captured *before* dispatch and skip adoption on mismatch.

---

## 10. File-operation semantics, including slot 5a's landed rename refusal

### The pure decision, and what it deliberately does not decide

`policy.rs:19-22`:

```rust
pub const MAX_UNIQUE_NAME_ATTEMPTS: u32 = 1_000;
```

`policy.rs:40-89`:

```rust
/// Decide what an inline rename commit means, without touching the filesystem.
///
/// Whether the destination already exists is deliberately **not** decided here:
/// it is a live filesystem fact that must be checked inside the worker while the
/// write guard is held — and the rename itself uses `RENAME_NOREPLACE` so the
/// check and the rename are one kernel operation. A decision taken on the GTK
/// thread would be stale by the time the rename runs.
///
/// A typed name containing a path separator is **refused**, not silently
/// reinterpreted. ...
/// Case-folding collisions are **not** refused here ...
#[must_use]
pub fn rename_intent(old_path: &Path, typed_name: &str) -> RenameIntent {
    let new_name = typed_name.trim();
    let old_name = old_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();

    if new_name.is_empty() || new_name == old_name {
        return RenameIntent::Cancel;
    }
    if name_is_not_a_plain_sibling(new_name) {
        return RenameIntent::Cancel;
    }

    RenameIntent::Rename {
        new_path: old_path.with_file_name(new_name),
        new_name: new_name.to_string(),
    }
}

/// Return whether a typed name would leave the row's own directory.
fn name_is_not_a_plain_sibling(name: &str) -> bool {
    name == "."
        || name == ".."
        || name.contains('/')
        || std::path::MAIN_SEPARATOR != '/' && name.contains(std::path::MAIN_SEPARATOR)
}
```

### The refusal type and its exact user-facing string

`policy.rs:91-111`:

```rust
/// Why a rename could not be performed, in the workflow's own vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceRenameRefusal {
    /// Something already exists at the typed name.
    ///
    /// The rename is refused rather than performed, because the platform rename
    /// silently replaces a regular destination and the replaced file's contents
    /// are unrecoverable.
    DestinationExists { name: String },
}

impl WorkspaceRenameRefusal {
    /// Return the user-facing explanation for this refusal.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::DestinationExists { name } => {
                format!("A file named '{name}' already exists in this folder")
            }
        }
    }
}
```

**Exact user-facing message string:**
`"A file named '{name}' already exists in this folder"` — pinned by
`policy.rs:218-225` (`destination_collision_names_the_file_the_user_typed`),
which asserts the rendered form `A file named 'final.md' already exists in this folder`.

### The guarded worker that refuses

`workspace_section/actions.rs:541-547`:

```rust
/// Why a guarded workspace rename did not happen.
enum RenameFailure {
    /// The workflow refused the rename; the user is told why.
    Refused(WorkspaceRenameRefusal),
    /// The platform rename failed.
    Io(std::io::Error),
}
```

`workspace_section/actions.rs:545-620`:

```rust
/// Rename one workspace path under the shared write guard, refusing to replace.
///
/// Three data-safety properties live here and nowhere else:
///
/// 1. **The destination is never replaced.** `rename_durable` is `rename(2)`,
///    which silently replaces a regular destination; the replaced file's
///    contents are unrecoverable. The existence check therefore happens *inside*
///    the worker while the guard is held, not on the GTK thread where it would
///    already be stale.
/// 2. **The rename is ordered against in-app writers.** An editor save resolves
///    its target, writes a temp file, and renames it into place. Without the
///    guard, a sidebar rename interleaved with that sequence lets the save's
///    final `rename()` **re-create the old filename** with the buffer bytes,
///    leaving the tab's new path stale on disk while the UI reports success.
/// 3. **Two guards cannot deadlock, and one target cannot deadlock against
///    itself.** ... Both are avoided by
///    resolving first, deduplicating, and acquiring in **resolved** order.
fn rename_target_guarded(old_path: &Path, new_path: &Path) -> Result<(), RenameFailure> {
    let source = fs_write::resolve_target_identity(old_path).map_err(RenameFailure::Io)?;
    let destination = fs_write::resolve_target_identity(new_path).map_err(RenameFailure::Io)?;

    let refuse_existing_destination = || {
        Err(RenameFailure::Refused(
            WorkspaceRenameRefusal::DestinationExists {
                name: new_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            },
        ))
    };

    if source == destination {
        // The two names already denote one target — a symlink renamed onto the
        // file it points at, most plainly. Refusing here is both the correct
        // answer and what keeps the two acquires below from deadlocking.
        return refuse_existing_destination();
    }

    let (first, second) = if source.as_path() <= destination.as_path() {
        (source, destination)
    } else {
        (destination, source)
    };
    let _first = fs_write::TargetWriteGuard::from_identity(first);
    let _second = fs_write::TargetWriteGuard::from_identity(second);

    // Atomic where the kernel supports it: `RENAME_NOREPLACE` makes "does the
    // destination exist" and "rename" one operation, so no other process can
    // create the destination between them. An `exists()` check plus a rename is
    // two syscalls and is therefore only best-effort against external writers —
    // adequate against LushText's own writers, which the guards above serialize,
    // but not against a concurrent `mv`.
    match fs_write::rename_durable_no_replace(old_path, new_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            refuse_existing_destination()
        }
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            // Older kernel or a filesystem without the flag: fall back to the
            // best-effort check, which still closes the in-app window.
            if fs_metadata::exists(new_path) {
                return refuse_existing_destination();
            }
            fs_write::rename_durable(old_path, new_path).map_err(RenameFailure::Io)
        }
        Err(error) => Err(RenameFailure::Io(error)),
    }
}
```

Surfacing, `workspace_section/actions.rs:361-369`:

```rust
                    Err(RenameFailure::Refused(refusal)) => {
                        // Refusing is the whole point: the platform rename
                        ...
                        section.emit_message(&refusal.message(), NotificationSeverity::Warning);
                        ...
                            // A refused *first* name still leaves the created
```

Unique creation, `workspace_section/actions.rs:622-631`:

```rust
/// Atomically create a file or directory with a unique name.
fn create_unique(dir: &Path, base: &str, is_dir: bool) -> std::io::Result<PathBuf> {
    for attempt in 1..MAX_UNIQUE_NAME_ATTEMPTS {
        let path = dir.join(policy::unique_name_candidate(base, attempt));
        let result = if is_dir {
            fs_write::create_dir_durable(&path)
        } else {
            fs_write::create_new_empty_file_durable(&path)
```

**Preservation obligation:** `rename_intent` must stay pure and must keep
deferring destination existence and case-folding to the worker; the destination
collision must keep being refused inside `rename_target_guarded` under two
`TargetWriteGuard`s acquired in **resolved** order after dedup, via
`rename_durable_no_replace` with the `Unsupported` `exists()` fallback; and the
user must keep seeing exactly `A file named '<name>' already exists in this folder`
as a `NotificationSeverity::Warning`, with `MAX_UNIQUE_NAME_ATTEMPTS = 1_000`
bounding `create_unique`.
