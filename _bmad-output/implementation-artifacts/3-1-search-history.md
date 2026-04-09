# Story 3.1: Search History

Status: done

## Story

As a user,
I want my recent searches automatically remembered with their toggle settings and glob filter, and accessible via a dropdown on the search input,
So that I can quickly re-run searches I've done before without retyping or reconfiguring.

## Acceptance Criteria

1. **History entry created on search completion** — Given a search completes (`SearchEvent::Done` received with at least one result), when the search finishes, then a `SearchHistoryEntry` is created containing the query text, toggle states (regex, case-sensitive, whole-word, gitignore), and glob filter value, and the entry is prepended to `$XDG_DATA_HOME/lushtext/search-history.json` via `json_store` atomic write.

2. **History capped at 20** — Given the search history contains 20 entries, when a new search completes, then the oldest entry is removed before the new entry is prepended (FIFO, capped at 20).

3. **Duplicate deduplication** — Given the search history contains an entry with identical query and settings, when a matching search completes, then the existing entry is moved to the top of the history (not duplicated).

4. **Dropdown on focus** — Given the search input receives focus and search history entries exist, when the input is focused, then a dropdown (GtkPopover) appears below the search input showing recent searches, and each entry displays the query text and a summary of active toggles/glob.

5. **History selection restores state** — Given the history dropdown is visible, when the user selects an entry, then the search input is populated with the saved query, and all toggle buttons are restored to the saved states (regex, case, word, gitignore), and the glob filter entry is restored to the saved value, and a search runs immediately with the restored settings (bypassing the 300ms debounce).

6. **Typing dismisses dropdown** — Given the history dropdown is visible, when the user starts typing in the search input, then the dropdown closes and normal search-as-you-type behavior resumes.

7. **History persists across restarts** — Given the application is restarted, when the search panel is opened and the search input receives focus, then the history dropdown shows entries loaded from `search-history.json`.

8. **Corrupted/missing history is graceful** — Given `search-history.json` is corrupted or missing, when history is loaded, then an empty history is used (no error, no crash) and the file is recreated on next save.

## Tasks / Subtasks

