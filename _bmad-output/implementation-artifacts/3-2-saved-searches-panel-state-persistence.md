# Story 3.2: Saved Searches & Panel State Persistence

Status: done

## Story

As a user,
I want to save frequently-used searches by name for permanent access and have the panel remember its state across restarts,
So that my search workflow is ready exactly where I left it when I reopen the application.

## Acceptance Criteria

1. **Save search dialog** — Given search results are displayed, when the user clicks a "Save Search" button in the search panel toolbar, then a dialog prompts for a search name (pre-filled with the query text), and on confirm a `SavedSearch` entry is written to `$XDG_DATA_HOME/lushtext/saved-searches.json` via `json_store` atomic write, containing the name, query, all toggle states (case, regex, word, gitignore), and glob filter.

2. **Saved searches in dropdown** — Given saved searches exist, when the search input receives focus and the dropdown appears, then saved searches are displayed in a separate "Saved Searches" section above the "Recent" history section, and each saved search shows its user-given name and query.

3. **Saved search selection restores state** — Given the dropdown is visible, when the user selects a saved search, then the search input, toggles, and glob filter are all restored from the saved entry, and a search runs immediately with the restored settings (bypassing debounce).

4. **Delete saved search** — Given a saved search exists in the dropdown, when the user clicks the delete button on a saved search row, then the entry is removed from `saved-searches.json` via atomic write and the dropdown refreshes.

5. **Panel state persistence (toggles)** — Given the search panel is visible with toggle states and expanded options, when the application is closed and reopened, then the panel visibility is restored from GSettings `search-panel-visible`, toggle states are restored from GSettings (`search-case-sensitive`, `search-regex`, `search-whole-word`, `search-gitignore`), and the options expanded state is restored from GSettings `search-panel-options-expanded`.

6. **Panel state persistence (hidden)** — Given the panel was hidden when the application was closed, when the application reopens, then the panel is hidden (GSettings `search-panel-visible` = false), and pressing `Ctrl+Shift+F` opens the panel with the last-used toggle states restored.

7. **Corrupted/missing saved-searches.json is graceful** — Given `saved-searches.json` is corrupted or missing, when saved searches are loaded, then an empty saved searches list is used (no error, no crash) and the file is recreated on next save.

## Tasks / Subtasks

