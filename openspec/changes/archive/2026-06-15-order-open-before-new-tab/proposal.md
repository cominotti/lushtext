## Why

The GNOME-style Open control is now LushText's primary gateway for bringing documents into the editor: recent search, the normal file chooser, and duplicate-safe document activation all start there. Keeping New File before Open preserves the old header order but makes the new GNOME-matching surface feel slightly secondary and inconsistent with GNOME Text Editor.

## What Changes

- Reorder the left side of the main header bar so the Open menu button appears before the New File/New Tab button.
- Preserve all existing actions, shortcuts, tooltips, accessibility labels, popover behavior, and compact-width Open button behavior.
- Add focused widget and visual coverage proving the order is stable in wide and constrained header presentations.

## Capabilities

### New Capabilities
- `header-open-action-order`: Header-bar primary action ordering for the GNOME-style Open control relative to New File/New Tab.

### Modified Capabilities

## Impact

- Affected UI resources: `resources/ui/window.blp`, generated `resources/ui/window.ui`, and the template contract.
- Affected tests: widget assertions for shell control order and focused visual geometry coverage for the header Open/New ordering.
- Affected documentation/spec context: this change builds on `match-gnome-open-popover`, but it does not alter recent-document persistence, Open popover behavior, shortcuts, or automation action semantics.
