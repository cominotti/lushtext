## Context

LushText currently renders all tab content inside the same `AdwApplicationWindow` surface as the rest of the shell. The active editor body is a `GtkSourceView` inside `ui/editor_page/`, the Markdown preview is a read-only `GtkTextView` inside `ui/markdown_preview/`, and the surrounding chrome is built from `AdwHeaderBar`, `AdwTabBar`, split-view sidebars, search-panel chrome, and the status bar in `resources/ui/window.ui`.

That split matters because the requested feature is not "fade a widget." GTK child widgets share the toplevel surface, and `gtk_widget_set_opacity()` would make text, cursors, minimap content, infobars, and other child content translucent along with the background. Earlier LushText experiments already showed the other common failure mode: semi-transparent overlay containers let underlying content bleed through, which later became a repo rule that overlay widgets must keep opaque backgrounds.

The design therefore needs to do two things at once:

- make only the tab document surfaces reveal desktop/background content;
- explicitly keep shell chrome and non-document helpers opaque.

This crosses preferences, GSettings, window-level styling, editor-page rendering, Markdown preview rendering, bundled GtkSourceView styling, and widget/live-runtime verification.

## Goals / Non-Goals

**Goals:**
- Add an always-visible `Transparency` control in `Preferences > Editor > Appearance`.
- Mirror the Fedora Ptyxis interaction model closely: row title, live percentage readout, and a popover-hosted slider.
- Apply the selected opacity to the main editor document background and Markdown preview background.
- Keep the minimap, header and tab chrome, side panels, status/search chrome, and other non-document surfaces opaque.
- Keep the result GTK-native and compatible with LushText's existing color-scheme and dark-mode flows.

**Non-Goals:**
- Whole-window opacity where all child content fades together.
- Blur, frosted-glass, or compositor-specific effects.
- Per-tab or per-workspace transparency values.
- Making the minimap translucent.
- Hiding the control until the user changes it, as Ptyxis currently does.

## Decisions

### 1. Add an always-visible Ptyxis-shaped preference row backed by a new opacity setting

LushText will add a new `double` GSettings key, `tab-content-opacity`, with default `1.0` and range `0..1`. The control will live in `Preferences > Editor > Appearance` as an `AdwActionRow` titled `Transparency`, with a suffix percentage label and a `GtkMenuButton` that opens a `GtkPopover` containing a `GtkScale`.

The interaction contract mirrors modern Fedora Ptyxis closely:

- live percentage readout;
- slider range `0..1`;
- step increment `0.05`;
- page increment `0.25`;
- marks at `0.25`, `0.5`, and `0.75`.

Why:
- The user explicitly asked for the Fedora terminal control shape rather than a generic slider row.
- The control belongs with the other editor appearance settings, not in window chrome.
- Keeping it always visible matches the user's chosen contract and avoids discoverability problems.

Alternatives considered:
- Hide the row until the value differs from `1.0`: rejected because the user explicitly chose always-visible behavior.
- Use an inline `AdwSpinRow` or naked `GtkScale`: rejected because it does not match the investigated Fedora terminal control pattern.

### 2. Use a window-level alpha background with explicit opaque chrome, not widget opacity

The window shell will gain a dedicated appearance path for tab-content transparency that adjusts the toplevel background color with alpha while explicitly painting opaque backgrounds on the shell surfaces that must remain solid.

Why:
- Desktop-visible transparency requires alpha at the toplevel painting layer; simply making editor widgets transparent would only reveal the parent's own opaque background.
- `gtk_widget_set_opacity()` is the wrong abstraction because it fades the entire widget subtree, including text and controls.
- This follows the same underlying GTK logic as terminal-style transparency while still allowing LushText to keep specific chrome opaque.

Alternatives considered:
- Call `set_opacity()` on `GtkSourceView`, `GtkTextView`, or editor-page containers: rejected because it would dim foreground content and child widgets.
- Only make GtkSourceView or GtkTextView backgrounds transparent: rejected because it would reveal the existing opaque window background instead of creating real terminal-style transparency.

### 3. Define explicit document-surface boundaries and paint everything else opaque

The feature will treat the following as document surfaces:

- the main editor text surface, including its gutter and current-line background;
- the Markdown preview text surface.

The following remain opaque:

- header bar;
- tab bar and tab-strip chrome;
- workspace sidebar;
- properties panel;
- status bar and search-panel chrome;
- editor infobars and find/replace chrome;
- minimap shell and minimap content.

