## Context

The `win.new-tab` action currently creates an untitled `LushtextEditorPage`, appends it to the `AdwTabView`, selects it, refreshes window chrome, and then returns. It does not explicitly move keyboard focus to the new page's `GtkSourceView`, so focus can remain on the widget that invoked the action. That makes immediate typing after the shortcut, header button, menu item, or command palette activation unreliable.

Other shell flows already treat editor focus as an explicit contract. Closing secondary panes schedules editor-focus restoration after GTK layout settles, and workspace-search result activation explicitly focuses the editor after opening or selecting a tab. New document creation should use the same style of explicit, GTK-aware focus handoff.

The shortcut contract is also inconsistent. The action is registered as `Ctrl+T`, while the menu and command palette already describe the action as creating a new file. This change intentionally treats the action as "new file/new document" rather than "new tab container" for user-facing labels and shortcuts.

## Goals / Non-Goals

**Goals:**

- Move keyboard focus to the `GtkSourceView` for every user-facing new document activation.
- Make the focus handoff reliable across direct shortcut use, header/menu activation, and command palette activation.
- Remove the old `Ctrl+T` binding and make `Ctrl+N` the only shortcut for the new document action.
- Align command palette metadata, shortcut overlay text, menu/header labels, and README shortcut documentation with the clean-break shortcut.
- Add widget coverage for focus ownership and old-shortcut removal.

**Non-Goals:**

- Do not introduce shortcut customization.
- Do not change file-backed open behavior beyond any shared focus helper needed for consistency.
- Do not change session/draft persistence semantics for untitled tabs.
- Do not add a compatibility alias where `Ctrl+T` continues creating documents.

## Decisions

### Use a selected-page-aware editor focus helper

Add a small window-level focus helper near the existing focus restoration code. The helper should target a specific tab page/editor pair or the active editor, schedule focus after the current GTK turn, and retry briefly if the editor is not focusable yet. Each attempt MUST first confirm that the target tab is still the selected page before calling `set_focus()` and `grab_focus()` on the editor's source view.

Alternative considered: call `editor.source_view().grab_focus()` inline at the end of `new_tab()`. That is simpler, but it can run before GTK has finished selecting/mapping the new tab and does not guard against a delayed focus attempt targeting a stale tab.

### Invoke focus from user-facing new document creation without changing restore semantics

The new document action path should request focus for the newly selected untitled editor. Startup session restoration also calls `new_tab()` for untitled restored tabs, so the implementation should avoid stealing focus during restore. This can be done either by suppressing the focus helper while `session.restoring` is true or by keeping the focus request in the user-facing action wrapper around `new_tab()`.

Alternative considered: always focus from `new_tab()`. That covers all callers, but it risks queued focus attempts interfering with session restore's final active-tab selection.

### Keep the internal action stable unless implementation proves a rename is cheap

The user-visible contract changes to `New File`/`Ctrl+N`; the internal `win.new-tab` action name may remain if renaming it would only create mechanical churn. The clean break is about the keyboard shortcut and visible workflow contract: no `Ctrl+T` alias remains.

Alternative considered: rename `win.new-tab` to `win.new-file`. That is clearer internally, but it increases the changed surface across action registration, command palette activation, menus, resources, and tests without changing user-visible behavior.

### Update all visible shortcut surfaces together

The implementation must update `setup_shortcuts()`, command palette command metadata, the GTK shortcuts overlay, the primary menu/header tooltip labels, and README shortcut table in the same change. This prevents the app from advertising a shortcut that no longer works.

Alternative considered: change only the runtime shortcut and tests. That would fix behavior but leave stale user-facing guidance.

## Risks / Trade-offs

- Delayed focus could steal focus from a control the user clicked immediately after creating a document -> Mitigate by checking that the target tab is still selected and keeping retry timing short.
- Command palette activation order can restore the previous focus after activating a command -> Mitigate by scheduling new-editor focus after the action returns, so it wins over palette cleanup for this specific workflow.
- Session restore may create untitled tabs before the window is fully ready -> Mitigate by suppressing user-action focus during `session.restoring` or invoking the helper only from actual action activation.
- Tests for shortcut removal can accidentally test helper functions rather than real shortcut registration -> Mitigate with widget tests that exercise the window shortcut controller/action path or inspect registered accelerators through the same surface existing tests use.

## Migration Plan

1. Add the focus helper and wire it into the user-facing new document path.
2. Replace the runtime shortcut with `Ctrl+N` and remove `Ctrl+T`.
3. Update user-visible labels and docs.
4. Add focused widget tests.
5. Run the widget test harness and the project verification commands appropriate for UI/resource changes.

Rollback is straightforward: revert the change files. There is no persisted data migration.

## Open Questions

None.
