## Why

LushText can persist recent-document entries while the Open popover still renders `No Recent Documents` after the corresponding tabs are closed in the same session. The recent list is a primary file-return workflow, so stale duplicate-tab bookkeeping must never hide valid closed documents.

## What Changes

- Keep the Open popover's visible recent rows synchronized with the actual mounted tab model after file open, duplicate focus, tab close, page detach, Save As, rename/delete, failed load, session restore, and canonical-path refresh flows.
- Prevent stale open-document identity state from suppressing closed recent documents.
- Add a broad regression suite across pure service behavior, window/widget state, same-session open/close flows, startup-loaded persistence, real action/D-Bus activation paths, visual geometry, and accessibility-relevant states.
- Preserve existing behavior where documents that are genuinely open remain hidden from the recent list and closed documents reappear without needing an app restart.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `recent-open-popover`: clarify that open-tab exclusion must be derived from the current tab reality, not stale duplicate-detection state, and require regression coverage for the same-session stale identity paths.

## Impact

- `crates/lushtext-core/src/ui/window/recent_open.rs`
- `crates/lushtext-core/src/ui/window/documents.rs`
- `crates/lushtext-core/src/ui/window/imp.rs`
- `crates/lushtext-core/src/ui/window/dialogs.rs`
- `crates/lushtext-core/src/ui/window/session_persistence.rs`
- `crates/lushtext-core/src/ui/window/tabs.rs`
- `crates/lushtext-core/src/services/recent_documents.rs`
- `crates/lushtext/tests/widget/app.rs`
- `crates/lushtext/tests/widget/open_popover.rs`
- `crates/lushtext/tests/widget/window.rs`
- `crates/cargo-gtk-proof/src/live.rs`
- `crates/cargo-gtk-proof/src/runner.rs`
- `scripts/visual-geometry-scenarios/open-popover.json`
- `docs/automation.md`
- Visual geometry, automation, and accessibility smoke coverage for the Open popover where affected
