## Why

A zoom step costs 13–16 ms on the dense lane documents and a font change
11.5–17.1 ms (C6's record, "Zoom step (13–16 ms; M4b)", and C3's footnote ³),
against `B_INTERACT_US` (8 ms) for the interaction dispatch. Both run through
one display-wide `GtkCssProvider` in `ui/theme.rs`, which `apply_font_css`
reloads with `load_from_string` on every `zoom-level`, `custom-font`,
`use-system-font` and desktop `monospace-font-name` change. A reload of a
display provider revalidates the CSS of every widget of every window: `perf`
put 32 % of the zoom dispatch in `gtk_css_node_validate`, and the cost grows
with the number of widgets the user has open, not with the widgets the font
reaches.

C6 evaluated a provider scoped to the editor and rejected it, because GTK 4
only offers per-widget providers through the deprecated `GtkStyleContext`.
That leaves a path C6 did not measure: **keep the one display provider, load
it once per base font with a rule set per zoom level keyed by a CSS class, and
make a zoom step a class swap on the widgets that follow the font.** A class
change re-matches only the widget it is set on and the descendants whose
selectors depend on it; nothing deprecated is involved. A focused probe
(design, "Measured baseline") shows the difference: with 1,500 unrelated rows
in the window, a reload-driven zoom frame costs 30.6 ms and a per-widget class
swap 14.3 ms; without the unrelated rows both cost what the text re-layout
costs (8.2–8.4 ms). The text re-layout at the new size stays: it is the
toolkit floor C6 already recorded.

## What Changes

- **Zoom classes on one display provider** (design D1). The font provider is
  loaded once per base font (family and size: custom or desktop) with rules
  for every zoom the menu reaches (50–400 % in steps of 10) and the current
  level, each keyed by a class `lushtext-zoom-<percent>`; the editor-surface
  rule keeps its line-height guard. Only a base-font change, or a zoom level
  off the grid set from outside the UI, reloads it.
- **A zoom step swaps classes, not rules** (D2). A small cross-cutting
  registry (`ui/font_zoom.rs`) holds every widget that follows the font: each
  editor surface view and every `.monospace` widget the application creates
  (search-result labels, file peek, minimap, Markdown preview code blocks and
  image fallbacks, notes and local-history previews, Line Inspector). A zoom
  step swaps each live widget's class inside the existing paced restyle
  (`restyle_everywhere`, D9 of C6), so dense documents keep their
  visible-first reveal. Widgets register at construction; the registry holds
  them weakly and prunes the dead.
- **What follows the font does not change** (D3). Every surface that follows
  zoom and the custom font today still does, at the same size; at 100 % with
  the system font, auxiliary `.monospace` widgets keep GTK's defaults as
  today. The file tree does not follow the font today (`50cd2e91` gave it the
  default font on purpose); C6's record said it did, and is corrected.
- **The font change keeps one whole-tree revalidation** (D4): a new family or
  base size needs new rules, and any provider load revalidates every widget.
  It is a deliberate preference action, recorded as an explicit exception
  against A86 with its measured figure.
- **Ledger A86** with a `gtk-lush-axioms` probe and sample: a display
  provider reload revalidates every widget and grows with unrelated widgets; a
  class swap on the font-following widgets applies the new font size and does
  not grow with unrelated widgets.
- **Lane**: the dense restyle cases interleaved with `main`, and a populated
  restyle case (sidebar with 300 entries shown, search results listed) that
  shows the zoom cost no longer grows with the window's unrelated widgets.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: a zoom step restyles only the widgets that
  follow the editor font; a font change is the one recorded whole-tree
  revalidation.
- `editor-glyph-rendering`: the font and zoom reach every font-following
  surface through the zoom classes, including widgets created after the
  change.
- `gtk-axiom-ledger`: A86.
- `responsiveness-measurement`: the populated restyle case.

## Impact

- `ui/theme.rs` (provider rules, base-font reload), `ui/font_zoom.rs` (new,
  cross-cutting, no role), the `.monospace` construction sites listed above,
  `ui/editor_page` (editor surface registration), the A86 probe and sample,
  widget tests, the lane driver, the programme record, the ledger, `ui.md`,
  AGENTS.md and the README.
- User-visible: zooming no longer slows down as more windows, tabs, panels or
  rows are open; on the dense lane documents the zoom step is expected to drop
  by the CSS-revalidation share (about a third) and stay within `B_SETTLE_US`.
  Fonts, sizes and line heights are unchanged.
