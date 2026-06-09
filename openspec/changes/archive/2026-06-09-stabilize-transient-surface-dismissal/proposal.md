## Why

The command palette can remain open after the user clicks outside it, and Escape then stops working because focus has moved away from the palette's search entry. This exposes a broader missing shell contract: dismissible transient surfaces must not depend on child-widget focus to close.

## What Changes

- Define a window-level transient-surface dismissal contract for the command palette and other dismissible shell overlays.
- Ensure outside clicks dismiss the command palette without breaking clicks inside the palette, its results, or child popups.
- Ensure Escape closes the topmost dismissible surface even when focus is in the editor, sidebar, status bar, or another non-modal child.
- Preserve real modal dialogs, destructive confirmations, file choosers, and child popup semantics so global dismissal does not bypass their own cancel/default behavior.
- Restore focus through the existing saved-focus/editor fallback path after dismissal.
- Encode the guardrails in repo guidance by updating the relevant `.agents/rules` and GTK testing skills during implementation.

## Capabilities

### New Capabilities
- `transient-surface-dismissal`: Defines topmost-surface dismissal, click-outside behavior, Escape behavior, focus restoration, and guardrails for shell-owned transient overlays.

### Modified Capabilities

## Impact

- Window shell integration in `crates/lushtext-core/src/ui/window/`, especially command-palette open/close wiring and any shared transient-surface helper.
- Command palette widget integration in `crates/lushtext-core/src/ui/command_palette/` only where needed to preserve internal interactions.
- Blueprint/UI templates only if an explicit outside-click scrim or event target is required.
- Widget tests in `crates/lushtext/tests/widget/` covering focus-independent Escape, outside-click dismissal, inside-click preservation, and topmost-surface ordering.
- Repo guidance in `AGENTS.md`, `.agents/rules/widget-wiring.md`, `.agents/rules/ui.md`, and relevant GTK testing/interaction skills so future overlay work follows the same dismissal contract.
