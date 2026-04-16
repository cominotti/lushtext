## Why

LushText's encoding toolkit is functionally strong, but its current status-bar and dialog surfaces are denser than the GNOME HIG-focused delta we just added to the spec. The shipped behavior still needs a narrower, more progressive-disclosure UI shape so encoding, line endings, and health details stay compact, plain-language, and accessible on smaller window widths.

## What Changes

- Tighten the status-bar encoding metadata so the always-visible labels stay short and scannable.
- Split the current encoding toolkit surface into a lightweight summary entry point plus dedicated modal chooser flows for reopen/save encoding decisions and invisible-character mode selection.
- Add a compact grouped document-format entry point for narrow windows so encoding, line endings, and issues remain reachable when the full metadata cluster does not fit comfortably.
- Keep mixed line endings and low-confidence decoding in non-blocking document-local warning and health surfaces, and refresh copy to match the GNOME-focused wording in the spec delta.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `encoding-toolkit`: Tighten the document-format UI to follow the updated GNOME HIG interaction rules around compact status-bar surfaces, grouped narrow-width access, dedicated chooser dialogs, and concise action copy.

## Impact

- Affected code: `crates/lushtext-core/src/ui/status_bar`, `crates/lushtext-core/src/ui/window/encoding.rs`, `crates/lushtext-core/src/ui/window/documents.rs`, `crates/lushtext-core/src/ui/window/actions.rs`, `resources/ui/status-bar.ui`, and widget tests covering the window/status-bar encoding flows.
- Affected systems: status-bar metadata rendering, encoding chooser workflows, line-ending access from mixed-ending warnings, and adaptive window chrome behavior.
- Dependencies and APIs: no new external dependencies expected; this is a UI-shell and test-surface refinement on top of the already-shipped encoding toolkit.
