## Why

GNOME 50-era `AdwSidebar` gives LushText a more native way to express shallow browse rails, but the current workspace file tree is too stateful and filesystem-heavy to replace safely. The best opportunity is to migrate the existing Notes and Local History dialog rails from hand-built `GtkListBox` rows to `AdwSidebar` while preserving the preview-first UX and all existing workflows.

## What Changes

- Replace the unified `Browse Notes...` dialog's browse rail with `AdwSidebar` sections for workspace notes, document notes, and range notes.
- Preserve the Notes browser's current search, preview, Open action, Markdown rendering, workspace-scope filtering, and compact `AdwNavigationSplitView` handoff, with sidebar pointer clicks selecting/previewing only.
- Keep note editing explicit from the Notes browser: the `Open` action opens the selected workspace, document, or range note editor; sidebar selection or pointer activation must not immediately open an editing popup.
- Keep the shared note editor popup geometry stable when switching between Edit and Render, and align the edit and render text-surface padding.
- Replace the Local History browser's snapshot list rail with `AdwSidebar` while preserving async preview loading, Copy, Restore, safety snapshots, compact handoff, and large-file availability gating.
- Add widget/regression coverage for section selection, explicit Open activation, search/filter empty state, compact dialog allocation, stable edit/render popup sizing, matching edit/render padding, and active item state after preview changes.
- Keep the existing workspace file sidebar on `GtkListView` / `GtkTreeListModel`; it is out of scope for this change.
- Keep the broader `AdwViewSwitcherSidebar` Document Activity/Inspector idea as a documented follow-up, not part of this implementation.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `workspace-notes`: The workspace-scoped notes browser must use an `AdwSidebar` rail for workspace-note entries while preserving scope-aware browsing, preview-only selection, explicit Open activation, and stable edit/render popup layout.
- `document-notes`: The workspace-scoped notes browser must use an `AdwSidebar` rail for document-note entries while preserving document-note preview, preview-only selection, explicit Open activation, and stable edit/render popup layout.
- `sidecar-annotations`: The workspace-scoped notes browser must use an `AdwSidebar` rail for range-note entries while preserving range-note preview, preview-only selection, explicit Open activation, and stable edit/render popup layout.
- `local-history`: The local-history browser must use an `AdwSidebar` rail for snapshots while preserving preview, copy, restore, empty/error states, and large-file policy.

## Impact

- Affected UI code:
  - `crates/lushtext-core/src/ui/window/notes.rs`
  - `crates/lushtext-core/src/ui/window/local_history.rs`
- Affected tests:
  - Focused widget tests for notes-browser and local-history browser behavior.
  - Existing note and local-history tests should remain green.
- Affected docs:
  - `docs/next/gnome-50-api-opportunities.md` records the implementation scope and keeps the Document Activity/Inspector idea as follow-up.
- Dependency/API impact:
  - Uses the existing `libadwaita` 0.9 / `v1_9` binding surface already declared in `Cargo.toml`.
  - Does not change file formats, sidecar storage, local-history storage, workspace persistence, or public CLI behavior.
