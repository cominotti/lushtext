## Context

`Browse Notes...` currently snapshots the active workspace scope, loads workspace notes, bookmark sidecars, and document-note sidecars on a background thread, then merges those persisted records into a sectioned `AdwSidebar`. Bookmark edits inside open editors use `GtkSourceMark` state and persist through a short debounced sidecar save, so the live editor can be newer than the bookmark sidecar when the user opens the browser.

The standalone bookmark browser already has a small `window-close-symbolic` button, but the unified notes browser builds its own `AdwDialog` with a `NavigationSplitView` and no visible close affordance. In collapsed layouts, either the sidebar page or the preview page can be the visible page, so a reliable close affordance needs to be reachable from both states.

## Goals / Non-Goals

**Goals:**

- Show bookmarks from open saved editors in `Browse Notes...` immediately after toggle, label, or line edits, even before the debounced sidecar write completes.
- Preserve workspace-scope filtering and the existing sidecar-based listing for closed files.
- Avoid duplicate bookmark rows when the same bookmark appears in both a persisted sidecar and a live open editor.
- Add a visible Close/X affordance to the populated and empty notes-browser states.
- Cover the repair with focused widget tests.

**Non-Goals:**

- Replacing the bookmark sidecar persistence model.
- Changing document-note or workspace-note persistence.
- Making `Browse Notes...` auto-refresh while already open after later bookmark edits.
- Adding a separate Favorites or file-pin feature.
- Changing row activation semantics; selecting a row still previews, and Open still performs navigation.

## Decisions

1. Merge live open-editor bookmark snapshots into the browser entry input.

   Before presenting `Browse Notes...`, the window should collect bookmark records from open saved editors whose file paths belong to the visible workspace roots. Those live rows should be merged with the background sidecar listing before `build_notes_browser_entries` is called, replacing the persisted rows for the same open file identity where possible. This makes the browser match what the user sees in the editor without waiting for the debounce timer.

   Alternative considered: flush pending bookmark persistence before listing. That couples a browse action to disk writes, can still race an in-flight save, and makes the UI wait on persistence work that is not required to display accurate live state.

2. Deduplicate at the document/bookmark identity boundary.

   A saved open editor may also have an existing sidecar row. The merge should avoid duplicate rows by using the same stable identity the sidecar service already uses for saved documents plus each bookmark's stable ID. If identity resolution fails for an open file, the implementation should skip the live overlay for that file and preserve the existing sidecar/error behavior.

   Alternative considered: deduplicate only by display path and line. That would wrongly collapse distinct bookmarks after line moves or label edits and would be weaker around symlink/canonical-path identities.

3. Keep all filesystem scanning off the GTK main thread, but keep live snapshot collection on the GTK main thread.

   The current sidecar listing should remain in `spawn_blocking_then`. The live editor snapshot is cheap GTK state and must be read on the main thread before or after the background listing result is merged. The implementation should pass pure data, not widgets, into background work.

   Alternative considered: query open editor widgets from the background task. GTK objects are main-thread-bound, so that would violate the existing threading model.

4. Add close buttons to every visible notes-browser page.

   The populated browser should expose a close icon button in the sidebar page and in the preview page, both wired to the same dialog close path. The empty-state dialog should also include a close affordance. Buttons should use the standard close icon, tooltip, and accessible label.

   Alternative considered: relying on Escape or dialog chrome. The reported bug shows that Escape can be focus-dependent for users, and the current dialog content has no obvious dismissal target.

## Risks / Trade-offs

- [Risk] Live and sidecar bookmark rows could duplicate each other. -> Mitigation: deduplicate by document identity and bookmark ID when merging live open-editor snapshots with sidecar rows.
- [Risk] Open files outside the selected workspace could leak into the current scope. -> Mitigation: apply the same current-scope root filtering used by sidecar listing before creating live rows.
- [Risk] A failed identity resolution for a live editor could hide a sidecar row. -> Mitigation: only overlay live rows for identities that resolve successfully; otherwise keep the existing sidecar listing.
- [Risk] Additional close buttons could clutter the browser header. -> Mitigation: use compact icon-only buttons with tooltip and accessible label, matching the existing standalone bookmark browser.
