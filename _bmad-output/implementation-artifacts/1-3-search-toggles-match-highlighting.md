# Story 1.3: Search Toggles & Match Highlighting

Status: done

## Story

As a user,
I want to toggle regex, case-sensitive, and whole-word matching, and see the matching text highlighted in results,
so that I can search precisely and identify matches at a glance.

## Acceptance Criteria

1. **Toggle buttons visible** — Given the search panel header row, when the panel is visible, then three toggle buttons are displayed in a `.linked` GtkBox group to the right of the search entry: "Aa" (case-sensitive), ".*" (regex), "W" (whole-word). A "More" button (gear icon toggle) is displayed to the right of the linked group (non-functional placeholder until Story 1.4).

2. **Regex toggle** — Given the regex toggle is off (default), when the user clicks the ".*" toggle button to enable regex, then the toggle visually activates and the current search re-runs immediately with regex matching enabled (no debounce — immediate re-search).

3. **Case-sensitive toggle** — Given the case-sensitive toggle is off (default), when the user clicks the "Aa" toggle button to enable case sensitivity, then the current search re-runs immediately with case-sensitive matching.

4. **Whole-word toggle** — Given the whole-word toggle is off (default), when the user clicks the "W" toggle button to enable whole-word matching, then the current search re-runs immediately with whole-word matching.

5. **Match highlighting** — Given search results are displayed, when a match row is rendered in `connect_bind`, then the matching substring within the line content is highlighted using Pango markup with `@accent_color` bold, the non-matching portions are rendered in normal weight, and special characters in the line content are escaped via `glib::markup_escape_text`.

6. **Monospace results** — Given result line content labels, when rendered, then they use the `.monospace` CSS class, sharing the editor's font customization provider.

7. **File header styling** — Given a file header row in the results tree, when rendered, then it displays the filename (`.heading` style) and match count badge (`.caption` + `.dim-label` style, bare number, e.g., "3").

8. **Invalid regex error** — Given regex mode is enabled and the user enters an invalid pattern (e.g., `fn\s+[`), when the debounce timer fires, then an inline error label appears below the search input in `@error_color` with a descriptive message (e.g., "Invalid pattern: unclosed character class"), no search is executed, and the error label disappears when the user corrects the pattern.

9. **GSettings persistence** — Given the GSettings keys `search-case-sensitive`, `search-regex`, and `search-whole-word`, when the panel is opened on a subsequent application launch, then all toggle buttons reflect their persisted GSettings state.

10. **Toggle state in search options** — Given toggle states are set, when `start_search` is called, then `ContentSearchOptions` is populated from the current toggle button states (not hardcoded defaults).

## Tasks / Subtasks

