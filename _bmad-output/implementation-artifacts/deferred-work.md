# Deferred Work — Hamburger Menu Feature

## Spec 2: Zoom Controls

Custom zoom widget in the hamburger menu matching GNOME Text Editor's zoom section. Three controls: zoom in (+), zoom out (−), reset (percentage label showing current level). Implementation requires:
- Custom widget via `PopoverMenu::add_child("zoom")`
- Font size scaling via CSS provider (relative to base font size)
- GSettings key for persisted zoom level
- Keyboard shortcuts: Ctrl+Plus, Ctrl+Minus, Ctrl+0
- Zoom range: 50%–400% (matching GNOME Text Editor)
- Command palette entries for zoom actions

## Spec 3: Print + Discard Changes

Two new per-page actions completing the GNOME Text Editor menu parity:

**Print (Ctrl+P):**
- `GtkPrintOperation` integration for the active editor's buffer
- Note: Ctrl+P currently used for command palette — need to resolve shortcut conflict (move palette to Ctrl+Shift+P or keep Print without shortcut)
- Menu item in Find/Replace section (after Find/Replace, before Fullscreen)

**Discard Changes:** ✅ Implemented in spec-discard-changes.md

## File Index: Incremental rename vs skip list inconsistency

When a directory is renamed to/from an ignored name (e.g., `src` → `target`) via sidebar inline rename, the incremental `rename_path()` method rewrites child paths but does not consult `IGNORED_INDEX_DIRS`. Files under the renamed-to-ignored directory remain searchable until the next full rebuild. Conversely, renaming away from an ignored name does not add children. Self-corrects on next workspace change or app restart. Low priority — renaming directories to well-known build names is extremely rare.

## Spec 4: Draft Deletion Safety

Both `wire_info_bar` discard and hamburger-menu `discard_changes` delete the draft file *before* `load_file_async` succeeds. If the backing file is deleted between confirmation and reload, the user's unsaved changes (stored in the draft) are permanently lost with no recovery path. The draft should only be deleted after a successful reload — or at minimum, the draft content should be kept until reload success is confirmed. This affects `wire_info_bar` (existing code) and `discard_changes` (new code).