Why:
- The user scoped the effect to tab backgrounds only and explicitly excluded top, side, bottom, and minimap surfaces.
- LushText already has a history of background contamination bugs when intermediate containers are left semi-transparent.
- An explicit boundary list makes the implementation and tests much less ambiguous.

Alternatives considered:
- Apply transparency to the entire center column or all `window-contents`: rejected because it would leak into helper chrome and contradict the accepted scope.
- Let editor-local helper surfaces inherit document transparency: rejected because the user asked for everything except document backgrounds to stay opaque.

### 4. Make editor transparency derive from the active GtkSourceView style scheme instead of a hardcoded color

The editor path will introduce a small appearance adapter that recomputes background colors whenever any of these change:

- `tab-content-opacity`;
- selected GtkSourceView style scheme;
- dark or light appearance.

That adapter will derive alpha-adjusted document backgrounds from the active style scheme instead of using a fixed neutral color. The implementation should continue to respect the chosen syntax theme while adjusting the background-bearing surfaces that define the editor body.

Why:
- LushText already supports multiple GtkSourceView schemes, not just the bundled Adwaita pair.
- Hardcoding one translucent background would drift away from the selected editor theme and look broken when the user switches schemes.
- GtkSourceView appearance already flows through style-scheme colors such as `text`, `line-numbers`, and `current-line`, so this is the natural place to keep the feature theme-aware.

Alternatives considered:
- Support only the bundled Adwaita schemes: rejected because it would turn transparency into a hidden theme compatibility trap.
- Add static transparency variants for every possible scheme: rejected because it would create an unbounded asset-maintenance problem.

### 5. Give Markdown preview the same opacity policy, but keep the minimap on its own opaque styling path

The Markdown preview's internal `GtkTextView` background will follow the same `tab-content-opacity` value as the editor so switching between source and rendered Markdown keeps a consistent document-surface feel. The minimap will not share that behavior; it will stay on an opaque rendering path even though its current CSS already uses transparent pieces.

Why:
- The user explicitly chose "Markdown preview follows the same transparency: yes" and "Minimap: no."
- The minimap is a compact navigation aid, not the primary reading surface.
- The current minimap styling intentionally uses transparent pieces for its own look, so leaving it on the transparent path would create desktop bleed-through immediately once the window background becomes alpha-enabled.

Alternatives considered:
- Apply the same transparency to the minimap for visual symmetry: rejected because it conflicts with the accepted scope and risks making the minimap harder to read.
- Keep Markdown preview opaque while only the editor changes: rejected because it would make preview mode feel like a different feature rather than the same document-surface preference.

## Risks / Trade-offs

- [Risk] Some GtkSourceView schemes may not provide all background colors cleanly enough for alpha derivation. -> Mitigation: define a fallback chain that uses the active scheme first and falls back to stable Adwaita/view colors when a needed background is absent.
- [Risk] Once the window background becomes alpha-enabled, any forgotten intermediate container can accidentally reveal the desktop. -> Mitigation: audit shell and editor helper surfaces and give every non-document region an explicit opaque background or `.background` styling path.
- [Risk] The minimap currently uses transparent CSS and could unintentionally join the new transparency path. -> Mitigation: give the minimap shell and minimap view their own explicit opaque styling contract as part of the same change.
- [Risk] Different windowing-system/compositor combinations can show visual quirks with alpha surfaces. -> Mitigation: keep the default at `1.0`, verify behavior in live runs, and treat any compositor-specific artifact as a reason to tighten the opaque surface list rather than broaden transparency.
- [Risk] The feature touches preferences, window styling, editor rendering, and preview rendering at once. -> Mitigation: keep one shared preference key and one window-level appearance update flow so the behavior is centralized instead of reimplemented per widget.

## Migration Plan

1. Add the new `tab-content-opacity` GSettings key with default `1.0`.
2. Add the always-visible preferences row and wire it to the new key.
3. Introduce the window-level transparency appearance path plus explicit opaque chrome styling.
4. Hook editor-page and Markdown preview updates into the same appearance inputs: opacity, style scheme, and dark-mode changes.
5. Add widget coverage for preferences visibility/persistence boundaries and manual live verification for alpha-surface behavior.

Rollback is straightforward: leave the key in place at `1.0`, stop applying the appearance path, and the app returns to fully opaque rendering without data migration.

## Open Questions

None currently. The user already resolved the main scope decisions that would otherwise block this design.
