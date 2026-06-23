# AdwSidebar / ViewSwitcher Spike Evidence

Date: 2026-06-23

## Platform Reference

- Libadwaita 1.9 provides `AdwSidebar` as an adaptive navigation sidebar with
  sections, items, filtering, placeholders, optional context menus, and
  drag-and-drop hooks.
- Libadwaita 1.9 provides `AdwViewSwitcherSidebar` as a sidebar controller for
  an `AdwViewStack`; page sections come from `AdwViewStackPage` metadata.
- `AdwViewStack` is page-oriented: it shows one named page at a time and expects
  stable page metadata such as title, icon, badge, and attention state.

Official references:

- https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/class.Sidebar.html
- https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/class.ViewSwitcherSidebar.html
- https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.9/class.ViewStack.html

## Current AdwSidebar Inventory

| Surface | Current implementation | Contract evidence | Coverage evidence | Finding |
| --- | --- | --- | --- | --- |
| Browse Notes | `crates/lushtext-core/src/ui/window/notes.rs:167`, `:1846`, `:2524`, `:3200`, `:3280` use `AdwSidebar` sections and items for bookmarks, folder notes, document notes, and open-tab rows. | `openspec/specs/workspace-notes/spec.md:174`; `openspec/specs/document-notes/spec.md:141`; `openspec/specs/line-bookmarks/spec.md:137`. | `crates/lushtext/tests/widget/window.rs:3685`, `:11315`, `:12372`, plus focused notes-browser selection/filtering tests. | Already covered. Keep as `AdwSidebar`; do not convert to `AdwViewSwitcherSidebar` because rows are dynamic records, not stable pages. |
| Local History | `crates/lushtext-core/src/ui/window/local_history.rs:59`, `:345`, `:847`, `:856` use `AdwSidebar` for snapshot rows. | `openspec/specs/local-history/spec.md:192`. | `crates/lushtext/tests/widget/window.rs:3599`, `:8698`, `:8762`, `:8894`. | Already covered. Keep as `AdwSidebar`; do not convert to `AdwViewSwitcherSidebar` because snapshots are dynamic records. |
| Workspace file tree | `crates/lushtext-core/src/ui/sidebar/AGENTS.md:3`; `crates/lushtext/tests/widget/window.rs:3182` asserts the workspace file tree is not an `AdwSidebar`. | Workspace sidebar guidance in root `AGENTS.md` and current sidebar specs require `GtkListView`, `GtkTreeListModel`, and `GtkTreeExpander`. | `test_workspace_file_sidebar_keeps_list_view_tree_model_rail`. | Rejected for both sidebar-family widgets. The primary file tree remains tree-shaped, filesystem-mutating, async, and width-constrained. |

## Candidate Matrix

| Candidate | Data shape | Widget fit | State-extreme notes | Recommendation |
| --- | --- | --- | --- | --- |
| Browse Notes | Shallow, sectioned, dynamic records; selection previews and Open activates. | `AdwSidebar` fit; `AdwViewSwitcherSidebar` rejected. | Empty, filtered, dense, preview, and open-tab states already have widget coverage. Compact handoff remains owned by the dialog shell. | Already covered. No product change. |
| Local History | Shallow dynamic snapshot records; selection previews, Copy/Restore act on selected item. | `AdwSidebar` fit; `AdwViewSwitcherSidebar` rejected. | Empty snapshot, unavailable, restore, error, and populated states already have widget coverage. | Already covered. No product change. |
| Document properties | Grouped document metadata and controls inside the adaptive right pane / compact bottom sheet. | Neither current `AdwSidebar` nor `AdwViewSwitcherSidebar` is a direct fit. | No-context means untitled/no saved file; representative means metadata and formatting rows; dense means long path, many health findings, and unavailable controls; constrained geometry is already handled by the properties shell. | Defer. A future Inspector proposal can decide whether stable pages improve this workflow. |
| File health | Document-local findings surfaced through document properties plus review dialogs. | Not a standalone sidebar fit today. It could become a stable `Health` page only as part of a broader Inspector. | Dense findings and warning visibility matter more than navigation. Existing contract keeps health details in properties, not the bottom bar. | Defer to a future Inspector proposal; reject a standalone sidebar. |
| Encoding and line endings | Quick bottom-bar controls plus document-modal chooser flows. | Not a sidebar fit today. It could become a stable `Encoding` page only as part of a broader Inspector. | No-context and compact-window states require bottom-bar reachability; moving this into a rail would duplicate existing quick actions. | Defer to a future Inspector proposal; reject a standalone sidebar. |
| Workspace sidebar / file tree | Hierarchical workspace folders and files with expansion, mutation, peek, inline rename, async scan, watchers, focus-folder behavior, and clipped labels. | `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` remain required. `AdwSidebar` and `AdwViewSwitcherSidebar` are rejected. | No workspace, zero-folder, dense/deep tree, constrained width, and watcher refresh states are tree-specific. | Reject. Preserve the existing file-tree ownership contract. |
| Document Activity / Inspector concept | Potential stable destinations for document-level workflows. | Possible `AdwViewSwitcherSidebar` candidate if modeled as `AdwViewStack` pages. | Viable pages must handle no saved file/no records, representative document state, dense findings/records, long labels, compact layout, large text, reduced motion, and focus/back/close visibility. | Adopt only in a separate proposal if the page model proves useful. |

## ViewStack Page Model For The Only Viable Candidate

The only candidate that can name stable pages is a future Document Activity or
Inspector surface. Candidate pages:

- `Document`: location, file size, formatting source, and core metadata.
- `Health`: file-health findings and follow-up actions.
- `Notes`: document notes, bookmarks, and related workspace note entry points.
- `History`: local-history snapshot access.
- `Encoding`: encoding and line-ending review plus longer chooser flows.

This is not enough to implement the surface in the current spike. It only shows
that a future `AdwViewSwitcherSidebar` proposal has a plausible stable
`AdwViewStack` shape.

## Workspace File Tree Non-Adoption Rationale

The workspace file tree is intentionally not a navigation-only row list. It owns
real hierarchy expansion through `GtkTreeListModel`, row presentation through
`GtkTreeExpander`, file/folder context actions, inline rename, new file/folder
creation, destructive delete flows, file peek, async bounded directory scans,
deep-folder focus, materialized watcher reconciliation, workspace scope, and the
no-horizontal-scrollbar clipping contract. Replacing that with `AdwSidebar`
would remove behavior or rebuild a custom tree inside a widget designed for
simpler navigation rows.

## Verification Evidence

- Existing widget coverage confirms Notes and Local History use `AdwSidebar`
  and that the workspace file tree does not.
- Runtime builder diagnostics intentionally instantiated Notes and Local History
  sidebar surfaces while probing generated templates:
  - `window::test_notes_browser_uses_sectioned_adw_sidebar_and_filters_note_body`
  - `window::test_local_history_browser_controls_expose_accessibility_roles`
- No new runtime prototype was needed because the only new product idea, the
  Document Activity / Inspector concept, is explicitly deferred to a separate
  proposal.

## Final Recommendation

- Keep Notes and Local History on `AdwSidebar`; they are already the right fit.
- Do not use `AdwViewSwitcherSidebar` for dynamic record lists.
- Keep the primary workspace file tree on `GtkListView`, `GtkTreeListModel`, and
  `GtkTreeExpander`.
- Do not change document properties, file health, or encoding controls in this
  spike.
- Create a separate OpenSpec proposal only if LushText wants to pursue a
  stable-page Document Activity or Inspector surface backed by `AdwViewStack`
  and `AdwViewSwitcherSidebar`.
