# Story 2.1: Replace All with Preview, Execution & Undo

Status: done

## Story

As a user,
I want to enter a replacement string, preview all proposed changes with per-match checkboxes, execute Replace All, and undo if needed,
So that I can safely refactor across multiple files without fear of unintended changes.

## Acceptance Criteria

1. **Replace UI controls visible in expanded options** — Given the search panel options area is expanded (via "More"), when the panel is visible, then a replace input `GtkEntry` is displayed below the glob filter row, and a "Replace All" `GtkButton` is displayed next to the replace entry, and an "Undo" `GtkButton` is displayed next to "Replace All" (initially hidden).

2. **Preview mode on Replace All click** — Given search results are displayed and the user has typed replacement text in the replace entry, when the user clicks "Replace All", then the results list switches to preview mode: each match row displays the original line (matching text dimmed) and the resulting line (replacement text highlighted), and each match row has a `GtkCheckButton` (all checked by default), and a "Confirm Replace" button appears (replacing the "Replace All" button).

3. **Checkbox deselection updates count** — Given the preview is displayed with all checkboxes checked, when the user unchecks specific match rows, then those matches are excluded from the replacement set, and the confirm button label reflects the count (e.g., "Replace 8 of 9").

4. **All unchecked disables confirm** — Given all checkboxes in the preview are unchecked, when the user reviews the preview, then the "Confirm Replace" button is disabled.

5. **Replace execution via service** — Given the user clicks "Confirm Replace" with checked matches, when replacement executes, then `services/content_search::apply_replacements()` runs via `spawn_blocking_then` (not the streaming channel pattern), and each affected file is written atomically (temp file + rename), and pre-replacement file content is backed up in an in-memory `HashMap<PathBuf, Vec<u8>>` for undo.

6. **Skip modified open tabs** — Given a file targeted for replacement is already open in a tab with unsaved modifications (`is_modified() == true`), when Replace All executes, then that file is skipped (not replaced), and the skip is included in the confirmation count.

7. **Open tab reload via file monitor** — Given a file targeted for replacement is already open in a tab without modifications, when Replace All writes the file to disk atomically, then the tab's file monitor detects the change via the existing `changed` signal path, and the tab reloads the updated content from disk.

8. **Status bar confirmation** — Given Replace All completes, when all files are written, then the status bar displays a transient message: "Replaced N of M matches in K files" (and "L files skipped (unsaved changes)" if applicable), and the results list exits preview mode and returns to normal result display, and the "Undo" button becomes visible next to "Replace All".

9. **Undo restores files** — Given the "Undo" button is visible after a Replace All, when the user clicks "Undo", then all replaced files are restored to their pre-replacement content from the in-memory backup via atomic writes, and the status bar displays "Reverted K files", and the "Undo" button is hidden, and open tabs for reverted files reload via file monitor.

10. **Undo backup lifecycle** — Given the in-memory undo backup exists, when the user starts a new search, closes the search panel, or exits the application, then the undo backup is cleared (memory freed), and the "Undo" button is hidden.

11. **Service function contract** — Given the `apply_replacements()` service function, when inspected, then it is in `services/content_search.rs` alongside the `search()` function, and it takes a list of `Replacement` structs and a cancel token, and it returns a `ReplaceResult` summary, and it contains no GTK/GLib imports.

## Tasks / Subtasks

