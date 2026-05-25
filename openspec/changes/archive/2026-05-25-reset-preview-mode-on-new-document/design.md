## Context

Markdown preview-only mode is a window-level shell state: `LushtextWindow` owns `preview_mode`, `editor_box`, `markdown_preview`, and the preview paned animation. Creating a new untitled document is a tab workflow in `documents.rs`, while the `win.new-tab` action schedules a delayed focus handoff to the selected editor after `new_tab()` returns.

Today `new_tab()` appends and selects a fresh `LushtextEditorPage` without clearing preview-only state. If the previous selected document was in preview-only mode, the new editor is selected but the editor container can remain hidden behind the preview shell, so the focus handoff cannot satisfy the new-document contract.

## Goals / Non-Goals

**Goals:**
- Make every New Document activation leave the newly selected untitled document in source-editor view.
- Keep the preview-only action state, internal `preview_mode` flag, animation state, and widget visibility synchronized.
- Preserve the existing delayed focus handoff behavior and stale-selection guard.
- Add deterministic widget coverage for the preview-only regression.

**Non-Goals:**
- Changing side-by-side Markdown preview behavior.
- Persisting preview-only mode across documents or sessions.
- Redesigning Markdown preview actions, shortcuts, or the primary menu.
- Broadening this change to all tab switching or file-opening behavior unless needed to satisfy the New Document contract.

## Decisions

### 1. Centralize preview-only reset in the preview workflow

Add a small window helper in `preview.rs` that force-exits preview-only mode for shell transitions that must reveal the source editor. The helper should clear `preview_mode`, set the `toggle-preview-mode` action state to `false`, cancel any active preview animation, mark animation state inactive, reveal `editor_box`, hide `markdown_preview`, and reset `shrink-start-child`. If the preview paned is already allocated, it should leave the editor side in a usable full-width state rather than a 1px preview-mode position.

Alternative considered: inline the reset directly in `new_tab()`. That would fix the immediate bug but duplicate preview internals outside the preview workflow and make future preview state changes easier to miss.

### 2. Call the reset from `new_tab()` before the focus handoff can run

`new_tab()` should invoke the preview-only reset as part of creating/selecting the new untitled editor. This covers all surfaces that call the same document creation method, including the shortcut, primary menu, header button, and command palette. The existing action-level `focus_selected_editor_after_action()` retry loop should remain responsible for focus timing.

Alternative considered: only reset in the `win.new-tab` action. That would miss any direct `new_tab()` callers and would split one user-facing operation across action glue and document lifecycle code.

### 3. Keep side-by-side preview independent

The reset should target `preview_mode` only. If side-by-side preview is visible, New Document may continue to show the preview pane with the active editor's placeholder/content according to existing `refresh_preview()` behavior.

Alternative considered: hide every Markdown preview surface when creating a new document. That would make the fix broader than the reported failure and surprise users who intentionally keep side-by-side preview open.

## Risks / Trade-offs

- [Preview animation is interrupted mid-transition] -> The reset helper must cancel the animation and synchronously restore stable widget state before focus restoration.
- [Action state and internal state drift apart] -> The helper must update both `preview_mode` and `toggle-preview-mode` state.
- [Focus Mode preview behavior regresses] -> Keep Focus Mode helpers and `Alt+P` paths unchanged, then cover the normal New Document regression with a focused widget test.

## Migration Plan

No migration is required. The change affects only live window state. Rollback means removing the reset call/helper and the regression test.

## Open Questions

None blocking.
