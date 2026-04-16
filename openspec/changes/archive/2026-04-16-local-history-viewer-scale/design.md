## Context

LushText already ships local history with an adaptive `AdwDialog` and
`AdwNavigationSplitView`, but the current browser still feels like a modest
modal sheet. The dialog uses a fixed default size and a fairly generous sidebar
rail, which means the preview often lacks the visual dominance users expect
from a history viewer for document content.

This follow-up does not change snapshot capture, restore safety, or lineage
rules. It is a presentation-focused refinement across the existing window
workflow, widget tests, and product/spec documentation. The main constraint is
that the browser should feel substantially larger and more viewer-like without
breaking the GNOME HIG expectation that secondary windows remain simple and do
not exceed the size of their parent window.

## Goals / Non-Goals

**Goals:**
- Open local history as a clearly viewer-first dialog on wide desktop windows.
- Make the preview area the dominant surface when the split view is expanded.
- Keep the browser bounded by the parent window instead of behaving like a new
  primary window.
- Preserve the existing adaptive collapsed navigation flow on narrower widths.
- Keep the no-snapshots experience simple and proportionate rather than turning
  it into a giant empty shell.
- Make the sizing and layout expectations explicit enough to verify in widget
  tests and in the living spec.

**Non-Goals:**
- Changing how local-history snapshots are captured, stored, restored, or
  deduplicated.
- Replacing the dialog with a persistent right-side utility pane or a separate
  top-level window.
- Adding diff, compare, filtering, or metadata-heavy browsing features.
- Reworking the empty state into a full viewer layout when there is no snapshot
  content to browse.

## Decisions

### 1. Keep the browser in an `AdwDialog`, but treat it as a large viewer surface

The browser should remain an on-demand modal dialog tied to the active window,
instead of moving into the persistent properties pane or spawning a separate
window. However, its sizing contract should change from a small fixed default to
an intentionally large viewer-like presentation.

Rationale:
- The current local-history workflow is still secondary to editing, so it fits
  better as a parent-owned transient surface than as a new primary window.
- GNOME HIG expects dialogs and secondary windows to stay parent-bound and not
  exceed the parent window, which matches the existing `AdwDialog` direction.
- The user’s feedback is about scale and feel, not about changing the
  navigation model entirely.

Alternatives considered:
- Move local history into the right properties pane: rejected because the pane
  is intentionally narrow and subordinate, which fights the desired viewer
  feel.
- Open a separate window: rejected because it adds lifecycle complexity and
  makes local history feel more detached than necessary.

### 2. Derive the default dialog size from the current parent window with generous clamps

When snapshots exist, the dialog should size itself from the current main
window dimensions so it occupies most of the available editing surface while
still leaving the parent frame visible around it. The sizing policy should use
large desktop-friendly clamps rather than a single modest fixed width/height.

Implementation direction:
- Measure the current `LushtextWindow` allocation when opening the browser.
- Target a large fraction of that space on both axes.
- Clamp the result into a comfortable desktop viewer range.
- Apply a final cap so the dialog never grows larger than the visible parent
  area.

Rationale:
- GNOME HIG says windows displaying large document-like content should be large
  enough for viewing without immediate resizing, while also keeping secondary
  windows inside the parent’s bounds.
- A parent-relative policy automatically scales up on large desktops and scales
  down on smaller laptops, which is more robust than chasing one ideal pixel
  size.
- This lets the browser feel intentionally large without turning it into a
  pseudo-fullscreen takeover.

Alternatives considered:
- Raise the fixed defaults from `1120 x 760` to a bigger constant: rejected
  because it still under-fits some parent windows and over-fits others.
- Let content-driven natural size choose the dialog geometry: rejected because
  the resulting size remains too dependent on incidental widget content rather
  than the intended viewer role.

### 3. Make the split layout preview-dominant on wide windows

The expanded split view should behave like a viewer with a history rail, not as
two peers competing equally for attention. The snapshot list should stay wide
enough to read timestamps and metadata comfortably, but it should remain
visibly secondary to the preview surface.

