## Context

`Browse Notes...` uses one shared preview pane for bookmarks, workspace notes, and document notes. Workspace-note and document-note entries render their saved note text through `LushtextMarkdownPreview`, but bookmark entries currently have no note body. Selecting a bookmark therefore calls the bookmark placeholder path and shows only metadata: label/fallback line title, source, file path, and line number.

That is technically accurate, but weak as a browsing experience. A bookmark points at a real line in a real file, so the preview should answer "what did I bookmark?" without forcing the user to open the file. The tricky part is doing this without turning the notes browser into a hidden full-file loader or a workspace-wide content search.

There is already a bounded file-peek service for sidebar previews, but it samples from the start of a file. Bookmark previews need an anchored excerpt around a known line. The design should reuse the same safety posture: GTK-free bounded service logic for disk reads, GTK adapter logic for live buffers and rendering, and explicit fallbacks for unsupported files.

## Goals / Non-Goals

**Goals:**

- Show source content in the `Browse Notes...` preview pane when the selected row is a bookmark.
- Include nearby context before and after the bookmarked line, not only content starting at the bookmarked line.
- Prefer live open-editor buffer text for open saved files, including unsaved edits and freshly moved bookmarks.
- Load closed-file bookmark excerpts on a background thread through the filesystem boundary.
- Keep excerpt extraction bounded by line count and byte/scan budgets.
- Render Markdown-like bookmark excerpts through the existing Markdown preview widget with the bookmarked file's render context.
- Render non-Markdown text excerpts as raw monospace content with the bookmarked line visually emphasized.
- Provide explicit loading and unavailable states for missing, unreadable, binary/non-UTF-8, too-large, or beyond-budget excerpts.
- Preserve existing search, sectioning, row selection, preview-only activation, and Open behavior.

**Non-Goals:**

- Full-document preview for bookmark rows.
- Workspace-wide content search across bookmarked files.
- Searching bookmark excerpt content before or during browser filtering.
- Persisting excerpt text or adding bookmark-sidecar schema fields.
- Adding syntax highlighting for arbitrary non-Markdown excerpts.
- Making invalid or binary files previewable beyond explicit fallback states.

## Decisions

1. Add a bookmark-specific bounded excerpt service.

   Create a GTK-free service module such as `services/bookmark_excerpt.rs`. It should expose pure value types for excerpt state and a bounded disk-load function for closed files. The service should classify results as text, unavailable, or budget-limited rather than leaking raw I/O errors into the UI.

   The disk path should read through `services::filesystem`, run off the GTK main thread via `spawn_blocking_then`, and cap the amount of data scanned while looking for the target line. If the bookmarked line cannot be reached within the scan budget, the service should return an explicit "line beyond preview budget" state instead of reading the whole file.

   Alternative considered: reuse `services::file_peek::load_snapshot` directly. That service is close in spirit, but it samples from file start and cannot answer "give me a window around line N" without changing its meaning for sidebar peek. A sibling service keeps responsibilities clear.

2. Use live editor buffers for open files.

   When a bookmark belongs to an open saved editor, the window should build the excerpt from that editor's current `GtkTextBuffer`/`GtkSourceBuffer` on the main thread using bounded line ranges only. This avoids stale disk reads and ensures the preview matches live bookmark state.

   The UI adapter should extract only the needed line range, then convert it into the same excerpt value shape used by the disk service. Services should not depend on GTK types.

   Alternative considered: always read from disk for consistency. That would ignore unsaved edits and can show content that disagrees with the editor the user already has open.

3. Represent bookmark preview state explicitly in the browser state.

   Extend the notes-browser entry or state with enough data to identify bookmark preview requests: file path, bookmark line, source metadata, live/open status, and a generation token. On selection, the preview should immediately show a loading state for closed-file requests, then apply the completed excerpt only if the selected row and generation still match.

   Existing `NotesBrowserState` already has a search generation pattern. Use the same local generation-counter style for preview loads so fast selection changes cannot render stale excerpts.

   Alternative considered: load all bookmark excerpts before presenting the dialog. That would make opening `Browse Notes...` slower and would perform content reads for rows the user may never inspect.

4. Choose presentation by file kind, not by row section.

   Markdown-like files (`.md`, `.markdown`, and equivalent aliases already treated as Markdown by the app) should render the excerpt through `LushtextMarkdownPreview::render_markdown_with_context`. The render context should use the bookmarked file path and workspace roots so relative links/images behave like document-note preview.

   Non-Markdown text files should render as raw monospace text. The bookmarked line should be emphasized without changing the file content itself, for example with a leading marker, line-number gutter styling, a text tag, or a narrow row highlight inside a read-only text view.

   Alternative considered: render all excerpts as Markdown. That makes plain source files vulnerable to accidental Markdown interpretation and hides the difference between prose preview and raw source context.

5. Keep excerpt dimensions bounded and layout-stable.

   Use a small default context such as 2-3 lines before and 6-10 lines after the bookmarked line. Cap the total rendered lines and bytes. Show truncation markers when context was clipped before or after the excerpt. Keep the preview pane within the existing Notes browser allocation and avoid resizing the dialog when selection changes.

   Alternative considered: show a much larger scrollable sample. That is tempting for inspection, but it turns the preview pane into a second editor and increases the risk of slow selection changes.

6. Keep search metadata-oriented.

   The notes-browser search should continue matching bookmark label, path, source, and line metadata. Excerpt text should not participate in search unless a future change explicitly designs an indexed or bounded content-search experience.

   Alternative considered: load excerpt text for every bookmark so search can match it. That would make search behavior expensive and surprising, especially with many bookmarks across large workspaces.

## Risks / Trade-offs

- [Risk] Markdown snippets can start inside a list, table, quote, or code block and render imperfectly. -> Mitigation: include nearby context before the target line, preserve readable fallback text, and keep the bookmark metadata visible above the rendered body.
- [Risk] Closed-file line lookup can be expensive for huge files or very deep line numbers. -> Mitigation: classify file size first, scan within a byte budget, and show an explicit beyond-budget fallback.
- [Risk] Fast row selection can race background preview loads. -> Mitigation: use a preview generation token and ignore stale completions.
- [Risk] Live-buffer preview extraction can accidentally copy too much text. -> Mitigation: derive a fixed line window from `GtkTextBuffer` iters and never snapshot the whole buffer for preview.
- [Risk] Adding a raw-text preview path can duplicate Markdown preview layout concerns. -> Mitigation: keep raw preview as one small read-only widget or a dedicated method on the preview surface, and cover layout stability in widget tests.
- [Risk] Users may expect preview text to affect search. -> Mitigation: keep search behavior documented as metadata/note-body search and leave full content search to the existing content-search workflow.

## Migration Plan

No data migration is required. Existing bookmark sidecars continue to store only bookmark identity, line, and optional label. Excerpts are derived at presentation time from the live editor buffer or source file.

Rollback is straightforward: keep bookmark metadata preview behavior and remove the excerpt service/UI path without touching persisted data.

## Open Questions

- Which exact extension aliases should be treated as Markdown-like for closed-file excerpts? Suggested initial set: `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn`.
- What default context window feels best in the dialog: 2 before/8 after, 3 before/7 after, or a symmetric 4/4?
- Should raw excerpts include line numbers in the text itself, or should line numbers be rendered as separate visual chrome?
