# Workspace-Wide Content Search

## Status: Proposed

## Description
A full-text search mode in the command palette (or a dedicated panel) that searches
file contents across all workspace roots at ripgrep-like speed, with streaming results,
match highlighting, and click-to-open-at-line. Triggered via `Ctrl+Shift+F`.

## Current State
- Command palette (`Ctrl+P`) supports fuzzy file name search using nucleo (SIMD-accelerated)
- `FileIndex` scans workspace roots and maintains an in-memory file list
- No file content search exists — users must alt-tab to a terminal and use grep/ripgrep
- The in-editor search bar (`Ctrl+F`) only searches the current buffer

## Motivation
Content search is the #1 feature gap between lightweight editors and power-user tools.
GNOME Text Editor, Mousepad, and most GTK editors lack it entirely. Users resort to
terminal grep workflows or open VS Code "just for search." Doing this natively with
streaming results and match context would be the single biggest reason power users
choose LushText.

## Implementation Plan

### Phase 1: Search Service (services/content_search.rs)
1. Add `grep` or `ripgrep` as a library dependency — evaluate:
   - `grep-regex` + `grep-searcher` crates (ripgrep's internal libraries, pure Rust)
   - `ignore` crate for respecting `.gitignore` during traversal
2. `ContentSearchService` struct with `search(query, roots, options) -> Receiver<SearchMatch>`
3. `SearchMatch`: `{ path, line_number, line_content, match_range, context_before, context_after }`
4. Options: regex toggle, case sensitivity, whole word, file glob filter
5. Cancellation via `Arc<AtomicBool>` token (same pattern as file load cancellation)
6. Run on background thread via `spawn_blocking_then` or dedicated thread pool

### Phase 2: Search Results UI
**Option A: Command Palette Mode**
1. Add a `SearchMode::ContentSearch` variant to `model/palette.rs`
2. Prefix detection: typing `>` switches to command mode (existing), typing without
   prefix does file search (existing), a new prefix or toggle switches to content search
3. Results displayed as `filename:line — matched line content` with match highlighted
4. Selecting a result opens the file and jumps to the line

**Option B: Dedicated Search Panel (recommended)**
1. New `LushtextSearchPanel` widget as a bottom/side panel (toggleable via `Ctrl+Shift+F`)
2. Search input with regex/case/word toggle buttons (matching the editor search bar style)
3. Results in a `GtkListView` with `GtkTreeListModel` grouped by file
4. Each file group shows match count; each match shows line number + context
5. Click opens file at line; double-click opens and closes panel
6. "Replace All" across files as a stretch goal

### Phase 3: Performance
1. Use `ignore` crate's parallel directory walker for traversal
2. Memory-map large files instead of reading fully into memory
3. Stream results to the UI as they arrive (channel-based, with `idle_add_local` batching)
4. Cap results at 10,000 matches with a "too many results" indicator
5. Debounce search input at 300ms (same pattern as palette file search)

### Phase 4: Integration
1. Status bar shows search progress ("Searching 1,234 / 5,678 files...")
2. File glob filter integrates with workspace root awareness
3. Search results respect `.gitignore` by default (toggleable)
4. Keyboard navigation: `F4` / `Shift+F4` to cycle through matches across files

## Architecture Considerations
- The `grep-searcher` + `grep-regex` crates are ripgrep's actual internals, well-tested
  and fast. They handle binary file detection, encoding, and memory mapping.
- The `ignore` crate handles `.gitignore`, `.ignore`, and hidden file filtering —
  reusable for both content search and file index scanning.
- Results streaming must not flood the GTK main loop. Batch results (e.g., 50 at a time)
  via `idle_add_local` to keep the UI responsive during large searches.
- The dedicated panel approach (Option B) is recommended over command palette integration
  because content search results need more space (context lines, file grouping) than the
  palette's single-line format allows.

## Dependencies
- `grep-regex` + `grep-searcher` crates (ripgrep internals)
- `ignore` crate (gitignore-aware traversal)
- Existing `FileIndex` and workspace root infrastructure
- New UI widget (search panel) or command palette extension

## Risks
- Adding ripgrep's crate ecosystem is a significant dependency increase. The crates are
  well-maintained (by BurntSushi) but pull in `regex-automata`, `aho-corasick`, etc.
- Memory usage during search of very large workspaces needs caps and cancellation.
- "Replace All" across files is high-risk (data loss potential) and should be gated
  behind confirmation dialogs and backup mechanisms.
