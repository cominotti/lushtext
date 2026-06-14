## Why

The document properties panel has drifted from GNOME Text Editor's more readable inspector styling: its rows sit on a side surface whose tone does not give enough separation or hierarchy. The workspace sidebar and document properties panel are both secondary side surfaces, so fixing only the properties panel risks making the two panes feel accidentally unrelated.

## What Changes

- Give the workspace sidebar and document properties panel a shared GNOME-like side-rail background tone so both side surfaces read as coordinated shell chrome.
- Keep the workspace sidebar's navigation-tree idiom intact: fixed workspace selector, section headers, `navigation-sidebar` file rows, no horizontal scrollbar, and dense tree behavior stay unchanged.
- Improve the document properties panel's inspector readability by making its grouped rows stand out clearly from the side-rail background in both spacious pane and compact bottom-sheet presentations.
- Preserve existing document-properties scope: no `Document Type` row, language picker, duplicate encoding control, or app-wide preferences controls are introduced.
- Add acceptance coverage for empty states, representative populated data, many or awkward rows, constrained geometry, light/dark schemes, and the compact bottom-sheet presentation.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `document-properties-pane`: add a visual contract for a readable GNOME-like inspector surface that remains distinct from the side-rail background across pane and sheet presentations.
- `workspace-sidebar-shell`: add a visual contract that the workspace sidebar shares the side-rail tone while preserving navigation-tree behavior, reachable controls, fixed selector, and constrained geometry.

## Impact

- Affected UI templates and styling: `resources/ui/window.blp`, `resources/ui/properties-panel.blp`, `resources/ui/sidebar.blp`, `resources/ui/workspace-section.blp`, generated `.ui` files, and `resources/style/style.css` or the dynamic transparency CSS in `crates/lushtext-core/src/lib.rs`.
- Affected tests/proof: Blueprint drift checks, widget tests that cover sidebar/properties shell behavior, and visual smoke/proof for side-surface readability in light and dark schemes.
- No service, model, persistence, automation API, or filesystem behavior changes are intended.
