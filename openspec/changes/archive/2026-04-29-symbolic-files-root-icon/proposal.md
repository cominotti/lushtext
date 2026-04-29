## Why

The regular file-tree icon change made the synthetic `Files` root row use the same colorful folder icon as real directories. That row is a workspace-section landmark rather than a real filesystem folder, so a symbolic icon better matches the surrounding workspace controls and keeps colorful icons reserved for actual content.

## What Changes

- Treat the single-directory workspace root row labeled `Files` as sidebar structure, not as a normal directory content row for icon selection.
- Render that synthetic `Files` root row with a symbolic icon, preferably `view-list-symbolic`, while preserving the `Files` label.
- Keep real directory rows, including drill-down root rows that show the actual focused folder name, on regular themed folder icons.
- Keep file rows on regular themed content-type icons.
- Keep existing row behavior for expansion, selection, refresh, file peek, context menus, and drill-down.

## Capabilities

### New Capabilities
- `symbolic-files-root-icon`: The synthetic `Files` root row uses a symbolic workspace/navigation icon while real file-tree content keeps regular themed icons.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`, `crates/lushtext-core/src/ui/sidebar/workspace_section/icon_presentation.rs`, and targeted workspace-section widget tests.
- GTK/GIO integration: no dependency changes; this is a presentation rule in the existing row factory.
- Tests: update coverage so the synthetic `Files` row is symbolic, drill-down roots remain regular folder rows, and file rows remain content-type regular icons.
- OpenSpec: this is a follow-up correction to the completed `regular-themed-file-tree-icons` change while it remains active.