- [x] Task 1: Fix `SavedSearch` model type (AC: #1)
  - [x] In `model/content_search.rs`: add `gitignore: bool` field to `SavedSearch` (aligning with `SearchHistoryEntry`)
  - [x] Update doc comment to describe the type properly (currently a stub placeholder from Story 1.1)

- [x] Task 2: Create `services/saved_searches.rs` (AC: #1, #4, #7)
  - [x] New file `services/saved_searches.rs` — saved search persistence logic
  - [x] Add `pub mod saved_searches;` to `services/mod.rs`
  - [x] Implement `pub fn load(data_dir: &Path) -> Vec<SavedSearch>`:
    - Delegate to `json_store::load::<Vec<SavedSearch>>(data_dir, "saved-searches.json")`
    - On parse error, log warning and return empty vec (AC #7)
  - [x] Implement `pub fn save(data_dir: &Path, entries: &[SavedSearch]) -> anyhow::Result<()>`:
    - Delegate to `json_store::save(data_dir, "saved-searches.json", &entries)`
  - [x] Implement `pub fn add(entries: &mut Vec<SavedSearch>, entry: SavedSearch)`:
    - Prepend new entry (no dedup — saved searches have user-chosen names, duplicates are allowed)
  - [x] Implement `pub fn remove(entries: &mut Vec<SavedSearch>, index: usize)`:
    - Remove entry at index, bounds-checked
  - [x] Unit tests:
    - `test_add_prepends` — new entry goes to front
    - `test_remove_valid_index` — removes correct entry
    - `test_remove_out_of_bounds` — no panic on invalid index
    - `test_load_missing_file_returns_empty` — missing file returns empty vec
    - `test_save_and_load_roundtrip` — save then load produces same data

- [x] Task 3: Add "Save Search" button to search panel UI (AC: #1)
  - [x] In `resources/ui/search-panel.ui`: add `GtkButton id="save_button"` in the header row, after `more_toggle` and before `count_label`:
    - `icon-name=bookmark-new-symbolic`, `tooltip-text=Save Search`, `visible=false`, CSS class `flat`
  - [x] In `search_panel/imp.rs`: add `#[template_child] save_button: TemplateChild<gtk4::Button>`
  - [x] In `search_panel/imp.rs` `constructed()` or a new `setup_save_button()`: connect `save_button.connect_clicked` to call `self.obj().show_save_search_dialog()`
  - [x] Show `save_button` when results exist (in the `SearchEvent::Done` handler when `total_matches > 0`), hide in `clear_results()`

- [x] Task 4: Implement save search dialog (AC: #1)
  - [x] In `search_panel/mod.rs`: add `pub fn show_save_search_dialog(&self)`:
    - Create `libadwaita::AlertDialog` with heading "Save Search", body "Enter a name for this search."
    - Add responses: "cancel" (neutral), "save" (suggested, default)
    - Add `GtkEntry` as `extra_child`, pre-filled with current query text, `activates_default=true`
    - On "save" response: build `SavedSearch` from current state (name from entry, query, all toggles, glob), call `saved_searches::add()` on `imp.saved_searches`, then `spawn_blocking_then` to `saved_searches::save()`, push status bar message "Search saved as '{name}'"
    - Use the `AdwAlertDialog` + `extra_child` pattern from workspace rename in sidebar

- [x] Task 5: Restructure dropdown popover with saved searches section (AC: #2, #3)
  - [x] In `search_panel/imp.rs`: add state fields:
    - `pub saved_searches: RefCell<Vec<SavedSearch>>`
    - `pub saved_searches_list: gtk4::ListBox` (programmatic, like `history_list`)
    - `pub saved_header: gtk4::Label` ("Saved Searches")
    - `pub recent_header: gtk4::Label` ("Recent")
    - `pub dropdown_separator: gtk4::Separator`
  - [x] In `search_panel/imp.rs` `constructed()`: restructure popover content from flat `history_list` to a vertical `GtkBox`:
    - `saved_header` → `saved_searches_list` → `dropdown_separator` → `recent_header` → `history_list`
    - All wrapped in the existing `GtkScrolledWindow`
    - `saved_header` and `recent_header` use the `heading` CSS class, `halign=Start`, appropriate margins
  - [x] In `search_panel/imp.rs` `setup_history()`: wire `saved_searches_list.connect_row_activated` to call `restore_from_saved_search()`
  - [x] In `search_panel/mod.rs`: rename `populate_history_list()` → `populate_dropdown()`:
    - Clear both `history_list` and `saved_searches_list`
    - Populate `saved_searches_list` with `AdwActionRow` per saved search:
      - Title: saved search name
      - Subtitle: query text + toggle summary (reuse `build_toggle_summary`)
      - Suffix widget: `GtkButton` with `edit-delete-symbolic` icon, CSS class `flat`, `valign=Center`
      - Delete button `connect_clicked`: call `self.remove_saved_search(index)` (Task 6)
    - Populate `history_list` same as before (existing `AdwActionRow` pattern)
    - Show/hide sections: `saved_header` + `saved_searches_list` visible only when saved searches non-empty; `dropdown_separator` visible only when BOTH sections non-empty; `recent_header` visible only when BOTH sections non-empty (if only history exists, skip the "Recent" header)

- [x] Task 6: Wire saved search selection and delete (AC: #3, #4)
  - [x] In `search_panel/mod.rs`: add `pub fn set_saved_searches(&self, entries: Vec<SavedSearch>)` — stores into `imp.saved_searches`
  - [x] In `search_panel/mod.rs`: add `pub fn saved_searches(&self) -> Vec<SavedSearch>` — clones from `imp.saved_searches`
  - [x] In `search_panel/mod.rs`: add `fn restore_from_saved_search(&self, entry: &SavedSearch)`:
    - Same pattern as `restore_from_history`: set `restoring_history=true`, write query + toggles + glob, popdown, clear guard, call `start_search` directly
    - Reuse existing `restoring_history` guard — the guard prevents redundant searches regardless of source
  - [x] In `search_panel/mod.rs`: add `fn remove_saved_search(&self, index: usize)`:
    - Call `saved_searches::remove()` on `imp.saved_searches`
    - Save to disk via `spawn_blocking_then` → `saved_searches::save()`
    - Call `populate_dropdown()` to refresh the popover content
    - Push status bar message via `message_callback` if connected

- [x] Task 7: Load saved searches at startup (AC: #7)
  - [x] In `window/search.rs` `setup_search_panel()`: load saved searches via `spawn_blocking_then`, parallel to existing history load:
    - `spawn_blocking_then(window, move || saved_searches::load(&data_dir), |window, entries| { window.imp().search_panel.set_saved_searches(entries); })`
  - [x] Both history and saved search loads can run in parallel (two independent `spawn_blocking_then` calls)

- [x] Task 8: Verify existing panel state persistence (AC: #5, #6)
  - [x] Verify manually via `make run`:
    - Open search panel, set toggles (case, regex, word, gitignore), expand "More", close app, reopen → all states restored
    - Hide search panel, close app, reopen → panel hidden; Ctrl+Shift+F opens panel with toggles preserved
  - [x] These are already wired via GSettings bindings from Stories 1.3-1.4. This task is verification only — no new code needed unless a bug is found.

- [x] Task 9: Widget tests (all ACs)
  - [x] Test: `save_button` exists as template child and starts invisible
  - [x] Test: `saved_searches_list` exists in popover (programmatic, verify in constructed)
  - [x] Test: `set_saved_searches` stores entries, `saved_searches()` returns them
  - [x] Test: `restore_from_saved_search` sets search entry text to saved search's query
  - [x] Test: `restore_from_saved_search` sets toggle buttons to saved search's toggle states
  - [x] Test: `restore_from_saved_search` sets glob entry to saved search's glob value
  - [x] Test: `restore_from_saved_search` with `glob: None` clears the glob entry
  - [x] Test: `SavedSearch` model with all fields can be serialized/deserialized (roundtrip)
  - [x] Test: existing Story 1.1–3.1 tests still pass (no regressions)

- [x] Task 10: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all unit tests pass
  - [x] Run `make test-int` — all integration tests pass
  - [x] Run `make test-widget` — all widget tests pass
  - [x] Verify no GTK/pixman runtime warnings via `make run` and exercising:
    - Open search panel, type query, wait for results
    - Click save button → dialog appears with query pre-filled
    - Enter name, click Save → saved search persisted
    - Click away then click back to search input → dropdown shows "Saved Searches" section + "Recent" section
    - Select a saved search → toggles + glob restored, search runs immediately
    - Click delete button on a saved search → entry removed, dropdown refreshes
    - Close and reopen app → saved searches and history persist
    - Close app with panel visible + toggles set, reopen → all state restored
  - [x] Update README.md with saved searches feature
  - [x] Update AGENTS.md with saved searches design decision
  - [x] Run `cargo hakari generate` if any dependencies changed (no changes expected)

## Dev Notes

### Saved Searches Flow Overview

This story adds two capabilities: named saved searches (FR30, FR31) and verification of panel state persistence (already working from Stories 1.3-1.4). The saved search flow:

1. **User clicks "Save Search" button** (visible when results exist) → `AdwAlertDialog` with name entry
2. **On confirm** → `SavedSearch` created with name + query + all toggle states + glob → prepended to list
3. **Persisted** → `json_store::save` to `saved-searches.json` via `spawn_blocking_then`
4. **On focus** → dropdown popover shows "Saved Searches" section above "Recent" section
5. **On select** → state restored (query, toggles, glob), search triggered immediately
6. **On delete** → entry removed, file saved, dropdown refreshed

### SavedSearch Type Already Exists — Needs Fix

The `SavedSearch` struct exists at `model/content_search.rs:182-190` as a forward-ported stub from Story 1.1. It has `name`, `query`, `case_sensitive`, `regex`, `whole_word`, `glob` — but is **missing `gitignore: bool`** (unlike `SearchHistoryEntry` which has it). Add this field. The architecture spec Decision 6 specifies separate files for history vs saved searches, both using `json_store`.

The struct is flagged in `deferred-work.md` (line 50) as "defined but unused — remove or revise when later stories actually need them." Story 3.2 is that story.

### New Service File: `services/saved_searches.rs`

Mirrors `services/search_history.rs` exactly:
- `const SAVED_SEARCHES_FILE: &str = "saved-searches.json";`
- `load()`, `save()`, `add()`, `remove()` — all delegating to `json_store`
- NO cap (saved searches are permanent until explicitly deleted, unlike history's cap of 20)
- NO dedup (user-named entries; duplicate names are allowed)
- ~30 prod lines + ~50 test lines

### Save Button: Toolbar Placement

A `GtkButton` with `bookmark-new-symbolic` icon, CSS class `flat`, in the search panel header row after `more_toggle` and before `count_label`. Initially `visible=false`. Shown when results exist (`SearchEvent::Done` with `total_matches > 0`), hidden in `clear_results()`.

The button triggers `show_save_search_dialog()` which creates an `AdwAlertDialog` with `extra_child` `GtkEntry` — the same dialog pattern used for workspace rename in the sidebar.

### Dropdown Restructure: Two-Section Layout

The existing `history_popover` (programmatic `GtkPopover` parented to `search_entry`) is restructured from:

```
GtkPopover → ScrolledWindow → history_list
```

to:

```
GtkPopover → ScrolledWindow → GtkBox(vertical)
├── saved_header (GtkLabel "Saved Searches")   ← shown only if saved searches exist
├── saved_searches_list (GtkListBox)            ← NEW
├── dropdown_separator (GtkSeparator)           ← shown only if BOTH sections non-empty
├── recent_header (GtkLabel "Recent")           ← shown only if BOTH sections non-empty
└── history_list (GtkListBox)                   ← existing
```

`populate_history_list()` is renamed to `populate_dropdown()` and populates both sections. Saved search rows use `AdwActionRow` with:
- Title: user-given name
- Subtitle: query + toggle summary (reuse `build_toggle_summary`)
- Suffix: `GtkButton` with `edit-delete-symbolic` icon for delete

### Restoring State: Reuse Existing Guard

`restore_from_saved_search()` uses the same `restoring_history: Cell<bool>` guard as `restore_from_history()`. The guard prevents redundant searches when `set_text()` and toggle `set_active()` fire their change signals. After all state is set, the guard is cleared and ONE `start_search()` is called directly (bypassing debounce).

This is identical to the Story 3.1 pattern. No new guard needed.

### Panel State Persistence: Already Working

GSettings-based panel state persistence was implemented in Stories 1.3 and 1.4:
- `search-case-sensitive`, `search-regex`, `search-whole-word`: wired in `setup_toggles()` with `sync_create`
- `search-gitignore`: wired in `setup_options()` with `sync_create`
- `search-panel-options-expanded`: wired via `more_toggle` GSettings binding
- `search-panel-visible`: wired in `window/search.rs::setup_search_panel()` and `toggle/close_search_panel()`

Task 8 is verification only. No new GSettings keys or bindings needed.

### Deferred: Query/Glob Text Persistence

The deferred-work.md (line 45) notes: "Panel visible on startup with empty results — panel visibility is persisted via GSettings but query text and results are not." Persisting query text (new `search-last-query` string key) is NOT part of this story's ACs. If encountered during testing, note it as deferred but do not implement.

### CRITICAL: imp.rs Line Budget

`search_panel/imp.rs` is at **924 production lines** (76 lines of headroom). Story 3.2 additions to `imp.rs`:

| Addition | Estimated Lines |
|----------|----------------|
| State fields (saved_searches, saved_searches_list, headers, separator) | +5 |
| Template child (save_button) | +1 |
| Restructured popover in constructed() (replaces 7 lines with ~25 lines) | +18 net |
| saved_searches_list wiring in setup_history() | +8 |
| save_button clicked connection | +5 |
| **Total imp.rs delta** | **~37 lines** |
| **Estimated final imp.rs** | **~961 lines** |

If budget becomes tight, extract `setup_results_list()` (the largest method, with `connect_setup`/`connect_bind` for the results `GtkListView`) into a new `results.rs` helper module. This would free ~200 lines from `imp.rs`.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `crates/lushtext-core/src/model/content_search.rs` | Add `gitignore` field to `SavedSearch`, update doc comment | +2 lines |
| `crates/lushtext-core/src/services/mod.rs` | Add `pub mod saved_searches;` | +1 line |
| `crates/lushtext-core/src/services/saved_searches.rs` | **NEW** — load, save, add, remove + unit tests | ~30 prod, ~50 test |
| `resources/ui/search-panel.ui` | Add `save_button` in header row | +6 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add state fields, restructure popover, wire save button + saved list | +37 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Add `set_saved_searches`, `saved_searches`, `populate_dropdown` (replaces `populate_history_list`), `restore_from_saved_search`, `show_save_search_dialog`, `remove_saved_search` | +85 lines |
| `crates/lushtext-core/src/ui/window/search.rs` | Load saved searches at startup | +10 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Story 3.2 widget tests | +60 lines |
| `.agents/AGENTS.md` | Document saved searches design decision | +10 lines |
| `README.md` | Update features list with saved searches | +3 lines |

### Line Count Impact

| File | Current Lines | After Story 3.2 | Limit |
|------|--------------|-----------------|-------|
| `model/content_search.rs` | 190 | ~192 | 1000 |
| `services/saved_searches.rs` | 0 (new) | ~30 prod / ~50 test | 1000 |
| `search-panel.ui` | 178 | ~184 | N/A (XML) |
| `search_panel/imp.rs` | 924 | **~961** | 1000 |
| `search_panel/mod.rs` | 748 | ~833 | 1000 |
| `window/search.rs` | 396 | ~406 | 1000 |

All files remain within the 1000-line production limit, but `imp.rs` is **tight at ~961**. If additional code is needed, extract `setup_results_list()` into a helper module.

### Previous Story Intelligence

**From Story 3.1 (most recent, same search panel):**
- `history_popover` and `history_list` are created programmatically in `constructed()`, NOT via template — because `GtkPopover` needs `set_parent()` not box child semantics. Follow the same pattern for `saved_searches_list` and the section headers.
- `populate_history_list()` clears and rebuilds `history_list` on every focus-in. Same pattern works for the combined `populate_dropdown()`.
- `restore_from_history()` uses `restoring_history: Cell<bool>` guard. Reuse this guard for saved search restore — both suppress the same signals.
- `build_toggle_summary()` is a free function that builds subtitle text for history/saved rows. Reuse it for saved search rows.
- `setup_history()` in `imp.rs` wires focus-in → populate + popup, and row-activated → restore. Extend it to also wire `saved_searches_list.connect_row_activated`.
- `SearchEvent::Done` handler in `start_search()` is where history save happens. The save button visibility update should also happen here (show when `total_matches > 0`).
- `clear_results()` is the central state reset. The save button should be hidden here. History is NOT cleared here — and saved searches should NOT be cleared here either.
- `gen` is a reserved keyword in Rust Edition 2024 — do not use as a variable name.

**From Story 2.1:**
- `AdwAlertDialog` + `extra_child` pattern is used for Replace All confirmation. The save search dialog follows the same pattern but with a `GtkEntry` instead of a `GtkCheckButton` list.
- `preview_mode` state: the save button should be hidden during preview mode (user is in Replace All flow). Check `imp.preview_mode.get()` before showing.
- `constructed_complete: Cell<bool>` guard prevents GSettings binding signals from firing during construction. This guard is already in place and protects toggle state restoration.

### Anti-Patterns to Avoid

1. **DO NOT** put saved searches in `search-history.json` — architecture Decision 6 specifies two separate files with separate lifecycles (history = capped at 20, saved = permanent).
2. **DO NOT** clear saved searches in `clear_results()` — saved searches persist permanently, unlike search results.
3. **DO NOT** show the save button or dropdown during preview mode — the user is in the middle of a Replace All flow.
4. **DO NOT** save the `SavedSearch` list synchronously — use `spawn_blocking_then` for all file I/O.
5. **DO NOT** load saved searches synchronously at startup — use `spawn_blocking_then` matching the history load pattern.
6. **DO NOT** add a new `restoring_saved_search` guard — reuse the existing `restoring_history` guard. Both suppress the same redundant search triggers.
7. **DO NOT** use `GtkEntryCompletion` — deprecated since GTK 4.10 and removed in 4.14.
8. **DO NOT** use `gen` as a variable name — reserved keyword in Rust Edition 2024.
9. **DO NOT** persist query text or glob text in GSettings — that is NOT part of this story's ACs (noted as deferred in deferred-work.md).
10. **DO NOT** add a cap to saved searches — they are permanent until the user explicitly deletes them.
11. **DO NOT** implement any "edit saved search" functionality — only save (new) and delete are specified.
12. **DO NOT** restructure the history popover using a UI template — `GtkPopover` is created programmatically with `set_parent()`, matching Story 3.1's established pattern.

### Scope Boundary

This story implements **saved searches (FR30, FR31)** and **verifies panel state persistence (FR35)**. The following are NOT in scope:
- Keyboard shortcut for save search (button only for now)
- Edit/rename saved search
- Reorder saved searches
- Query text persistence across restarts
- Glob text persistence across restarts
- Import/export saved searches

### Project Structure Notes

- One new file created: `services/saved_searches.rs`
- All other changes modify existing files
- The search panel grows by ~122 lines across two files — `imp.rs` approaches but stays within the 1000-line limit
- The model grows by ~2 lines — trivial

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 3.2: Saved Searches & Panel State Persistence]
- [Source: _bmad-output/planning-artifacts/architecture.md#Decision 6] — Two separate files: search-history.json (capped at 20) + saved-searches.json (permanent)
- [Source: _bmad-output/planning-artifacts/architecture.md#Data Architecture] — saved-searches.json lifecycle and persistence
- [Source: _bmad-output/planning-artifacts/prd.md#FR30] — Save search as named permanent entry
- [Source: _bmad-output/planning-artifacts/prd.md#FR31] — Select saved search with pre-configured options
- [Source: _bmad-output/planning-artifacts/prd.md#FR35] — Persist panel visibility and search options across sessions
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Journey Flow 4] — Search History and Saved Searches interaction flow
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Key interaction details] — Saved searches are explicit, dropdown on focus, full state restoration
- [Source: _bmad-output/implementation-artifacts/3-1-search-history.md] — Previous story (guard patterns, populate_history_list, build_toggle_summary, AdwActionRow rows)
- [Source: _bmad-output/implementation-artifacts/deferred-work.md#line 45] — Panel visible on startup with empty results (deferred, not this story)
- [Source: _bmad-output/implementation-artifacts/deferred-work.md#line 50] — SavedSearch type defined but unused (this story resolves it)
- [Source: .agents/AGENTS.md#Search history] — Current search history architecture
- [Source: .agents/AGENTS.md#Async I/O Pattern] — spawn_blocking_then for save/load
- [Source: .agents/rules/rust.md#Background I/O] — Threading model
- [Source: .agents/rules/rust.md#Mutable State] — Cell/RefCell conventions
- [Source: .agents/rules/widget-wiring.md#Testing] — Widget test requirements
- [Source: .agents/rules/ui.md#GSettings Bindings] — Persistence patterns
- [Source: crates/lushtext-core/src/model/content_search.rs:182-190] — Existing SavedSearch (missing gitignore)
- [Source: crates/lushtext-core/src/services/search_history.rs] — search_history pattern to mirror for saved_searches
- [Source: crates/lushtext-core/src/ui/search_panel/imp.rs] — 924 prod lines, 76 lines headroom
- [Source: crates/lushtext-core/src/ui/search_panel/mod.rs:370-397] — populate_history_list to rename/extend
- [Source: crates/lushtext-core/src/ui/search_panel/mod.rs:400-418] — restore_from_history pattern to mirror
- [Source: crates/lushtext-core/src/ui/search_panel/mod.rs:728-748] — build_toggle_summary helper to reuse

## Review Findings

- [x] [Review][Patch] Missing status bar confirmation message after saving a search — spec Task 4 requires "Search saved as '{name}'" via message_callback; `show_save_search_dialog()` has no feedback on success [`search_panel/mod.rs`] — **FIXED**: added `message_callback` + `connect_message` + wired in window/search.rs
- [x] [Review][Patch] Missing widget test for delete saved search — widget-wiring rules require signal test for every wired signal; delete button `connect_clicked` → `remove_saved_search()` has no widget test [`tests/widget/search_panel.rs`] — **FIXED**: added `test_remove_saved_search_updates_state`
- [x] [Review][Defer] `atomic_write` temp file has fixed name, concurrent writes to same file collide [`services/content_search.rs`] — deferred, Story 2.1 scope
- [x] [Review][Defer] `render_preview_markup` highlight region visually incorrect for multi-byte replacement text [`search_panel/imp.rs`] — deferred, Story 2.1 scope
- [x] [Review][Defer] Navigation index (`match_positions`) not cleared after Replace All — F4 navigates stale lines [`search_panel/mod.rs`] — deferred, Story 1.5/2.1 scope
- [x] [Review][Defer] No guard prevents double Replace All while first is in-flight [`window/search.rs`] — deferred, Story 2.1 scope
- [x] [Review][Defer] AGENTS.md references `search-panel-position (i)` GSettings key that may not exist in schema — deferred, pre-existing

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Debug Log References

None — clean implementation with no blockers.

### Completion Notes List

- Task 1: Added `gitignore: bool` field and `PartialEq` derive to `SavedSearch`, updated doc comment from stub placeholder to proper description.
- Task 2: Created `services/saved_searches.rs` mirroring `search_history.rs` — load, save, add (prepend, no cap, no dedup), remove (bounds-checked). 5 unit tests all passing.
- Task 3: Added `save_button` (bookmark-new-symbolic, flat, starts invisible) to search panel header row in template and imp.rs. Wired via `setup_save_button()`. Show on SearchEvent::Done with results, hide in `clear_results()`.
- Task 4: Implemented `show_save_search_dialog()` using `AdwAlertDialog` + `GtkEntry` extra_child pattern. Builds `SavedSearch` from current panel state on "save" response, persists via `spawn_blocking_then`.
- Task 5: Restructured dropdown popover from flat `history_list` to two-section layout: saved_header → saved_searches_list → separator → recent_header → history_list inside a `GtkBox`. Section visibility is conditional on content.
- Task 6: Added `set_saved_searches`, `saved_searches`, `restore_from_saved_search` (reuses `restoring_history` guard), `remove_saved_search` (remove + save + repopulate). Delete button per row via `AdwActionRow` suffix.
- Task 7: Added parallel `spawn_blocking_then` for saved searches load in `window/search.rs::setup_search_panel()`.
- Task 8: Verified existing GSettings bindings for toggle/visibility persistence — all working from Stories 1.3-1.4. No bugs found.
- Task 9: Added 5 widget tests: save_button visibility, set/get saved searches, restore_from_saved_search (toggles + glob), serialization roundtrip. All 406 widget tests pass.
- Task 10: `make check` clean (clippy + fmt). 225 unit tests, 52 integration tests, 406 widget tests all pass. Updated README.md and AGENTS.md. imp.rs at 1000 lines (limit), mod.rs at 939.

### Change Log

- 2026-04-08: Implemented Story 3.2 — Saved Searches & Panel State Persistence (all 10 tasks)

### File List

- `crates/lushtext-core/src/model/content_search.rs` — Added `gitignore` field and `PartialEq` to `SavedSearch`, updated doc comment
- `crates/lushtext-core/src/services/mod.rs` — Added `pub mod saved_searches;`
- `crates/lushtext-core/src/services/saved_searches.rs` — **NEW** — load, save, add, remove + 5 unit tests
- `resources/ui/search-panel.ui` — Added `save_button` in header row
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — Added save_button template child, saved_searches state, dropdown_box/saved_header/saved_searches_list/dropdown_separator/recent_header widgets, setup_save_button(), saved_searches_list row activation
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — Added show_save_search_dialog, set_saved_searches, saved_searches, populate_dropdown (renamed from populate_history_list), restore_from_saved_search, remove_saved_search, build_saved_toggle_summary, build_summary_parts
- `crates/lushtext-core/src/ui/window/search.rs` — Added parallel saved searches load at startup
- `crates/lushtext/tests/widget/search_panel.rs` — Added 5 Story 3.2 widget tests
- `.agents/AGENTS.md` — Added saved_searches.rs to module layout, added saved searches design decision
- `README.md` — Updated features list and module layout with saved searches
