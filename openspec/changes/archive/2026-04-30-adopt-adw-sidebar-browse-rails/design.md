## Context

LushText currently has two shallow browse/preview dialog rails that are implemented with custom `GtkListBox` rows:

- The unified Notes browser in `ui/window/notes.rs`, which lists workspace notes, document notes, and range notes for the current workspace scope and renders the selected note in a Markdown preview.
- The Local History browser in `ui/window/local_history.rs`, which lists snapshots newest-first and previews the selected snapshot before Copy or Restore.

Both dialogs already use `AdwNavigationSplitView` so they can show the rail and preview side by side on wide layouts and collapse into a navigation flow on narrow layouts. That makes them a better fit for `AdwSidebar` than the main workspace file sidebar, which owns a real filesystem tree, file operations, file peek, async scanning, watcher reconciliation, deep-folder focus, and a carefully clamped shell width contract.

## Goals / Non-Goals

**Goals:**

- Use `AdwSidebar` for the Notes browser rail.
- Use `AdwSidebar` for the Local History snapshot rail.
- Preserve the current preview-first dialog shapes, sizing, compact behavior, explicit activation flows, search/filter behavior, and safety behavior.
- Ensure Notes browser sidebar selection and pointer clicks preview only; opening an editing popup remains an explicit `Open` action.
- Ensure the shared note editor surface keeps a stable popup size and matching text-origin padding when switching between Edit and Render.
- Keep domain workflows in the existing window modules so sidebar activation routes through the same note-opening and history-restore paths as today.
- Add focused widget tests for section selection, filtering, empty states, compact handoff, activation, and selected-item stability.

**Non-Goals:**

- Do not replace the main workspace file tree with `AdwSidebar`.
- Do not change note, bookmark, annotation, workspace, or local-history storage formats.
- Do not introduce a new document-properties/activity surface in this change.
- Do not use `AdwViewSwitcherSidebar` yet; reserve it for a future stable `AdwViewStack`-backed Document Activity/Inspector surface.
- Do not add diff-only local-history controls or broaden local-history retention policy.

## Decisions

### Use `AdwSidebar` for dynamic browse rows

`AdwSidebar` fits the Notes and Local History rails because they are shallow, sectionable, activation-oriented lists. Notes entries are dynamic data rows grouped by note kind; Local History entries are dynamic snapshot rows that need preview activation. `AdwViewSwitcherSidebar` is a poorer fit for this change because it is tied to stable pages in an `AdwViewStack`, while these dialogs have one preview page whose content changes with selection.

Alternative considered: keep the current `GtkListBox` rails. That avoids API churn, but it leaves custom row selection, empty-state, and keyboard behavior in places where Adwaita now provides a native sidebar pattern.

### Keep `AdwNavigationSplitView` as the dialog shell

Both dialogs already use `AdwNavigationSplitView` for wide side-by-side and narrow list-to-preview flows. The new sidebar should replace only the rail child inside that split view. The preview page, back-button behavior, dialog sizing, and `set_show_content` handoff should remain equivalent.

Alternative considered: move these flows into the main document-properties pane. That would make browse-heavy secondary workflows compete with the active editor shell and compact one-secondary-surface policy. Dialogs remain the safer fit.

### Preserve existing workflow ownership

The Notes browser sidebar should look up the selected backing entry for preview and compact handoff only. Opening/editing a note should remain bound to the explicit `Open` button, which then calls the same helpers used today:

- Workspace note entries open `open_workspace_note_for_root`.
- Document note entries open/focus the file and open the document note for that path.
- Range note entries open/focus the file and reopen the selected range note.

Local History remains different: selecting a snapshot item loads the preview asynchronously, and activating a snapshot in compact mode may navigate to the preview page. Restore still captures the safety snapshot before mutating the editor buffer.

The sidebar item should carry only enough identity to route back into these workflows. It should not own persistence, editor mutation, note rendering, or history restore logic.

Alternative considered: create custom `AdwSidebarItem` subclasses with behavior embedded in each item. That would couple UI row setup to domain workflows and make tests harder to reason about.

