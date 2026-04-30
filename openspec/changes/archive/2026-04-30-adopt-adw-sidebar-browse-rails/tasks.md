## 1. API and Model Preparation

- [x] 1.1 Verify `libadwaita` `Sidebar`, `SidebarSection`, `SidebarItem`, and `SidebarMode` are available through the current Rust bindings and build/runtime configuration.
- [x] 1.2 Identify whether the existing Flatpak/runtime path can instantiate the Adwaita sidebar widgets without increasing the app's declared platform requirements beyond the GNOME 50 target.
- [x] 1.3 Add small backing item/model helpers for mapping sidebar item selection back to `NotesBrowserEntry` values without moving note-opening logic out of `ui/window/notes.rs`.
- [x] 1.4 Add small backing item/model helpers for mapping sidebar item selection back to `LocalHistorySnapshotMeta` values without moving restore or preview-loading logic out of `ui/window/local_history.rs`.

## 2. Notes Browser Rail

- [x] 2.1 Replace the Notes browser's hand-built `GtkListBox` browse rail with an `AdwSidebar` inside the existing `AdwNavigationSplitView` sidebar page.
- [x] 2.2 Group note entries into dedicated sidebar sections for workspace notes, document notes, and range notes.
- [x] 2.3 Preserve Notes browser search matching across row title, workspace/file metadata, line-range metadata, and note body text.
- [x] 2.4 Preserve empty filtered state behavior when no note entries match the current search.
- [x] 2.5 Preserve preview updates, Markdown rendering context, Open action sensitivity, and activation routing for workspace-note, document-note, and range-note entries.
- [x] 2.6 Preserve compact split-view handoff so selecting/activating a note item navigates to preview on collapsed layouts and the back affordance returns to the notes rail.

## 3. Local History Rail

- [x] 3.1 Replace the Local History browser's hand-built `GtkListBox` snapshot rail with an `AdwSidebar` inside the existing `AdwNavigationSplitView` sidebar page.
- [x] 3.2 Preserve newest-first snapshot ordering and semantic snapshot metadata in sidebar items.
- [x] 3.3 Preserve generation-guarded asynchronous preview loading when the selected sidebar item changes.
- [x] 3.4 Preserve Copy, Restore, safety snapshot capture, modified-buffer state, and immediate undo affordance behavior for the selected snapshot.
- [x] 3.5 Preserve explicit loading, empty snapshot, preview unavailable, no snapshots, and huge-file unavailable states.
- [x] 3.6 Preserve compact split-view handoff so selecting/activating a snapshot item navigates to preview on collapsed layouts and the back affordance returns to the snapshot rail.

## 4. Regression Coverage

- [x] 4.1 Add or update widget tests covering Notes sidebar section creation for workspace, document, and range notes.
- [x] 4.2 Add or update widget tests covering Notes search/filter matches and empty filtered state.
- [x] 4.3 Add or update widget tests covering Notes selection, activation routing, preview updates, and compact handoff.
- [x] 4.4 Add or update widget tests covering Local History sidebar ordering, selection, preview generation guarding, Copy/Restore sensitivity, and compact handoff.
- [x] 4.5 Add or update tests proving the workspace file sidebar remains on `GtkListView` / `GtkTreeListModel` and is not replaced by `AdwSidebar`.

## 5. Verification and Documentation

- [x] 5.1 Run focused widget tests for Notes browser behavior.
- [x] 5.2 Run focused widget tests for Local History browser behavior.
- [x] 5.3 Run `cargo test -p lushtext-core`.
- [x] 5.4 Run the relevant `cargo test -p lushtext --test widget ...` suites for sidebar/window dialog regressions.
- [x] 5.5 Review `README.md`, `AGENTS.md`, nested `AGENTS.md`, and `.agents/rules/*.md` for required documentation updates after the code changes.
- [x] 5.6 Verify `docs/next/gnome-50-api-opportunities.md` still documents the `AdwViewSwitcherSidebar` Document Activity/Inspector idea as a follow-up rather than part of this change.

## 6. Notes Browser Corrective Amendment

- [x] 6.1 Change Notes browser `AdwSidebar` pointer activation so clicking a workspace, document, or range note previews/selects only and does not open an editing popup.
- [x] 6.2 Keep the Notes browser `Open` button as the only route from the browser into workspace-note, document-note, or range-note editing.
- [x] 6.3 Preserve keyboard navigation and compact `AdwNavigationSplitView` handoff so selection still updates the preview and moves to the preview page when collapsed.
- [x] 6.4 Add or update widget tests proving mouse/pointer selection of each note kind does not open an editing popup while the `Open` button still does.

## 7. Shared Note Editor Layout Amendment

- [x] 7.1 Stabilize the shared note editor surface so switching Edit -> Render in document-note and range-note popups does not change the popup's outer size.
- [x] 7.2 Apply the same shared-surface stability to workspace-note popups so the common editor widget has one consistent contract.
- [x] 7.3 Align Edit and Render text-origin padding/margins so plain note text does not shift horizontally or vertically when switching modes.
- [x] 7.4 Add or update widget tests covering stable Edit/Render popup geometry and matching text-origin spacing for document notes and range notes.

## 8. Amendment Verification

- [x] 8.1 Run focused widget tests for Notes browser click/selection/Open behavior.
- [x] 8.2 Run focused widget tests for document-note and range-note Edit/Render layout behavior.
- [x] 8.3 Run `cargo test -p lushtext-core`.
- [x] 8.4 Run the relevant `cargo test -p lushtext --test widget ...` suites for notes/sidebar/dialog regressions.
- [x] 8.5 Re-run `openspec validate adopt-adw-sidebar-browse-rails --type change --strict`.
