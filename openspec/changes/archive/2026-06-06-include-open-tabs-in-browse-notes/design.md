## Context

`Browse Notes...` currently snapshots the active workspace scope, derives visible workspace roots, and loads workspace notes, bookmark sidecars, and document-note sidecars for those roots. If there are no visible workspace roots, the browser exits early with a warning. The notes-browser entry model also assumes every bookmark and document-note row can be mapped to a workspace through `workspace_for_path`.

That model is correct for workspace browsing, but it misses saved files the user already has open outside the current scope. The editor can already expose live bookmark state for open tabs, and the bookmark/document-note services already support loading sidecars by explicit saved path, so this change can add open-tab coverage without changing persistence or scanning arbitrary filesystem areas.

## Goals / Non-Goals

**Goals:**

- Add a clearly labeled `Open Tabs` section to `Browse Notes...` for saved open files outside the current workspace scope.
- Include both live bookmarks and document notes for those open tabs.
- Preserve the existing workspace-scoped `Bookmarks`, `Workspace Notes`, and `Document Notes` sections.
- Allow `Browse Notes...` to open when there are no restored workspaces but eligible saved open-tab entries exist.
- Keep all broad sidecar scans on background threads and keep GTK widget reads on the main thread.
- Add focused tests for the new browse behavior.

**Non-Goals:**

- Replacing workspace-scoped browsing.
- Showing closed files outside workspace roots.
- Creating document notes for open tabs that do not already have one.
- Creating bookmarks or document notes for untitled tabs.
- Changing bookmark or document-note persistence identity.
- Adding a permanent Favorites or pinned-file feature.

## Decisions

1. Model open-tab rows as their own browser source.

   `NotesBrowserEntry` should gain a source shape that can represent workspace-owned rows and open-tab rows without inventing fake workspace metadata. Workspace rows keep their current workspace name/root. Open-tab rows should carry the saved path plus an optional known workspace label if the path belongs to a restored workspace outside the current scope.

   Alternative considered: put every open-tab row into the existing `Bookmarks` or `Document Notes` sections with a synthetic workspace name. That blurs the meaning of workspace scope and makes the UI harder to reason about when the same file is outside the selected scope.

2. Treat open tabs outside the current scope as supplemental, not replacement, data.

   For the active `All workspaces` scope, an open tab is supplemental only when it is outside every restored workspace root. For a concrete workspace scope, an open tab is supplemental when it is outside that selected workspace root, even if it belongs to another restored workspace. Rows already inside the current scope should continue through the normal workspace sections, including the existing live bookmark overlay.

   Alternative considered: only show open tabs outside all restored workspaces. That solves the original no-workspace-file case but still surprises users who are focused on one workspace while another open file has a useful note or bookmark.

3. Collect open-tab path and live bookmark snapshots on the GTK main thread, then load sidecars by explicit path in background work.

   The window should snapshot saved open editor paths and live bookmark records before spawning the blocking load. Background work can then load document notes for those paths with `document_note_service::load_for_path` and shape live bookmark records into browse rows. This avoids a broad scan outside workspace roots and avoids querying GTK objects from the background thread.

   Alternative considered: extend workspace listing functions to scan all sidecars and filter after the fact. That would make browsing open tabs more expensive and weaken the current scope boundary.

4. Keep duplicate handling identity-based.

   If an open tab is inside the current workspace scope, the existing workspace listing plus live bookmark overlay owns it. If an open tab is outside the current scope, it appears in `Open Tabs`. The implementation should still guard against duplicates by document sidecar identity when an open path also appears in the workspace result set through overlapping roots or scope changes.

   Alternative considered: deduplicate by display path and line only. That is weaker around symlinks, canonical identities, and moved bookmark records.

5. Make preview and Open behavior source-aware.

   Open-tab bookmark rows should preview as bookmark metadata and open/focus the saved file at the bookmarked line. Open-tab document-note rows should preview the note body and open/focus the saved file before opening its document-note surface. When no workspace root is available, Markdown preview context should use the file path as the document context and an empty or nearest-safe root context rather than pretending there is a workspace.

   Alternative considered: block document-note opening when no workspace root exists. That would keep implementation smaller, but it would undercut the main UX promise: open saved tabs should be first-class enough to navigate back to their notes.

6. Use neutral search and explicit open-tab metadata.

   The browser search placeholder should become `Search Notes...` so it remains truthful when results include both workspace rows and open-tab rows. Open-tab row metadata should say `Open tab · <workspace name>` when the path belongs to a restored workspace outside the current scope, and `Open tab · Outside workspace` when no restored workspace owns the path.

   Alternative considered: keep `Search Current Workspace...`. That would be accurate for the workspace sections but misleading once the browser deliberately includes out-of-scope open-tab rows.

## Risks / Trade-offs

- [Risk] Users may think concrete workspace scope is no longer strict. -> Mitigation: keep workspace sections strict and put every out-of-scope open file under a separate `Open Tabs` section with source metadata.
- [Risk] The same bookmark or note could appear in both workspace and open-tab sections. -> Mitigation: classify open tabs as supplemental only when outside the current scope and use document identity for defensive deduplication.
- [Risk] Loading document notes for every open tab could add latency with many tabs. -> Mitigation: only load explicit saved open paths, keep work off the GTK thread, and reuse existing per-path sidecar helpers.
- [Risk] Markdown preview context may miss workspace-relative assets for open files outside any workspace. -> Mitigation: render with the saved file path as the document context and avoid fake roots; relative-to-file links remain the meaningful fallback.
- [Risk] No-workspace Browse Notes behavior could become ambiguous when there are no eligible open-tab entries. -> Mitigation: keep the existing warning/empty handling when neither workspace rows nor open-tab rows exist.

## Migration Plan

No data migration is required. Existing bookmark and document-note sidecars remain keyed by saved-document identity. The change only expands how `Browse Notes...` collects and labels rows at presentation time.

## Open Questions

- None.
