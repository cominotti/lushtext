## 1. Regression Coverage

- [x] 1.1 Add or update widget helpers that capture a presented modal's outer allocation and relevant text-origin bounds after layout settles.
- [x] 1.2 Replace note editor geometry assertions that allow one-pixel drift with exact outer-size stability checks.
- [x] 1.3 Add a failing widget regression for a new range note: open Add Range Note, type text in Edit, switch to Render for the first time, and assert exact modal outer-size and text-origin stability.
- [x] 1.4 Add matching newly typed first-Render coverage for initially empty document-note and workspace-note editors.
- [x] 1.5 Keep existing saved document-note and saved range-note Edit/Render geometry coverage, upgraded to the exact stability contract.

## 2. Shared Note Editor Stabilization

- [x] 2.1 Stabilize the Render page host in `build_note_editor_surface()` so placeholder and rendered Markdown states advertise identical geometry before and after first Render.
- [x] 2.2 Ensure the Edit `GtkTextView` and Render preview text surface keep matching horizontal and vertical text origins for plain note text.
- [x] 2.3 Preserve existing non-empty note pre-render behavior where it still helps, without relying on pre-rendering as the only geometry guard.
- [x] 2.4 If the fix changes shared `LushtextMarkdownPreview` behavior, keep full-document Markdown preview behavior and tests unchanged or update them to preserve their existing contracts.

## 3. Modal Surface Audit

- [x] 3.1 Inventory modal and popup surfaces reachable from current window, sidebar, search, notes, local-history, and encoding workflows.
- [x] 3.2 Classify each surface as static, fixed-size dynamic, or content-following dynamic.
- [x] 3.3 Add or extend focused widget coverage for dynamic fixed-size modal browsers such as Notes and Local History to prove selection, filtering, preview, and empty-state changes do not resize the shell.
- [x] 3.4 Apply targeted stabilization fixes for any content-following dynamic modal found by the audit, or record why no code change is needed when the surface cannot mutate visible content while open.

## 4. Verification

- [x] 4.1 Run targeted headless widget tests for note editor modal geometry.
- [x] 4.2 Run targeted headless widget tests for audited modal browser geometry.
- [x] 4.3 Run `make check`.
- [x] 4.4 Run `openspec validate stabilize-modal-geometry --strict`.
