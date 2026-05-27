## 1. Core Feature Removal

- [x] 1.1 Delete the annotation model and service modules, remove their `model::annotation` and `services::annotation_service` exports, and remove their unit tests.
- [x] 1.2 Remove annotation sidecar loading, saving, export, delete, and rename handling from all window and sidebar workflows.
- [x] 1.3 Remove annotation-specific comments from shared sidecar helpers while keeping bookmark, document-note, workspace-note, and local-history identity behavior intact.

## 2. Editor Projection Removal

- [x] 2.1 Delete the editor-page annotation projection module and remove public editor-page methods for loading, clearing, creating, updating, deleting, finding, focusing, reconciling, and highlighting annotations.
- [x] 2.2 Remove `LiveAnnotation`, `AnnotationState`, pending annotation focus, annotation persistence state, and annotation GSettings handler fields from editor-page implementation state.
- [x] 2.3 Remove annotation provider setup, theme/highlight refresh handling, and end-user-action range reconciliation from editor-page construction and disposal paths.
- [x] 2.4 Keep bookmark projection, minimap, local-history, document-note, workspace-note, and rich note editor behavior compiling and covered after the annotation state is gone.

## 3. UI, Actions, And Settings Cleanup

- [x] 3.1 Remove Range Note actions from `ui/window/actions.rs`, application accelerators, command-palette command definitions, the shortcuts overlay, and editor context menus.
- [x] 3.2 Remove Range Note items from the header-bar Notes menu and its menu-only action sensitivity while preserving `Browse Notes...`, bookmark toggle, `Open Document Note...`, and `Open Workspace Note...`.
- [x] 3.3 Remove Range Note dialog, Range Note export, Range Note browser entry, Range Note search matching, and pending annotation-open routing from `ui/window/notes.rs`.
- [x] 3.4 Remove `annotation-highlights-visible` from config constants, GSettings schema, preferences UI, and preferences binding code.

## 4. Tests And Documentation

- [x] 4.1 Delete or rewrite integration and widget tests that assert annotation sidecars, Range Note shortcuts, Range Note dialogs, Range Note browser rows, Range Note export, or annotation range tracking.
- [x] 4.2 Add or update focused tests proving the remaining Notes menu and `Browse Notes...` browser still work for bookmarks, document notes, and workspace notes.
- [x] 4.3 Update README, metainfo, root and nested AGENTS files, and next docs so active documentation no longer presents Range Notes or annotation sidecars as supported features.
- [x] 4.4 Update active OpenSpec specs through this change so `sidecar-annotations` is retired and remaining capabilities describe only bookmarks, document notes, and workspace notes.

## 5. Verification

- [x] 5.1 Run `cargo fmt --check`.
- [x] 5.2 Run `git diff --check`.
- [x] 5.3 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.4 Run focused Rust tests for notes, editor page, and window widget coverage affected by the removal.
- [x] 5.5 Run `./scripts/run-widget-tests.sh --auto`.
- [x] 5.6 Run `openspec validate remove-range-notes --strict` and `openspec validate --all --strict`.
- [x] 5.7 Run `rg -n -i "range note|range-note|annotation|annotations/|sidecar-annotations|annotation_service|annotation-highlights|notes-add-annotation|add-annotation" --glob '!openspec/changes/**'` and resolve every active leftover.