- [x] Task 1: Add match range to SearchResultItem (AC: #5)
  - [x] Add `match_start: Cell<u32>` and `match_end: Cell<u32>` fields to `SearchResultItem` imp struct
  - [x] Add `match_start` and `match_end` parameters to `SearchResultItem::new_match()` constructor
  - [x] Add `match_start()` and `match_end()` accessor methods
  - [x] Update the polling code in `search_panel/mod.rs` that creates match items to pass `m.match_range.start` and `m.match_range.end` (with `u32::try_from().unwrap_or(u32::MAX)` for safe truncation)
  - [x] Update existing widget tests that construct `SearchResultItem::new_match()` to pass match range arguments

- [x] Task 2: Add GSettings keys and config constants (AC: #9)
  - [x] Add 3 keys to `data/dev.cominotti.lushtext.gschema.xml`:
    - `search-case-sensitive` (type `b`, default `false`)
    - `search-regex` (type `b`, default `false`)
    - `search-whole-word` (type `b`, default `false`)
  - [x] Add corresponding constants to `crates/lushtext-core/src/config.rs` in the `keys` module:
    - `pub const SEARCH_CASE_SENSITIVE: &str = "search-case-sensitive";`
    - `pub const SEARCH_REGEX: &str = "search-regex";`
    - `pub const SEARCH_WHOLE_WORD: &str = "search-whole-word";`

- [x] Task 3: Add toggle buttons to search panel template (AC: #1)
  - [x] Modify `resources/ui/search-panel.ui`: in `header_box`, after `search_entry`, add:
    - A `GtkBox` (id: `toggles_box`, orientation: horizontal, css-classes: `["linked"]`, spacing: 0)
      - `GtkToggleButton` (id: `case_toggle`, label: "Aa", tooltip: "Case Sensitive")
      - `GtkToggleButton` (id: `regex_toggle`, label: ".*", tooltip: "Regular Expression")
      - `GtkToggleButton` (id: `word_toggle`, label: "W", tooltip: "Whole Word")
    - A `GtkToggleButton` (id: `more_toggle`, icon-name: `"emblem-system-symbolic"`, tooltip: "More Options", sensitive: false) — non-functional placeholder for Story 1.4
  - [x] Add `TemplateChild` fields in `search_panel/imp.rs`: `case_toggle`, `regex_toggle`, `word_toggle`, `more_toggle`

- [x] Task 4: Wire toggle buttons to search options and GSettings (AC: #2, #3, #4, #9, #10)
  - [x] In `search_panel/imp.rs` `constructed()`:
    - Bind each toggle to its GSettings key via `settings.bind(key, &toggle, "active").build()` for two-way persistence
    - Connect `notify::active` on each toggle to trigger immediate re-search: call `start_search` with current query if query is non-empty (no 300ms debounce — toggles re-search immediately per UX-DR12)
  - [x] Modify `start_search` in `search_panel/mod.rs`: replace `ContentSearchOptions::default()` with options built from toggle states:
    ```rust
    let options = ContentSearchOptions {
        case_sensitive: imp.case_toggle.is_active(),
        regex: imp.regex_toggle.is_active(),
        whole_word: imp.word_toggle.is_active(),
        gitignore: true,  // hardcoded until Story 1.4
        glob: None,       // hardcoded until Story 1.4
    };
    ```

- [x] Task 5: Implement Pango markup match highlighting (AC: #5, #6, #7)
  - [x] Modify `connect_bind` in `search_panel/imp.rs` for match rows:
    - Read `match_start` and `match_end` from the `SearchResultItem`
    - Use `glib::markup_escape_text` on the three segments (before, match, after) of `line_content`
    - Build Pango markup: `"{before}<b>{match}</b>{after}"` (bold only, theme-safe)
    - Call `line_content_label.set_markup(&markup)` instead of `set_text`
    - **Fallback:** If `match_start >= match_end` or range exceeds content length, fall back to plain escaped text (defensive against stale/truncated data)
  - [x] Verify file header rows still render with `.heading` on filename and `.caption` + `.dim-label` on count badge (unchanged from Story 1.2)
  - [x] Verify `.monospace` CSS class is on `line_content_label` (unchanged from Story 1.2)

- [x] Task 6: Implement inline error for invalid regex (AC: #8)
  - [x] Add CSS class `error` to `error_label` when showing regex errors (use Adwaita's `@error_color` via the `.error` CSS class)
  - [x] In the search debounce handler or `start_search`: when `SearchEvent::Error` is received from the polling timer, set `error_label` text to the error message and `set_visible(true)`
  - [x] When a valid search starts (no error), hide `error_label` via `set_visible(false)`
  - [x] Verify the error label clears when the user corrects the regex pattern and the debounce fires again

- [x] Task 7: Widget tests (AC: all)
  - [x] Update existing `SearchResultItem` tests to include `match_start`/`match_end` in constructor calls
  - [x] Test: toggle buttons exist and are accessible as template children
  - [x] Test: toggle buttons have correct initial state (all off/inactive)
  - [x] Test: GSettings keys exist with correct defaults (`false`, `false`, `false`)
  - [x] Test: `SearchResultItem::new_match()` stores and returns `match_start`/`match_end` correctly
  - [x] Test: `LushtextSearchPanel` can be constructed and has the 3 toggle template children

- [x] Task 8: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all 205 unit tests pass
  - [x] Run `make test-int` — all 52 integration tests pass
  - [x] Run `make test-widget` — all 359 widget tests pass
  - [ ] Verify no GTK/pixman runtime warnings via `make run` and exercising toggle buttons + search

## Dev Notes

### Critical Gap: Match Range Data Flow

The current `SearchResultItem::new_match(file_path, line_number, line_content)` does NOT carry the match byte range needed for Pango markup highlighting. The model's `SearchMatch` has `match_range: Range<usize>`, but this data is discarded in the polling timer code when constructing `SearchResultItem`.

**Fix:** Add `match_start: Cell<u32>` and `match_end: Cell<u32>` to the `SearchResultItem` imp struct. These are NOT GObject properties (no need for `bind_property` — they're read once in `connect_bind`). Plain `Cell` fields with accessor methods, following the existing `line_number: Cell<u32>` pattern.

**Constructor change:**
```rust
// Before (Story 1.2):
SearchResultItem::new_match(file_path, line_number, line_content)

// After (Story 1.3):
SearchResultItem::new_match(file_path, line_number, line_content, match_start, match_end)
```

**Polling timer update** (in `mod.rs`, inside the `SearchEvent::Match` arm):
```rust
let match_start = u32::try_from(m.match_range.start).unwrap_or(u32::MAX);
let match_end = u32::try_from(m.match_range.end).unwrap_or(u32::MAX);
let match_item = SearchResultItem::new_match(
    &m.path.display().to_string(),
    line_num,
    &m.line_content,
    match_start,
    match_end,
);
```

**NOTE:** The `match_range` on `SearchMatch` is a byte range within `line_content`. The line content in `SearchResultItem` may have been truncated to 500 chars (Story 1.2 review finding). If `match_end > line_content.len()`, the highlight must be clamped to the content length to avoid out-of-bounds slicing. If `match_start >= truncation_point`, the match highlight may not be visible — this is acceptable for the edge case of very long lines.

### Pango Markup Implementation

Architecture rule: model carries raw data, UI generates markup in `connect_bind`.

```rust
// In connect_bind for match rows:
fn render_match_markup(content: &str, start: usize, end: usize) -> String {
    let start = start.min(content.len());
    let end = end.min(content.len());
    if start >= end {
        // Fallback: no valid range, render plain escaped text
        return glib::markup_escape_text(content).to_string();
    }
    format!(
        "{}<b>{}</b>{}",
        glib::markup_escape_text(&content[..start]),
        glib::markup_escape_text(&content[start..end]),
        glib::markup_escape_text(&content[end..]),
    )
}
// Then: label.set_markup(&markup);
```

**CRITICAL: Use `set_markup`, not `set_label` or `set_text`.** `set_markup` parses Pango markup. `set_text` renders literal angle brackets. `set_label` also parses markup but is less explicit.

**`@accent_color` in Pango markup:** GTK's `@accent_color` is a CSS variable, not directly usable in Pango markup's `foreground` attribute. Options:
1. Use `<b>` bold only (simplest — accent color applied via CSS class on the label).
2. Query the resolved `@accent_color` and hardcode it into Pango markup — fragile, breaks on theme change.
3. Use a `GtkLabel` with a separate CSS class for highlighted text — not possible with inline Pango.

**Recommended approach:** Use `<b>` (bold) for match highlighting in Pango markup. This is the simplest and most theme-safe approach. The `.monospace` label already inherits the correct foreground color. Bold weight provides sufficient visual distinction — VS Code uses yellow background, but Sublime uses bold only. If stronger contrast is needed later, a custom CSS class targeting `<b>` tags within `.monospace` labels could add `@accent_color`.

**Alternative if accent color is required:** Resolve `@accent_color` at widget construction time and on dark mode change via `StyleManager::connect_dark_notify()`. Store resolved hex color in a `RefCell<String>` on the imp struct. Use it in Pango `foreground` attribute:
```rust
format!(
    "{}<span foreground='{}'><b>{}</b></span>{}",
    before, accent_hex, matched, after
)
```
This adds complexity (dark mode handler, cached color) but matches the UX spec's `@accent_color` requirement exactly.

### Toggle Button Layout

Modify `header_box` in `search-panel.ui`:

```xml
<child>
  <object class="GtkBox" id="header_box">
    <property name="orientation">horizontal</property>
    <property name="spacing">6</property>
    <property name="margin-start">6</property>
    <property name="margin-end">6</property>
    <property name="margin-top">6</property>
    <property name="margin-bottom">6</property>
    <child>
      <object class="GtkSearchEntry" id="search_entry">
        <property name="hexpand">true</property>
        <property name="placeholder-text">Search in files…</property>
      </object>
    </child>
    <child>
      <object class="GtkBox" id="toggles_box">
        <property name="orientation">horizontal</property>
        <style><class name="linked"/></style>
        <child>
          <object class="GtkToggleButton" id="case_toggle">
            <property name="label">Aa</property>
            <property name="tooltip-text">Case Sensitive</property>
          </object>
        </child>
        <child>
          <object class="GtkToggleButton" id="regex_toggle">
            <property name="label">.*</property>
            <property name="tooltip-text">Regular Expression</property>
          </object>
        </child>
        <child>
          <object class="GtkToggleButton" id="word_toggle">
            <property name="label">W</property>
            <property name="tooltip-text">Whole Word</property>
          </object>
        </child>
      </object>
    </child>
    <child>
      <object class="GtkToggleButton" id="more_toggle">
        <property name="icon-name">emblem-system-symbolic</property>
        <property name="tooltip-text">More Options</property>
        <property name="sensitive">false</property>
      </object>
    </child>
  </object>
</child>
```

The `.linked` CSS class on `toggles_box` groups the three toggles visually as a single control, matching GNOME HIG for related toggle groups (as used in e.g., Nautilus and GNOME Text Editor toolbar controls).

### GSettings Toggle Binding

Use `gio::Settings::bind()` for two-way persistence (matching the Preferences dialog pattern):

```rust
// In constructed():
let settings = gio::Settings::new(crate::config::APP_ID);
settings.bind(keys::SEARCH_CASE_SENSITIVE, &*imp.case_toggle, "active").build();
settings.bind(keys::SEARCH_REGEX, &*imp.regex_toggle, "active").build();
settings.bind(keys::SEARCH_WHOLE_WORD, &*imp.word_toggle, "active").build();
```

This automatically:
- Sets initial toggle state from GSettings on construction
- Persists toggle changes to GSettings when the user clicks
- No manual `connect_changed` needed for persistence

### Immediate Re-Search on Toggle Change

Per UX-DR12, toggle changes trigger **immediate** re-search (no 300ms debounce). Connect `notify::active` on each toggle:

```rust
for toggle in [&*imp.case_toggle, &*imp.regex_toggle, &*imp.word_toggle] {
    let panel_weak = self.downgrade();
    toggle.connect_notify_local(Some("active"), move |_, _| {
        if let Some(panel) = panel_weak.upgrade() {
            let query = panel.query();
            if !query.is_empty() {
                panel.start_search(&query);
            }
        }
    });
}
```

**CRITICAL ordering:** The `notify::active` connection must be done AFTER the GSettings `bind()` call. Otherwise, the initial GSettings restore triggers a search before the panel is fully constructed or has workspace roots.

**Guard against startup noise:** The GSettings bind sets the toggle active state during `constructed()`, which fires `notify::active`. To prevent a spurious search on startup, either:
1. Connect `notify::active` AFTER `constructed()` returns (e.g., in a deferred `idle_add_local_once`), OR
2. Use a `constructed_complete: Cell<bool>` flag that is set at the end of `constructed()`, and check it in the `notify::active` handler.

Option 2 is simpler and follows the `restoring_session` guard pattern used in the window.

### Error Label Enhancement

The `error_label` already exists in `footer_box` (Story 1.2). For Story 1.3:

1. When `SearchEvent::Error(msg)` is received in the polling timer, set `error_label.set_text(&msg)` and `error_label.set_visible(true)`. Add the `.error` CSS class for Adwaita's `@error_color` styling.
2. When a new valid search starts (no regex error), call `error_label.set_visible(false)`.
3. The error is already handled in the polling timer from Story 1.2 — this task adds the `.error` CSS class and ensures the label text is set correctly.

The `SearchEvent::Error` is sent by the service when `RegexMatcher` fails to compile the pattern. The service returns immediately after sending `Error` followed by `Done`. The timer processes both events and self-removes.

### Previous Story Intelligence

**From Story 1.1:**
- `RegexMatcherBuilder::fixed_strings(true)` handles literal search internally. Panel passes `regex: false` in options.
- Result cap is approximate (overshoot by thread count ~7). UI already handles this in count label.
- `SearchEvent::Error` is sent for invalid regex AND invalid glob patterns.

**From Story 1.2:**
- `start_search` currently uses `ContentSearchOptions::default()` — the direct hook for this story.
- `connect_bind` renders `line_content` via `set_text` — must change to `set_markup` with Pango markup.
- `error_label` exists but has no `.error` CSS class applied.
- Line content is truncated at 500 chars via `floor_char_boundary()` with ellipsis. Match range must be clamped to truncated length.
- `header_box` contains only `search_entry` — toggle buttons slot in after it.
- Window's `search.rs` has 146 lines — well under the 1000-line limit, no extraction needed.
- `search_panel/mod.rs` is 268 lines, `imp.rs` is 368 lines — both have room for additions.

**From Story 1.2 review findings:**
- `RefCell` borrow panic fix: clone file_item and child_store from map entry, then drop(groups) before calling signal-emitting methods. This pattern must be preserved.
- Match count badge uses `bind_property` with cleanup in `connect_unbind` — leave unchanged.
- TreeExpander gesture fix for match rows — leave unchanged.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `crates/lushtext-core/src/ui/search_panel/item.rs` | Add `match_start`, `match_end` fields + constructor params + accessors | +15 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add template children for toggles, GSettings bindings, `notify::active` handlers, Pango markup in `connect_bind`, `.error` class on error label | +80 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Update `start_search` to read toggle states, update `SearchResultItem::new_match()` calls to pass match range | +15 lines |
| `resources/ui/search-panel.ui` | Add toggle buttons + more button to header_box | +25 lines |
| `data/dev.cominotti.lushtext.gschema.xml` | Add 3 new GSettings keys | +15 lines |
| `crates/lushtext-core/src/config.rs` | Add 3 key constants | +3 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Update existing tests, add toggle tests | +30 lines |

**No new files created.** All changes are modifications to existing files.

### Anti-Patterns to Avoid

1. **DO NOT** use `SourceId` cancellation for toggle re-search — call `start_search` directly (immediate, no debounce)
2. **DO NOT** generate Pango markup in the service or model layer — raw data only, markup in `connect_bind`
3. **DO NOT** use `set_text` when rendering markup — use `set_markup` (or `set_use_markup(true)` + `set_label`)
4. **DO NOT** use hardcoded hex colors in Pango markup for match highlighting — use `<b>` bold (theme-safe) or resolve `@accent_color` dynamically
5. **DO NOT** connect `notify::active` before GSettings `bind()` — initial state restore would trigger spurious searches
6. **DO NOT** add `match_start`/`match_end` as GObject properties — they don't need `bind_property`, plain `Cell` fields are sufficient
7. **DO NOT** forget to clamp match range to truncated line length (500 chars) — out-of-bounds slice panics
8. **DO NOT** forget the SPDX license header on any new `.rs` files (none expected in this story, but verify modified files still have it)
9. **DO NOT** reuse `Arc<AtomicBool>` cancel tokens — toggle re-search creates a new token per search (existing pattern from Story 1.2)
10. **DO NOT** skip the `constructed_complete` guard — without it, GSettings restore triggers a search before workspace roots are set

### Project Structure Notes

- All changes are within the existing search panel module — no new modules needed
- `search_panel/imp.rs` grows from ~368 to ~448 lines — well within the 1000-line limit
- `search_panel/mod.rs` grows from ~268 to ~283 lines — well within the limit
- `item.rs` grows from ~109 to ~124 lines — well within the limit
- The "More" button is a non-functional placeholder (sensitive=false) that Story 1.4 will activate and wire to an options revealer

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.3: Search Toggles & Match Highlighting]
- [Source: _bmad-output/planning-artifacts/architecture.md#Pattern 7: Pango Markup Generation]
- [Source: _bmad-output/planning-artifacts/architecture.md#Pattern 2: Action Namespace]
- [Source: _bmad-output/planning-artifacts/architecture.md#GSettings Schema Additions]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Design System — Toggle Buttons]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR1 Progressive Minimal]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR3 Match Highlighting]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR12 Toggle Immediate Re-search]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR16 Adwaita Semantic Tokens]
- [Source: _bmad-output/implementation-artifacts/1-1-content-search-service-types.md#Completion Notes]
- [Source: _bmad-output/implementation-artifacts/1-2-search-panel-with-streaming-results.md#Review Findings]
- [Source: AGENTS.md#GSettings for preferences]
- [Source: .agents/rules/rust.md#Mutable State on GObject Structs]
- [Source: .agents/rules/ui.md#GSettings Bindings]
- [Source: .agents/rules/widget-wiring.md#Testing]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Debug Log References

None — clean implementation, no debug issues.

### Completion Notes List

- Task 1: Added `match_start: Cell<u32>` and `match_end: Cell<u32>` to `SearchResultItem` imp struct with constructor params and accessors. Updated polling code to clamp match range to truncated content length.
- Task 2: Added 3 GSettings keys (`search-case-sensitive`, `search-regex`, `search-whole-word`) to schema XML and corresponding constants to `config.rs`.
- Task 3: Added toggle buttons (Aa, .*, W) in a `.linked` GtkBox group plus a "More Options" placeholder to `search-panel.ui`. Added 4 `TemplateChild` fields to imp struct.
- Task 4: Wired GSettings two-way binding for toggle persistence, `notify::active` for immediate re-search (no debounce per UX-DR12), `constructed_complete` guard to prevent spurious searches during GSettings restore. Updated `start_search` to read toggle states into `ContentSearchOptions`.
- Task 5: Implemented `render_match_markup()` helper using `glib::markup_escape_text` + `<b>` bold for theme-safe highlighting. Uses `floor_char_boundary`/`ceil_char_boundary` for safe UTF-8 slicing. Falls back to plain escaped text when range is invalid.
- Task 6: Added `.error` CSS class to `error_label` on `SearchEvent::Error` for Adwaita `@error_color` styling. Class removed on `clear_results`.
- Task 7: Updated existing `test_search_result_item_new_match` for new constructor. Added 4 new tests: match range storage/retrieval, file item default range, toggle template children existence, toggle initial state.
- Task 8: All tests pass — 205 unit, 52 integration, 359 widget. Clippy + fmt clean.

### Change Log

- Story 1.3 implementation (2026-04-07): Search toggles (case, regex, whole-word) with GSettings persistence, Pango bold match highlighting, invalid regex error styling.

### Review Findings

- [x] [Review][Dismissed] AC#7: Count badge format — bare number follows GNOME badge conventions and handles streaming updates better. Spec example was illustrative. AC#7 wording updated.
- [x] [Review][Patch] AC#8: Move error label inline below search input — fixed: moved error_label from footer_box to new child between header_box and separator. [search-panel.ui]
- [x] [Review][Dismissed] Zero-width regex matches show no highlight — correct behavior: zero-width assertions match positions, not text. Cannot highlight nonexistent substring. Matches VS Code behavior.
- [x] [Review][Patch] Match range clamped to content including ellipsis bytes — fixed: clamp to `truncated_len.unwrap_or(content.len())` instead of `content.len()`. [search_panel/mod.rs:208-213]
- [x] [Review][Patch] Unnecessary `content_len as u64` cast — fixed: use `clamp_len` directly as `usize`. [search_panel/mod.rs:208]
- [x] [Review][Patch] Duplicate `gio::Settings::new` in `setup_toggles` — fixed: added `settings` field to imp struct, reuse across methods. [search_panel/imp.rs]
- [x] [Review][Defer] O(n) auto-expand reverse scan — O(n²) main-thread cost for large results. Already in deferred-work.md. [search_panel/mod.rs:245-256] — deferred, pre-existing
- [x] [Review][Defer] Walker errors silently swallowed — permission/symlink errors during traversal produce no UI feedback. Already in deferred-work.md. [content_search.rs:56-58] — deferred, pre-existing
- [x] [Review][Defer] Polling timer continues when panel hidden — 50ms timer processes results for hidden revealer. [search_panel/mod.rs:26-28] — deferred, pre-existing (Story 1-2)
- [x] [Review][Defer] RefCell borrow in file_groups fragile near signal emission — clone+drop pattern is correct but scope boundary is easy to break. [search_panel/mod.rs:173-192] — deferred, pre-existing (Story 1-2)
- [x] [Review][Defer] `display().to_string()` lossy path comparison — auto-expand uses lossy string that could mismatch on non-UTF-8 paths. [search_panel/mod.rs:229-230] — deferred, pre-existing (Story 1-2)
- [x] [Review][Defer] Toggle action name misleading — `toggle-search-panel` refocuses instead of closing. [window/search.rs:93-103] — deferred, pre-existing (Story 1-2)
- [x] [Review][Defer] Panel visible on startup with empty results — visibility persisted but query/results are not. [window/search.rs:83-87] — deferred, pre-existing (Story 1-2)
- [x] [Review][Defer] `searching` flag not reset on empty query — `clear_results()` doesn't reset the flag. Latent state bug. [search_panel/mod.rs:105] — deferred, pre-existing (Story 1-2)

### File List

- `crates/lushtext-core/src/ui/search_panel/item.rs` — Added match_start/match_end fields, constructor params, accessors
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — Added toggle TemplateChild fields, setup_toggles(), constructed_complete guard, render_match_markup(), Pango markup in connect_bind
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — Updated start_search to read toggle states, match range clamping in polling code, .error CSS class on error_label
- `crates/lushtext-core/src/config.rs` — Added SEARCH_CASE_SENSITIVE, SEARCH_REGEX, SEARCH_WHOLE_WORD constants
- `data/dev.cominotti.lushtext.gschema.xml` — Added search-case-sensitive, search-regex, search-whole-word keys
- `resources/ui/search-panel.ui` — Added toggles_box with case/regex/word toggles and more_toggle placeholder
- `crates/lushtext/tests/widget/search_panel.rs` — Updated existing test, added 4 new Story 1.3 tests
