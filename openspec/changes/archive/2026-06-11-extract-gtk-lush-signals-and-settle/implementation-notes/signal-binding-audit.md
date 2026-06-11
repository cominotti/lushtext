# Signal and Binding Ownership Audit

Scope: current checkout under `crates/lushtext-core/src/ui/**` plus the
placeholder GTK Lush crates. This note records the starting inventory and
migration outcome for `extract-gtk-lush-signals-and-settle`.

## Method

- Searched for manual `glib::SignalHandlerId` storage, replacement, explicit
  disconnect calls, row-data handler IDs, `glib::Binding` storage, binding
  `unbind()`, GTK settings bindings, and Rust `bind_property` uses.
- Compared the inventory with the archived Phase 0 declarative-binding audit
  and the new `gtk-lush-signals` spec.
- Classified sites as first-wave migration candidates, binding candidates, or
  retained explicit sites.

## First-Wave Signal Candidates

These sites fit the first `gtk-lush-signals` contract because they store
handler IDs to disconnect later from a known lifecycle owner:

| Area | Current ownership | Planned treatment |
| --- | --- | --- |
| `ui/editor_page/imp.rs` preference/style handlers | `PreferenceBindingState` stores many `RefCell<Option<SignalHandlerId>>` fields for `gio::Settings` and `StyleManager` handlers. | Replace with grouped signal ownership. Keep semantic grouping so settings/style lifetimes stay readable. |
| `ui/editor_page/imp.rs` buffer handlers | `modified_handler_id`, `buffer_changed_handler_id`, and `end_user_action_handler_id` are disconnected from the current buffer in `disconnect_buffer_handlers`. | Replace with buffer-scoped registration owner that clears before buffer replacement or page teardown. |
| `ui/editor_page/minimap.rs` | Minimap buffer handlers store insert/delete/modified/changed IDs for later disconnection. | Replace with a minimap buffer registration group. |
| `ui/editor_page/focus_mode.rs` | Focus-mode buffer handlers store mark-set and changed IDs. | Replace with a focus-mode buffer registration group. |
| `ui/editor_page/local_history.rs` | Local-history buffer modified handler stores one ID. | Replace with local-history buffer registration ownership. |
| `ui/window/documents.rs` | Connects editor buffer modified/changed handlers and stores IDs on the editor page. | Move recording to the new owner while preserving document workflow callbacks. |
| `ui/search_bar/mod.rs` and `ui/search_bar/imp.rs` | Search context occurrence handler is stored and disconnected explicitly when the context changes. | Replace with context-scoped registration ownership. |

## Migrated Signal and Binding Sites

| Area | Migrated owner | Notes |
| --- | --- | --- |
| `ui/editor_page/imp.rs` preference/style handlers | `PreferenceBindingState::signals: SignalBag` | Settings and StyleManager handlers are disconnected as one global lifecycle group during editor dispose. |
| `ui/window/documents.rs` editor buffer modified/changed handlers | `LushtextEditorPage::document_buffer_signals: SignalBag` | The group is cleared before rewiring a tab page so stale title, draft, and preview closures cannot remain attached. |
| `ui/editor_page/imp.rs` end-user-action handler | `LushtextEditorPage::editing_buffer_signals: SignalBag` | Editor-local edit action observer is disconnected with the tab. |
| `ui/editor_page/minimap.rs` buffer observers | `MinimapState::buffer_signals: SignalBag` | Insert, delete, modified, and changed observers now share the minimap lifecycle group. |
| `ui/editor_page/focus_mode.rs` buffer observers | `FocusModeEditorState::buffer_signals: SignalBag` | Mark-set and changed observers remain page-lifetime but use grouped ownership. |
| `ui/editor_page/local_history.rs` buffer observer | `LocalHistoryState::buffer_signals: SignalBag` | Automatic capture observer is disconnected with local-history state. |
| `ui/search_bar/mod.rs` occurrence-count observer | `occurrences_signals: SignalBag` | SearchContext attachment clears the previous occurrence observer before replacing the context. |
| `ui/search_panel/list_factory.rs` count label binding | `BindingBag` stored on the recycled `GtkListItem` | Factory unbind steals and clears the bag; binding drop remains idempotent. |
| `ui/sidebar/workspace_section/imp.rs` expanded-row hook | `SignalBag` stored on the `GtkTreeListRow` | The previous row-data `SignalHandlerId` is now an owned signal group cleared on unbind. |

## Binding Candidates

Most settings/widget bindings created in constructed-time setup naturally live
for the widget lifetime and do not currently retain explicit `glib::Binding`
values. The clear first-wave binding ownership candidate is virtualized row
recycling:

| Area | Current ownership | Planned treatment |
| --- | --- | --- |
| `ui/search_panel/list_factory.rs` | Stores a count `glib::Binding` in list-item data and calls `unbind()` in `connect_unbind`. | Replace with an owned binding registration attached to the list-item lifecycle if the first API supports row-local binding bags cleanly. |

Other `.bind()` and `bind_property()` calls are already declarative and may
remain explicit if the crate API would only add noise. The final audit must
record any retained binding sites with their lifecycle reason.

## Retained Explicit Or Deferred Registration Sites

These are not first-wave conversions unless implementation proves a safe
fit:

- Short, unowned `connect_clicked`, `connect_activate`,
  `connect_change_state`, and dialog response handlers whose source and target
  share a widget/action lifetime and do not retain handler IDs today.
- Event-controller handlers on transient gestures and key controllers, where
  the controller itself is owned by the widget and no manual handler cleanup is
  currently required.
- Existing Rust-created property bindings in preference/search/window setup
  that are safely widget-lifetime-bound and do not need individual unbind.
- Remaining row-data uses are retained only for non-signal scalar row state,
  for example drag-and-drop suppression flags in `workspace_section/dnd.rs`.

## Conflict Check

Active OpenSpec changes at inventory time:

- `add-snap-packaging`: packaging, Snap CI, and sandbox identity artifacts only;
  no GTK Lush crate, `crate::ui::settle`, or rule-section overlap found.
- `evaluate-qt-quick-redefinition`: listed by OpenSpec with no task artifacts
  and no files under its change directory in this checkout.

No conflicting active OpenSpec work was found for the GTK Lush crates,
`crate::ui::settle`, or the rule sections this change will later update.
