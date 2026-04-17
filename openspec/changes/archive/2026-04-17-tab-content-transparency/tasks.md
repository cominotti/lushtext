## 1. Preference model and UI

- [x] 1.1 Add a new persisted `tab-content-opacity` setting to the app config and GSettings schema with default `1.0` and the required numeric bounds.
- [x] 1.2 Add an always-visible `Transparency` preference row under `Preferences > Editor > Appearance` with a live percentage label and a popover-hosted slider that matches the approved Fedora-style interaction shape.
- [x] 1.3 Wire the new preference control so changing the slider updates the stored value immediately and restoring Preferences shows the current persisted percentage.

## 2. Window-level transparency plumbing

- [x] 2.1 Add a centralized window appearance update path that reacts to opacity changes without using `gtk_widget_set_opacity()` on editor or preview widgets.
- [x] 2.2 Define explicit opaque styling for shell chrome surfaces so the header bar, tab chrome, side panels, bottom chrome, and search-panel chrome remain solid while transparency is enabled.

## 3. Document-surface rendering

- [x] 3.1 Implement editor-surface background updates that derive translucent backgrounds from the active GtkSourceView style scheme and refresh when opacity, style scheme, or dark mode changes.
- [x] 3.2 Apply the same opacity behavior to the Markdown preview background so preview mode matches the editor surface instead of falling back to a separate opaque background.
- [x] 3.3 Keep non-document editor helpers on an explicit opaque path, including the minimap, infobars, and in-editor find or replace chrome.

## 4. Verification and contract updates

- [x] 4.1 Add widget coverage for the always-visible preference row, percentage readout, immediate apply behavior, and persisted restore behavior.
- [x] 4.2 Add widget coverage for the rendering boundary contract so editor and Markdown preview follow the setting while chrome and minimap stay opaque.
- [x] 4.3 Run the targeted Rust and GTK verification commands for preferences, editor-page, markdown-preview, and window-shell paths, then update any docs that describe the new transparency contract if implementation changes them.
