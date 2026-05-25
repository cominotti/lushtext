## Why

Creating a new untitled document while Markdown preview-only mode is active currently leaves the window in preview-only mode, so the fresh tab inherits a hidden editor shell and cannot receive normal source-editor focus. This is a critical regression because the New Document action must always produce an immediately editable tab.

## What Changes

- Ensure creating a new untitled document exits Markdown preview-only mode before handing focus to the new editor.
- Keep side-by-side Markdown preview behavior unchanged; the fix targets the full replacement preview-only shell.
- Keep Focus Mode's existing `Alt+P` preview-only behavior intact, while ensuring a normal New Document action does not strand the selected tab behind the preview widget.
- Add widget regression coverage for creating a new document from preview-only mode.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `new-document-flow`: New document creation must leave preview-only mode and reveal the newly selected source editor before focus restoration completes.

## Impact

- Affected code: `crates/lushtext-core/src/ui/window/preview.rs`, `crates/lushtext-core/src/ui/window/documents.rs`, and the window widget tests in `crates/lushtext/tests/widget/window.rs`.
- No data migration, storage format, dependency, or public API changes.
