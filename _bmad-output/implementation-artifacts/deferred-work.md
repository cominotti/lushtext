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

## Deferred from: code review of 1-5-match-navigation-progress-reporting (2026-04-08)

- **500ms timer accumulation across rapid keystrokes** — stale `timeout_add_local_once` closures from rapid typing can set `show_progress=true` prematurely for a new search. The spec prescribes this pattern without a generation counter; adding one would be a minor improvement but not critical.
- **Progress delay not reset on toggle-button search triggers** — toggling case/regex/whole-word re-triggers search via option `notify::active` handlers, but `connect_search_changed` doesn't fire so the 500ms delay flag is never reset. Could cause progress to show immediately for toggle-triggered searches.
- **Duplicate (path, line) selects first matching row** — when a file has multiple matches on the same line (common with minified code), `select_match_in_results()` always highlights the first visual row in the model. Navigation index advances correctly but the visual highlight doesn't distinguish same-line matches.
- **`set_restore_position` scroll context for matches in first 3 lines** — `line_0.saturating_sub(3)` evaluates to 0 for lines 1-3, pinning the viewport to the top. Pre-existing pattern from the open_file_at_line extraction (originally in Story 1.2's connect_open_file).

## Deferred from: code review of 2-1-replace-all-with-preview-execution-undo (2026-04-08)

- **flush() without sync_all() in atomic_write** — `atomic_write()` calls `flush()` but not `sync_all()` before `rename()`. On power failure or kernel panic, the temp file may be renamed but contain incomplete data. Pre-existing project pattern (json_store::save has the same issue). Both locations should eventually use `fsync()` for crash-safe writes.

## Deferred from: code review of 3-1-search-history (2026-04-08)

- **Mixed line ending normalization** — `detect_line_ending()` returns one ending for the whole file; `str::lines()` strips both `\r\n` and `\n`; `join()` normalizes all endings; files with mixed endings are silently changed. Story 2.1 scope.
- **Undo backup memory unbounded** — `undo_backup` stores full raw bytes of every replaced file. Replace All across thousands of large files could consume hundreds of MB. Needs a design decision on memory limits. Story 2.1 scope.
- **Overlapping regex matches on same line** — rightmost-first sort handles non-overlapping same-line matches correctly, but overlapping regex ranges (possible in theory) cause stale byte offset corruption after the first mutation. Extremely edge-case. Story 2.1 scope.
- **Regex preview captures on extracted slice vs full-line** — `re.captures(&original_line[start..end])` applies regex to just the match range, which may behave differently for patterns with anchors or lookaround. Fallback handles gracefully (logs warning, keeps original). Story 2.1 scope.
- **Blocking `fs::metadata` in `reload_affected_tabs`** — calls `std::fs::metadata()` synchronously on the main thread for each affected tab. Negligible for local disk but could block on NFS/USB with many affected files. Story 2.1 scope.

## Deferred from: code review of 3-2-saved-searches-panel-state-persistence (2026-04-08)

- **`atomic_write` temp file name collision** — `atomic_write()` uses `.{filename}.replace-tmp` as the fixed temp path. Concurrent Replace All or undo operations targeting the same file will race on the same temp file. Low probability (guarded at UI level) but the function itself is not concurrency-safe. Story 2.1 scope.
- **`render_preview_markup` multi-byte highlight inaccuracy** — The byte-length arithmetic for computing the replacement highlight region in preview rows doesn't account for UTF-8 character alignment differences between original and replaced text. `ceil_char_boundary` prevents panics but the visual highlight may be off by a character for multi-byte replacements. Story 2.1 scope.
- **Navigation index stale after Replace All** — After `apply_replacements()` writes new file content, `match_positions` still holds pre-replacement `(path, line_number)` pairs. If replacements shift line numbers (e.g., multi-line replacements), F4/Shift+F4 navigates to wrong lines until the next search clears the index. Story 1.5/2.1 scope.
- **No guard against double Replace All** — The `connect_replace_all` callback uses `spawn_blocking_then` but no flag prevents the user from clicking Replace All again while the first is in-flight. The TOCTOU guard catches stale lines (no data corruption), but the second Replace All's empty backup replaces the first's, losing undo capability. Story 2.1 scope.
- **CLAUDE.md `search-panel-position` GSettings key** — The CLAUDE.md Content search panel section references `search-panel-position (i)` as a GSettings key, but this key may not exist in the schema XML. Documentation-code inconsistency. Pre-existing.

## Deferred from: code review of search-panel-ui-polish (2026-04-08)

- **Nested revealer animation interaction not tested** — The outer `search_panel_revealer` (slide-down, 250ms) and inner `options_revealer` (slide-down, 150ms) can animate simultaneously when "More Options" is toggled while the panel is already open. No test covers this concurrent animation path.

## Spec 4: Draft Deletion Safety

Both `wire_info_bar` discard and hamburger-menu `discard_changes` delete the draft file *before* `load_file_async` succeeds. If the backing file is deleted between confirmation and reload, the user's unsaved changes (stored in the draft) are permanently lost with no recovery path. The draft should only be deleted after a successful reload — or at minimum, the draft content should be kept until reload success is confirmed. This affects `wire_info_bar` (existing code) and `discard_changes` (new code).
