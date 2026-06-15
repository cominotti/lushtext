## Why

The `Preferences > Data` page can look unfinished when app data is already current: the Actions section remains visible with no rows, and a fast refresh returns to the same state before users can tell anything happened. This change makes the normal/current state feel intentionally quiet while still making manual verification visible and reassuring.

## What Changes

- Hide the Data page Actions group whenever no action rows are available, so the current/no-op state does not present an empty or irrelevant section.
- Add an explicit verified-current affordance to the Data Format row, shown after a completed scan confirms the app data is current.
- Keep manual refresh visibly in progress for a short minimum dwell time, with the refresh control disabled while verification is running.
- Preserve existing Convert/Retry behavior for supported older metadata and failure states.
- Extend widget coverage for the current-state empty Actions behavior, refresh in-flight feedback, verified-current indicator, and existing state extremes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `format-upgrade-workflow`: refine `Preferences > Data` current-state and manual rescan behavior so empty actions are hidden and successful verification has visible feedback.

## Impact

- `resources/ui/preferences.blp`
- `resources/ui/preferences.ui`
- `resources/ui/template-contract.json`
- `crates/lushtext-core/src/ui/preferences/imp.rs`
- `crates/lushtext/tests/widget/preferences.rs`
- Existing format-upgrade workflow tests and visual/template validation lanes
