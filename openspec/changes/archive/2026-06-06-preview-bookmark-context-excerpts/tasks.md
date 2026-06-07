## 1. Excerpt Model And Service

- [x] 1.1 Add a GTK-free bookmark excerpt module under `crates/lushtext-core/src/services/` and export it from `services/mod.rs`.
- [x] 1.2 Define value types for excerpt status, presentation mode, line window metadata, target-line position, truncation, and unavailable reasons.
- [x] 1.3 Implement pure line-window extraction from already available text/line slices for live editor previews.
- [x] 1.4 Implement bounded closed-file excerpt loading through `services::filesystem`, including metadata classification, UTF-8 validation, binary detection, scan-byte budget, line-count budget, and beyond-budget fallback.
- [x] 1.5 Implement Markdown-like path classification for bookmark excerpt presentation without introducing GTK or GtkSourceView dependencies into services.
- [x] 1.6 Add unit tests for line-window extraction near file start, middle, and end; truncation markers; target-line indexing; Markdown classification; binary/non-UTF-8/unreadable/too-large/beyond-budget fallback states.

## 2. Notes Browser Preview State

- [x] 2.1 Extend the notes-browser bookmark preview path so selected bookmark entries request a bookmark excerpt instead of using metadata-only placeholder copy.
- [x] 2.2 Use live open-editor buffer text for open saved files, extracting only the configured bounded line window around the bookmark.
- [x] 2.3 Use `spawn_blocking_then` for closed-file excerpt loads and add a generation token so stale preview completions cannot replace the current selection.
- [x] 2.4 Preserve existing preview metadata, Open action targeting, sidebar sectioning, and row activation behavior while excerpt loading is pending or unavailable.
- [x] 2.5 Keep bookmark excerpt loading out of notes-browser search filtering so search does not trigger content reads for unselected rows.

## 3. Preview Rendering

- [x] 3.1 Add or adapt a preview surface for raw monospace bookmark excerpts with stable sizing, internal scrolling/clipping, line breaks preserved, and the bookmarked line visually emphasized.
- [x] 3.2 Render Markdown-like bookmark excerpts through `LushtextMarkdownPreview::render_markdown_with_context` using the bookmarked file path and source workspace roots.
- [x] 3.3 Render loading and unavailable bookmark states with explicit text that keeps the bookmark metadata visible.
- [x] 3.4 Ensure switching among Markdown bookmark previews, raw bookmark previews, workspace-note previews, and document-note previews clears stale content from the previous preview mode.
- [x] 3.5 Keep the populated notes-browser dialog allocation stable across bookmark excerpt states and selection changes.

## 4. Widget And Integration Coverage

- [x] 4.1 Add widget coverage for selecting a Markdown bookmark and seeing rendered excerpt content from around the bookmarked line.
- [x] 4.2 Add widget coverage for selecting a non-Markdown bookmark and seeing raw monospace context with the target line emphasized.
- [x] 4.3 Add widget coverage proving open-editor bookmark previews use live buffer text rather than stale disk content.
- [x] 4.4 Add widget or service coverage for closed-file loading states and unavailable states such as missing, binary/non-UTF-8, too-large, or beyond-budget files.
- [x] 4.5 Add widget coverage proving fast selection changes ignore stale closed-file excerpt completions.
- [x] 4.6 Add widget coverage proving browser search remains metadata-oriented and does not require closed-file excerpt reads.
- [x] 4.7 Keep or update existing notes-browser tests for sectioned sidebars, Open action behavior, layout stability, and close-button behavior.

## 5. Validation

- [x] 5.1 Run `cargo fmt --check`.
- [x] 5.2 Run `cargo check -p lushtext-core -p lushtext`.
- [x] 5.3 Run service/unit tests covering bookmark excerpt extraction.
- [x] 5.4 Run targeted headless widget tests for notes-browser bookmark excerpt behavior.
- [x] 5.5 Run `cargo clippy -p lushtext-core -p lushtext --all-targets -- -D warnings`.
- [x] 5.6 Run `openspec validate preview-bookmark-context-excerpts --strict`.
- [x] 5.7 Run `git diff --check`.
- [x] 5.8 Confirm `openspec status --change preview-bookmark-context-excerpts --json` reports all tasks complete after implementation.
