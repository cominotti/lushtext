## Context

LushText's minimap is a `GtkSourceMap` bound to the active editor page's `GtkSourceView`, with a narrow custom `DrawingArea` layered at the map edge for semantic markers. The editor also has dynamic EOF overscroll: after allocation, the source view bottom margin grows from the visible editor height so the final lines can travel upward and the native map keeps useful navigation room near the end of the file.

The current marker strip does not use the source map's rendered geometry. It collects line numbers for bookmarks, active in-tab search matches, modified-since-save marks, and optional long-line warnings, then paints each marker with `line / total_lines * strip_height`. That ignores map margins, layout scaling, and the EOF overscroll tail. The result is visible drift: search markers can be painted in the blank tail below the rendered document content.

## Goals / Non-Goals

**Goals:**

- Make every semantic minimap marker category use the same vertical geometry as the bound `GtkSourceMap`.
- Keep markers out of blank EOF overscroll tail space unless a real rendered line occupies that position.
- Preserve the existing marker model, colors, lane widths, minimap availability policy, and native `GtkSourceMap` click or drag behavior.
- Add extensive regression coverage across pure projection behavior, real GTK widget allocation, search lifecycle, all marker categories, EOF overscroll, resize behavior, and headless CI execution.
- Keep the fix tab-local in `ui/editor_page/minimap.rs`.

**Non-Goals:**

- Replacing `GtkSourceMap` with a custom minimap renderer.
- Changing workspace-wide search panel behavior or adding workspace-search minimap markers.
- Changing the minimap preference surface, marker colors, marker lanes, or file-size availability policy.
- Adding a new standalone E2E framework unless the existing widget harness cannot prove a required contract.

## Decisions

### 1. Project marker rectangles through the source map layout

Marker collection should remain semantic and line-oriented, but drawing should resolve each marker's line range through the bound `sourceview5::Map` before painting. The projection helper should use GTK text-view geometry from the map itself, such as iter locations plus coordinate conversion and widget bounds, so the marker strip follows the same layout scale, top margin, bottom margin, and EOF tail as the visible map.

Alternatives considered:

- Keep `line / total_lines` and subtract an estimated bottom margin. This is too fragile because it reintroduces guessed geometry and does not account for map layout details.
- Draw markers directly inside the `GtkSourceMap`. That would reduce coordinate translation, but it would mix app-specific semantic lanes into the native map widget and make category-specific drawing harder to test.
- Rebuild the whole minimap as a custom renderer. That is disproportionate for a marker projection bug and would discard native map navigation behavior that already works.

### 2. Keep the marker data model simple and move geometry to render time

The existing `MinimapMarker` model can continue to store `kind`, `start_line`, and `end_line`. Geometry should be calculated at draw time against the current map and marker-strip allocation, because resize, font, theme, wrapping, and overscroll changes can alter where the same line appears without changing marker semantics.

Alternatives considered:

- Store pixel rectangles in marker state during `refresh_minimap()`. That risks stale rectangles after allocation or source-map layout changes.
- Store source marks for every category. Search and long-line markers are transient projections, and the custom strip still needs lane-specific rendering.

### 3. Treat unprojectable or collapsed geometry defensively

If a marker's line range cannot be resolved through the current map layout, drawing should skip that marker rather than falling back to full-height line math. For very small projected line heights, the existing minimum visual marker height can remain, but it should expand around the projected line position without crossing below the map's rendered document region.

Alternatives considered:

- Use the old proportional projection as a fallback. That would keep the exact bug alive in rare states.
- Panic or log loudly when geometry is unavailable. GTK allocation and map realization can legitimately be transient during construction or disposal.

### 4. Make testing broad but harness-native

Testing should use several layers:

- Pure unit tests for projection helper behavior with synthetic map and strip geometry where possible.
- Existing minimap marker normalization tests to keep semantic marker grouping stable.
- Widget tests in `crates/lushtext/tests/widget/editor_page.rs` or adjacent files that present a real editor page, enable the minimap, create documents with EOF overscroll, and assert marker bounds do not enter the blank tail.
- Window-level widget tests where needed for real in-tab search, bookmarks, save/modified lifecycle, long-line preference toggling, and resize behavior.
- Headless widget runs through the existing script or make target so CI exercises the same compositor path.
- Live visual verification or screenshot inspection only as a final confidence check, not as a new default test framework.

Alternatives considered:

- Only adding a unit test for the math. That would miss the real GTK allocation and `GtkSourceMap` geometry that caused the regression.
- Only adding screenshot tests. Pixel assertions are brittle in GTK themes and are not the current repo's preferred first line of defense.

## Risks / Trade-offs

- [Risk] Source-map text geometry may be temporarily unavailable during construction, disposal, or before allocation. Mitigation: skip unprojectable markers for that draw and refresh on the existing debounced signals.
- [Risk] Mapping between the source map and marker strip can be off by a few pixels due to CSS margins or overlay allocation. Mitigation: use widget bounds and allocation-aware conversion rather than assuming equal heights.
- [Risk] Very dense search results can still produce a visually busy marker strip. Mitigation: preserve the existing match cap and run normalization behavior.
- [Risk] Widget tests that assert exact pixels can become flaky across GTK versions or themes. Mitigation: assert stable geometry relationships, such as "marker bottom is above the rendered final-line/tail boundary", with tolerance where needed.
- [Risk] Broad minimap tests can slow the widget suite. Mitigation: keep documents large enough to prove geometry, but not enormous, and use `wait_until` predicates instead of sleeps.

## Migration Plan

No user data migration is required. The change only alters runtime drawing and tests. Rollback is limited to restoring the previous marker-strip projection logic, though that would reintroduce the visual regression.

## Open Questions

None blocking. During implementation, confirm the exact GTK coordinate conversion path with the current `sourceview5` bindings before finalizing the projection helper.
