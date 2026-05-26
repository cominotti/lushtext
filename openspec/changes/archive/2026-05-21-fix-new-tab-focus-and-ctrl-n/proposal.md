## Why

Creating a new untitled document currently leaves keyboard focus wherever it was before the new tab was created, so typing immediately after the shortcut can appear to do nothing. The same workflow also uses `Ctrl+T`, while LushText presents the action as creating a new file/document and GNOME conventions use `Ctrl+N` for new content.

## What Changes

- Ensure every user-facing path that creates or selects a new untitled document moves focus reliably to that document's editor surface.
- **BREAKING**: Replace `Ctrl+T` with `Ctrl+N` as the only shortcut for the new document action.
- Align the main menu, command palette, shortcut overlay, and README so the action is consistently presented as creating a new file/document with `Ctrl+N`.
- Add focused widget coverage proving the editor receives focus after new document creation and that the old `Ctrl+T` binding is removed.

## Capabilities

### New Capabilities

- `new-document-flow`: Defines the new untitled document workflow, including editor focus ownership and the clean-break keyboard shortcut contract.

### Modified Capabilities

- None.

## Impact

- Affected UI code: window document creation, action/shortcut registration, command palette command metadata, and keyboard shortcut overlay resources.
- Affected documentation: README shortcut table and any nearby user-facing labels that still describe the action as `New Tab` instead of `New File` or `New Document`.
- Affected tests: widget tests for focus after new document creation and shortcut registration behavior.
- No new dependencies or external APIs.
