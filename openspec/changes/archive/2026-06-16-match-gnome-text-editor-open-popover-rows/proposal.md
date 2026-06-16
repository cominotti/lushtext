## Why

The Open popover currently looks close in broad layout, but its recent-row rendering still diverges from GNOME Text Editor: selected/accent highlighting, row spacing, text widgets, and close-button alignment come from LushText's custom row rather than GNOME's source structure. Since the Open popover is meant to feel identical to GNOME Text Editor, the contract needs to require source-level row parity instead of a loose visual resemblance.

## What Changes

- Match GNOME Text Editor 50.1 recent-row presentation for the Open popover, using the same row structure, margins, padding, text overflow behavior, close-button placement, and hover/focus highlight behavior.
- Remove selection-model-driven accent highlighting from recent rows; navigation and activation should behave like GNOME Text Editor's position/focus-driven list, not a selected-row list.
- Preserve LushText behavior for app-owned recent history, already-open tab exclusion, row removal, search, file-chooser routing, and duplicate-safe document activation.
- Expand regression coverage substantially across pure recent-history behavior, GTK widget structure, keyboard navigation, accessibility anchors, visual geometry, constrained layouts, awkward labels, and open/closed row state matrices.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `recent-open-popover`: Strengthen the Open popover layout and verification requirements so recent rows must match GNOME Text Editor 50.1 source-level structure and styling, including no selection-accent row state.

## Impact

- Affected UI resources and styling: `resources/ui/open-popover.blp`, generated UI resources, and `resources/style/style.css`.
- Affected Rust UI code: `crates/lushtext-core/src/ui/open_popover/imp.rs` and any extracted row widget/module needed to mirror GNOME Text Editor's row composition.
- Affected tests and proof assets: Open popover model/service tests, GTK widget tests, keyboard tests, accessibility checks, visual geometry proof scenarios, and smoke coverage for recent-row state extremes.
- Documentation or automation references must be updated if accessible anchors, action catalog entries, automation snapshots, or proof scenario names change.
