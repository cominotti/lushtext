## Why

LushText's header Open control currently opens the file chooser directly, while GNOME Text Editor uses an Open menu button with a searchable recent-document popover. Matching that GNOME surface gives LushText the expected visual style, keyboard behavior, and quick-reopen workflow while preserving normal file chooser access.

## What Changes

- Replace the header Open button with a GNOME Text Editor-style flat Open menu button.
- Add a custom Open popover containing a search entry, a file-chooser button, a recent-document list, and a readable empty state.
- Maintain a recent-document model that can contain more entries than are visible at once, sorted newest first and excluding documents already open when appropriate.
- Show 10 recent rows at a time before the list region scrolls; the popover header and chooser action remain fixed.
- Match GNOME Text Editor's keyboard and activation behavior, including focused search on open, result filtering, Enter-to-open-first-result, row activation, Escape dismissal, and arrow navigation between search and rows.
- Add broad widget, model, accessibility, visual, and smoke coverage for empty, representative, dense, awkward, and constrained states.

## Capabilities

### New Capabilities
- `recent-open-popover`: GNOME Text Editor-style header Open menu button, searchable recent-document popover, row activation behavior, 10-row visible viewport, and high-coverage UI/test contract.

### Modified Capabilities

## Impact

- Affected UI resources: `resources/ui/window.blp`, generated `resources/ui/window.ui`, and likely a new Open popover template.
- Affected window workflows: `crates/lushtext-core/src/ui/window/`, especially action wiring, open-file dialog routing, focus restoration, action catalog metadata, and automation-visible anchors.
- Affected app logic: a new or extended recent-document model/service under `model/`, `services/`, and/or `ui/` depending on final layering.
- Affected persistence: recent files need durable low-stakes app-data storage or a documented integration with existing session/draft state.
- Affected verification: widget tests, state-matrix tests, accessibility smoke, visual geometry proof, automation docs/checks, Blueprint validation, and pre-commit gates.