Implementation direction:
- Narrow the sidebar clamp compared with the current implementation.
- Keep the preview in the content area so it receives the remaining majority of
  width by default.
- Retain the existing row list and preview-header/actions composition rather
  than adding extra columns or dense controls.

Rationale:
- The user’s “big viewer pane” request is as much about balance as absolute
  size.
- A narrower browse rail improves the perception of the preview as the primary
  reading surface even before any new visual styling is introduced.
- This stays aligned with GNOME’s utility-pane guidance, where supplementary
  controls sit beside the main content rather than sharing equal status.

Alternatives considered:
- Leave the current sidebar width limits unchanged and only enlarge the dialog:
  rejected because the preview would gain room, but the overall composition
  would still read more like a balanced split than a viewer-first surface.
- Hide the list entirely on wide layouts: rejected because fast snapshot
  browsing depends on visible list context.

### 4. Keep the current collapsed navigation flow for narrow widths

The existing `NavigationSplitView` collapse behavior should remain the adaptive
path when the browser cannot comfortably show both areas side by side. The
change is about making wide layouts more generous, not about forcing a split
layout on every form factor.

Rationale:
- The current adaptive behavior already matches the living spec and GNOME
  expectations for constrained widths.
- Preserving it avoids turning the “larger desktop viewer” request into a
  regression on smaller windows.

Alternatives considered:
- Force the split view to remain expanded until extremely narrow widths:
  rejected because it makes the list and preview both cramped on mid-sized
  windows.

### 5. Keep the empty-state dialog compact

If a saved file has no snapshots yet, the dialog may continue using a smaller
empty-state presentation instead of reserving the full viewer-scale shell.

Rationale:
- GNOME HIG warns against oversized secondary windows with limited content.
- A large viewer shell is justified when there is document history to inspect;
  it is unnecessary chrome when the user only needs a short explanatory state.
- This avoids a follow-up regression where “make it bigger” produces a large
  blank surface with little user value.

Alternatives considered:
- Reuse the large viewer shell for the empty state: rejected because it spends a
  lot of space on a status page with no preview content.

### 6. Add widget-test coverage for dialog scale and preview emphasis

This change should add focused widget assertions that verify the browser opens
with a large content size relative to the parent window and that the wide split
view preserves preview dominance.

Implementation direction:
- Present the browser from a known test-window size.
- Assert the dialog content dimensions or actual allocation exceed a large
  threshold relative to that parent.
- Assert the split view’s sidebar clamp stays below the preview-dominant ratio
  expected by the new contract.

Rationale:
- The request is explicitly about feel, which is easy to hand-wave unless the
  geometry contract is made testable.
- Existing local-history widget coverage already exercises the dialog and split
  view, so this remains a natural extension of the current harness.

Alternatives considered:
- Rely on manual review only: rejected because layout regressions are likely to
  reappear without guardrails.

## Risks / Trade-offs

- [Large defaults feel cramped on medium laptop windows] → derive size from the
  parent and clamp it instead of using a single desktop-only constant.
- [Preview emphasis makes the snapshot list too narrow to scan] → keep a
  readable minimum rail width and validate it in widget tests.
- [A “bigger modal” starts to feel like a pseudo-window takeover] → cap the
  dialog below the parent’s visible size and keep the empty-state dialog compact.
- [Geometry assertions become brittle in tests] → prefer parent-relative
  thresholds and split-view clamp checks over pixel-perfect allocation matches.

## Migration Plan

1. Update the local-history OpenSpec requirement and the product note so the new
   viewer-scale behavior is part of the accepted contract.
2. Adjust the dialog sizing and split-view rail limits in
   `ui/window/local_history.rs`.
3. Extend widget coverage for wide-window presentation and preview-dominant
   layout.
4. No data migration is required; rollback simply restores the previous dialog
   geometry behavior.

## Open Questions

- None blocking. The remaining questions are implementation-detail tuning of the
  exact clamp values, which can be finalized during apply work as long as the
  viewer-first, parent-bounded contract is preserved.
