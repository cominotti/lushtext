## 1. Geometry Investigation

- [x] 1.1 Confirm the exact `sourceview5::Map` coordinate conversion path for mapping buffer line ranges into marker-strip coordinates.
- [x] 1.2 Identify the rendered document-content bottom boundary when dynamic EOF overscroll is active.
- [x] 1.3 Decide the smallest stable test-facing API or helper needed to inspect projected marker rectangles without relying only on screenshots.

## 2. Marker Projection Implementation

- [x] 2.1 Add a projection helper that resolves `MinimapMarker` line ranges through the bound source-map layout instead of raw `line / total_lines` math.
- [x] 2.2 Update marker-strip drawing to use projected rectangles while preserving marker lanes, colors, minimum visible height, and density normalization.
- [x] 2.3 Handle unallocated or unprojectable source-map geometry by skipping affected markers for that draw instead of falling back to proportional full-height placement.
- [x] 2.4 Ensure marker projection refreshes after search changes, buffer edits, minimap width changes, word-wrap changes, dynamic overscroll updates, and editor reallocation.

## 3. Pure and Focused Tests

- [x] 3.1 Add pure tests for projection helper behavior with synthetic geometry, including top margin, bottom EOF tail, collapsed line heights, and clamping.
- [x] 3.2 Keep or extend normalization tests so adjacent semantic lines still merge and duplicate lines still deduplicate before projection.
- [x] 3.3 Add focused tests proving unprojectable geometry does not use the old full-height proportional fallback.

## 4. GTK Widget Regression Tests

- [x] 4.1 Add a presented editor-page widget test where active search markers stay above the blank EOF overscroll tail.
- [x] 4.2 Add widget coverage that bookmark, search-match, modified-since-save, and enabled long-line markers all use the same source-map projection boundary.
- [x] 4.3 Add widget coverage that closing or clearing search removes search markers without leaving stale projected rectangles.
- [x] 4.4 Add widget coverage that saving clears modified markers and later edits project new modified markers correctly.
- [x] 4.5 Add widget coverage that toggling long-line markers preserves other marker categories and keeps long-line markers aligned.
- [x] 4.6 Add resize or reallocation widget coverage showing marker positions refresh when the minimap/source-map layout changes.
- [x] 4.7 Add a regression assertion that the bottom-most real document marker does not paint into the source-map EOF tail, with tolerance for GTK theme/layout differences.

## 5. Visual and Harness Verification

- [x] 5.1 Run the focused minimap/editor-page widget tests locally.
- [x] 5.2 Run the full widget suite through the existing widget harness.
- [x] 5.3 Run the headless widget suite through `make test-widget-headless` or `scripts/run-widget-tests.sh --headless`.
- [x] 5.4 Run relevant unit tests and the broad project test target required by the repository workflow.
- [x] 5.5 Perform live visual verification with the screenshot scenario from the report: search results visible, minimap enabled, dynamic EOF overscroll present, and no orange markers extending beyond rendered document content.
- [x] 5.6 Record any remaining visual limitation in the change notes before implementation is marked complete.

## 6. Development Tooling

- [x] 6.1 Add an idempotent Make target that includes `flatpak-deps` and installs live GTK debugging helpers.
- [x] 6.2 Update GTK debugging skill guidance so live interaction and screenshots require a liveness/tool check.
- [x] 6.3 Ensure the GTK debugging workflow can reject pre-existing LushText instances when the debug session is expected to own the target.

## 7. OpenSpec Validation

- [x] 7.1 Run `openspec validate fix-minimap-marker-projection --strict`.
- [x] 7.2 Run any stricter repository OpenSpec validation targets needed before archive or handoff.
