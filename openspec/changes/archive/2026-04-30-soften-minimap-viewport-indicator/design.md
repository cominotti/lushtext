## Context

The editor minimap is implemented with `GtkSourceMap` inside each `LushtextEditorPage`. LushText adds a narrow custom drawing layer for semantic markers, while the visible-document viewport indicator remains the native `GtkSourceMap` slider. Current CSS styles that slider with `@accent_bg_color` and `@accent_color`, which makes the indicator appear as a bright blue accent block in the dark editor surface.

The existing minimap spec already requires a visible viewport overlay. This change refines that requirement: the overlay must remain readable, but it should not look like an active selection, button, or warning.

## Goals / Non-Goals

**Goals:**

- Make the minimap viewport indicator visually quieter in both light and dark themes.
- Keep the indicator distinct enough to locate the current editor viewport at a glance.
- Preserve existing minimap geometry, click, drag, scroll, and EOF overscroll behavior.
- Preserve existing semantic marker colors and marker strip behavior.
- Add regression coverage that guards against reintroducing accent-colored viewport styling.

**Non-Goals:**

- Replacing `GtkSourceMap` with a custom minimap widget.
- Changing minimap preference storage, width settings, keyboard shortcuts, or availability rules.
- Changing bookmark, search, modified-line, or long-line marker colors.
- Adding user-facing color customization for the minimap.

## Decisions

1. Keep the native `GtkSourceMap` slider and adjust only its visual style.

   The slider already tracks the editor viewport and owns pointer navigation. A CSS-only treatment keeps those toolkit contracts intact and avoids duplicating scroll math that is already covered by `minimap-navigation-parity`.

   Alternative considered: replace the viewport indicator with a custom overlay drawing area. That would make colors easy to control, but it would also create another geometry layer that must stay synchronized with `GtkSourceMap`.

2. Use neutral theme colors rather than accent colors.

   The viewport indicator should read as editor chrome, not as a selected control. Neutral tokens such as foreground/border-derived alpha colors keep the overlay visible while allowing semantic marker colors to remain the attention-grabbing signals.

   Alternative considered: reduce the alpha on the existing accent colors. This would still make the indicator follow user accent choices and could remain overly saturated on dark themes.

3. Preserve the current minimap width, padding, border radius, and slider expansion geometry.

   The screenshot issue is color dominance, not layout. Keeping geometry stable reduces the chance of regressing hit targets, full-document overlay behavior, and EOF navigation parity.

   Alternative considered: shrinking the slider or removing the border. That could make the overlay too subtle when the file is long or when the editor surface is transparent.

4. Test the styling contract directly.

   The regression should assert that minimap slider styling no longer references `@accent_color` or `@accent_bg_color`, and should keep existing widget tests for native `GtkSourceMap` geometry/navigation unchanged.

## Risks / Trade-offs

- [Risk] The neutral overlay may become too subtle on one theme variant. → Mitigation: choose theme-derived colors with enough alpha contrast and verify with a live GTK screenshot in dark mode.
- [Risk] CSS selector changes may accidentally affect semantic marker visibility. → Mitigation: keep marker strip drawing unchanged and test marker counts/visibility behavior separately from slider CSS.
- [Risk] Removing accent styling could make the viewport indicator less discoverable for first-time users. → Mitigation: keep a border and translucent fill instead of making the slider purely transparent.
