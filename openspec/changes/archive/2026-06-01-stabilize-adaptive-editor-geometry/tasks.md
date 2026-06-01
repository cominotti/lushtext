## 1. Adaptive Shell State

- [x] 1.1 Add a pure adaptive-shell geometry helper that derives workspace presentation, document-properties presentation, compact active surface, and split-width budgets from stable inputs.
- [x] 1.2 Update document-properties guard math to use requested workspace layout intent and the selected preset's effective clamped width instead of temporary `shows_sidebar()` state.
- [x] 1.3 Align shell width constants with the actual rendered properties-panel and workspace-sidebar minimums.
- [x] 1.4 Refactor secondary-surface synchronization into compute and apply phases, with no-op application when widget state already matches the derived result.
- [x] 1.5 Add widget tests for both requested surfaces across the no-workspace guard and Small, Comfy, and Large workspace-aware guard boundaries.
- [x] 1.6 Add a stability test that counts layout, split-view, and bottom-sheet state notifications after settling and fails on repeated oscillation without a new input.

## 2. Short-Height Chrome

- [x] 2.1 Define and apply the normal-mode minimum height budget for header chrome, tab strip, status bar, and a minimal editor viewport.
- [x] 2.2 Update search-panel result height clamping to use available content height and yield before clipping the status bar.
- [x] 2.3 Ensure workspace sidebar, document properties, minimap, and editor overlays scroll, compact, or truncate inside the remaining content area at short heights.
- [x] 2.4 Add widget tests that assert the status bar has nonzero allocation at the normal-mode minimum height with optional surfaces open.
- [x] 2.5 Add or update headless Mutter capture coverage for the short-height reproducer and assert raw app stderr is free of GTK/Libadwaita allocation warnings.

## 3. Narrow Workspace And Editor Left Edge

- [x] 3.1 Distinguish passive workspace split-view collapse from an explicit compact-sidebar open request.
- [x] 3.2 Suppress unintended compact workspace overlays after passive shrink so the editor gutter and line starts remain visible.
- [x] 3.3 Clamp editor horizontal adjustment back to the left edge after passive layout changes when there is no explicit horizontal-scroll intent.
- [x] 3.4 Add widget or runtime tests for the good-to-bad narrow-width transition with the workspace sidebar requested open.
- [x] 3.5 Add long-line coverage proving passive narrowing does not preserve stale rightward scroll.

## 4. Minimap Viewport Projection

- [x] 4.1 Add diagnostic coverage or focused tests that record editor visible buffer range, editor allocation, source-map allocation, wrap modes, and scroll adjustments across sidebar show/hide.
- [x] 4.2 Choose and implement the minimap wrapping policy: either align `GtkSourceMap` wrapping with the editor where viable or project a custom viewport overlay from the editor's visible buffer range.
- [x] 4.3 Refresh minimap viewport and semantic-marker geometry after width-only editor allocations, wrap-driven reflow, and end-of-file overscroll recalculation.
- [x] 4.4 Add minimap regression tests with word wrap enabled at intermediate widths where showing the workspace sidebar narrows the editor.
- [x] 4.5 Add word-wrap-disabled minimap control coverage so viewport alignment is not accidentally tied to the wrapped-line case only.

## 5. Validation

- [x] 5.1 Run the relevant widget-test filters for window geometry, search panel height, editor horizontal adjustment, and minimap viewport behavior under the headless harness.
- [x] 5.2 Run headless Mutter captures for the medium-width flicker band, the short-height reproducer, and the narrow-width workspace collapse reproducer.
- [x] 5.3 Confirm captures and raw stderr show no repeated adaptive-state flips, status-bar clipping, unintended workspace overlay, minimap viewport drift, or GTK/Libadwaita allocation warnings.
- [x] 5.4 Run `openspec validate stabilize-adaptive-editor-geometry --strict`.
- [x] 5.5 Run the repo's required formatting and test checks for the touched Rust, UI-template, and CSS files.
