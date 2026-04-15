## 1. Preferences surface and sidebar cleanup

- [x] 1.1 Add a workspace sidebar width `AdwComboRow` under `Preferences > Workspace` and wire it to the existing preset state with immediate apply behavior.
- [x] 1.2 Remove the sidebar footer `Small` or `Comfy` or `Large` controls from `resources/ui/sidebar.ui` and retire any sidebar-local wiring that existed only to drive those footer buttons.

## 2. Adaptive preset width policy

- [x] 2.1 Extend `WorkspaceSidebarWidthPreset` with preset-specific hint fractions and clamped `sp` bounds while preserving nearest-preset snapping for stored values.
- [x] 2.2 Update the workspace split-view helpers in `ui/window/imp.rs` so the visible left pane locks to the clamped target width and derives its effective fraction from that width instead of the raw preset hint.
- [x] 2.3 Recalculate properties-pane width and breakpoint helpers from the effective workspace sidebar width so ultrawide windows no longer behave as if the left pane still consumed the full unclamped fraction.

## 3. Restore behavior and regression coverage

- [x] 3.1 Keep startup restore and persistence behavior working for exact preset values and older stored fractions that must snap to the nearest supported preset.
- [x] 3.2 Add widget coverage for the new Preferences row, the absence of the old sidebar footer controls, and adaptive sidebar widths at representative window sizes such as `900sp`, `1200sp`, `1400sp`, and `2000sp`.
- [x] 3.3 Add widget coverage for preset-driven properties-pane breakpoint recalculation and run the targeted Rust and GTK test commands for the touched shell and preferences paths.

## 4. Contract and documentation alignment

- [x] 4.1 Update `README.md`, `AGENTS.md`, and any nearby UI guidance that still describes the workspace width control as a persistent sidebar footer or as an unbounded raw `20%` or `30%` or `40%` window fraction.
