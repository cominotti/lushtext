## Why

The bottom status bar is useful for transient feedback and compact document metadata, but its current compact treatment can make messages and controls harder to read than they need to be. We want a slightly calmer, more legible status strip without giving it the same visual weight as the header bar.

## What Changes

- Increase the status bar's readable vertical comfort modestly while preserving its subordinate, status-strip role.
- Keep the bar visibly lower-prominence than the header bar; it must not look like a second primary toolbar.
- Preserve the existing three-part structure: workspace sidebar toggle, full-width message area, and compact document metadata controls.
- Preserve message-area flash behavior, severity contrast, and the left gap between the workspace toggle and flash region.
- Verify the visual contract across empty/no-document, representative populated, awkward-message, narrow-width, short-height, light/dark, and high-contrast states.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `status-bar-notifications`: Add readability and visual-prominence requirements for the persistent bottom status bar.

## Impact

- Affected UI resources and styles: `resources/ui/status-bar.blp`, generated `resources/ui/status-bar.ui`, and `resources/style/style.css`.
- Affected widget code/tests as needed: `crates/lushtext-core/src/ui/status_bar/`, `crates/lushtext/tests/widget/status_bar.rs`, and window geometry/status-bar tests.
- No new runtime dependencies, public APIs, persistence formats, or automation payload fields are expected.
