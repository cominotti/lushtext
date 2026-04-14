# Bookmarks & Annotations: Shipped Scope and Future Work

## Status: Partially Shipped

LushText now ships the first iteration of saved-file bookmarks and sidecar
annotations. The core user promise is in place:

- bookmarks never modify the source file
- annotations never modify the source file
- both persist outside version-controlled content
- both restore automatically when a saved file reopens

## Shipped Scope

### Bookmarks

- Per-file bookmark gutter indicators backed by `GtkSourceMark`
- Optional bookmark labels
- `F2` / `Shift+F2` next/previous navigation inside the active file
- Searchable workspace bookmark browser
- Save-file-only workflow with explicit feedback for untitled tabs

### Annotations

- Sidecar line-range annotations with note text and first-release styles
- Theme-aware highlight indicators in the editor
- Create, edit, and delete flows from window actions
- Searchable workspace annotation browser
- Markdown export grouped by file with line ranges and source excerpts

### Lifecycle Guarantees

- Bookmark and annotation sidecars migrate across **in-app sidebar renames**
- **Save As** starts a new note identity instead of copying the old one
- Normal open flows and session restore reload sidecars automatically
- While a file stays open, annotation anchors track line insertions/deletions and
  remove themselves if the entire annotated range is deleted

## First-Release Limitations

- Notes still use **path-based identity**, so external filesystem moves or copies
  performed outside LushText behave like a new file identity.
- Annotation indicators are currently **highlight-based**, not clickable gutter
  popovers or inline rendered note blocks.
- Annotation export is **markdown only** in the first release.
- External edits can still make persisted line ranges feel stale between app
  launches; the live anchor tracking only exists while the file is open.

## Good Next Steps

1. Add clickable gutter affordances or hover popovers for annotations so users
   can reopen a note directly from the highlighted range.
2. Add richer sidecar reconciliation for externally moved or heavily rewritten
   files, likely by combining path identity with content fingerprints.
3. Explore optional inline annotation rendering once the lightweight highlight
   workflow has proven itself in real use.
4. Consider a command-palette note mode if bookmark and annotation counts grow
   enough that the dedicated browse dialogs feel too separated from file search.
