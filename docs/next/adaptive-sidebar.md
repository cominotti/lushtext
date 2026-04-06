# Adaptive Sidebar: HIG-Level Narrow Support

Path to GNOME HIG-compliant narrow-window behavior, allowing the window
to shrink below 640px by collapsing the sidebar into an overlay drawer.

## Current State

LushText uses `GtkPaned` with a draggable divider between the sidebar and
content stack. A 640px `width-request` on the window prevents geometry
violations (`sidebar_min + handle + stack_min > window_width`). The sidebar
is always side-by-side with the editor — it never collapses.

## Goal

Support window widths down to ~360px (GNOME HIG phone target) by
collapsing the sidebar at a breakpoint, matching the Nautilus / GNOME
Text Editor pattern while preserving the draggable divider at wider widths.

## Recommended Approach: AdwBreakpoint + GtkPaned

Use `AdwBreakpoint` (available since libadwaita 1.4) to toggle the sidebar
between side-by-side (GtkPaned) and overlay (GtkRevealer or manual overlay)
modes. This preserves the draggable divider at desktop widths.

### Phase 1: AdwBreakpoint Integration

1. Add an `AdwBreakpoint` on the window with condition `max-width: 500sp`.
2. When the breakpoint activates (narrow):
   - Hide the sidebar from the GtkPaned (set `start-child` visibility
     or reparent to an overlay).
   - Show a sidebar toggle button in the header bar.
   - Toggling shows the sidebar as a `GtkRevealer` overlay (slide-in from
     left) above the content, with a dimming scrim behind it.
   - Reduce `width-request` to ~360px.
3. When the breakpoint deactivates (wide):
   - Restore the sidebar as GtkPaned's start child.
   - Hide the header bar toggle button.
   - Restore `width-request` to 640px.

### Phase 2: Overlay Sidebar Widget

Extract the overlay behavior into a reusable pattern:

- `LushtextAdaptiveSidebar` — wraps the sidebar content and handles
  the transition between paned-child and overlay modes.
- Uses `GtkRevealer(transition-type=slide-right)` for the overlay.
- A semi-transparent scrim (`GtkGestureClick` to dismiss) covers the
  content area when the overlay is open.
- Sidebar width in overlay mode matches `saved_sidebar_pos` (the last
  dragged width), clamped to a reasonable range.

### Phase 3: Gesture Support

- Swipe-from-left gesture to reveal the sidebar in narrow mode
  (similar to AdwOverlaySplitView's built-in gesture).
- `GtkGestureDrag` on the window's left edge, threshold ~20px.

## Alternative Considered: AdwOverlaySplitView

Replacing GtkPaned with `AdwOverlaySplitView` would get adaptive collapse
for free but **loses the draggable divider entirely**. No GNOME widget
provides both adaptive collapse and user-resizable width. The
AdwBreakpoint approach preserves both at the cost of more implementation.

## Reference: What GNOME Apps Do

| App               | Widget                  | Draggable | Adaptive |
|-------------------|-------------------------|-----------|----------|
| Nautilus (Files)  | AdwOverlaySplitView     | No        | Yes      |
| GNOME Text Editor | AdwOverlaySplitView     | No        | Yes      |
| Epiphany (Web)    | AdwOverlaySplitView     | No        | Yes      |
| GNOME Builder     | libpanel PanelDock      | Yes       | No       |
| **LushText (now)**| GtkPaned                | Yes       | No       |
| **LushText (goal)**| GtkPaned + AdwBreakpoint| Yes       | Yes      |

## Dependencies

- libadwaita >= 1.4 (already satisfied: LushText uses 0.9 / Adw 1.6+)
- `AdwBreakpoint` support in gtk4-rs 0.11 / libadwaita 0.9

## Related

- `fix(ui): prevent GtkStack measurement warning during window resize` —
  the 640px `width-request` floor is a prerequisite for this work. The
  adaptive sidebar removes that floor when in narrow mode.
