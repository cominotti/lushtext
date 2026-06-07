## Why

Opening a document from GNOME Files, another desktop surface, or the CLI must honor the file the user explicitly selected, even when session restore is rebuilding older failed tabs at the same time. The current activation path can focus a stale failed tab or silently ignore a non-path `gio::File`, leaving the requested document unopened and the user staring at yesterday's error.

## What Changes

- Treat external document activation as a stronger user intent than restored failed-load placeholders.
- Preserve existing duplicate-tab behavior for successfully loaded or still-pending real documents.
- Keep failed restore/open tabs visible with their error message, but prevent them from blocking a later explicit activation for the same path.
- Handle `gio::File` inputs without a local filesystem path through a user-visible, non-crashing failure path instead of silently dropping them.
- Add extensive regression coverage for activation/session-restore races, failed-tab duplicate bookkeeping, non-path URI activation, multi-file activation, existing-window reuse, and file chooser path handling where applicable.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `desktop-document-handlers`: define the runtime document-activation contract for stale failed tabs, explicit activation focus, and non-path file inputs.
- `desktop-open-activation-coverage`: require regression tests for stale failed-tab activation, session-restore ordering, URI inputs, and duplicate behavior.
- `portal-sandbox-workflow-coverage`: require portal/sandbox diagnostics for URI or document-portal-style open inputs that cannot be represented as local paths.

## Impact

- `crates/lushtext-core/src/app.rs`: external activation dispatch and non-path `gio::File` handling.
- `crates/lushtext-core/src/ui/window/documents.rs`: document-open duplicate decisions and failed-load state handling.
- `crates/lushtext-core/src/ui/editor_page/*`: likely editor-scoped load state needed to distinguish pending, loaded, failed, and preserved-error tabs.
- `crates/lushtext-core/src/ui/window/session_persistence.rs`: interaction between explicit activation and startup restore selection.
- `crates/lushtext/tests/widget/app.rs` and related widget helpers: regression tests for open activation behavior.
- Potential smoke/diagnostic coverage for portal or sandbox activation paths where a `gio::File` has a URI but no local path.
