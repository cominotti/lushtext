## Context

`ui/theme.rs::load_css` adds one `GtkCssProvider` for the whole display at
`STYLE_PROVIDER_PRIORITY_USER` and `apply_font_css` rewrites it on every change
of `zoom-level`, `custom-font`, `use-system-font` and the desktop
`monospace-font-name`, inside `restyle_everywhere` (C6's paced restyle, D9).
The rules are:

- `textview.tab-content-editor-surface` and its `text` node: the family, the
  zoomed size and `line-height: EDITOR_GLYPH_LINE_HEIGHT`, always;
- `.monospace`: the family and the zoomed size, except with the system font at
  100 %, where auxiliary monospace widgets keep GTK's defaults.

Widgets that carry `.monospace` today (directly, or through
`gtk_text_view_set_monospace`): search-result line numbers and line text
(`search_panel/list_factory.rs`), the file peek's path label and text view
(`sidebar/workspace_section/peek_execution.rs`), the native minimap
(`editor_page/minimap/watch.rs`), Markdown preview code blocks and image
fallback bodies (`markdown_preview/code_blocks.rs`, `images.rs`), the notes
browser's raw preview, the local-history preview, and the Line Inspector
(`line-inspector.blp`). The file-tree rows do **not**: `50cd2e91` moved them to
the default font, so C6's "Zoom step" note ("zoom and custom font also restyle
the `.monospace` file tree") was wrong about the tree, and this change corrects
the record.

The cost was attributed in C6 (M4b): over the four over-budget zoom rows, 32 %
of the dispatch is whole-tree CSS revalidation (`gtk_css_node_validate`) after
the provider reload, and most of the rest is Pango layout and paint of the
visible lines at the new size. The interaction-dispatch budget is
`B_INTERACT_US` (8 ms); later dispatches `B_SETTLE_US` (16 ms).

## Goals / Non-Goals

**Goals:**

- A zoom step revalidates the CSS of the widgets that follow the font and
  nothing else, with no deprecated API.
- Every surface that follows zoom and the custom font today keeps following
  them, at the same size and line height, including widgets created after the
  change (a new tab, a search row realized later, a preview rendered later).
- The zoom step's cost no longer grows with the number of unrelated widgets or
  windows.
- Measure the result in the lane, interleaved with `main`, and record any
  residual over `B_INTERACT_US` explicitly.

**Non-Goals:**

- Removing the re-layout of the visible paragraphs at the new size (C6's
  recorded toolkit floor; the paced reveal already bounds the rest).
- Making a font-family change cheaper than one whole-tree revalidation (D4).
- Making the file tree follow the editor font.
- Changing the zoom range, step, or the persisted keys.

## Measured baseline

A focused probe (kept for task 1.1 as the A86 probe; run with the
`gtk-lush-axioms` probe runner under private headless Mutter, GTK 4.22.5,
Libadwaita 1.9.3, Pango 1.57.1, GtkSourceView 5.20.0, GSK cairo, CPUs 8–23,
host 1-minute load 1.2–2.8 before and after every run, 2026-11-05). A
1280×800 window holds a `GtkSourceView` (word-char wrap, `monospace` off,
class `tab-content-editor-surface`) with 160 lines of 710 characters, R
unrelated rows (a `.toolbar` box with two labels, one `.dim-label`), and M
`.monospace` label rows in their own scroller. Each figure is the median of 9
longest frame-painting dispatches after the change, in µs; the zoom alternates
100 % and 110 % of 11 pt. Columns:

- *reload same*: `load_from_string` with unchanged text (pure revalidation);
- *reload zoom*: the current mechanism;
- *ancestor class*: the zoom rules keyed by a class on the view and on the
  `.monospace` rows' scroller (`.lz-N .monospace`);
- *per-widget class*: the class on the view and on each `.monospace` label
  (`.monospace.lz-N`);
- *window class*: the class on the window (`.lz-N .monospace`, `.lz-N textview…`).

| Text, R, M | redraw | reload same | reload zoom | ancestor class | per-widget class | window class |
|---|---|---|---|---|---|---|
| text, 0, 0 | 265 | 2,891 | 8,445 | 8,185 | — | 8,011 |
| text, 300, 200 | 2,047–2,178 | 11,684–13,703 | 14,414–18,721 | 15,386–16,169 | 13,883 | 18,640–19,640 |
| none, 300, 200 | 2,111–2,136 | 11,747–12,575 | 12,466–13,217 | 12,562–13,179 | 10,395 | 16,324–16,967 |
| none, 1,500, 0 | 4,767 | 23,573 | 23,281 | 6,440 | — | 24,749 |
| text, 1,500, 200 | 5,296 | 26,735 | 30,610 | 18,000 | 14,269 | 35,024 |

A class swap that changes no value costs 0.9–1.0 ms per widget set (the
per-widget form) and 7.8–8.9 ms on the ancestor (200 rows re-matched). Both
modes changed the view's Pango font size (15,019 ↔ 16,521 Pango units), and
the labels' with the per-widget form.

Reading:

1. A provider reload costs in proportion to every widget in the window
   (2.9 ms with nothing but the view, 23.6 ms with 1,500 unrelated rows, the
   font unchanged). A class on the window is the same thing.
2. A class swap re-matches the subtree it is set on: on a scroller of 200 rows
   it re-matches all 200 rows (Adwaita's descendant selectors give their nodes
   a parent-class dependency); on the labels themselves it re-matches only
   them.
3. What remains is the font-following widgets' own re-layout at the new size:
   the visible text (8.2–8.4 ms here) and the `.monospace` labels that change
   size (about 9 ms for 200 labels in one column box). That part is the cost
   of following the font and is unchanged by any mechanism.

The lane figures on `main` (C6, C3): zoom step 13–16 ms on the four
over-budget dense rows, 9.6 ms on dense/120 wrap-on; font change 11.5 ms
(1 row), 9.2–13.1 ms on dense/120, 11.0–15.4 ms at load 0.6–1.0. Removing the
32 % revalidation share predicts about 9–11 ms for the zoom step: within
`B_SETTLE_US`, still over `B_INTERACT_US`.

## Decisions

### D1. Display providers loaded per base font, with zoom-keyed rules

The font rules stay display-wide at `STYLE_PROVIDER_PRIORITY_USER` (in one of
the two providers of D4; the one holding the current font generation `g`).
They are loaded when the base font changes (custom font, `use-system-font`, the desktop
`monospace-font-name`) and at startup, with, for every zoom `z` in the
*zoom set* `{50, 60, …, 400} ∪ {z' ∈ [50, 400] : z' ≡ current (mod 10)}`
(the menu's grid, plus the lattice the current level steps along):

```css
textview.tab-content-editor-surface.lushtext-font-g-zoom-z,
textview.tab-content-editor-surface.lushtext-font-g-zoom-z text { font-family; font-size; line-height }
.monospace.lushtext-font-g-zoom-z { font-family; font-size }   /* omitted for z = 100 with the system font */
```

The rule text is a pure function of (generation, family, base points, zoom set,
system-font flag), unit-tested against the rules `apply_font_css` writes
today, so a class reproduces exactly today's computed font for that zoom. A
zoom level off the grid (written from outside the UI, for example 55) adds the
whole lattice it steps along (55, 65, …, 395) in one load (D4's two-step
form), because the zoom actions step by 10 from the current level: without
the lattice every later step from 55 would be off the grid and reload too. The
zoom set therefore holds at most 72 levels, and only a level whose residue
modulo 10 is new reloads.

*Rationale*: a class selects among rules already in the cascade, so a zoom step
needs no provider change. This is also GTK's own recommendation: the GTK 4 → 5
migration guide says local stylesheets are going away and that applications
should "use a global stylesheet (`gtk_style_context_add_provider_for_display`)
and rely on style classes to make your CSS apply only where desired". 36–72 rule pairs keyed by one class each are matched
through the selector tree's class buckets; task 3.4 checks that an unrelated
restyle (a hover change, a plain redraw) does not get slower.

*Alternatives*: a per-widget provider (`gtk_style_context_add_provider`,
deprecated since 4.10; rejected by C6 and by the no-deprecated-API rule); a
CSS custom property for the size (`var()`, GTK 4.16+), rejected because
changing it still needs a provider load; a tag-carried font (rejected by
ledger A72: the gutter takes its font from CSS and the empty last line has no
character to tag); Pango attributes on labels only (leaves the views, which
have no font property, on CSS anyway, and splits one contract in two).

### D2. Classes on each font-following widget, through one registry

`ui/font_zoom.rs` (cross-cutting coordination, no role; it serves the zoom
action, the font preference and every surface that renders monospace text)
owns the current class and a list of weak references. `follow_editor_font`
registers a widget: it adds the current class and the weak reference.
`apply_zoom` swaps the class on every live entry and drops dead ones; it runs
inside `restyle_everywhere`, so the paced reveal of dense editors is unchanged.
Every site in Context registers its widget at construction, the editor page
registers its `source_view`; `set_monospace(true)` and
`add_css_class("monospace")` outside `ui/font_zoom.rs` without a registration
fail a policy check (task 4.4).

The registry is bounded by the live widgets: list rows are recycled (at most
about 205 realized rows per list, ledger A1), and preview embeds are bounded by
the preview's own embed budget. Pruning runs on every swap and on registration
when the list has doubled since the last prune, so the list stays O(live).

*Rationale*: the probe's per-widget column is the cheapest (13.9 against
16.2 ms on the ancestor at R = 300, M = 200), and only it is independent of the
rows around the widget. A registry is needed because GTK has no lookup by CSS
class.

*Alternatives*: the class on each window (measured: as costly as the reload);
on container ancestors (search results list, peek popover; measured: re-matches
every row of the container, 7.8–8.9 ms for 200 rows with no value change).

### D3. Semantics are preserved exactly

At every (family, zoom, system-font) state, each registered widget computes
the same font family, size and (for the editor) line height as on `main`. A
widget registered after a zoom step gets the current class at once. A widget
that is not registered but carries `.monospace` (a toolkit-internal one) keeps
GTK's default, as with the system font at 100 % on `main`; outside that state
it would no longer follow the zoom, which the policy check of D2 prevents for
application widgets. Widget tests compare computed Pango font descriptions per
surface against `main`'s rules (task 4.3), and one test walks every widget of
every open window and dialog and fails on any widget that carries `.monospace`
or `GtkTextView:monospace` without being registered (task 4.4), so a
toolkit-internal one would be found rather than assumed absent (LushText's
`AdwAboutDialog` sets no debug information, the usual source of one).

The following consumers read the font through the widget, so they see the same
value at the same time as on `main` and need no change; each is still covered
by a test (task 4.3):

- **Text scale** (`gtk-xft-dpi`, the desktop's large-text setting): the rules
  are the same point sizes, so DPI scaling applies to them exactly as on
  `main`. **High contrast**: Adwaita's high-contrast sheet changes colours and
  borders, not fonts; it sits at theme priority, below the font provider.
- **Focus Mode readable-column margins** read
  `approximate_char_width` from the source view's Pango context
  (`ui/editor_page/focus_mode.rs`), which follows the view's computed font
  after its restyle, whichever mechanism caused it; they are recomputed on the
  same triggers as on `main`.
- **Minimap**: the `GtkSourceMap` carries `.monospace` and is registered like
  the other auxiliary widgets; its `.minimap-view` metrics (1.75 pt,
  `line-height: 5px`, application priority) meet the same USER-priority rule
  as on `main`.
- **Markdown preview code blocks and image fallbacks** register when built, so
  a preview rendered after a zoom step takes the current class.
- **Several windows**: the registry and the provider are per process and per
  display, the zoom key is global, so one step swaps the classes in every
  window, as today's display-wide reload restyles every window.

### D4. A font change: one whole-tree revalidation, in its own dispatch

A new family or base size needs new rules, and any provider load revalidates
every widget (A86, part 1). That cannot be avoided, but it can be separated
from the re-layout. The font module owns **two** display providers at
`STYLE_PROVIDER_PRIORITY_USER` and a font generation `g`; the classes are
`lushtext-font-<g>-zoom-<z>`. A base-font change (or a new zoom lattice, D1):

1. loads the rules of generation `g + 1` into the provider that does not hold
   generation `g`. No widget carries a `g + 1` class yet, so no computed value
   changes: this dispatch is the probe's *reload same* (pure revalidation, no
   text re-layout), 2.9 ms with only the view and growing with the window;
2. on the next frame (after its paint), inside `restyle_everywhere`, swaps
   every registered widget from its `g` class to its `g + 1` class: the
   re-layout of the font-following widgets only, as a zoom step.

The provider that held `g` keeps its now unused rules until the next font
change reloads it, because clearing it would be a second whole-tree
revalidation. On `main` both costs land in one dispatch (11.0–15.4 ms at low
load, 17.1 ms in C3's footnote ³). The whole-tree part is recorded as an
explicit exception against A86 with the lane figures of task 5.2 if step 1
exceeds `B_SETTLE_US` on any lane case; its owner is GTK's provider-change
contract. Zoom steps, the frequent interaction, pay only step 2.

### D5. Adoption criterion

The change is adopted only if, interleaved with `main` at host load below 4,
the zoom-step interaction dispatch on the dense restyle cases drops by at least
2 ms in median and no restyle, open or keystroke case regresses beyond one
reference frame; and the populated restyle case shows the zoom step within
1 ms of its control (the same window geometry, the sidebar shown over an
empty folder; D7). The font change is adopted in its two-step form if each of
its dispatches is at most `B_SETTLE_US` on the dense restyle cases, or, where
step 1 is not, if it is at most the single dispatch `main` takes; otherwise
the font change keeps one dispatch and that is recorded. Otherwise the change is abandoned and the
measurement recorded in the programme record. Residuals over `B_INTERACT_US`
are recorded per row against C6's text-re-layout floor.

### D6. GTK behaviour relied on is a ledger axiom

A86: after `gtk_css_provider_load_from_string` on a display provider, even with
unchanged text, the next frame revalidates every widget's CSS and its cost
grows with the number of widgets, while rules keyed by classes no widget
carries change no computed font (what D4's first step relies on); after a class swap on a set of widgets whose
rules are keyed by that class, the widgets compute the new font, and the next
frame's cost does not grow with unrelated widgets. Designs that change the
editor font or zoom cite A86. If a GTK update makes a class swap re-match the
whole window, D2's benefit is gone and this design is revisited before the
probe is changed.

### D7. The populated case isolates unrelated widgets from font-following ones

Showing or hiding the sidebar changes the editor's width, and so its wrapped
re-layout; listing search results adds widgets that **do** follow the font
(about 9 ms for 200 labels in the probe). Neither isolates what this change
removes. The populated restyle case therefore runs with the sidebar shown in
both arms, over a folder of 300 entries against an empty folder, the search
panel closed: same geometry, only unrelated widgets differ. A second variant
lists search results and records the labels' own re-layout as the cost of
following the font, against `B_SETTLE_US` and with a recorded residual over
`B_INTERACT_US`; it is not part of the 1 ms criterion.

## Risks / Trade-offs

- **A missed registration** leaves a surface at the old size after a zoom
  step → the policy check of D2 and a widget test per surface (task 4.3).
- **More rules in the cascade** could slow every restyle slightly → measured in
  task 3.4 (hover restyle and redraw before and after); if it costs more than
  noise, the grid shrinks to the menu's levels within ±5 steps of the current
  one and reloads when the user leaves that window (a recorded fallback).
- **The labels' own re-layout** (about 9 ms for 200 `.monospace` labels in the
  probe) remains with a long search-result list visible; it is the cost of
  following the font, recorded with the populated lane case.
- **Hidden or unrealized widgets** take the class without a restyle until they
  are shown (A47), so a later reveal pays their own restyle once, as on
  `main`.

## Migration Plan

No persisted state changes. Rollback is reverting the change; the provider
returns to rewriting its rules.

## Open Questions

None. The adoption criterion (D5) decides whether the change ships.
