## Context

LushText currently projects saved-file bookmarks into `GtkSourceMark`s using a dedicated mark category and `GtkSourceMarkAttributes`. That gives users a visible gutter icon, tooltip text, navigation, minimap markers, and debounced sidecar persistence, but editing a bookmark label still depends on placing the cursor on the bookmarked line and invoking `Edit Bookmark Label`.

GtkSourceView exposes `line-mark-activated` for button activation in the line-mark gutter. That is a better fit than drag for the first interaction upgrade: activation provides the clicked line, while the existing live bookmark projection can resolve the bookmark at that line and route through the normal save path. The edit surface should be a small modal dialog first, not an anchored popover, because it is easier to make accessible, stable under scroll/resizes, and consistent with the existing label editor.

## Goals / Non-Goals

**Goals:**

- Let users activate an existing bookmark gutter mark and open a bookmark edit dialog.
- Let that dialog update the bookmark label and reassign the bookmark to another line in the active file.
- Reuse the existing live mark projection, minimap refresh, and debounced sidecar persistence.
- Keep existing bookmark navigation, browse surfaces, sidecar identity, rename migration, and Save As behavior unchanged.
- Preserve source file bytes while editing bookmark metadata.

**Non-Goals:**

- Dragging bookmark icons between lines.
- Custom gutter rendering or pixel-perfect popover anchoring.
- A separate Favorites or file-pin feature.
- Semantic re-anchoring when a bookmarked line is deleted or externally moved while the file is closed.
- Changing bookmark sidecar schema beyond the already persisted line and label fields.

## Decisions

1. Use `GtkSourceView::line-mark-activated` as the entry point.

   The existing mark icon is rendered by GtkSourceView, not by an app-owned child widget. `line-mark-activated` is the toolkit-supported click path for line marks and provides a `TextIter` for the activated line. The editor layer should resolve whether that line has one of LushText's bookmark marks before asking the window layer to present editing UI.

   Alternative considered: attach a general `GestureClick` to the source view and manually hit-test the gutter. That would duplicate toolkit behavior and risk interfering with text selection or editor input.

2. Keep the edit UI modal for this change.

   LushText already has an Adwaita alert dialog for bookmark label editing. Extending that flow to an "Edit Bookmark" dialog with a label entry and line number control is lower risk than anchoring a custom popover to gutter geometry. It also gives keyboard and screen-reader users a predictable focus model.

   Alternative considered: a small `GtkPopover` anchored beside the gutter icon. That can be revisited later, but it requires robust line geometry and scroll handling and is not necessary to validate the bookmark-line editing behavior.

3. Move live marks by stable bookmark identity.

   Bookmark records already have stable IDs and the editor holds each `BookmarkRecord` beside its live `GtkSourceMark`. The editor should expose a small bookmark-edit API that targets a bookmark ID or activated bookmark record, validates a 1-based user line against the current buffer line count, moves the live mark to the target iter, updates the label, emits `bookmarks_changed`, and refreshes the minimap.

   Alternative considered: delete and recreate the bookmark at the new line. That risks losing timestamps or ID continuity and complicates browse surfaces that rely on stable record identity.

4. Handle collisions explicitly.

   LushText currently allows one bookmark per line through toggle behavior. If the user chooses a target line that already has a different bookmark, the edit dialog should not silently merge or overwrite. The save response should keep the dialog open and show clear feedback so the user can choose another line or remove the other bookmark separately.

   Alternative considered: swap bookmark lines. That is clever but surprising, and it adds edge cases without a current user need.

## Risks / Trade-offs

- [Risk] `line-mark-activated` can be emitted for any line mark category. -> Mitigation: resolve only LushText bookmark records at the activated line and ignore non-bookmark marks.
- [Risk] Dialog line numbers are user-facing 1-based while records are zero-based. -> Mitigation: keep conversion inside the editor/window boundary and test first, last, and out-of-range lines.
- [Risk] Moving a bookmark to a line already containing another bookmark could corrupt user intent. -> Mitigation: reject collisions and leave persisted state unchanged.
- [Risk] Existing `Edit Bookmark Label` naming may become inaccurate. -> Mitigation: update visible labels, action documentation, and README/manual checks to describe `Edit Bookmark`.
- [Risk] Large or evicted editor states may not have a usable live buffer. -> Mitigation: only expose gutter activation and line reassignment for active, loaded editors with live bookmark projection.
