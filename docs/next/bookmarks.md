# Bookmarks: Shipped Scope and Future Work

## Status: Shipped

LushText ships saved-file bookmarks as a lightweight navigation aid. The core
user promise is in place:

- bookmarks never modify the source file
- bookmarks persist outside version-controlled content
- bookmarks restore automatically when a saved file reopens

## Shipped Scope

- Per-file bookmark gutter indicators backed by `GtkSourceMark`
- Optional bookmark labels
- `F2` / `Shift+F2` next/previous navigation inside the active file
- Searchable workspace bookmark browser
- Save-file-only workflow with explicit feedback for untitled tabs

### Lifecycle Guarantees

- Bookmark sidecars migrate across **in-app sidebar renames**
- **Save As** starts a new note identity instead of copying the old one
- Normal open flows and session restore reload sidecars automatically

## First-Release Limitations

- Notes still use **path-based identity**, so external filesystem moves or copies
  performed outside LushText behave like a new file identity.

## Good Next Steps

1. Add richer sidecar reconciliation for externally moved or heavily rewritten
   files, likely by combining path identity with content fingerprints.
2. Consider a command-palette note mode if bookmark counts grow enough that the
   dedicated browse dialogs feel too separated from file search.
