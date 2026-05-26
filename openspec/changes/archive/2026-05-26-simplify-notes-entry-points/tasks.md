## 1. Notes Menu Simplification

- [x] 1.1 Update the `Notes` menu resource so it contains only `Browse Notes…`, the bookmark toggle, `Add Range Note…`, `Open Document Note…`, `Open Workspace Note…`, and `Export Range Notes…`.
- [x] 1.2 Remove `Browse Bookmarks…`, `Edit Bookmark Label…`, and `Edit Range Note…` from the header-bar `Notes` menu while preserving their plain window actions for shortcuts and command palette.
- [x] 1.3 Add dynamic bookmark toggle labeling so the menu shows `Add Bookmark` off bookmarked lines and `Remove Bookmark` on bookmarked lines.
- [x] 1.4 Update Notes-menu-only action sensitivity so the simplified menu stays enabled or disabled according to saved-file and workspace-scope context.

## 2. Unified Notes Browser

- [x] 2.1 Extend the Notes browser backing entry model with bookmark entries built from `bookmark_service::list_workspace_bookmarks`.
- [x] 2.2 Add a dedicated `Bookmarks` `AdwSidebar` section ahead of or alongside the existing workspace, document, and range note sections.
- [x] 2.3 Implement bookmark selection preview with explicit bookmark metadata, including label or fallback title, workspace, file path, and line number.
- [x] 2.4 Route the Notes browser Open action for bookmark entries through the existing open-or-focus-at-line workflow.
- [x] 2.5 Update Notes browser search so bookmark rows match by label, workspace/file metadata, and line number while preserving body-text search for note entries.
- [x] 2.6 Keep pointer activation preview-only for all browser entry types, including bookmarks.

## 3. Contextual Entry Points

- [x] 3.1 Add `Open Document Note…` to the sidebar file context menu for file rows and route it through the existing document-note workflow.
- [x] 3.2 Add `Open Workspace Note…` to the workspace header context menu and route it to that concrete workspace root.
- [x] 3.3 Add editor-context entries for eligible bookmark and range-note operations without duplicating note persistence logic.
- [x] 3.4 Ensure contextual entries are keyboard-accessible through existing actions or equivalent GTK action wiring.

## 4. Command Palette And Shortcuts

- [x] 4.1 Keep direct command-palette entries for expert note commands and adjust labels only where the user-facing terminology changes.
- [x] 4.2 Update the shortcuts overlay to include the existing `Ctrl+Alt+M` `Edit Range Note` shortcut.
- [x] 4.3 Verify shortcut, command-palette, and menu routes still share the same workflow guards and status messages.

## 5. Tests And Verification

- [x] 5.1 Update widget tests for the simplified `Notes` menu labels, ordering, primary-menu exclusion, dynamic bookmark label, and action sensitivity.
- [x] 5.2 Add widget coverage for the `Bookmarks` section in `Browse Notes…`, including selection preview, Open behavior, workspace-scope filtering, and search matching.
- [x] 5.3 Add widget coverage for contextual document-note, workspace-note, bookmark, and range-note entry points.
- [x] 5.4 Run targeted widget/integration tests for note workflows.
- [x] 5.5 Run `openspec validate simplify-notes-entry-points --strict`.
