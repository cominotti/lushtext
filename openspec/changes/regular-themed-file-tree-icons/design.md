## Context

Workspace-section file rows are rendered by a `GtkSignalListItemFactory` in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`. The factory currently creates one `GtkImage` per recycled row and binds it with hard-coded symbolic icon names: `folder-symbolic` for directories and `text-x-generic-symbolic` for files. Root presentation and placeholder rows also use symbolic names.

GTK already resolves regular and symbolic themed icons through `GtkImage` and `GtkIconTheme`, and the application already depends on `gio`, `glib`, and `gtk4`. GIO can infer file content types from paths and expose the matching themed icon through `content_type_get_icon`. This gives the file tree platform-native regular icons without bundling custom file-type artwork.

The design boundary is intentionally narrow: file-tree content rows can use regular themed icons, while sidebar controls and status/placeholder affordances remain symbolic so LushText's Libadwaita chrome stays visually consistent.

## Goals / Non-Goals

**Goals:**

- Show regular themed folder icons for directory rows in the workspace file tree.
- Show regular themed content-type icons for file rows when GIO can infer a useful type from the path.
- Provide deterministic fallbacks for unknown file types or unavailable theme icons.
- Keep symbolic icons for controls, placeholder/status rows, and non-file-tree UI.
- Preserve row recycling, selection, expansion, drill-down, file peek, inline rename, and refresh behavior.
- Keep the implementation GTK-native and dependency-free beyond the existing GTK/GIO stack.

**Non-Goals:**

- Do not ship or design a custom file-type icon pack.
- Do not change the application icon, desktop metadata, GNOME Shell dock behavior, or packaging icon assets.
- Do not recolor symbolic icons or introduce a preference for choosing icon style.
- Do not make all sidebar or app chrome icons regular/full-color.
- Do not guarantee that every theme renders file-tree icons in color; the contract is regular themed icon lookup with stable fallbacks.

## Decisions

### Use regular themed icon lookup only for actual file-tree content rows

Directory and file rows represent filesystem content, so regular themed icons fit the GNOME exception for file-manager-like views. Buttons and command affordances are ordinary UI controls, so they stay symbolic.

Alternatives considered:

- **Switch every sidebar icon to regular icons.** Rejected because it would make controls visually inconsistent with Libadwaita conventions and broaden the change beyond the file tree.
- **Keep all icons symbolic.** Rejected because it does not improve scanability for file-tree content.

### Derive file icons from GIO content types

For file rows, derive the icon from the file path using GIO content-type inference and `gio::content_type_get_icon`. Bind the resulting `gio::Icon` to the existing row image with `gtk4::Image::set_from_gicon`. Use a regular `text-x-generic` fallback when content type inference is unknown or unsuitable.

Alternatives considered:

- **Use one regular `text-x-generic` icon for every file.** Simpler, but it misses the useful scanability improvement for common file groups such as images, scripts, archives, office documents, audio, and video.
- **Maintain an extension-to-icon map in LushText.** Rejected because GIO already owns content-type knowledge and user icon themes can supply the matching icon names.
- **Inspect file contents during row binding.** Rejected because row binding happens on the GTK main thread and must remain cheap.

### Keep icon derivation lightweight and row-recycling friendly

The row factory should compute icon presentation from data already present on `FileTreeItem`: whether the row is a directory, placeholder, and its path. It should not perform blocking filesystem I/O during binding. Path-based content-type guessing is acceptable; content-sniffing is not.

If a helper is added, keep it near the sidebar adapter layer because this is visual presentation, not domain model state. `FileTreeItem` should not grow persistent icon state unless implementation shows a measurable need.

Alternatives considered:

- **Store icon names on `FileTreeItem` at scan time.** Rejected for the initial design because scan services should remain focused on filesystem entries, and icon choice is presentation-specific.
- **Cache every icon result globally.** Deferred until evidence shows binding work is expensive; GTK icon theme and GIO already provide caching beneath the app.

### Keep fallback behavior explicit

The tree should prefer:

1. Placeholder/status rows: existing symbolic status icon.
2. Directory rows: regular `folder`.
3. File rows: GIO regular content-type icon.
4. Unknown/unavailable file rows: regular `text-x-generic`.

The implementation may use `GtkIconTheme::has_gicon` or equivalent probing when needed to avoid visible missing-icon fallbacks, but the user-facing result must remain stable even when a theme lacks specialized file icons.

## Risks / Trade-offs

- Theme variance -> Regular icons depend on the active icon theme, so color and detail can differ across systems. Mitigation: specify regular themed lookup rather than exact colors, and require stable fallback icons.
- Visual busyness -> Richer icons can make dense project trees feel heavier. Mitigation: limit regular icons to file-tree content rows and keep controls/status symbolic.
- Binding overhead -> MIME/icon inference during row binding could add work while scrolling large trees. Mitigation: use path-only GIO inference, avoid file reads, keep helper logic small, and add tests around helper behavior rather than expensive UI simulations.
- Missing theme icons -> Some themes may not provide specialized regular icons. Mitigation: fall back to `text-x-generic` for files and `folder` for directories.

## Migration Plan

This is a presentation-only change. Existing workspace data, session data, drafts, notes, local history, and sidebar tree models require no migration. Rollback is limited to restoring the previous symbolic icon binding in the workspace-section row factory.

## Open Questions

- None for the MVP.
