## 1. Settings and editor-page scaffolding

- [x] 1.1 Add `show-minimap` and `minimap-width` GSettings keys, matching config constants, and the window action or shortcut wiring needed to persist the minimap visibility preference.
- [x] 1.2 Extend `resources/ui/editor-page.ui` and `ui/editor_page/imp.rs` with a dedicated right-side minimap container and explicit state handles without regressing the existing search revealer layout.
- [x] 1.3 Create a new `ui/editor_page/minimap.rs` workflow module and hook it into `mod.rs` so editor pages can materialize or suppress minimap instances based on preference, viewport value, file-size policy, and eviction state.

## 2. Core minimap behavior

- [x] 2.1 Attach a read-only `sourceview5::Map` to the active editor source view and keep its overview synchronized with buffer and viewport changes.
- [x] 2.2 Implement minimap click and drag navigation so interaction scrolls the main editor to the corresponding document region while preserving editor-focus behavior.
- [x] 2.3 Suppress the per-tab minimap when the document fully fits in the viewport or exceeds the minimap-supported file-size tier, and surface user feedback when the current document cannot show a minimap.

## 3. Semantic marker projections

- [x] 3.1 Add editor-page change-tracking state for modified-since-save line ranges and clear or rebuild it in the existing load, save, discard, reload, and dispose flows.
- [x] 3.2 Build the semantic-marker rendering helper that merges bookmarks, active in-tab search matches, modified ranges, and long-line warnings into normalized minimap markers.
- [x] 3.3 Wire debounced marker refreshes from bookmark updates, in-tab search attach or detach and query changes, buffer edits, and successful save or load transitions.

## 4. Verification and polish

- [x] 4.1 Add focused tests for minimap preference restoration, availability gating, and the editor-page layout contract around the new minimap container.
- [x] 4.2 Add widget or helper coverage for minimap navigation mapping, semantic marker projection, modified-marker reset after save, and marker removal after search close or bookmark deletion.
- [x] 4.3 Run the relevant Rust and GTK test targets and update nearby comments or docs needed to explain the minimap workflow and guardrails.

## 5. Contract alignment follow-up

- [x] 5.1 Keep the minimap visible on supported tabs whenever the minimap preference is enabled, even when the full document fits inside the editor viewport.
- [x] 5.2 Add an explicit semi-transparent rectangular viewport overlay that clearly tracks the visible document region and remains obvious above the minimap content.
- [x] 5.3 Update minimap tests to cover the always-visible-on-supported-tabs rule and the viewport-overlay presence or synchronization contract, then rerun the relevant Rust and GTK suites.

## 6. Native viewport indicator correction

- [x] 6.1 Replace the custom-drawn viewport rectangle with the native `GtkSourceMap` viewport indicator and style that indicator so it remains clearly visible.
- [x] 6.2 Remove or simplify any minimap code and tests that depend on LushText owning viewport-geometry math that `GtkSourceMap` should own instead.
- [x] 6.3 Rerun the relevant Rust and GTK suites after the viewport-indicator correction.

## 7. Scheme-aware viewport visibility fix

- [x] 7.1 Bundle the official default GtkSourceView `Adwaita` and `Adwaita-dark` schemes in app resources and register that search path during startup so `map-overlay` is reliably available.
- [x] 7.2 Align the minimap shell with GNOME Text Editor's `GtkSourceMap` geometry and selector path so the native slider styling applies to the actual map widget.
- [x] 7.3 Add or update focused tests around scheme registration or minimap widget configuration, then rerun the relevant Rust and GTK suites.
