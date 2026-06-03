## Why

Rapidly repeated status-bar notifications, such as pressing Save several times and seeing "File saved", currently look unchanged. Users cannot tell whether the visible message belongs to the previous action or the latest one.

## What Changes

- Add a brief severity-colored flash when the visible status-bar notification is newly published or meaningfully updated.
- Highlight the entire horizontal message area available between the workspace toggle and document metadata controls, not only the message text.
- Use semantic info, warning, and error colors with readable foreground contrast during the flash.
- Ensure repeated identical messages can restart the flash so each action receives visible acknowledgement.
- Keep background maintenance renders, expiry sweeps, and unchanged progress heartbeats from creating distracting flashes.

## Capabilities

### New Capabilities
- `status-bar-notifications`: Covers transient and progress notification presentation in the persistent bottom status bar, including severity styling and update acknowledgement.

### Modified Capabilities

## Impact

- Affects `LushtextStatusBar`, the status-bar UI template, application CSS, and the window notification rendering/publication bridge.
- Adds focused unit or widget coverage for repeated identical status messages and severity-specific pulse state.
- No storage format, public API, command-line, or dependency changes are expected.
