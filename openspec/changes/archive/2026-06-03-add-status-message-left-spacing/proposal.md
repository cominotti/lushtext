## Why

The new status-bar message flash correctly highlights the full message lane, but its left edge sits too close to the workspace-pane toggle icon. A small gap will make the bottom bar feel less cramped while preserving the clear flash acknowledgement.

## What Changes

- Add a small, stable horizontal inset between the workspace-sidebar toggle and the status-bar message area.
- Keep the message-area flash covering the full remaining message lane, including empty space to the right of short messages.
- Ensure the inset is outside the pulse background so the workspace toggle remains visually separate during info, warning, and error flashes.
- Preserve existing text alignment, bottom-bar height, metadata positioning, and notification pulse behavior.

## Capabilities

### New Capabilities

### Modified Capabilities
- `status-bar-notifications`: Refine the status-bar message area layout so its flash and text start with a small visual gap after the workspace-sidebar toggle.

## Impact

- Affects the status-bar UI template and possibly scoped status-bar CSS.
- Adds or updates widget coverage for the message-area left spacing contract.
- No storage format, public API, command-line, dependency, or notification lifecycle changes are expected.
