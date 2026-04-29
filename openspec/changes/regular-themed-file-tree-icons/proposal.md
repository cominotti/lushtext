## Why

The workspace sidebar file tree currently uses monochrome symbolic icons for every directory and file, which makes the tree quieter than a file-browser surface needs to be and makes different file kinds harder to scan. GNOME's guidance keeps symbolic icons for ordinary UI controls, but allows full-color file and folder icons in file-manager-like views, which matches the sidebar tree without changing the app's chrome.

## What Changes

- Render regular themed icons for actual file-tree rows in workspace sections.
- Use a regular folder icon for directory rows, including workspace-root rows that are shown as file-tree entries.
- Use the platform content-type icon for file rows when GIO can infer one from the path, with a stable regular text-file fallback when inference or theme lookup is unavailable.
- Keep symbolic icons for non-file-tree controls and status rows, including toolbar/header buttons, focus-folder controls, placeholder rows, warning/status affordances, and other application chrome.
- Preserve existing sidebar behavior for expansion, selection, drill-down, file peek, context menus, inline rename, refresh, and workspace filtering.

## Capabilities

### New Capabilities
- `regular-file-tree-icons`: Workspace file-tree rows use regular themed file and folder icons while the rest of the sidebar controls remain symbolic.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` and any small helper module needed to derive row icons.
- GTK/GIO integration: use existing `gtk4`, `gio`, and `glib` dependencies; no new third-party crate is expected.
- Tests: add focused coverage that directory, file, placeholder, and chrome/control icon choices stay within the intended regular-versus-symbolic boundary.
- Packaging: no custom icon assets should be shipped for this change; icons come from the active GTK icon theme with existing fallbacks.
