## Context

GNOME Text Editor 50.1 builds the Open popover recent list from source-level pieces that LushText only partially mirrors today. The current LushText CSS already follows GNOME's `.open-popover listview row` rules closely, but the row widget and model differ: LushText uses `GtkSingleSelection`, a manually built `GtkGrid` with `GtkLabel`s, a 48px height request, and a close button in the final text column. GNOME Text Editor 50.1 uses `GtkNoSelection`, position/focus-driven navigation, a builder row widget equivalent to `EditorSidebarRow`, `GtkInscription` for row text, a leading homogeneous stack/marker column, and a remove button in column 3 spanning both text rows.

The source of truth for this change is GNOME Text Editor 50.1, verified against `src/style.css`, `src/editor-open-popover.ui`, `src/editor-sidebar-row.ui`, and the Open popover controller code. The relevant GNOME details are:

- Popover class `.open-popover`, width request 350, fixed search/chooser row with 6px margins, separator, and stack.
- Recent scroller with `hscrollbar-policy=never`, `min-content-width=250`, `max-content-width=250`, `max-content-height=600`, `propagate-natural-height=true`, and `vexpand=true`.
- List model as `GtkNoSelection`, with `single-click-activate=true` and activation by list position.
- Row CSS: row margin `3px 6px`, first row top margin `6px`, row border radius `6px`, row button padding `3px`, margin `0`, and min width/height `24px`.
- Row content grid: margins top/bottom `3`, start `0`, end `6`; row spacing `3`; column spacing `6`; marker stack in column 0; title inscription in column 1 spanning columns 1-2 with middle ellipsizing; subtitle inscription in row 1 column 1 with `caption` and `dim-label`; optional age inscription in row 1 column 2; close button in column 3 spanning rows 0-1, `flat` and `circular`.

## Goals / Non-Goals

**Goals:**

- Make Open popover recent rows visually and structurally match GNOME Text Editor 50.1.
- Remove selected-row/accent highlighting caused by the current `GtkSingleSelection` model.
- Preserve LushText's recent-history rules, already-open-tab exclusion, row removal semantics, file-chooser route, search filtering, and duplicate-safe document activation.
- Add many regression tests across service/model behavior, widget structure, keyboard navigation, accessibility, visual geometry, state extremes, and close/reopen workflows.

**Non-Goals:**

- Do not redesign the rest of the Open popover beyond GNOME row/scroller parity.
- Do not replace LushText's app-owned recent-document persistence with GNOME Text Editor's session model.
- Do not change file-opening semantics, duplicate-tab handling, or recent-history pruning except where tests expose a bug.
- Do not require pixel-perfect screenshots across all platform themes; use source-level structure, CSS properties, geometry, and state checks as the stable contract, with visual proof for realistic light/dark rendered states.

## Decisions

1. Use GNOME Text Editor 50.1 source as the parity baseline.

   The implementation should compare against the tagged 50.1 source files, not an approximate screenshot. This makes the work reproducible and avoids tuning around transient theme artifacts. If GNOME changes the row in a later release, that should be a separate explicit baseline update.

2. Replace recent-list selection with `GtkNoSelection`.

   GNOME's Open popover does not maintain a selected recent row. Keyboard movement changes focus and list position, while activation receives a position. LushText should follow that model so hovering/focusing a row gets GTK's normal list-row treatment without the current selected/accent background. The existing activation callbacks can remain position-based after resolving the row from the filtered store.

3. Build a GNOME-shaped recent row instead of tuning the existing row.

   The row should mirror GNOME's `EditorSidebarRow` structure: a `GtkGrid`, leading homogeneous stack/marker spacer, `GtkInscription` title/subtitle/age cells, and a `flat circular` remove button spanning both text rows. This is less brittle than trying to pad a different widget tree into the same apparent shape, and it gives tests a clear structure to assert.

4. Preserve LushText-owned data and callbacks.

   The row widget should receive a `RecentDocumentRow` item and keep LushText's existing open/remove callbacks. The remove button must still remove only the recent entry, avoid row activation, and leave the popover open. Opening a row must still route through the normal duplicate-safe document-open workflow.

5. Match GNOME scroller sizing where it conflicts with the current ten-row approximation.

   LushText currently sets the recent scroller max height from an approximate row height. GNOME Text Editor 50.1 declares `max-content-height=600` directly. The implementation should use the GNOME value and update tests so the contract is "GNOME scroller parity plus the existing item-region-only scrolling behavior," not a hard-coded 48px row assumption.

6. Test the behavior as a state matrix, not only as a happy path.

   The regression suite should cover no recents, no matches, one row, representative rows, exactly ten rows, more than ten rows, awkward long labels, constrained geometry, all rows open, all rows closed, open/close in the same session, removal while visible, search while visible, keyboard traversal, pointer activation, accessibility names, visual light/dark proof, and absence of selected/accent row state.

## Risks / Trade-offs

- `GtkInscription` may require small gtk4-rs binding adjustments in tests or helper code -> Use the existing GTK4 dependency surface first, and fall back to direct property assertions only where typed helpers are awkward.
- Visual proof can become brittle across themes or toolkit versions -> Assert source-level structure and CSS/geometry as the normative contract, then use screenshots to catch gross rendered regressions rather than theme-private pixel values.
- Removing `GtkSingleSelection` changes the internal keyboard path -> Add focused tests for Down from search, Up from first row, Enter activation, pointer activation, Escape dismissal, and activation after filtering.
- A custom row widget could regress accessibility labels -> Preserve or improve row and remove-button accessible labels in the same implementation and test them directly.
- Copying GNOME's visual structure could accidentally import unrelated GNOME Text Editor behavior -> Keep LushText persistence, search ranking, open-tab exclusion, and activation callbacks owned by the existing model/service code.

## Migration Plan

This is an in-place UI behavior change with no persisted format migration. The implementation should update the Open popover widget, regenerate UI resources if the `.blp` changes, and keep existing recent-document data compatible.

Rollback is straightforward: revert the row/model/scroller changes and their tests. No user data conversion is involved.

## Open Questions

- Should the leading marker column always remain an empty spacer for recent rows, or should LushText expose a modified marker later for draft/session concepts? For this change, the spacer should be present only to match GNOME row geometry.
- Should the visual proof compare against a captured GNOME Text Editor fixture, or should it assert LushText's rendered geometry against the 50.1 source-derived constants? The safer default is source-derived geometry, because a live GNOME fixture is environment-dependent.
