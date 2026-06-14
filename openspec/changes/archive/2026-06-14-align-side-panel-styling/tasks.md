## 1. Styling Surface Audit

- [x] 1.1 Identify every current use of `shell-chrome-opaque`, `.sidebar`, `.navigation-sidebar`, and side-panel background styling in Blueprint, generated UI, static CSS, and dynamic transparency CSS.
- [x] 1.2 Choose semantic styling names for coordinated side rails and unrelated opaque shell surfaces, keeping the workspace sidebar and document properties panel separable from empty states, search bars, and preview placeholders.

## 2. Coordinated Side-Rail Styling

- [x] 2.1 Add or revise CSS so the workspace sidebar and document properties panel share a GNOME-like side-rail background tone that remains explicitly opaque when tab-content transparency is enabled.
- [x] 2.2 Apply the side-rail styling only to the intended side surfaces in `resources/ui/window.blp` and regenerate the corresponding `.ui` files.
- [x] 2.3 Preserve the existing background treatment for unrelated opaque surfaces such as empty states, search bar chrome, preview placeholders, and editor/preview document content.

## 3. Document Properties Inspector Readability

- [x] 3.1 Tune the document properties panel so its `Adw.PreferencesGroup` content remains visibly separated from the side-rail background in light and dark schemes.
- [x] 3.2 Verify no-document, untitled, file-backed, long-location, formatting-source, statistics, and multi-finding health states stay readable without stale metadata or overlapping controls.
- [x] 3.3 Verify the compact bottom-sheet presentation uses the same readable inspector relationship and does not add document type, language, duplicate encoding or line-ending, or app-wide Preferences controls.

## 4. Workspace Sidebar Navigation Preservation

- [x] 4.1 Keep the workspace sidebar as a navigation surface: fixed workspace selector, section headers, `navigation-sidebar` tree rows, disclosure, selection, context menu, file peek, focus-folder, and inline rename behavior remain intact.
- [x] 4.2 Verify no-workspace, zero-folder, one-folder, many-folder, long-name, duplicate-basename, overlapping-folder, and constrained-width states preserve readable controls and no horizontal scrollbar.
- [x] 4.3 Verify workspace-folder reorder handles, insertion indicators, hover, selection, and drop-shield states remain legible without new filled row highlights or drop-into-folder cues.

## 5. Validation

- [x] 5.1 Run `make blueprint-generate` and `make check-blueprint`.
- [x] 5.2 Run focused widget coverage for document properties and workspace sidebar behavior, including compact and constrained geometry cases.
- [x] 5.3 Run `make visual-geometry-smoke` or the focused visual proof lane that covers both side panels in light and dark schemes, refreshing intentional visual artifacts if required.
- [x] 5.4 Run `openspec validate align-side-panel-styling --strict` and `git diff --check`.
- [x] 5.5 Run the repo's final applicable gate, such as `make pre-commit`, before publishing the implementation.
