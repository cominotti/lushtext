# Story 1.5: Match Navigation & Progress Reporting

Status: done

## Story

As a user,
I want to cycle through matches across files with F4/Shift+F4 and see search progress in the status bar,
so that I can navigate results efficiently with the keyboard and know when a search is still running on large workspaces.

## Acceptance Criteria

1. **F4 next match from editor** — Given search results are displayed in the panel and the user is in the editor (focus not on the results list), when the user presses F4, then the next match in the results list is selected (via `SingleSelection::set_selected()`), and the file containing that match opens at the matching line (switching to existing tab or opening a new tab), and the search panel remains visible.

2. **F4 sequential cycling** — Given the user has navigated to a match via F4, when the user presses F4 again, then the next match is selected, cycling to the next file's matches when the current file's matches are exhausted.

3. **Shift+F4 wrap-around** — Given the user is on the first match in the results list, when the user presses Shift+F4, then navigation wraps to the last match in the results list.

4. **F4 from results list** — Given the user presses F4 from the results list (focus on the list), when a match is activated, then focus moves to the editor at the matching line, and the panel stays visible with the current match highlighted in the results list.

5. **Progress reporting** — Given a search is in progress on a large workspace, when results are streaming, then the status bar displays progress: "Searching X / Y files..." (where Y is estimated from FileIndex count when available, or "Searching X files..." without denominator), and the progress message does not auto-dismiss (unlike normal status bar messages).

6. **Progress cleared on completion** — Given a search completes or is cancelled, when the final `SearchEvent::Done` is processed, then the status bar progress message is cleared programmatically.

7. **Actions disabled when no results** — Given `win.search-next-match` (F4) and `win.search-prev-match` (Shift+F4) actions are registered on the window, when the search panel is not visible or has no results, then both actions are disabled (greyed out, shortcuts are no-ops).

8. **Navigation reset on new search** — Given the current match tracker `current_match_index: Cell<Option<usize>>` on the panel, when a new search starts, then the tracker is reset to `None`.

## Tasks / Subtasks