### Treat Notes `AdwSidebar::activated` as preview/navigation, not edit

`AdwSidebar` pointer clicks can emit activation as part of normal navigation. In the Notes browser, that means binding `activated` directly to `open_selected()` makes mouse users jump straight into the edit popup instead of browsing previews.

The Notes browser should therefore either avoid connecting `activated` or handle it as selection/preview only. The `Open` button is the stable, visible editing affordance and should be the only route from the browser into workspace-note, document-note, or range-note editing.

Alternative considered: keep activation opening notes and require users to use only keyboard navigation for preview. That breaks the preview-first UX and makes pointer browsing unusable.

### Stabilize the shared note editor surface

Document notes, workspace notes, and range notes share `build_note_editor_surface()`. The Edit page uses a `GtkTextView` inside a `GtkScrolledWindow`; the Render page uses `LushtextMarkdownPreview`, whose internal `GtkTextView` has different margins and whose rendered content is populated on first switch.

The shared surface should reserve a stable content size for both pages before the user switches modes. Edit and Render should present the same text origin for plain note text, so switching modes feels like changing representation rather than moving the document.

Implementation may do this by aligning margins on the edit `GtkTextView` and rendered preview `GtkTextView`, setting consistent min content dimensions on both pages, and preventing the `AdwAlertDialog` extra child from remeasuring to a new natural size after first render.

Alternative considered: accept the natural-size jump because Markdown content can be taller than edit text. That makes the popup feel unstable, and the user explicitly reported it as wrong.

### Model search as sidebar filtering

The Notes browser search entry should continue matching row title, subtitle/path metadata, and note text. The filtered results should be reflected through `AdwSidebar` visibility or filter support, with an explicit empty state when no note entries match. Filtering MUST keep the preview and Open button in a consistent empty-selection state.

Local History does not currently expose search and should not gain search as part of this change.

Alternative considered: keep rebuilding separate filtered `GtkListBox` rows for Notes only. That would undermine the point of adopting the new sidebar widget and preserve a parallel custom selection path.

## Risks / Trade-offs

- `AdwSidebar` selection cannot be permanently disabled -> empty and filtered-empty states must use placeholder or disabled/non-visible items rather than relying on a permanently unselected list.
- Sidebar row APIs may expose less custom layout control than the hand-built rows -> preserve essential title, subtitle, icon, tooltip, and metadata first; avoid recreating complex custom row boxes unless a real usability gap appears.
- `AdwSidebar::activated` may fire for pointer navigation -> Notes browser activation must not be treated as edit/open.
- Notes filtering includes note body text that may not be visible in sidebar item labels -> keep the backing entry model as the filter authority rather than limiting matches to rendered title/subtitle strings.
- Note editor rendering is shared across workspace, document, and range notes -> fixes for document/range popup stability should be applied to the shared surface so workspace notes do not retain the same defect.
- Local History preview loading is asynchronous -> use the existing generation-counter pattern so stale preview loads cannot update the preview after the selected sidebar item changes.
- Widget availability depends on the existing libadwaita `v1_9` binding/runtime surface -> implementation should verify the Flatpak/runtime path before replacing the current rows broadly.

## Migration Plan

1. Introduce small backing models/helpers for Notes and Local History sidebar items so UI selection maps back to existing `NotesBrowserEntry` and `LocalHistorySnapshotMeta` values.
2. Replace the Notes browser list rail with an `AdwSidebar` inside the existing sidebar page.
3. Replace Notes filtering with sidebar-backed filtering and preserve current preview/Open state updates.
4. Replace the Local History snapshot rail with an `AdwSidebar` inside the existing snapshot page.
5. Preserve the existing preview, Copy, Restore, compact handoff, and error/empty states.
6. Amend Notes browser activation so sidebar clicks select/preview only and `Open` remains the only edit action.
7. Stabilize the shared note editor surface so Edit/Render mode switching keeps size and text-origin spacing consistent.
8. Add focused widget tests and run the relevant widget/core suites.

Rollback is local: restore the previous `GtkListBox` rail builders in `notes.rs` and `local_history.rs`. No storage migration is involved.
