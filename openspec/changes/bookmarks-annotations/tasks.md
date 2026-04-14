## 1. Models and persistence foundations

- [x] 1.1 Add bookmark and annotation model modules, including stable IDs, timestamps, styles, and canonical-path identity helpers for file-backed notes.
- [x] 1.2 Implement bookmark and annotation services for sidecar load/save/delete/move operations under the LushText data directory.
- [x] 1.3 Add markdown export support for persisted annotations, including grouped-by-file output with line ranges and source excerpts.
- [x] 1.4 Add settings and preferences plumbing for bookmark gutter visibility and any first-release annotation visibility controls.

## 2. Bookmark editor workflow

- [x] 2.1 Load persisted bookmarks into file-backed editor pages as `GtkSourceMark` instances and save bookmark mutations back through the bookmark service.
- [x] 2.2 Add bookmark toggle and label-edit actions, including clear feedback when the active document is not yet saved to disk.
- [x] 2.3 Implement next/previous bookmark navigation for the active file and ensure the cursor jumps to the selected bookmark line.
- [x] 2.4 Add a searchable bookmark list workflow that opens or focuses the bookmarked file and jumps to the selected bookmark.

## 3. Annotation editor workflow

- [x] 3.1 Load persisted annotations into file-backed editor pages using live range anchors that stay connected to the buffer while the file is open.
- [x] 3.2 Add create, edit, and delete annotation flows for selected line ranges, including presentation-style editing and unsaved-document feedback.
- [x] 3.3 Update annotation ranges on line insertions and deletions, and remove annotations whose entire anchored range is deleted.
- [x] 3.4 Render annotation indicators and browsing surfaces that let users reopen existing annotations from the editor.

## 4. File lifecycle and window integration

- [x] 4.1 Preserve bookmark and annotation sidecars across in-app sidebar renames while treating Save As as a new file identity.
- [x] 4.2 Restore bookmarks and annotations when files reopen through normal open flows or session restore.
- [x] 4.3 Add window-level actions, menus, shortcuts, and status messaging for bookmark, annotation, and export workflows.
- [x] 4.4 Implement workspace annotation export entry points and file-save behavior for the generated markdown report.

## 5. Verification and documentation

- [x] 5.1 Add unit tests for sidecar persistence, identity migration, and annotation export formatting.
- [x] 5.2 Add widget or integration tests for bookmark toggling, bookmark navigation, annotation restoration, and annotation range tracking during edits.
- [x] 5.3 Update the relevant user-facing documentation and future-work notes to describe bookmark and annotation behavior, scope, and limitations.
