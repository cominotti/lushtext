# Deferred Work — Hamburger Menu Feature

## Spec 2: Zoom Controls

Custom zoom widget in the hamburger menu matching GNOME Text Editor's zoom section. Three controls: zoom in (+), zoom out (−), reset (percentage label showing current level). Implementation requires:
- Custom widget via `PopoverMenu::add_child("zoom")`
- Font size scaling via CSS provider (relative to base font size)
- GSettings key for persisted zoom level
- Keyboard shortcuts: Ctrl+Plus, Ctrl+Minus, Ctrl+0
- Zoom range: 50%–400% (matching GNOME Text Editor)
- Command palette entries for zoom actions

## Spec 3: Print + Discard Changes

Two new per-page actions completing the GNOME Text Editor menu parity:

**Print (Ctrl+P):**
- `GtkPrintOperation` integration for the active editor's buffer
- Note: Ctrl+P currently used for command palette — need to resolve shortcut conflict (move palette to Ctrl+Shift+P or keep Print without shortcut)
- Menu item in Find/Replace section (after Find/Replace, before Fullscreen)

**Discard Changes:** ✅ Implemented in spec-discard-changes.md

## File Index: Incremental rename vs skip list inconsistency

When a directory is renamed to/from an ignored name (e.g., `src` → `target`) via sidebar inline rename, the incremental `rename_path()` method rewrites child paths but does not consult `IGNORED_INDEX_DIRS`. Files under the renamed-to-ignored directory remain searchable until the next full rebuild. Conversely, renaming away from an ignored name does not add children. Self-corrects on next workspace change or app restart. Low priority — renaming directories to well-known build names is extremely rare.

## Deferred from: code review of 1-1-content-search-service-types (2026-04-07)

- **Symlinked files silently skipped** — Walker uses `follow_links=false` (ignore crate default). Monorepos with symlinked sources will miss matches. Consider adding a `follow_symlinks` option to `ContentSearchOptions` in a future story.
- **Non-existent root path produces no error** — When roots contain a non-existent path, the walker emits an error entry that is silently consumed (WalkState::Continue). No `SearchEvent::Error` is sent. Low priority since roots come from validated workspace config.

## Deferred from: code review of 1-2-search-panel-with-streaming-results (2026-04-07)

- **O(n) reverse scan for auto-expand** [mod.rs:195-205] — After appending a new file group to root_store, the code reverse-scans the entire TreeListModel to find the corresponding TreeListRow for auto-expansion. With many file groups, this approaches O(n²) on the main thread. Consider using `root_store.n_items()-1` position mapping or a direct lookup.
- **"No results found" not centered in results area** [mod.rs:248] — The spec envisions a centered empty-state message (like AdwStatusPage) in the results scroll area. The current implementation uses the footer count_label. Cosmetic improvement for a future polish pass.
- **window/mod.rs pre-existing over 1000-line limit** — At 1091 lines (1084 before this story + 7 net lines). The search.rs extraction was correct but the file was already over the limit. Further extraction opportunities exist.

## Deferred from: code review of 1-3-search-toggles-match-highlighting (2026-04-07)

- **Polling timer continues when panel hidden** [search_panel/mod.rs:26-28] — 50ms polling timer processes results and updates UI widgets inside a hidden GtkRevealer. Wastes main-thread CPU. Consider cancelling or pausing the timer when the panel is hidden.
- **RefCell borrow in file_groups fragile near signal emission** [search_panel/mod.rs:173-192] — The clone+drop pattern correctly prevents RefCell panics, but the scope boundary is easy to accidentally extend. Consider restructuring to a block scope for implicit drop.
- **`display().to_string()` lossy path comparison** [search_panel/mod.rs:229-230] — Auto-expand comparison uses lossy string representation from `Path::display()`. Non-UTF-8 paths could mismatch. Consider comparing via PathBuf directly.
- **Toggle action name misleading** [window/search.rs:93-103] — `toggle-search-panel` (Ctrl+Shift+F) refocuses and selects text instead of closing. Consider adding true toggle behavior or renaming to `focus-search-panel`.
- **Panel visible on startup with empty results** [window/search.rs:83-87] — Panel visibility is persisted via GSettings but query text and results are not. On restart, the panel shows empty. Consider persisting query text or not persisting visibility.
- **`searching` flag not reset on empty query** [search_panel/mod.rs:105] — `clear_results()` does not reset `searching` to false. When user clears query text, the flag remains stale. Currently harmless (no timer reads it) but latent state bug.

## Deferred from: code review of 1-4-gitignore-toggle-glob-filter-options-panel (2026-04-07)

- **Premature model types** — `Replacement`, `ReplaceResult`, `SearchHistoryEntry`, `SavedSearch` in `model/content_search.rs:68-103` are defined but unused (forward-ported from story 1-1 spec). Remove or revise when later stories actually need them.
- **Single-slot `connect_workspace_changed` callback fragile** — Sidebar uses `Option<Box<dyn Fn()>>` single-slot callback. The combined callback in `window/search.rs:77-86` replaces the one in `window/imp.rs:651`. Any future code adding another callback will silently overwrite. Consider migrating to a signal or callback list.
- **Search threads bypass MAX_CONCURRENT_SPAWNS** — `search_panel/mod.rs:148` uses raw `std::thread::spawn` (streaming pattern doesn't fit `spawn_blocking_then`). The search thread plus ignore's parallel walker can consume up to 9 threads. Rapid toggle changes spawn immediate re-searches without debounce, potentially accumulating threads.
- **`find_match_range` only highlights first match per line** — `services/content_search.rs:164` uses `find_at(line, 0)` returning only the first match. Lines with multiple matches show partial highlighting.
- **`OverrideBuilder::new(roots[0])` glob for multi-root** — `services/content_search.rs:100` anchors the override builder to the first root. Path-anchored globs (e.g., `src/*.rs`) may not match files under other workspace roots.
- **Multiline selection pre-fill** — `window/search.rs:107-112` pre-fills the search query with the full multiline selection. In literal mode, newlines won't match line-oriented search, producing silent no-results.

## Spec 4: Draft Deletion Safety

Both `wire_info_bar` discard and hamburger-menu `discard_changes` delete the draft file *before* `load_file_async` succeeds. If the backing file is deleted between confirmation and reload, the user's unsaved changes (stored in the draft) are permanently lost with no recovery path. The draft should only be deleted after a successful reload — or at minimum, the draft content should be kept until reload success is confirmed. This affects `wire_info_bar` (existing code) and `discard_changes` (new code).
