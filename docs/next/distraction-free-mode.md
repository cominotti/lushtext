# Distraction-Free Writing Mode

## Status: Proposed

## Description
A single keybinding transitions the editor into a focused writing mode: fullscreen,
sidebar hidden, headerbar auto-hiding, text centered in a readable column width (~80
characters), with optional typewriter scrolling. Designed for prose writers and anyone
who needs sustained focus without visual clutter.

## Current State
- Sidebar toggle exists (`F9`, `win.toggle-sidebar` action) with smooth animation
- No fullscreen mode
- No header bar auto-hide
- No text centering or column width limiting
- Word wrap is supported and defaults to enabled

## Motivation
GNOME Text Editor lacks a distraction-free mode. Sublime Text and VS Code offer
half-hearted versions (no centering, no typewriter scroll). For a Libadwaita app, the
animated transitions and clean design language would feel premium. This is the feature
that makes non-programmers choose LushText for prose — writers, students, journalers.

## Implementation Plan

### Phase 1: Fullscreen + Hide Chrome
1. New `win.toggle-focus-mode` stateful boolean action
2. Keyboard shortcut: `Ctrl+Shift+F11` (avoid `F11` which is often system fullscreen)
3. On activate:
   - Call `window.fullscreen()`
   - Hide sidebar via existing `animate_sidebar(false)`
   - Set `AdwHeaderBar` to auto-reveal mode (`show-title` + overlay mode if Adwaita
     supports it, or use `GtkRevealer` with hover detection)
   - Hide `AdwTabBar`
   - Hide `LushtextStatusBar`
4. On deactivate: reverse all above, restoring previous sidebar/fullscreen state
5. Persist focus mode state? Probably not — always start in normal mode on launch.

### Phase 2: Centered Text Column
1. Add left and right margins to the `GtkSourceView` to center text:
   - `set_left_margin()` and `set_right_margin()` dynamically based on view width
   - Target line width: configurable, default 80 characters
   - Calculate margin: `(view_width - char_width * column_count) / 2`
2. Recalculate on `size_allocate` to stay centered during resize
3. Use `pango::FontDescription` to measure character width for the current font
4. Only apply centering in focus mode — normal mode uses standard margins

### Phase 3: Typewriter Scrolling (optional, toggleable)
1. Keep the cursor line vertically centered in the viewport at all times
2. Override the default scroll behavior: after each cursor movement, scroll the view
   so the cursor line is at 50% of the viewport height
3. Use `GtkSourceView::scroll_to_iter()` with appropriate margins
4. Toggle in preferences: "Typewriter scrolling in focus mode" (default: off)
5. Smooth scrolling via the existing animation infrastructure

### Phase 4: Visual Polish
1. Subtle vignette effect at screen edges using CSS gradient overlay (optional)
2. Slightly dimmed line numbers (or hide them entirely in focus mode)
3. Animated transitions for entering/leaving focus mode:
   - Sidebar slides out (existing animation)
   - Tab bar fades out
   - Header bar fades out
   - Margins animate from current to centered (use `AdwTimedAnimation`)
4. `Escape` exits focus mode (in addition to the keybinding)

## Architecture Considerations
- Header bar auto-hide is the trickiest part. `AdwHeaderBar` doesn't natively support
  overlay/auto-reveal. Options:
  - Wrap in `GtkRevealer` and show on mouse proximity to top edge (using
    `GtkEventControllerMotion`)
  - Use `GtkOverlay` to layer the header over content, showing on hover
  - Accept that the header is simply hidden and require the keybinding to exit
- The centered text approach using `GtkSourceView` margins is the cleanest and avoids
  wrapping the view in additional containers. The margins recalculate per-frame during
  resize via `size_allocate`, which is fast (just two property sets).
- Focus mode state should be per-window, not global. Multiple windows can independently
  enter/leave focus mode.
- The `Escape` shortcut conflicts with search bar close and command palette close. Focus
  mode `Escape` should only fire when no overlay is active. Use action enabled state to
  manage priority.

## Dependencies
- Existing sidebar toggle animation (`animate_sidebar`)
- Existing `AdwTimedAnimation` infrastructure
- `GtkSourceView` margin properties
- `pango::FontDescription` for character width measurement
- New GSettings keys: `focus-mode-column-width` (i, default 80),
  `focus-mode-typewriter` (b, default false)

## Risks
- Header bar auto-hide may feel janky if the hover detection zone is too small or the
  animation too slow. Extensive UX testing needed.
- `GtkSourceView` margin-based centering may interact poorly with word wrap — if the
  view width minus margins is less than the wrap width, text may wrap unexpectedly.
  Need to coordinate wrap mode with the effective column width.
- Typewriter scrolling can feel disorienting if the scroll animation is too aggressive.
  A gentle, interruptible animation is essential.