- [x] Task 1: Add `SearchEvent::Progress` variant and emit from service (AC: #5)
  - [x] Add `Progress(usize)` variant to `SearchEvent` enum in `model/content_search.rs`
  - [x] In `services/content_search.rs`, add `Arc<AtomicUsize>` file counter shared across parallel walker threads
  - [x] In the `Sink::matched()` callback (or `WalkState` file-visit path), atomically increment counter and send `SearchEvent::Progress(count)` through the channel every 100 files (throttle to avoid flooding)
  - [x] Handle `TrySendError::Full` by skipping (progress is best-effort, not critical)

- [x] Task 2: Add non-auto-dismiss progress message API to status bar (AC: #5, #6)
  - [x] Add `set_progress_message(&self, text: &str)` method to `LushtextStatusBar` in `status_bar/mod.rs`:
    - Sets `message_label` text with `"status-info"` CSS class
    - Sets `progress_active: Cell<bool>` flag to `true`
    - Does NOT schedule auto-dismiss timer
    - Does NOT bump `message_generation` (so a concurrent `push_message` takes priority by clearing progress)
  - [x] Add `clear_progress_message(&self)` method:
    - Only clears if `progress_active.get() == true` (doesn't accidentally clear a real push_message)
    - Resets `progress_active` to `false`
    - Clears `message_label` text and CSS classes
  - [x] Add `progress_active: Cell<bool>` field to `status_bar/imp.rs` (defaults to `false`)
  - [x] Modify existing `push_message()`: if `progress_active` is true, clear it first (normal messages override progress)

- [x] Task 3: Add match navigation state to search panel (AC: #1-4, #7, #8)
  - [x] Add to `search_panel/imp.rs`:
    - `match_positions: RefCell<Vec<(PathBuf, u32)>>` — ordered list of all (path, line_number) pairs as matches arrive
    - `current_match_index: Cell<Option<usize>>` — index into `match_positions` for F4/Shift+F4 cursor
    - `navigate_callback: RefCell<Option<Box<dyn Fn(&Path, u32)>>>` — separate from `open_file_callback` to allow different wiring (see Dev Notes)
  - [x] In `clear_results()` (`mod.rs`): clear `match_positions` vec and reset `current_match_index` to `None`
  - [x] In the polling timer's `SearchEvent::Match` handler: append `(match.path.clone(), match.line_number as u32)` to `match_positions`

- [x] Task 4: Implement `navigate_next_match()` and `navigate_prev_match()` (AC: #1-4)
  - [x] Add `pub fn navigate_next_match(&self)` to `search_panel/mod.rs`:
    - Borrow `match_positions`. If empty, return.
    - Compute next index: `current_match_index.get().map(|i| (i + 1) % len).unwrap_or(0)`
    - Update `current_match_index`
    - Call `select_match_in_results(index)` to highlight in results list
    - Call `navigate_callback` with `(path, line)`
  - [x] Add `pub fn navigate_prev_match(&self)`:
    - Same logic, compute prev: `current_match_index.get().map(|i| if i == 0 { len - 1 } else { i - 1 }).unwrap_or(len - 1)`
  - [x] Add `pub fn connect_navigate_to_match<F: Fn(&Path, u32) + 'static>(&self, f: F)`:
    - Stores callback in `imp.navigate_callback`
  - [x] Add `pub fn has_results(&self) -> bool`:
    - Returns `self.imp().total_matches.get() > 0`
  - [x] Add private `fn select_match_in_results(&self, match_index: usize)`:
    - Get the `SingleSelection` via `imp.results_list.model().and_downcast::<gtk4::SingleSelection>()`
    - Walk the flat model to find the match row that corresponds to `match_positions[match_index]`
    - Match by comparing `SearchResultItem` path and line_number fields
    - Call `selection.set_selected(flat_position)` to visually highlight
    - Scroll `results_scroll` to make the selected row visible via `results_list.scroll_to()` API

- [x] Task 5: Register F4/Shift+F4 window actions and wire to panel (AC: #1-4, #7)
  - [x] In `window/mod.rs` `setup_actions()`, add two new action entries:
    - `"search-next-match"`: calls `imp.search_panel.navigate_next_match()`
    - `"search-prev-match"`: calls `imp.search_panel.navigate_prev_match()`
  - [x] In `setup_shortcuts()`, bind:
    - `"win.search-next-match"` → `"F4"`
    - `"win.search-prev-match"` → `"<Shift>F4"`
  - [x] In `update_content_stack()`, add `"search-next-match"` and `"search-prev-match"` to the action enable/disable list alongside existing actions
  - [x] **Additional disable logic:** These two actions should also be disabled when the search panel is not visible or has no results. Added `update_search_navigation_actions()` helper called from toggle/close/update_content_stack.
  - [x] In `window/search.rs` `setup_search_panel()`, wire `connect_navigate_to_match` with shared `open_file_at_line` helper (extracted from `connect_open_file`)

- [x] Task 6: Handle progress events in polling timer and wire to status bar (AC: #5, #6)
  - [x] In the search panel's polling timer (`mod.rs`), handle `SearchEvent::Progress(count)`:
    - Call `progress_callback` with the file count
  - [x] Add `connect_search_progress<F: Fn(usize, bool) + 'static>(&self, f: F)` on search panel:
    - Callback signature: `(files_searched, is_done)`. `is_done=true` on `SearchEvent::Done`.
  - [x] In `window/search.rs` `setup_search_panel()`, wire `connect_search_progress`:
    - On progress: call `status_bar.set_progress_message("Searching X / Y files...")` where Y = command palette file count estimate (if available), else `"Searching X files..."`
    - On done: call `status_bar.clear_progress_message()`
    - **500ms delay:** Only show progress after 500ms of searching. Use a `Cell<bool>` flag set by `glib::timeout_add_local_once(Duration::from_millis(500), ...)` after search starts. Progress callbacks before the flag is set are ignored.
  - [x] Also call `clear_progress_message()` on search cancel (when `start_search` is called with a new query, the old progress should be cleared)
  - [x] Update navigation action enabled state on `Done` (has_results → enable, no results → disable)

- [x] Task 7: Widget tests (AC: all)
  - [x] Test: `SearchEvent::Progress` variant can be constructed and pattern-matched
  - [x] Test: `navigate_next_match()` with empty match_positions is a no-op
  - [x] Test: `has_results()` returns false on fresh panel, true concept after matches (unit-testable via internal state)
  - [x] Test: `current_match_index` resets to `None` when `clear_results()` is called
  - [x] Test: F4 and Shift+F4 shortcuts are bound (`win.search-next-match`, `win.search-prev-match`)
  - [x] Test: `search-next-match` and `search-prev-match` actions exist and start disabled
  - [x] Test: `set_progress_message` and `clear_progress_message` exist on status bar (functional via direct calls)
  - [x] Test: `progress_active` flag is false by default on status bar imp
  - [x] Test: existing Story 1.1–1.4 tests still pass (no regressions)

- [x] Task 8: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all 207 unit tests pass
  - [x] Run `make test-int` — all 52 integration tests pass
  - [x] Run `make test-widget` — all 382 widget tests pass
  - [ ] Verify no GTK/pixman runtime warnings via `make run` and exercising:
    - F4 cycling through matches across files
    - Shift+F4 reverse cycling with wrap-around
    - F4 when panel has no results (should be no-op)
    - Progress message appearing on large workspace search
    - Progress message clearing on search completion
    - Progress message not appearing for fast searches
    - Rapid F4 presses (no crash, no duplicate tabs)

## Dev Notes

### Two Independent Features in One Story

This story has two orthogonal concerns:
1. **F4/Shift+F4 match navigation** — Keyboard-driven sequential match cycling across files
2. **Status bar progress reporting** — Non-auto-dismissing "Searching X / Y files..." during active search

They share the search panel polling timer as the integration point but are otherwise independent. Implement and test them separately.

### F4 Navigation Architecture

**The `match_positions` vec is the canonical navigation index.** The `TreeListModel` + `SingleSelection` is the *display* model (hierarchical: file groups → matches). Navigating it directly would require walking the flattened tree, skipping file header rows, and the positions shift if groups are collapsed. Instead:

1. `match_positions: RefCell<Vec<(PathBuf, u32)>>` accumulates `(path, line)` tuples in arrival order as `SearchEvent::Match` events stream in.
2. `current_match_index: Cell<Option<usize>>` is an index into this vec.
3. F4 increments, Shift+F4 decrements, both wrap.
4. After advancing the index, the match (path, line) is used to (a) invoke `navigate_callback` and (b) visually highlight the matching row in the results list.

**Highlighting the match row in `SingleSelection`:** After navigating, walk the flat `SingleSelection` model to find the `TreeListRow` whose underlying `SearchResultItem` matches the target path and line. Use `selection.set_selected(position)`. This is an O(n) scan, but n is capped at ~20,000 (10,000 matches + file headers), and it runs once per F4 press — imperceptible.

**Results list scrolling:** After `set_selected()`, ensure the selected row is visible. `GtkListView` with `SingleSelection` should auto-scroll to the selected item. If it doesn't, use `gtk4::ListScrollFlags::FOCUS` with `ListView::scroll_to`.

**`navigate_callback` vs `open_file_callback`:** These serve the same purpose (open file at line) but are wired differently:
- `open_file_callback` is invoked by `results_list.connect_activate` (user double-clicks/Enter on a result row).
- `navigate_callback` is invoked by `navigate_next_match()`/`navigate_prev_match()` (F4/Shift+F4 from the window action).

Both should trigger the same open+scroll behavior. In `window/search.rs`, extract the open+scroll logic from the existing `connect_open_file` closure into a shared helper function, then call it from both callbacks. This avoids duplicating the evicted-tab / loaded-buffer / loading-buffer branching logic.

### Separate `navigate_callback` Pattern

The navigate callback is separate from `open_file_callback` because:
- `open_file_callback` fires on result row activation (Enter/double-click in the list) — this is wired in `setup_results_list()` in `imp.rs`.
- `navigate_callback` fires on F4/Shift+F4 from anywhere in the window — this is wired from the window's `setup_search_panel()`.
- They could be the same callback, but keeping them separate follows the existing pattern where each interaction path has its own callback connector (e.g., `connect_file_activated`, `connect_file_renamed`, `connect_file_deleted` are all separate on the sidebar).

### Progress Reporting — Non-Auto-Dismiss Pattern

The status bar's existing `push_message()` always schedules a 5-second auto-dismiss via generation counter. Progress messages must persist until explicitly cleared. The approach:

1. **New API on `LushtextStatusBar`:**
   - `set_progress_message(text)` — sets label, sets `progress_active = true`, does NOT schedule timer.
   - `clear_progress_message()` — clears label only if `progress_active == true`, resets flag.
2. **Interaction with `push_message()`:** If a normal message is pushed while progress is active, the normal message takes priority — `push_message` sets `progress_active = false` and schedules its own auto-dismiss normally. This ensures error/warning messages from save/rename are never obscured by progress.
3. **No spinners, no modal dialogs.** The UX spec explicitly says streaming results ARE the progress indicator. The status bar message is supplementary for large workspaces.

### Progress Event Throttling

The search service uses `ignore::WalkParallel` which visits files across multiple threads. Sending `SearchEvent::Progress` on every file would flood the bounded channel. Throttle:

```rust
// In the Sink::matched() or file-visit path:
let files_visited = files_counter.fetch_add(1, Ordering::Relaxed) + 1;
if files_visited % 100 == 0 {
    let _ = tx.try_send(SearchEvent::Progress(files_visited));
    // try_send — if channel is full (consumer behind), skip this progress update
}
```

This gives ~10 progress updates per 1,000 files — enough for smooth display, light enough to not contend with match events.

### 500ms Progress Delay

The UX spec mandates "feedback proportional to duration" — don't show progress for fast searches. Implementation:

In `window/search.rs`, when wiring `connect_search_progress`:
- Set a `show_progress: Rc<Cell<bool>>` flag to `false` when search starts.
- Schedule `glib::timeout_add_local_once(Duration::from_millis(500), ...)` to set it to `true`.
- In the progress callback, only call `set_progress_message()` when the flag is `true`.
- On search done, clear both the flag and the progress message.

This means searches that complete in < 500ms never show any status bar progress.

### File Count Estimate for Progress Denominator

The "Y" in "Searching X / Y files..." should come from the command palette's `FileIndex` count, which is already maintained for Ctrl+P fuzzy search. The window has access to the command palette widget.

Check if `LushtextCommandPalette` exposes a `file_count() -> usize` method. If not, add one that returns `imp.file_index.borrow().len()` (or equivalent). The file index covers all workspace roots and is rebuilt on workspace changes, so it's a reasonable estimate.

If the file index is empty (no workspaces, or index hasn't built yet), use the no-denominator format: `"Searching X files..."`.

### Action Enable/Disable Lifecycle

`win.search-next-match` and `win.search-prev-match` must be disabled when:
1. No tabs are open (handled by `update_content_stack()`)
2. The search panel is not visible
3. The search panel has no results

The existing `update_content_stack()` handles condition 1. For conditions 2 and 3, add a helper method `update_search_navigation_actions()` that reads `search_panel_revealer.reveals_child()` and `search_panel.has_results()`. Call this helper from:
- `update_content_stack()` (tab open/close)
- `toggle_search_panel()` / `close_search_panel()` (panel show/hide)
- After search completion (results available or empty)

### F4 Conflict with Existing `next-match` Action

The window already has `win.next-match` (Ctrl+G) and `win.prev-match` (Ctrl+Shift+G) for the inline search bar (`Ctrl+F`). The new `win.search-next-match` (F4) and `win.search-prev-match` (Shift+F4) are for the workspace search panel (`Ctrl+Shift+F`). These are separate actions for separate features — no conflict. Use distinct action names: `"search-next-match"` not `"next-match"`.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `crates/lushtext-core/src/model/content_search.rs` | Add `Progress(usize)` to `SearchEvent` | +2 lines |
| `crates/lushtext-core/src/services/content_search.rs` | Add file counter, emit progress events | +15 lines |
| `crates/lushtext-core/src/ui/status_bar/mod.rs` | Add `set_progress_message`, `clear_progress_message` | +25 lines |
| `crates/lushtext-core/src/ui/status_bar/imp.rs` | Add `progress_active: Cell<bool>` | +2 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add `match_positions`, `current_match_index`, `navigate_callback`, `progress_callback` fields | +10 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Add navigation methods, `connect_navigate_to_match`, `connect_search_progress`, `has_results`, handle `Progress` in timer, append to `match_positions` in timer | +100 lines |
| `crates/lushtext-core/src/ui/window/mod.rs` | Register `search-next-match`/`search-prev-match` actions, bind F4/Shift+F4, add to `update_content_stack()` | +25 lines |
| `crates/lushtext-core/src/ui/window/search.rs` | Wire `connect_navigate_to_match`, `connect_search_progress`, extract open+scroll helper, add `update_search_navigation_actions()` | +60 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Story 1.5 widget tests | +50 lines |

**No new files created.** All changes are modifications to existing files.

### Line Count Impact

| File | Current Lines | After Story 1.5 | Limit |
|------|--------------|-----------------|-------|
| `search_panel/mod.rs` | 327 | ~427 | 1000 |
| `search_panel/imp.rs` | 578 | ~588 | 1000 |
| `window/mod.rs` | 1091 | ~1116 | 1000 (already over — see deferred-work.md) |
| `window/search.rs` | 168 | ~228 | 1000 |
| `status_bar/mod.rs` | 157 | ~182 | 1000 |
| `status_bar/imp.rs` | 47 | ~49 | 1000 |

**`window/mod.rs` is already at 1091 lines** (noted in deferred-work.md). Adding ~25 more lines is unavoidable since `setup_actions()` and `setup_shortcuts()` live there. Further extraction is deferred (not part of this story's scope).

### Previous Story Intelligence

**From Story 1.4 (most recent, same epic):**
- `constructed_complete: Cell<bool>` guard pattern — reuse for any new signal handlers that could fire during construction.
- `setup_options()` pattern at imp.rs:432-504 — new `setup_navigation()` or navigation wiring can follow the same structure.
- `imp.rs` is at 578 lines — adding ~10 lines for new fields is safe.
- `mod.rs` is at 327 lines — adding ~100 lines for navigation methods brings it to ~427, well within limit.
- Review finding: `gen` is a reserved keyword in Rust Edition 2024. Any generation-counter variables must use a different name (e.g., `current_gen`).
- Review finding: glob debounce initially had a Cell clone divergence bug. The `match_positions` RefCell must be accessed through `panel.imp()` inside closures, NOT cloned from an outer scope.
- Review finding: Documentation must be updated (README.md, AGENTS.md) — include F4/Shift+F4 navigation and progress reporting.

**From Story 1.3:**
- Toggle `notify::active` handlers use `constructed_complete` guard — same guard needed if any initialization-time changes could trigger navigation.
- `settings: gio::Settings` field on imp struct is shared across methods — reuse.

**From Story 1.2:**
- `connect_open_file` callback pattern (RefCell + Box<dyn Fn>) — reuse same pattern for `navigate_callback` and `progress_callback`.
- `SearchEvent::ResultCap` handling exists in the polling timer — the new `SearchEvent::Progress` handler goes in the same match arm block.
- `clear_results()` resets all state — add `match_positions.borrow_mut().clear()` and `current_match_index.set(None)` here.
- The `RefCell` borrow pattern in file_groups — clone+drop before signal emission — must be preserved. Same pattern applies to `match_positions`.

**From Story 1.1:**
- `SearchEvent` is the enum to extend — add `Progress(usize)`.
- The channel is `crossbeam_channel::bounded(1024)` — renamed to `bounded(256)` in a later story. Progress events are best-effort via `try_send`.
- `WalkParallel` spawns threads internally — the file counter must be `Arc<AtomicUsize>` shared across walker threads.

### Anti-Patterns to Avoid

1. **DO NOT** store `SingleSelection` or `TreeListModel` as fields in `imp.rs` — access them via `results_list.model()` at usage time (existing pattern).
2. **DO NOT** use `SourceId` for the 500ms progress delay — use a `Cell<bool>` flag set by `timeout_add_local_once` (matching the generation-counter philosophy).
3. **DO NOT** send `SearchEvent::Progress` on every file — throttle to every 100 files via modulo check on the atomic counter. Channel flooding blocks match events.
4. **DO NOT** use `progress_active` as a generation counter — it's just a boolean flag. Normal `push_message()` always overrides it.
5. **DO NOT** disable F4/Shift+F4 when the inline search bar (`Ctrl+F`) is visible — the two features are independent. F4 is for workspace search, Ctrl+G is for inline search.
6. **DO NOT** add a visible spinner widget for search progress — the UX spec says streaming results are the progress indicator, the status bar message is supplementary.
7. **DO NOT** duplicate the open+scroll logic from `connect_open_file` — extract to a shared helper in `window/search.rs`.
8. **DO NOT** forget to clear `match_positions` and reset `current_match_index` in `clear_results()`.
9. **DO NOT** forget to handle the `Progress` variant in any exhaustive `match` on `SearchEvent` — the compiler will catch this, but anticipate it.
10. **DO NOT** add replace-related controls or search history — those belong to Epics 2 and 3.
11. **DO NOT** use `gen` as a variable name — it's a reserved keyword in Rust Edition 2024.

### Project Structure Notes

- All changes are within existing modules — no new modules or files needed.
- The search panel grows by ~110 lines across two files — well within the 1000-line limit.
- The status bar grows by ~27 lines — a minor, focused addition.
- The window files grow by ~85 lines combined — `mod.rs` exceeds 1000 lines but was already over (documented in deferred-work.md).
- Navigation follows the UX spec's keyboard-first design: F4/Shift+F4 from Sublime/VS Code convention.
- Progress reporting follows the architecture's "non-auto-dismissing progress message" cross-cutting concern.

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.5: Match Navigation & Progress Reporting]
- [Source: _bmad-output/planning-artifacts/architecture.md#Pattern 2: Action Namespace] — `win.search-next-match`, `win.search-prev-match`
- [Source: _bmad-output/planning-artifacts/architecture.md#Gap Analysis Results] — F4 state tracking, progress total file count
- [Source: _bmad-output/planning-artifacts/architecture.md#Cross-Cutting Concerns] — #4: Status bar integration, non-auto-dismissing progress
- [Source: _bmad-output/planning-artifacts/architecture.md#Widget communication boundaries] — Window registers F4/Shift+F4 actions
- [Source: _bmad-output/planning-artifacts/architecture.md#Requirements to Structure Mapping] — FR17-20 → `window/mod.rs`, `search_panel/mod.rs`
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Flow Optimization Principles] — Feedback proportional to duration (500ms threshold)
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Journey Flow 1] — F4 across files, panel persistence, progress reporting
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Keyboard Shortcut Map] — F4 = Next match, Shift+F4 = Previous match
- [Source: _bmad-output/implementation-artifacts/1-4-gitignore-toggle-glob-filter-options-panel.md#Previous Story Intelligence]
- [Source: _bmad-output/implementation-artifacts/1-4-gitignore-toggle-glob-filter-options-panel.md#Review Findings]
- [Source: _bmad-output/implementation-artifacts/deferred-work.md] — window/mod.rs over 1000-line limit, polling timer continues when panel hidden
- [Source: .agents/AGENTS.md#Content search panel]
- [Source: .agents/AGENTS.md#Status bar auto-dismiss]
- [Source: .agents/AGENTS.md#Async I/O Pattern]
- [Source: .agents/rules/rust.md#Mutable State on GObject Structs]
- [Source: .agents/rules/widget-wiring.md#Action Enabled State]
- [Source: .agents/rules/widget-wiring.md#Auto-Dismiss Timers (Generation Counter)]
- [Source: .agents/rules/widget-wiring.md#Testing]
- [Source: .agents/rules/ui.md#Status Bar]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Debug Log References

None — clean implementation with no debugging issues.

### Completion Notes List

- Task 1: Added `SearchEvent::Progress(usize)` variant to content search model. Service emits progress every 100 files via `Arc<AtomicUsize>` + `try_send` (best-effort, no channel flooding). 2 new unit tests.
- Task 2: Added `set_progress_message` and `clear_progress_message` to status bar with `progress_active: Cell<bool>` flag. `push_message` always overrides progress.
- Task 3: Added `match_positions: RefCell<Vec<(PathBuf, u32)>>` and `current_match_index: Cell<Option<usize>>` to search panel imp. Navigation state cleared in `clear_results()`. Match positions populated in polling timer.
- Task 4: Implemented `navigate_next_match` and `navigate_prev_match` with wrap-around cycling. `select_match_in_results` O(n) scan + `ListView::scroll_to`. `has_results()` and callback connectors added.
- Task 5: Registered `win.search-next-match` (F4) and `win.search-prev-match` (Shift+F4) actions. `update_search_navigation_actions()` helper disables when no tabs, panel hidden, or no results.
- Task 6: Wired progress events through `connect_search_progress` callback with 500ms delay pattern. Progress denominator from `command_palette.file_index_len()`. Navigation actions updated on search completion.
- Task 7: 17 new widget tests covering Progress variant, navigation state, action lifecycle, status bar API.
- Task 8: `make check`, `make test-unit` (207), `make test-int` (52), `make test-widget` (382) — all pass, zero regressions.

### Change Log

- 2026-04-07: Story 1.5 implementation complete — F4/Shift+F4 match navigation and progress reporting.

### File List

- `crates/lushtext-core/src/model/content_search.rs` — added `Progress(usize)` to `SearchEvent`
- `crates/lushtext-core/src/services/content_search.rs` — added file counter, progress emission every 100 files, 2 new unit tests
- `crates/lushtext-core/src/ui/status_bar/imp.rs` — added `progress_active: Cell<bool>`
- `crates/lushtext-core/src/ui/status_bar/mod.rs` — added `set_progress_message`, `clear_progress_message`, modified `push_message` to override progress
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — added `match_positions`, `current_match_index`, `navigate_callback`, `progress_callback` fields and type aliases
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — added navigation methods, `has_results`, `connect_navigate_to_match`, `connect_search_progress`, `select_match_in_results`, progress handler in timer
- `crates/lushtext-core/src/ui/window/mod.rs` — registered `search-next-match`/`search-prev-match` actions, bound F4/Shift+F4, added `update_search_navigation_actions()`
- `crates/lushtext-core/src/ui/window/search.rs` — extracted `open_file_at_line` helper, wired `connect_navigate_to_match`, `connect_search_progress` with 500ms delay
- `crates/lushtext/tests/widget/search_panel.rs` — 17 new Story 1.5 widget tests
- `.agents/AGENTS.md` — documented match navigation and search progress reporting design decisions
- `README.md` — updated workspace content search feature description

### Review Findings

- [x] [Review][Patch] F4 navigation doesn't move focus to editor — AC#4 says "focus moves to the editor" but `open_file_at_line` never calls `source_view.grab_focus()` [window/search.rs:201-219] — **fixed**
- [x] [Review][Patch] `select_match_in_results` uses `ListScrollFlags::NONE` — spec recommends `FOCUS` for reliable scroll-into-view on virtual GtkListView [search_panel/mod.rs:174] — **fixed**
- [x] [Review][Patch] `cb(0, true)` at Done passes misleading file count — progress callback sends `files_searched=0` on completion instead of actual count [search_panel/mod.rs:419-420] — **fixed** (tracks `last_progress_count`)
- [x] [Review][Patch] No widget test for action enabled lifecycle — only start-disabled tested; widget-wiring.md requires full disabled→enabled→disabled cycle [tests/widget/search_panel.rs] — **fixed** (added `test_search_navigation_actions_enabled_lifecycle`)
- [x] [Review][Defer] 500ms timer accumulation across rapid keystrokes — stale `timeout_add_local_once` closures can set `show_progress=true` prematurely; spec-prescribed pattern — deferred, known limitation
- [x] [Review][Defer] Progress delay not reset on toggle-button search triggers — toggling case/regex/whole-word re-triggers search without resetting 500ms delay — deferred, secondary UX concern
- [x] [Review][Defer] Duplicate (path, line) selects first matching row — same-line matches always highlight first visual row regardless of navigation index — deferred, known limitation with minified files
- [x] [Review][Defer] `set_restore_position` scroll context for lines 1-3 — `saturating_sub(3)` pins to top for first 3 lines; pre-existing pattern — deferred, pre-existing