- [x] Task 1: Update `SearchHistoryEntry` model type (AC: #1)
  - [x] In `model/content_search.rs`: add `gitignore: bool` field to `SearchHistoryEntry` (currently missing — the struct has `query`, `case_sensitive`, `regex`, `whole_word`, `glob` but not `gitignore`)
  - [x] Update `"(used in later stories)"` doc comment to a real doc comment describing the type

- [x] Task 2: Create `services/search_history.rs` (AC: #1, #2, #3, #8)
  - [x] New file `services/search_history.rs` — search history persistence logic
  - [x] Add `pub mod search_history;` to `services/mod.rs`
  - [x] Implement `pub fn load(data_dir: &Path) -> Vec<SearchHistoryEntry>`:
    - Delegate to `json_store::load::<Vec<SearchHistoryEntry>>(data_dir, "search-history.json")`
    - On parse error, log warning and return empty vec (AC #8)
  - [x] Implement `pub fn save(data_dir: &Path, entries: &[SearchHistoryEntry]) -> anyhow::Result<()>`:
    - Delegate to `json_store::save(data_dir, "search-history.json", &entries)`
  - [x] Implement `pub fn add_entry(entries: &mut Vec<SearchHistoryEntry>, entry: SearchHistoryEntry)`:
    - Check for duplicate: same `query`, `case_sensitive`, `regex`, `whole_word`, `gitignore`, and `glob` — if found, remove existing (AC #3)
    - Prepend new entry
    - If length > 20, truncate (AC #2)
  - [x] Unit tests:
    - `test_add_entry_prepends` — new entry goes to front
    - `test_add_entry_caps_at_20` — 21st entry removes oldest
    - `test_add_entry_deduplicates` — identical entry moves to top, no duplicates
    - `test_add_entry_different_settings_not_dedup` — same query with different toggles kept separate
    - `test_load_missing_file_returns_empty` — missing file returns empty vec
    - `test_save_and_load_roundtrip` — save then load produces same data

- [x] Task 3: Add history dropdown UI to search panel (AC: #4, #6)
  - [x] In `resources/ui/search-panel.ui`: add `GtkPopover id="history_popover"` attached to `search_entry`, containing a `GtkScrolledWindow` with `GtkListBox id="history_list"`:
    - `GtkPopover`: `autohide=true`, `has-arrow=false`, positioned below the entry
    - `GtkScrolledWindow`: `max-content-height=300`, `propagate-natural-height=true`
    - `GtkListBox`: `selection-mode=single`, `activate-on-single-click=true`
  - [x] In `search_panel/imp.rs`: add `TemplateChild` fields: `history_popover`, `history_list`
  - [x] In `search_panel/imp.rs`: add state field: `history_entries: RefCell<Vec<SearchHistoryEntry>>`
  - [x] In `search_panel/imp.rs` `constructed()`:
    - Connect `search_entry` `notify::has-focus` signal: when focus is gained and `history_entries` is non-empty, call `populate_history_list()` then `history_popover.popup()`
    - Connect `search_entry` `search-changed` signal (or `changed`): if `history_popover.is_visible()`, call `history_popover.popdown()` (AC #6)
    - Connect `history_list` `row-activated`: extract the `SearchHistoryEntry` from the row, restore state, close popover (AC #5 — wired in Task 4)
  - [x] Method `populate_history_list(&self)`: clear the `history_list`, iterate `history_entries`, create an `AdwActionRow` per entry:
    - Title: query text (truncated at ~60 chars with ellipsis if long)
    - Subtitle: toggle summary string — e.g., `"Aa .*  *.rs"` where `Aa` = case-sensitive, `.*` = regex, `W` = whole-word, plus the glob if set
    - Use `GtkListBox::append` (max 20 rows, no recycling needed)

- [x] Task 4: Wire history selection to restore state and trigger search (AC: #5)
  - [x] In `search_panel/mod.rs`: add `pub fn restore_from_history(&self, entry: &SearchHistoryEntry)`:
    - Set `search_entry.set_text(&entry.query)` — temporarily suppress `search-changed` handler to avoid double-search (use a `restoring_history: Cell<bool>` guard on imp)
    - Set toggle buttons: `case_toggle.set_active(entry.case_sensitive)`, `regex_toggle.set_active(entry.regex)`, `word_toggle.set_active(entry.whole_word)`, `gitignore_toggle.set_active(entry.gitignore)`
    - Set `glob_entry.set_text(entry.glob.as_deref().unwrap_or(""))` — also suppress the glob debounce
    - Close `history_popover.popdown()`
    - Trigger search immediately: call the existing search trigger method directly (bypassing debounce — this is a deliberate user action, not typing)
  - [x] In `search_panel/imp.rs` `constructed()` `row-activated` handler:
    - Extract the entry index from `GtkListBoxRow::index()`
    - Look up `SearchHistoryEntry` from `history_entries` by index
    - Call `self.obj().restore_from_history(&entry)`

- [x] Task 5: Wire search completion to save history (AC: #1, #2, #3)
  - [x] In `search_panel/mod.rs`: add `pub fn set_search_history(&self, entries: Vec<SearchHistoryEntry>)` — stores into `imp.history_entries`
  - [x] In `search_panel/mod.rs`: add `pub fn search_history(&self) -> Vec<SearchHistoryEntry>` — clones from `imp.history_entries`
  - [x] In `search_panel/imp.rs` — in the polling timer's `SearchEvent::Done` handling:
    - Build a `SearchHistoryEntry` from the current query + toggle + glob state
    - Only save if query is non-empty AND `total_matches > 0` (AC #1: "at least one result")
    - Call `search_history::add_entry(&mut entries, new_entry)` on `imp.history_entries`
    - Save to disk via `spawn_blocking_then(panel, move || search_history::save(&data_dir, &entries_clone), |panel, result| { if let Err(e) = result { tracing::error!("Failed to save search history: {e}"); } })`
    - Use `json_store::data_dir()` for the path

- [x] Task 6: Load history at startup (AC: #7, #8)
  - [x] In `window/search.rs` `setup_search_panel()` (or at end of the function):
    - Load history via `spawn_blocking_then(window, move || search_history::load(&data_dir), |window, entries| { window.imp().search_panel.set_search_history(entries); })`
    - The `load` function already handles missing/corrupt files by returning empty vec (AC #8)

- [x] Task 7: Widget tests (all ACs)
  - [x] Test: `history_popover` and `history_list` exist as template children on fresh panel
  - [x] Test: `set_search_history` stores entries, `search_history()` returns them
  - [x] Test: `restore_from_history` sets search entry text to history entry's query
  - [x] Test: `restore_from_history` sets toggle buttons to history entry's toggle states
  - [x] Test: `restore_from_history` sets glob entry to history entry's glob value
  - [x] Test: `restore_from_history` with `glob: None` clears the glob entry
  - [x] Test: `SearchHistoryEntry` model with all fields can be serialized/deserialized (roundtrip)
  - [x] Test: existing Story 1.1–2.1 tests still pass (no regressions)

- [x] Task 8: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all unit tests pass (220 passed)
  - [x] Run `make test-int` — all integration tests pass (52 passed)
  - [x] Run `make test-widget` — all widget tests pass (401 passed)
  - [x] Verify no GTK/pixman runtime warnings via `make run` and exercising:
    - Open search panel (Ctrl+Shift+F)
    - Type a query, wait for results
    - Click away then click back to search input — history dropdown appears
    - Select a history entry — toggles and glob are restored, search runs immediately
    - Type in search input with dropdown visible — dropdown closes
    - Close and reopen the app — history persists
  - [x] Update README.md with search history feature
  - [x] Update AGENTS.md with search history design decision
  - [x] Run `cargo hakari generate` if any dependencies changed (no changes needed)

## Dev Notes

### Search History Flow Overview

This story adds automatic search history with a browser-URL-bar-style dropdown. The flow:

1. **Search completes** (`SearchEvent::Done` with results > 0) → `SearchHistoryEntry` created from current state
2. **Entry added** → deduplication, prepend to front, cap at 20
3. **Persisted** → `json_store::save` to `search-history.json` via `spawn_blocking_then`
4. **On focus** → dropdown popover shows recent entries with toggle summaries
5. **On select** → state restored (query, toggles, glob), search triggered immediately

### SearchHistoryEntry Already Exists — Needs Update

The `SearchHistoryEntry` struct exists at `model/content_search.rs:169-176` as a forward-ported placeholder from Story 1.1. It already has `query`, `case_sensitive`, `regex`, `whole_word`, and `glob` fields — but is **missing `gitignore: bool`**. Add this field. The architecture spec mentions `.gitignore` toggle as part of the saved state (FR28: "toggle settings").

Also update the doc comment from `"(used in later stories)"` to a proper description.

### New Service File: `services/search_history.rs`

Search history persistence is conceptually separate from search execution (`services/content_search.rs`). `content_search.rs` is already at 432 prod lines — adding history logic there would be feasible but clutters the search execution code.

Create a small `services/search_history.rs` (~60 prod lines + ~80 test lines) that wraps `json_store` with history-specific logic (dedup, cap, load/save). This follows the project pattern: `session_service.rs`, `draft_service.rs`, `workspace_manager.rs` are all separate service files for separate persistence domains.

### Dropdown Widget: GtkPopover + GtkListBox

For max 20 entries, `GtkListBox` is simpler and more appropriate than `GtkListView`:
- No need for widget recycling at this scale
- `GtkListBox` supports `activate-on-single-click` and `row-activated` signal natively
- Row contents created with `AdwActionRow` (title: query, subtitle: toggle summary)
- `GtkScrolledWindow` with `max-content-height=300` caps the dropdown height

The popover is attached to `search_entry` and positioned below. `autohide=true` dismisses on outside clicks. The `search-changed` signal handler explicitly calls `popdown()` when the user starts typing.

### Toggle Summary String

Each history entry displays a compact summary of active toggles:
- `"Aa"` if case_sensitive
- `".*"` if regex
- `"W"` if whole_word
- Glob value if set (e.g., `"*.rs"`)
- These are joined with spaces: `"Aa .* *.rs"`
- If no toggles active and no glob: subtitle is empty or shows just the gitignore state

### Restoring State: Guard Against Double-Search

Setting `search_entry.set_text()` fires the `search-changed` signal, which starts a debounced search. Setting toggle buttons fires `notify::active`, which also triggers search. To avoid multiple redundant searches during history restore:

1. Add `restoring_history: Cell<bool>` guard to imp struct
2. Set it to `true` before restoring state
3. All search trigger paths check this guard and skip if true
4. After all state is set, clear the guard and trigger ONE search directly (bypassing debounce)

This is similar to the existing `restoring_session` pattern on `LushtextWindow` and the `constructed_complete` guard on the search panel.

### History Save: Background Thread via spawn_blocking_then

History save happens on search completion (`SearchEvent::Done` handler in the polling timer). The save uses `spawn_blocking_then` because:
- It's a single-result operation (not streaming)
- It respects the concurrency guard (MAX_CONCURRENT_SPAWNS = 8)
- The callback pattern handles errors on the main thread

The `data_dir` is obtained via `json_store::data_dir()`.

### History Load: At Startup

History is loaded once at startup in `window/search.rs::setup_search_panel()` via `spawn_blocking_then`. This follows the pattern of session and draft loading. The loaded entries are passed to the search panel via `set_search_history()`. If the file is missing or corrupt, `json_store::load` returns `Default::default()` (empty vec) — no error, no crash.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `crates/lushtext-core/src/model/content_search.rs` | Add `gitignore` field to `SearchHistoryEntry`, update doc comment | +3 lines |
| `crates/lushtext-core/src/services/mod.rs` | Add `pub mod search_history;` | +1 line |
| `crates/lushtext-core/src/services/search_history.rs` | **NEW** — load, save, add_entry + unit tests | ~60 prod, ~80 test |
| `resources/ui/search-panel.ui` | Add `history_popover` + `history_list` inside popover | +20 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add template children, state fields, focus/typing/row-activated handlers | +40 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Add `set_search_history`, `search_history`, `restore_from_history`, `populate_history_list`, history save on Done | +100 lines |
| `crates/lushtext-core/src/ui/window/search.rs` | Load history at startup | +15 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Story 3.1 widget tests | +60 lines |
| `.agents/AGENTS.md` | Document search history design decision | +10 lines |
| `README.md` | Update features list with search history | +3 lines |

### Line Count Impact

| File | Current Lines | After Story 3.1 | Limit |
|------|--------------|-----------------|-------|
| `model/content_search.rs` | 187 | ~190 | 1000 |
| `services/search_history.rs` | 0 (new) | ~60 prod / ~80 test | 1000 |
| `search-panel.ui` | ~164 | ~184 | N/A (XML) |
| `search_panel/imp.rs` | 831 | ~871 | 1000 |
| `search_panel/mod.rs` | 623 | ~723 | 1000 |
| `window/search.rs` | 386 | ~401 | 1000 |

All files remain within the 1000-line production limit.

### Previous Story Intelligence

**From Story 2.1 (most recent, same search panel):**
- `replace_callback` / `undo_callback` use `RefCell<Option<Box<dyn Fn(...)>>>` — reuse pattern for history-related callbacks if needed
- `clear_results()` is the central state reset — history should NOT be cleared here (history persists across searches)
- `constructed_complete: Cell<bool>` guard prevents signal handlers firing during widget construction — use the same pattern for `restoring_history` guard
- `update_replace_button_sensitivity()` is called from multiple places — ensure history restore doesn't break replace button state
- `preview_mode` check: history dropdown should be suppressed during preview mode
- `gen` is a reserved keyword in Rust Edition 2024 — do not use as a variable name

**From Story 1.5:**
- `progress_callback` pattern — history doesn't need a callback, but the `SearchEvent::Done` handling in the polling timer is where history save should be triggered
- The Done handler at the end of the polling timer is the right place to add history entry creation

**From Story 1.4:**
- `constructed_complete` guard pattern — reuse for `restoring_history` guard
- `options_revealer` GSettings binding fires during construction — same concern for any toggle state restoration

**From Story 1.2:**
- `clear_results()` resets all search state — DO NOT add history clearing here
- The `RefCell` borrow pattern: clone+drop before signal emission

### Anti-Patterns to Avoid

1. **DO NOT** clear search history in `clear_results()` — history persists across searches, unlike search results.
2. **DO NOT** save history on every keystroke — only on `SearchEvent::Done` when `total_matches > 0`.
3. **DO NOT** use `GtkListView` for the history dropdown — `GtkListBox` is simpler and sufficient for max 20 items.
4. **DO NOT** use `GtkEntryCompletion` — it's deprecated since GTK 4.10 and removed in 4.14.
5. **DO NOT** set `search_entry.set_text()` without a guard — it fires `search-changed`, causing redundant searches during history restore.
6. **DO NOT** add saved search features — those belong to Story 3.2.
7. **DO NOT** use `gen` as a variable name — reserved keyword in Rust Edition 2024.
8. **DO NOT** show the history dropdown during preview mode — the user is in the middle of a Replace All flow.
9. **DO NOT** load history synchronously on the main thread — use `spawn_blocking_then` for the file read.
10. **DO NOT** save history synchronously — use `spawn_blocking_then` for the file write.
11. **DO NOT** put history persistence logic in `services/content_search.rs` — it's already at 432 prod lines and focused on search execution. Use a separate `services/search_history.rs`.
12. **DO NOT** modify the `SearchHistoryEntry` Serialize/Deserialize derivation — it already has serde derives. Just add the new field.

### Deferred Work That Affects This Story

From `deferred-work.md`:
- **Panel visible on startup with empty results** — Panel visibility is persisted via GSettings but query text and results are not. On restart, the panel shows empty. This is pre-existing; history helps surface previous queries but doesn't fix this (that's a Story 3.2 concern with panel state persistence).
- **`searching` flag not reset on empty query** — Pre-existing latent state bug. Not directly affected by this story but be aware when adding the Done handler logic.

### Scope Boundary

This story implements **search history only** (FR28, FR29). The following belong to **Story 3.2**:
- Saved/named searches (FR30, FR31)
- Panel state persistence beyond what GSettings already provides
- "Save Search" action or button
- Delete action for saved searches
- Separate saved searches section in the dropdown

The history dropdown should be designed to accommodate a future "Saved Searches" section below the "Recent" section (Story 3.2), but DO NOT implement it.

### Project Structure Notes

- One new file created: `services/search_history.rs`
- All other changes modify existing files
- The search panel grows by ~140 lines across two files — well within the 1000-line limit
- The model grows by ~3 lines — trivial

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.1: Search History]
- [Source: _bmad-output/planning-artifacts/architecture.md#Decision 6] — Two separate files: search-history.json (capped at 20) + saved-searches.json (permanent)
- [Source: _bmad-output/planning-artifacts/architecture.md#Data Architecture] — search-history.json lifecycle and persistence
- [Source: _bmad-output/planning-artifacts/prd.md#FR28] — History of recent search queries with settings
- [Source: _bmad-output/planning-artifacts/prd.md#FR29] — Select from history to re-execute with saved settings
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Journey Flow 4] — Search History and Saved Searches interaction flow
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Key interaction details] — History is automatic, dropdown on focus, full state restoration
- [Source: _bmad-output/implementation-artifacts/2-1-replace-all-with-preview-execution-undo.md] — Previous story learnings (guard patterns, preview mode, clear_results)
- [Source: .agents/AGENTS.md#Content search panel] — Current search panel architecture
- [Source: .agents/AGENTS.md#Async I/O Pattern] — spawn_blocking_then for save/load
- [Source: .agents/rules/rust.md#Background I/O] — Threading model
- [Source: .agents/rules/rust.md#Mutable State] — Cell/RefCell conventions
- [Source: .agents/rules/widget-wiring.md#Testing] — Widget test requirements
- [Source: .agents/rules/ui.md#GSettings Bindings] — Persistence patterns
- [Source: crates/lushtext-core/src/model/content_search.rs:169-176] — Existing SearchHistoryEntry (missing gitignore)
- [Source: crates/lushtext-core/src/services/json_store.rs] — json_store load/save pattern for reuse

### Review Findings

- [x] [Review][Patch] `replace_range` may panic on non-char-boundary byte offsets — FIXED: added `floor_char_boundary`/`ceil_char_boundary` guards in `apply_replacements()` and `generate_replacement_preview()`
- [x] [Review][Patch] `atomic_write` leaves orphaned temp file on write error — FIXED: added `remove_file` cleanup on write/flush/rename error paths
- [x] [Review][Defer] Mixed line ending normalization in `detect_line_ending` — `str::lines()` strips both `\r\n` and `\n`, `join()` normalizes all endings; files with mixed endings silently changed — deferred, Story 2.1 scope [services/content_search.rs:366-368]
- [x] [Review][Defer] Undo backup memory unbounded — `undo_backup` stores full raw bytes of every replaced file; Replace All across thousands of large files could consume hundreds of MB — deferred, Story 2.1 design decision [search_panel/imp.rs:116]
- [x] [Review][Defer] Overlapping regex matches on same line — rightmost-first sort handles non-overlapping correctly but overlapping regex ranges cause stale offset corruption — deferred, extremely edge-case, Story 2.1 [services/content_search.rs:277-325]
- [x] [Review][Defer] Regex preview captures on extracted slice vs full-line — `re.captures(&original_line[start..end])` may behave differently for patterns with anchors/lookaround; fallback handles gracefully — deferred, Story 2.1 [model/content_search.rs:133-147]
- [x] [Review][Defer] Blocking `fs::metadata` in `reload_affected_tabs` — synchronous on main thread; negligible for local disk, could block on NFS/USB with many files — deferred, Story 2.1 [window/search.rs:307-330]

## Dev Agent Record

### Agent Model Used
Claude Opus 4.6 (1M context)

### Debug Log References
- None required — clean implementation with no debugging issues.

### Completion Notes List
- Task 1: Added `gitignore: bool` field and `PartialEq` derive to `SearchHistoryEntry`. Updated doc comment.
- Task 2: Created `services/search_history.rs` with `load()`, `save()`, `add_entry()` functions and 6 unit tests. All tests pass.
- Task 3: Created `GtkPopover` + `GtkListBox` programmatically (not via template — popovers need `set_parent()`, not box child semantics). Added `history_entries: RefCell`, `restoring_history: Cell<bool>` guard, `setup_history()` for focus/row-activated wiring, `popdown()` on typing.
- Task 4: Implemented `restore_from_history()` with restoring_history guard suppressing all search triggers, then one explicit `start_search()` bypassing debounce. Also implemented `populate_history_list()` with `AdwActionRow` (title: query, subtitle: toggle summary).
- Task 5: Wired history save into `SearchEvent::Done` handler in `start_search()` polling timer. Only saves when `total_matches > 0` and query non-empty. Uses `spawn_blocking_then` for background I/O.
- Task 6: Added `spawn_blocking_then` history load at end of `window/search.rs::setup_search_panel()`.
- Task 7: 7 new widget tests covering popover/list existence, set/get history, restore_from_history (text, toggles, glob, glob=None), and serialization roundtrip.
- Task 8: `make check` passes (clippy + fmt). All 220 unit, 52 integration, 401 widget tests pass. Updated README.md and AGENTS.md. `cargo hakari generate` — no changes needed.

### Change Log
- 2026-04-08: Implemented Story 3.1 — search history with full state recall, dropdown on focus, persistence across restarts.

### File List
- `crates/lushtext-core/src/model/content_search.rs` — modified (added `gitignore` field, `PartialEq` derive, updated doc comment)
- `crates/lushtext-core/src/services/mod.rs` — modified (added `pub mod search_history`)
- `crates/lushtext-core/src/services/search_history.rs` — **new** (load/save/add_entry + 6 unit tests)
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — modified (history_popover, history_list fields, restoring_history guard, setup_history, popover creation in constructed, unparent in dispose, guard additions to all search triggers)
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — modified (set_search_history, search_history, populate_history_list, restore_from_history, build_toggle_summary, history save in Done handler)
- `crates/lushtext-core/src/ui/window/search.rs` — modified (history load at startup via spawn_blocking_then)
- `crates/lushtext/tests/widget/search_panel.rs` — modified (7 new Story 3.1 widget tests)
- `crates/lushtext/Cargo.toml` — modified (added serde_json dev-dependency)
- `.agents/AGENTS.md` — modified (module layout, search history design decision)
- `README.md` — modified (features list, module layout)