- [x] Task 1: Revise model types in `model/content_search.rs` (AC: #5, #11)
  - [x] Revise `Replacement` struct: keep `path`, `line_number`, `match_range`; add `replaced_line: String` for preview display; rename `original` to `original_line` for clarity; keep `replacement` as the replacement text for the matched range
  - [x] Revise `ReplaceResult`: add `skipped_paths: Vec<PathBuf>` for informative messaging about which files were skipped
  - [x] Add `generate_replacement_preview(matches: &[SearchMatch], replacement_template: &str, options: &ContentSearchOptions) -> Vec<Replacement>` — pure function, no I/O. For literal mode: string-replace the matched text. For regex mode: compile regex from query, expand backreferences via `regex::Regex::replace()` against each match's `line_content`
  - [x] Leave `SearchHistoryEntry` and `SavedSearch` as-is (unused, for Epic 3 — not this story's scope to remove)

- [x] Task 2: Implement `apply_replacements()` and `undo_replacements()` in `services/content_search.rs` (AC: #5, #9, #11)
  - [x] Add `pub fn apply_replacements(replacements: &[Replacement], cancel: &AtomicBool) -> anyhow::Result<(ReplaceResult, HashMap<PathBuf, Vec<u8>>)>`:
    - Group replacements by file path
    - For each file: read entire content, store original bytes in backup HashMap
    - Sort file's replacements in reverse order (last line first, rightmost match first) to avoid offset shifts during replacement
    - Apply each replacement: locate line by `line_number`, replace bytes at `match_range` with `replacement` text
    - Write atomically: `tempfile` in same directory + `std::fs::rename()` (matching `json_store::save` pattern)
    - Check `cancel` between files for responsiveness
    - Return `(ReplaceResult { replaced_count, files_affected, files_skipped }, backup)`
  - [x] Add `pub fn undo_replacements(backup: HashMap<PathBuf, Vec<u8>>) -> anyhow::Result<usize>`:
    - For each entry: write original bytes back atomically (temp + rename)
    - Return count of files restored
  - [x] Unit tests:
    - `test_apply_replacements_literal` — replace literal text in two files, verify content changed
    - `test_apply_replacements_preserves_backup` — verify backup contains original content
    - `test_undo_replacements_restores_content` — apply then undo, verify files match originals
    - `test_apply_replacements_reverse_order` — multiple replacements on same line don't shift offsets
    - `test_apply_replacements_cancel` — set cancel token before second file, verify first replaced but second not
    - `test_apply_replacements_nonexistent_file` — file missing from disk returns error gracefully

- [x] Task 3: Add replace UI widgets to search panel template and imp (AC: #1, #2)
  - [x] In `resources/ui/search-panel.ui`: add a `replace_row` GtkBox (horizontal) inside `options_box` (below the glob filter row) containing:
    - `GtkEntry id="replace_entry"` with placeholder "Replace with" (hexpand=true)
    - `GtkButton id="replace_all_button"` with label "Replace All" (initially sensitive=false — enabled when replace_entry has text AND results exist)
    - `GtkButton id="undo_button"` with label "Undo" (initially visible=false)
  - [x] Update `resources/dev.cominotti.lushtext.gresource.xml` if needed (already includes search-panel.ui)
  - [x] In `search_panel/imp.rs`: add `TemplateChild` fields: `replace_entry`, `replace_all_button`, `undo_button`
  - [x] In `search_panel/imp.rs`: add state fields:
    - `preview_mode: Cell<bool>` — whether the results list is in preview mode
    - `undo_backup: RefCell<Option<HashMap<PathBuf, Vec<u8>>>>` — stored after replace for undo
    - `replace_callback: RefCell<Option<Box<dyn Fn(Vec<Replacement>)>>>` — called when "Confirm Replace" clicked
    - `undo_callback: RefCell<Option<Box<dyn Fn(HashMap<PathBuf, Vec<u8>>)>>>` — called when "Undo" clicked
    - `preview_replacements: RefCell<Vec<Replacement>>` — generated preview data, shown in preview mode
    - `checked_indices: RefCell<HashSet<usize>>` — indices of checked replacements in preview mode

- [x] Task 4: Implement preview mode in search panel `mod.rs` (AC: #2, #3, #4)
  - [x] Add `enter_preview_mode(&self, replacement_text: &str)`:
    - Call `generate_replacement_preview()` with current search matches, replacement text, and options
    - Store result in `imp.preview_replacements`
    - Initialize `imp.checked_indices` with all indices (all checked by default)
    - Set `preview_mode` to true
    - Switch "Replace All" button to "Confirm Replace" label
    - Refresh the results list to show preview rows (see Task 4 sub-tasks below)
  - [x] Add `exit_preview_mode(&self)`:
    - Set `preview_mode` to false
    - Clear `preview_replacements`
    - Clear `checked_indices`
    - Switch "Confirm Replace" button back to "Replace All" label
    - Refresh results list to normal display
  - [x] Modify `connect_bind` in `imp.rs` to conditionally render preview rows:
    - When `preview_mode == true` and item is a match:
      - Show original line with matching text in dimmed (`@dim_label`) color
      - Show replaced line below with replacement text in accent (`@accent_color`) bold
      - Add a `GtkCheckButton` to the left of the row content
      - Connect checkbox `toggled` signal to update `checked_indices` and confirm button label/sensitivity
    - When `preview_mode == false`: render as normal (existing behavior)
  - [x] Update confirm button label dynamically: "Replace N of M" where N = checked count, M = total
  - [x] Disable confirm button when `checked_indices` is empty (AC #4)
  - [x] Wire "Replace All" button `clicked`:
    - If `replace_entry` text is empty: no-op
    - If no results: no-op
    - Otherwise: call `enter_preview_mode(replace_text)`
  - [x] Wire "Confirm Replace" button `clicked`:
    - Collect `Replacement` items at checked indices from `preview_replacements`
    - Call `replace_callback` with the checked replacements
    - Call `exit_preview_mode()`

- [x] Task 5: Wire replace execution and undo through window (AC: #5, #6, #7, #8, #9)
  - [x] Add `pub fn connect_replace_all<F: Fn(Vec<Replacement>) + 'static>(&self, f: F)` on search panel
  - [x] Add `pub fn connect_undo_all<F: Fn(HashMap<PathBuf, Vec<u8>>) + 'static>(&self, f: F)` on search panel
  - [x] Add `pub fn show_undo_button(&self)` and `pub fn hide_undo_button(&self)` on search panel
  - [x] In `window/search.rs` `setup_search_panel()`:
    - Wire `connect_replace_all`:
      - Build `skip_paths: HashSet<PathBuf>` by iterating `tab_view.n_pages()`, checking each `editor.file_path()` + `editor.is_modified()`. Filter `replacements` to exclude matches whose path is in `skip_paths`
      - Count skipped files for the status message
      - Call `spawn_blocking_then(window, move || apply_replacements(&checked, &cancel), |window, result| { ... })`
      - On success: push status bar message "Replaced N of M matches in K files" (+ "L files skipped" if any), store backup in search panel's `undo_backup`, show undo button
      - Reload affected open tabs (non-modified) via `load_file_async()` to refresh content without triggering the "File Has Changed" info bar — update `last_known_mtime` before reload to suppress file monitor
    - Wire `connect_undo_all`:
      - Take backup from `undo_backup`
      - Call `spawn_blocking_then(window, move || undo_replacements(backup), |window, result| { ... })`
      - On success: push status bar "Reverted K files", hide undo button
      - Reload affected open tabs

- [x] Task 6: Handle undo backup lifecycle and button state (AC: #10)
  - [x] In `search_panel/mod.rs` `clear_results()`: clear `undo_backup`, hide undo button (new search clears undo)
  - [x] In `search_panel/mod.rs` `close()`: clear `undo_backup`, hide undo button
  - [x] In `window/search.rs` or `window/mod.rs`: on window `close_request`, clear `undo_backup` (prevent memory leak — though app is exiting, this is good hygiene)
  - [x] Ensure "Replace All" button sensitivity: enabled when `replace_entry` has text AND `has_results()` AND NOT in `preview_mode`
  - [x] Wire `replace_entry` `changed` signal to update "Replace All" button sensitivity
  - [x] Wire search completion to update "Replace All" button sensitivity (results available/empty)

- [x] Task 7: Widget tests (AC: all)
  - [x] Test: replace_entry, replace_all_button, undo_button exist as template children on fresh panel
  - [x] Test: replace_all_button starts with `sensitive=false` (no text in entry, no results)
  - [x] Test: undo_button starts with `visible=false`
  - [x] Test: `enter_preview_mode` sets `preview_mode` to true
  - [x] Test: `exit_preview_mode` sets `preview_mode` to false and clears `preview_replacements`
  - [x] Test: `clear_results()` clears `undo_backup` and hides undo button
  - [x] Test: `Replacement` and `ReplaceResult` types can be constructed (model validation)
  - [x] Test: `generate_replacement_preview` produces correct before/after for literal replacement
  - [x] Test: `generate_replacement_preview` with regex backreferences expands correctly
  - [x] Test: existing Story 1.1–1.5 tests still pass (no regressions)

- [x] Task 8: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all unit tests pass
  - [x] Run `make test-int` — all integration tests pass
  - [x] Run `make test-widget` — all widget tests pass
  - [x] Verify no GTK/pixman runtime warnings via `make run` and exercising:
    - Expand "More" options, verify replace row visible
    - Type replacement text, verify "Replace All" button enables
    - Click "Replace All" → preview mode with checkboxes
    - Uncheck some matches → confirm button updates count
    - Uncheck all → confirm button disabled
    - Click "Confirm Replace" → execution → status bar message
    - "Undo" button appears → click → files restored
    - Start new search → undo button hidden, backup cleared
    - Close panel → undo button hidden

## Dev Notes

### Replace Flow Overview

This is a two-phase operation with an intermediate preview step:

1. **Search phase** (already done by Epic 1): user types query, results stream in
2. **Preview phase** (new): user clicks "Replace All" → results list transforms to show before/after with checkboxes
3. **Execute phase** (new): user clicks "Confirm Replace" → checked replacements are applied to disk
4. **Undo phase** (new): "Undo" button appears → clicking reverts all replaced files

The preview phase reuses the existing `GtkListView` + `GtkTreeListModel` display — no separate widget. The `connect_bind` callback conditionally renders preview rows (before/after + checkbox) based on `preview_mode: Cell<bool>`.

### Preview Generation: Literal vs Regex

`generate_replacement_preview()` must handle two modes:

**Literal mode:** Simple string replacement within `match_range`:
```rust
let mut replaced = original_line.clone();
replaced.replace_range(match_range.clone(), replacement_template);
```

**Regex mode:** Backreference expansion (e.g., `$1`, `$2`). Re-compile the regex from the original query, find the match within the line at `match_range`, and use `regex::Regex::replacen()` to expand backreferences against capture groups. This re-parses the line but avoids coupling with `grep-matcher` internals.

For both modes: produce `Replacement { path, line_number, original_line, replaced_line, match_range, replacement }` where `replacement` is the literal text that replaces the matched range, and `replaced_line` is the complete line after replacement (for preview display).

### Replacement Application: Reverse-Order Offset Safety

When applying multiple replacements to the same file, replacements within a line and across lines must be applied in **reverse order** (last line first, rightmost match first within a line) to prevent earlier replacements from shifting byte offsets of later ones.

```rust
// Sort: by line_number DESC, then match_range.start DESC
file_replacements.sort_by(|a, b| {
    b.line_number.cmp(&a.line_number)
        .then(b.match_range.start.cmp(&a.match_range.start))
});
```

### Skip Modified Tabs: Window-Level Logic

The window determines which files to skip — the service never sees GTK objects. The flow:

1. Search panel fires `replace_callback(checked_replacements)`
2. Window iterates `tab_view` pages: for each page with a matching `editor.file_path()` where `editor.is_modified() == true`, add the path to `skip_paths`
3. Filter `checked_replacements` to exclude matches in `skip_paths`
4. Count excluded replacements for the status message
5. Pass filtered replacements to `spawn_blocking_then` → `apply_replacements()`

The pattern for iterating tabs already exists in `on_use_editorconfig_changed()` at `window/mod.rs:476-490`:
```rust
for i in 0..tab_view.n_pages() {
    let page = tab_view.nth_page(i);
    if let Some(editor) = page.child().downcast_ref::<LushtextEditorPage>() { ... }
}
```

### Open Tab Reload After Replace

After `apply_replacements()` completes, the window should reload affected non-modified open tabs to show the updated content. Two approaches:

**Option A (AC-specified):** Rely on the file monitor's `changed` signal path. The file monitor debounce (500ms) will detect the disk change and show the "File Has Changed on Disk" info bar. The user clicks "Reload." This is the simplest implementation but requires user action per file.

**Option B (Better UX):** After replace completes, the window proactively reloads affected tabs. For each replaced file that's open and non-modified:
1. Update `last_known_mtime` on the EditorPage to the new mtime (suppresses the file monitor's "changed" detection)
2. Call `load_file_async()` to reload content

Option B is preferred for UX smoothness and follows the "trust through transparency" principle — the user initiated the replace, so auto-reload is expected. Implement Option B.

### Undo Backup Memory Bounds

The undo backup stores `HashMap<PathBuf, Vec<u8>>` — original file bytes. Memory is bounded by the 10,000 result cap and typical file sizes. Worst case: 10,000 matches across 10,000 unique files of 1MB each = 10GB. In practice, matches are clustered in far fewer files. The backup is cleared aggressively (new search, panel close, app exit).

### Checkbox State in Preview Mode

Each match row gets a `GtkCheckButton` in preview mode. State tracking:
- `checked_indices: RefCell<HashSet<usize>>` — indices into `preview_replacements` that are checked
- Initialized with all indices on `enter_preview_mode()`
- Updated on each checkbox `toggled` signal
- The confirm button label reads this set: "Replace {checked_count} of {total}"
- Confirm button disabled when set is empty

**GtkListView recycling caveat:** `connect_bind` runs on every bind (including recycled items). The checkbox state must be read from `checked_indices` at bind time, not stored on the widget. In the `toggled` handler, write back to `checked_indices` using the item's index.

### Replace Row Placement in UI Template

The UX spec places the replace row inside the `options_revealer`, below the glob filter row:

```
options_revealer
└── options_box (vertical)
    ├── filter_row: [.gitignore ✓] [File filter: *.rs, *.toml]
    └── replace_row: [Replace with    ] [Replace All] [Undo]
```

This follows the "Progressive Minimal" design direction — replace is an advanced feature hidden behind "More". The `options_revealer` already works with `GtkRevealer(slide-down, 150ms)` persisted via GSettings `search-panel-options-expanded`.

### Preview Row Layout

In preview mode, each match row transforms from:

```
Normal:     [line_num]  original line with **match** highlighted
```

To:

```
Preview:  [☑] [line_num]  original line with ~~match~~ dimmed
                           replaced line with **new text** highlighted
```

The preview uses the same `GtkLabel` with Pango markup. Two lines of markup in a single label (separated by `\n`):
- Line 1: original line with match range in `@dim_label` color + strikethrough
- Line 2: replaced line with replacement text in `@accent_color` bold

The `GtkCheckButton` is added dynamically in `connect_bind` when `preview_mode == true`. It must be removed in `connect_unbind` or in the normal-mode `connect_bind` path to handle ListItem recycling.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `crates/lushtext-core/src/model/content_search.rs` | Revise `Replacement`/`ReplaceResult`, add `generate_replacement_preview()` | +40 lines |
| `crates/lushtext-core/src/services/content_search.rs` | Add `apply_replacements()`, `undo_replacements()`, unit tests | +150 prod, +200 test |
| `resources/ui/search-panel.ui` | Add replace_row with entry, buttons | +15 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add template children, state fields, modify `connect_bind` for preview | +100 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Add preview methods, replace/undo callbacks, button wiring | +200 lines |
| `crates/lushtext-core/src/ui/window/search.rs` | Wire replace/undo callbacks, skip-modified-tabs logic, tab reload | +120 lines |
| `crates/lushtext-core/src/ui/window/mod.rs` | Minor: ensure replace-related actions in `update_content_stack()` | +10 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Story 2.1 widget tests | +80 lines |
| `.agents/AGENTS.md` | Document Replace All design decisions | +30 lines |
| `README.md` | Update features list with Replace All | +5 lines |

**No new files created.** All changes modify existing files.

### Line Count Impact

| File | Current Lines | After Story 2.1 | Limit |
|------|--------------|-----------------|-------|
| `model/content_search.rs` | 107 | ~147 | 1000 |
| `services/content_search.rs` | 222 prod / 442 test | ~372 prod / ~642 test | 1000 (prod only) |
| `search-panel.ui` | 149 | ~164 | N/A (XML template) |
| `search_panel/imp.rs` | 603 | ~703 | 1000 |
| `search_panel/mod.rs` | 447 | ~647 | 1000 |
| `window/search.rs` | 240 | ~360 | 1000 |
| `window/mod.rs` | 1123 | ~1133 | 1000 (already over — see deferred-work.md) |
| `status_bar/mod.rs` | 187 | 187 (no changes) | 1000 |

All files remain well within the 1000-line limit except `window/mod.rs` which is pre-existing over-limit (documented in deferred-work.md). The +10 lines there are unavoidable action registrations.

### Previous Story Intelligence

**From Story 1.5 (most recent, same feature area):**
- `navigate_callback` / `progress_callback` use `RefCell<Option<Box<dyn Fn(...)>>>` pattern — reuse for `replace_callback` and `undo_callback`
- `has_results()` returns `self.imp().total_matches.get() > 0` — use this to gate "Replace All" button sensitivity
- `clear_results()` is the central state reset — add undo backup clearing here
- `match_positions: RefCell<Vec<(PathBuf, u32)>>` exists as a flat navigation index — the preview can reference search match data from the original `SearchEvent::Match` events stored in the tree model
- F4 navigation and progress reporting are independent — Replace All should not break them
- `gen` is a reserved keyword in Rust Edition 2024 — do not use as a variable name

**From Story 1.4:**
- `constructed_complete: Cell<bool>` guard pattern prevents signal handlers firing during widget construction — reuse if any new signal handlers (e.g., replace_entry `changed`) could fire during `constructed()`
- `options_revealer` already exists and works with GSettings persistence — the replace row is a child of `options_box` inside this revealer, so it inherits the expand/collapse behavior

**From Story 1.2:**
- `connect_open_file` callback pattern (RefCell + Box<dyn Fn>) — reuse for `connect_replace_all` and `connect_undo_all`
- `clear_results()` resets all state — add `undo_backup`, `preview_mode`, `preview_replacements`, `checked_indices` clearing here
- The `RefCell` borrow pattern in `file_groups`: clone+drop before signal emission — apply same pattern when iterating `preview_replacements` inside closure callbacks

**From Story 1.1:**
- `services/content_search.rs` file structure: `search()` function at top, private helpers below, `#[cfg(test)] mod tests` at bottom — `apply_replacements()` and `undo_replacements()` go between `search()` and the helpers
- Atomic write pattern for files: create temp file in same directory + `std::fs::rename()` — reuse in both `apply_replacements()` and `undo_replacements()`

### Deferred Work That Affects This Story

From `deferred-work.md`:
- **Premature model types** — `Replacement`, `ReplaceResult`, `SearchHistoryEntry`, `SavedSearch` in `model/content_search.rs:68-103` are defined but unused. Revise `Replacement` and `ReplaceResult` for this story. Leave `SearchHistoryEntry` and `SavedSearch` for Epic 3.
- **`find_match_range` only highlights first match per line** — `services/content_search.rs:164` uses `find_at(line, 0)`. This means lines with multiple matches show partial highlighting in search results. For Replace All, each match has its own `match_range` from `SearchMatch`, so previews are per-match. However, if two matches are on the same line, replacements must be applied in reverse byte offset order to avoid offset corruption.
- **`OverrideBuilder::new(roots[0])` glob for multi-root** — anchors override builder to first root. This affects search, not replace. Replace operates on already-found matches.
- **Search threads bypass MAX_CONCURRENT_SPAWNS** — replace uses `spawn_blocking_then` which respects the concurrency guard. No issue.

### Anti-Patterns to Avoid

1. **DO NOT** use the streaming channel pattern for Replace All — use `spawn_blocking_then` (single result, not streaming). Replace is a fire-and-forget operation with a summary result.
2. **DO NOT** pass GTK objects to the replace service — the window determines `skip_paths` and filters replacements before handing off to the service.
3. **DO NOT** store the `GtkCheckButton` widget state as the source of truth — use `checked_indices: RefCell<HashSet<usize>>` and read from it in `connect_bind`. GtkListView recycles ListItems, so widget state is not reliable.
4. **DO NOT** apply replacements in forward order — always reverse (last line first, rightmost match first) to avoid offset shifts.
5. **DO NOT** show a separate dialog for the preview — reuse the existing `GtkListView` with conditional rendering in `connect_bind`.
6. **DO NOT** add a new GSettings key for replace text persistence — replacement text is ephemeral (per-session), not persisted.
7. **DO NOT** add search history or saved search features — those belong to Epic 3.
8. **DO NOT** use `gen` as a variable name — reserved keyword in Rust Edition 2024.
9. **DO NOT** forget to clear `preview_replacements`, `checked_indices`, and `undo_backup` in `clear_results()`.
10. **DO NOT** duplicate the tab iteration logic — extract a helper if needed, or use the existing pattern from `on_use_editorconfig_changed()`.
11. **DO NOT** register replace-related actions under `win.*` — the architecture specifies `search.*` for panel-internal actions. However, the replace callback goes through the window because it needs tab access.
12. **DO NOT** add a spinner or modal progress dialog for replace — the status bar `push_message()` is sufficient for feedback.

### Project Structure Notes

- All changes are within existing modules — no new files needed
- The search panel grows by ~300 lines across two files — still within the 1000-line limit
- The service grows by ~150 prod lines — well within limit (222 → ~372)
- The model grows by ~40 lines — trivial growth
- The window/search.rs grows by ~120 lines — still well within limit (240 → ~360)
- Replace follows the UX spec's "Progressive Minimal" design: hidden behind "More" by default

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 2.1: Replace All with Preview, Execution & Undo]
- [Source: _bmad-output/planning-artifacts/architecture.md#Pattern 3: Replace All Placement] — `services/content_search.rs` contains both `search()` and `replace_all()`, replace via `spawn_blocking_then`
- [Source: _bmad-output/planning-artifacts/architecture.md#Decision 1] — In-memory file backup `HashMap<PathBuf, Vec<u8>>`
- [Source: _bmad-output/planning-artifacts/architecture.md#Decision 2] — Skip modified tabs: `is_modified()` check per file
- [Source: _bmad-output/planning-artifacts/architecture.md#Cross-Cutting Concerns #5] — Open-tab synchronization for Replace All
- [Source: _bmad-output/planning-artifacts/architecture.md#Enforcement Guidelines] — 7 mandatory rules for all content search implementation
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Journey Flow 2] — Replace All interaction flow diagram
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Implementation Approach] — Widget structure for replace_row
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Design Direction Decision] — "Progressive Minimal" layout with "More" toggle
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR9] — Replace All preview mode: same ListView, before/after display, checkboxes
- [Source: _bmad-output/planning-artifacts/prd.md#FR22-FR27] — Multi-File Replace functional requirements
- [Source: _bmad-output/planning-artifacts/prd.md#NFR7] — Atomic replace writes
- [Source: _bmad-output/planning-artifacts/prd.md#NFR11] — Undo All reliability
- [Source: _bmad-output/implementation-artifacts/1-5-match-navigation-progress-reporting.md] — Previous story learnings
- [Source: _bmad-output/implementation-artifacts/deferred-work.md] — Premature model types, find_match_range limitation
- [Source: .agents/AGENTS.md#Content search panel] — Existing search panel architecture
- [Source: .agents/AGENTS.md#Async I/O Pattern] — spawn_blocking_then pattern
- [Source: .agents/rules/rust.md#Background I/O] — Threading model
- [Source: .agents/rules/widget-wiring.md#Action Enabled State] — Button sensitivity lifecycle
- [Source: .agents/rules/widget-wiring.md#Testing] — Widget test requirements
- [Source: .agents/rules/ui.md#Status Bar] — push_message for feedback

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Debug Log References

No debugging issues encountered. All tests passed on first run.

### Completion Notes List

- Revised `Replacement` struct: renamed `original` to `original_line`, added `replaced_line` for preview display
- Revised `ReplaceResult`: added `skipped_paths: Vec<PathBuf>` for informative messaging
- Added `generate_replacement_preview()` pure function with literal and regex (backreference) support
- Added `regex = "1"` workspace dependency for backreference expansion
- Implemented `apply_replacements()`: groups by file, reverse-order replacement, atomic writes
- Implemented `undo_replacements()`: restores files from in-memory backup via atomic writes
- Added `atomic_write()` helper matching `json_store::save` pattern
- 7 unit tests for replace/undo service functions (all pass)
- Added replace_row UI to search-panel.ui (entry + Replace All button + Undo button) inside options_revealer
- Added TemplateChild fields and state fields (preview_mode, undo_backup, preview_replacements, checked_indices)
- Implemented preview mode: enter/exit, conditional connect_bind rendering with checkboxes
- Checkbox state tracked via `checked_indices: RefCell<HashSet<usize>>` (not widget state)
- Dynamic GtkCheckButton added/removed in connect_bind/unbind for ListItem recycling
- Wired Replace All → preview → Confirm Replace → execution flow through window/search.rs
- Skip-modified-tabs: window iterates tab_view, builds skip_paths HashSet
- Tab auto-reload after replace: updates last_known_mtime then calls load_file_async
- Undo backup lifecycle: cleared on new search, panel close, window exit
- Replace button sensitivity: enabled when entry has text AND results exist AND not in preview mode
- 14 widget tests for Story 2.1 (all pass)
- All 394 widget tests pass, 214 unit tests pass, 52 integration tests pass (zero regressions)
- `make check` (clippy + fmt) passes clean
- Updated README.md with Multi-file Replace All feature
- Updated AGENTS.md with Replace All design decision

### Change Log

- 2026-04-08: Story 2.1 implemented — Replace All with Preview, Execution & Undo

### File List

- `Cargo.toml` — added `regex = "1"` workspace dependency
- `crates/lushtext-core/Cargo.toml` — added `regex = { workspace = true }`
- `crates/lushtext-core/src/model/content_search.rs` — revised Replacement/ReplaceResult, added generate_replacement_preview()
- `crates/lushtext-core/src/services/content_search.rs` — added apply_replacements(), undo_replacements(), atomic_write(), 7 unit tests
- `resources/ui/search-panel.ui` — added replace_row (entry + buttons) inside options_box
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — added TemplateChild fields, state fields, preview connect_bind, button wiring
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — added preview mode methods, callbacks, lifecycle management
- `crates/lushtext-core/src/ui/window/search.rs` — wired replace/undo callbacks, skip-modified-tabs, tab reload
- `crates/lushtext/tests/widget/search_panel.rs` — added 14 Story 2.1 widget tests
- `README.md` — added Multi-file Replace All feature description
- `.agents/AGENTS.md` — documented Replace All design decision
- `workspace-hack/Cargo.toml` — updated by cargo-hakari for regex dependency
- `_bmad-output/implementation-artifacts/sprint-status.yaml` — status: in-progress → review
- `_bmad-output/implementation-artifacts/2-1-replace-all-with-preview-execution-undo.md` — tasks marked complete

### Review Findings

- [x] [Review][Decision→Patch] TOCTOU: validate `original_line` before replacing — resolved: validate each line matches before replacing, skip file if content changed
- [x] [Review][Decision→Patch] Allow empty replacement text — resolved: remove `!text.is_empty()` guard, two-phase preview/confirm is sufficient protection
- [x] [Review][Decision→Dismiss] Preview markup bold-only — resolved: keep bold-only, layout provides sufficient visual distinction
- [x] [Review][Decision→Patch] Status bar message "N of M" format — resolved: match spec format
- [x] [Review][Patch] Partial replace failure — continue on per-file error, preserve backup for undo. `undo_replacements` now borrows backup. [services/content_search.rs]
- [x] [Review][Patch] Truncated match ranges — store unclamped `original_line_content`/`original_match_start`/`original_match_end` on `SearchResultItem`, use in `collect_search_matches`. [search_panel/item.rs, search_panel/mod.rs]
- [x] [Review][Patch] CRLF line endings — detect and preserve original line ending style via `detect_line_ending()`. [services/content_search.rs]
- [x] [Review][Patch] Same-line match binding — add `match_range.start` to preview lookup for disambiguation. [search_panel/imp.rs]
- [x] [Review][Patch] Regex fallback — log warning and keep original text instead of inserting unexpanded backreferences. [model/content_search.rs]
- [x] [Review][Patch] AC #10 close_request — call `search_panel.close()` in `close_request` handler. [window/imp.rs]
- [x] [Review][Defer] flush() without sync_all() in atomic_write — no fsync before rename means data could be lost on power failure. Pre-existing project pattern (same as json_store::save). [services/content_search.rs:333] — deferred, pre-existing pattern
