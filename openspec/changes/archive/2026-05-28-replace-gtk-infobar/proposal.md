## Why

GTK marks `GtkInfoBar` as deprecated and its GTK 5 migration guide says it is going away. LushText currently uses `GtkInfoBar` for editor-scoped recovery and error alerts, so replacing it now removes a GTK5 blocker while preserving the current inline workflow.

## What Changes

- Replace the two `GtkInfoBar` children inside `LushtextInfoBar` with supported GTK widgets.
- Preserve editor-scoped warning and error alerts above the editor, including title, body, dismiss, and one or two action buttons.
- Keep existing notification bus, editor-page placement, and action callback API stable for callers.
- Style the replacement with Adwaita-compatible semantic warning and error colors instead of relying on `GtkInfoBar` message-type styling.
- Remove `#[expect(deprecated)]` allowances that exist only for `GtkInfoBar`.
- Add tests proving the replacement no longer instantiates `GtkInfoBar` while keeping current action visibility and dismissal behavior.

## Capabilities

### New Capabilities
- `editor-inline-alerts`: Editor-scoped inline warning and error alerts for recoverable document workflows.

### Modified Capabilities

None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/info_bar`, `resources/ui/info-bar.ui`, `resources/style/style.css`, editor/window notification wiring tests, and related GTK widget tests.
- Public Rust API impact: `LushtextInfoBar` should keep its existing caller-facing methods (`render_notification`, action connectors, and dismissal connector).
- Dependency impact: no new crate or runtime dependency is expected.
- GTK impact: removes the only known use of a GTK widget listed for GTK5 removal while staying on GTK4/Libadwaita-supported primitives.
