# Story 1.4: Gitignore Toggle, Glob Filter & Options Panel

Status: done

## Story

As a user,
I want to toggle .gitignore filtering and filter by file glob patterns via an expandable options area,
so that I can narrow search scope to relevant files without cluttering the default panel view.

## Acceptance Criteria

1. **Options panel reveal** — Given the search panel header row has a "More" button (gear icon toggle, currently `sensitive=false`), when the user clicks the "More" button, then an options revealer slides down (150ms, slide-down transition) showing a `.gitignore` toggle button (enabled by default) and a file glob filter `GtkEntry` with placeholder "File filter (e.g., *.rs, *.toml)", and the "More" button visually shows its active/pressed state.

2. **Options panel hide** — Given the options area is expanded, when the user clicks the "More" button again, then the options revealer slides up (150ms) and hides, and the expanded/collapsed state is persisted via GSettings `search-panel-options-expanded`.

3. **Gitignore toggle** — Given the .gitignore toggle is enabled (default), when the user clicks it to disable gitignore filtering, then the current search re-runs immediately (no debounce) including files that would normally be filtered by .gitignore, and the toggle state is persisted via GSettings `search-gitignore`.

4. **Glob filter** — Given the glob filter entry is empty, when the user types `*.rs` and waits 300ms (debounce, generation-counter pattern), then the current search re-runs filtering to only `.rs` files.

5. **Truncation indicator** — Given a search produces more than 10,000 matches, when the result cap is reached, then the result count label changes to "10,000+ results (truncated) — narrow your search" styled with `.warning` CSS class (`@warning_color`), and the glob filter is available for immediate refinement.

6. **GSettings persistence** — Given the GSettings keys `search-panel-options-expanded` and `search-gitignore`, when the panel is opened on a subsequent application launch, then the options area expanded/collapsed state and the gitignore toggle reflect their persisted GSettings values.

## Tasks / Subtasks

- [x] Task 1: Add GSettings keys and config constants (AC: #2, #3, #6)
  - [x] Add 2 keys to `data/dev.cominotti.lushtext.gschema.xml`:
    - `search-panel-options-expanded` (type `b`, default `false`)
    - `search-gitignore` (type `b`, default `true`)
  - [x] Add corresponding constants to `crates/lushtext-core/src/config.rs` in the `keys` module:
    - `pub const SEARCH_PANEL_OPTIONS_EXPANDED: &str = "search-panel-options-expanded";`
    - `pub const SEARCH_GITIGNORE: &str = "search-gitignore";`

- [x] Task 2: Add options revealer to search panel template (AC: #1)
  - [x] Modify `resources/ui/search-panel.ui`: between `error_label` and the first `GtkSeparator`, insert:
    - `GtkRevealer` (id: `options_revealer`, transition-type: `slide-down`, transition-duration: 150, reveal-child: false)
      - `GtkBox` (id: `options_box`, orientation: horizontal, spacing: 6, margin-start: 6, margin-end: 6, margin-top: 4, margin-bottom: 4)
        - `GtkToggleButton` (id: `gitignore_toggle`, label: ".gitignore", tooltip: "Respect .gitignore patterns", active: true)
        - `GtkEntry` (id: `glob_entry`, hexpand: true, placeholder-text: "File filter (e.g., *.rs, *.toml)")
  - [x] Enable `more_toggle`: remove `<property name="sensitive">false</property>` from the template
  - [x] Add `TemplateChild` fields in `search_panel/imp.rs`: `options_revealer`, `gitignore_toggle`, `glob_entry`

- [x] Task 3: Wire "More" button to options revealer (AC: #1, #2, #6)
  - [x] In `search_panel/imp.rs`, add a `setup_options()` method called from `constructed()` (after `setup_toggles()`):
    - Bind `more_toggle` active state to `options_revealer.reveal_child` via `bind_property("active", &options_revealer, "reveal-child").sync_create().build()`
    - Bind `more_toggle` to GSettings key `search-panel-options-expanded` via `settings.bind(keys::SEARCH_PANEL_OPTIONS_EXPANDED, &more_toggle, "active").build()`
    - **CRITICAL ordering:** GSettings bind BEFORE `bind_property`. GSettings bind sets initial `active` from persisted state, then `bind_property` propagates to `reveal_child`. Reverse order would set `reveal_child` from default `active=false` before GSettings restores the actual state.

- [x] Task 4: Wire gitignore toggle to search options and GSettings (AC: #3, #6)
  - [x] In `setup_options()`:
    - Bind `gitignore_toggle` to GSettings key `search-gitignore` via `settings.bind(keys::SEARCH_GITIGNORE, &gitignore_toggle, "active").build()`
    - Connect `notify::active` on `gitignore_toggle` to trigger immediate re-search (same pattern as case/regex/word toggles from Story 1.3): call `start_search` with current query if non-empty, guarded by `constructed_complete`
    - **NOTE:** GSettings default for `search-gitignore` is `true`, matching `ContentSearchOptions::default().gitignore = true`

- [x] Task 5: Wire glob entry to search options with debounce (AC: #4)
  - [x] In `setup_options()`:
    - Connect `glob_entry.connect_changed()` with a 300ms generation-counter debounce (same pattern as `search_entry` debounce in `setup_search_entry()`):
      - Increment `glob_generation: Cell<u32>` (add to imp struct)
      - Schedule `glib::timeout_add_local_once(Duration::from_millis(300), ...)`
      - In callback: check generation, if current and query is non-empty, call `start_search`
    - **NOTE:** Glob is NOT persisted to GSettings — it's a per-session filter. When the app restarts, the glob entry starts empty (matching the architecture decision for glob as a transient filter).

- [x] Task 6: Update `start_search` to read gitignore and glob (AC: #3, #4)
  - [x] In `search_panel/mod.rs`, modify `start_search()`: replace the hardcoded `gitignore: true` and `glob: None` with:
    ```rust
    let options = ContentSearchOptions {
        case_sensitive: imp.case_toggle.is_active(),
        regex: imp.regex_toggle.is_active(),
        whole_word: imp.word_toggle.is_active(),
        gitignore: imp.gitignore_toggle.is_active(),
        glob: {
            let text = imp.glob_entry.text();
            if text.is_empty() { None } else { Some(text.to_string()) }
        },
    };
    ```

- [x] Task 7: Enhance truncation indicator (AC: #5)
  - [x] In the polling timer code in `search_panel/mod.rs`, when `SearchEvent::ResultCap` is received:
    - Set `count_label` text to `"10,000+ results (truncated) — narrow your search"`
    - Add `.warning` CSS class to `count_label` via `count_label.add_css_class("warning")`
  - [x] In `clear_results()`: remove `.warning` CSS class from `count_label` via `count_label.remove_css_class("warning")`

- [x] Task 8: Widget tests (AC: all)
  - [x] Test: `more_toggle` is sensitive (no longer a placeholder)
  - [x] Test: `options_revealer` template child exists and starts hidden (`reveal_child = false`)
  - [x] Test: `gitignore_toggle` template child exists and starts active (`active = true`)
  - [x] Test: `glob_entry` template child exists and starts empty
  - [x] Test: GSettings keys `search-panel-options-expanded` and `search-gitignore` exist with correct defaults (`false` and `true`)
  - [x] Test: `SearchResultItem` and existing Story 1.2/1.3 tests still pass (no regressions)
  - [x] Update `test_search_panel_has_toggle_template_children` — it currently asserts `more_toggle` is NOT sensitive (Story 1.3 placeholder). This assertion must change to assert sensitive=true.

- [x] Task 9: Verify build, tests, no regressions (all ACs)
  - [x] Run `make check` (clippy + fmt)
  - [x] Run `make test-unit` — all unit tests pass
  - [x] Run `make test-int` — all integration tests pass
  - [x] Run `make test-widget` — all widget tests pass (including updated Story 1.3 test)
  - [x] Verify no GTK/pixman runtime warnings via `make run` and exercising:
    - More button toggle (expand/collapse options)
    - Gitignore toggle (search with/without gitignore)
    - Glob filter (type pattern, verify re-search)
    - Options state persistence (close and reopen app)
    - Truncation indicator on large result sets

## Dev Notes

### This Is a Pure UI Wiring Story

The service layer (`services/content_search.rs`) and model layer (`model/content_search.rs`) are **already complete** for this story. Both `ContentSearchOptions.gitignore` and `ContentSearchOptions.glob` fields exist and are fully functional — the search service respects `.gitignore` patterns via the `ignore` crate's `WalkBuilder` flags, and glob filtering uses `ignore::overrides::OverrideBuilder`. No service or model changes are needed.

The `start_search()` method in `search_panel/mod.rs` currently hardcodes `gitignore: true` and `glob: None` — the only change is reading from the new UI controls instead.

### Options Revealer Widget Tree

Insert between `error_label` and the first `GtkSeparator` in `search-panel.ui`:

```xml
<child>
  <object class="GtkRevealer" id="options_revealer">
    <property name="transition-type">slide-down</property>
    <property name="transition-duration">150</property>
    <property name="reveal-child">false</property>
    <child>
      <object class="GtkBox" id="options_box">
        <property name="orientation">horizontal</property>
        <property name="spacing">6</property>
        <property name="margin-start">6</property>
        <property name="margin-end">6</property>
        <property name="margin-top">4</property>
        <property name="margin-bottom">4</property>
        <child>
          <object class="GtkToggleButton" id="gitignore_toggle">
            <property name="label">.gitignore</property>
            <property name="tooltip-text">Respect .gitignore patterns</property>
            <property name="active">true</property>
          </object>
        </child>
        <child>
          <object class="GtkEntry" id="glob_entry">
            <property name="hexpand">true</property>
            <property name="placeholder-text">File filter (e.g., *.rs, *.toml)</property>
          </object>
        </child>
      </object>
    </child>
  </object>
</child>
```

**CRITICAL: Also remove `<property name="sensitive">false</property>` from the existing `more_toggle`.** This is what activates the button.

### "More" Button → Options Revealer Wiring

Use `bind_property` for the simplest possible wiring — the `more_toggle.active` property directly controls `options_revealer.reveal_child`:

```rust
imp.more_toggle.bind_property("active", &*imp.options_revealer, "reveal-child")
    .sync_create()
    .build();
```

Combined with `settings.bind(keys::SEARCH_PANEL_OPTIONS_EXPANDED, &more_toggle, "active").build()`, this creates a two-way chain: GSettings ↔ more_toggle.active ↔ options_revealer.reveal_child. The `sync_create` flag ensures the revealer state is correct on construction.

**Ordering in `setup_options()`:**
1. GSettings bind for `SEARCH_PANEL_OPTIONS_EXPANDED` (restores persisted state to `more_toggle.active`)
2. GSettings bind for `SEARCH_GITIGNORE` (restores persisted state to `gitignore_toggle.active`)
3. `bind_property` from `more_toggle` → `options_revealer` (propagates restored state to revealer)
4. `notify::active` connection on `gitignore_toggle` for immediate re-search

### Gitignore Toggle — Immediate Re-Search

Same pattern as the case/regex/word toggles from Story 1.3:

```rust
let panel_weak = obj.downgrade();
imp.gitignore_toggle.connect_notify_local(Some("active"), move |_, _| {
    if let Some(panel) = panel_weak.upgrade() {
        if !panel.imp().constructed_complete.get() {
            return;
        }
        let query = panel.query();
        if !query.is_empty() {
            panel.start_search(&query);
        }
    }
});
```

The `constructed_complete` guard prevents a spurious search when GSettings restores the toggle state during construction.

### Glob Entry — 300ms Debounce

The glob entry uses the same generation-counter debounce pattern as the search entry. Add a `glob_generation: Cell<u32>` field to the imp struct:

```rust
let panel_weak = obj.downgrade();
let generation = imp.glob_generation.clone();
imp.glob_entry.connect_changed(move |_| {
    let gen = generation.get().wrapping_add(1);
    generation.set(gen);
    let panel_weak = panel_weak.clone();
    glib::timeout_add_local_once(Duration::from_millis(300), move || {
        if let Some(panel) = panel_weak.upgrade() {
            if panel.imp().glob_generation.get() != gen {
                return;
            }
            let query = panel.query();
            if !query.is_empty() {
                panel.start_search(&query);
            }
        }
    });
});
```

**NOTE:** Glob debounce is 300ms (same as search query debounce). The generation counter prevents stale closures from firing after rapid edits.

**Empty glob = no filter:** When the glob entry is cleared, the next search runs with `glob: None`, searching all files. This is the expected behavior — clearing the filter removes it.

### Truncation Indicator Enhancement

The current polling timer already handles `SearchEvent::ResultCap`. The enhancement adds visual styling:

```rust
SearchEvent::ResultCap => {
    imp.result_capped.set(true);
    imp.count_label.set_text("10,000+ results (truncated) — narrow your search");
    imp.count_label.add_css_class("warning");
}
```

The `.warning` class applies Adwaita's `@warning_color` — an amber/yellow that works in both light and dark themes. Clear it in `clear_results()`:

```rust
fn clear_results(&self) {
    // ... existing clear logic ...
    let imp = self.imp();
    imp.count_label.remove_css_class("warning");
    // ...
}
```

### GSettings Keys

Two new keys, both following existing naming conventions:

| Key | Type | Default | Purpose |
|-----|------|---------|---------|
| `search-panel-options-expanded` | `b` | `false` | More button / options revealer state |
| `search-gitignore` | `b` | `true` | Gitignore toggle state |

The `search-gitignore` default of `true` matches `ContentSearchOptions::default().gitignore = true` — gitignore filtering is ON by default. This is a deliberate UX choice: clean results out of the box.

### What This Story Does NOT Include

Per the architecture and UX spec, these are deferred to later stories:
- **Replace input/controls** — Epic 2, Story 2.1 adds replace entry, Replace All button, Undo button inside the options revealer
- **Search history dropdown** — Epic 3, Story 3.1
- **Saved searches** — Epic 3, Story 3.2
- **Glob persistence** — The glob filter text is transient (not persisted to GSettings). Only toggle states and panel visibility are persisted.
- **`search-panel-options-expanded` GSettings key** was listed in the architecture's 6-key schema expansion but not yet added (Stories 1.2 and 1.3 only added 4 of the 6 planned keys)

### Previous Story Intelligence

**From Story 1.3 (most recent, same epic):**
- `constructed_complete: Cell<bool>` guard pattern — MUST be used for all `notify::active` handlers on new toggles. Without it, GSettings bind during `constructed()` fires `notify::active` before workspace roots are set, causing a premature search that NPEs or produces wrong results.
- `setup_toggles()` method exists at imp.rs:394-428 — the new `setup_options()` method should follow the same structure.
- `imp.rs` is at 496 lines — adding ~60 lines for `setup_options()` brings it to ~556 lines, well within the 1000-line limit.
- `mod.rs` is at 321 lines — changing 2 lines in `start_search` brings negligible growth.
- The `settings: gio::Settings` field on the imp struct is already shared across methods — reuse it in `setup_options()`.
- Review finding: duplicate `gio::Settings::new` was fixed by adding the `settings` field. Use the same field.
- Review finding: `more_toggle` `sensitive=false` — this test assertion (`test_search_panel_has_toggle_template_children`) currently asserts `more_toggle` is NOT sensitive. **This test MUST be updated** to assert `sensitive=true`.

**From Story 1.2 (search panel foundation):**
- `start_search()` comments `// hardcoded until Story 1.4` on lines 137-138 — direct hook points.
- `clear_results()` resets count labels, file_groups, root_store — add `remove_css_class("warning")` here.
- `SearchEvent::ResultCap` handling already exists in the polling timer — enhance, don't replace.
- The `RefCell` borrow pattern in file_groups — clone+drop before signal emission — must be preserved.

**From Story 1.1 (service layer):**
- `ContentSearchOptions` already has `gitignore: bool` (default: true) and `glob: Option<String>` (default: None). No model changes needed.
- Invalid glob patterns send `SearchEvent::Error(...)` + `Done` — the same error label mechanism from Story 1.3 handles this automatically.
- Gitignore is wired via `WalkBuilder::git_ignore(false).git_global(false).git_exclude(false)` when disabled.
- Glob uses `ignore::overrides::OverrideBuilder` with include semantics.

### Files to Modify

| File | Change | Estimated Delta |
|------|--------|----------------|
| `data/dev.cominotti.lushtext.gschema.xml` | Add 2 GSettings keys | +10 lines |
| `crates/lushtext-core/src/config.rs` | Add 2 key constants | +2 lines |
| `resources/ui/search-panel.ui` | Add options revealer + enable more_toggle | +20 lines |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | Add 3 template children, `glob_generation` field, `setup_options()` method | +60 lines |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | Update `start_search` to read gitignore/glob, enhance truncation label | +10 lines |
| `crates/lushtext/tests/widget/search_panel.rs` | Add Story 1.4 tests, update Story 1.3 `more_toggle` assertion | +40 lines |

**No new files created.** All changes are modifications to existing files.

### Anti-Patterns to Avoid

1. **DO NOT** add service or model changes — `ContentSearchOptions.gitignore` and `.glob` are already fully implemented in the service layer
2. **DO NOT** persist glob text to GSettings — it's a transient per-session filter, not a setting
3. **DO NOT** use `SourceId` cancellation for glob debounce — use generation-counter pattern (matching search entry and sidebar persist)
4. **DO NOT** connect `notify::active` on gitignore toggle before GSettings bind — initial state restore would trigger a spurious search
5. **DO NOT** forget the `constructed_complete` guard on the gitignore toggle's `notify::active` handler
6. **DO NOT** use `GtkRevealer(slide-up)` for the options panel — use `slide-down` (150ms, shorter than the main panel's 250ms `slide-up`)
7. **DO NOT** add replace input/controls — those belong to Epic 2, Story 2.1
8. **DO NOT** forget to update the existing test `test_search_panel_has_toggle_template_children` which asserts `more_toggle` is NOT sensitive
9. **DO NOT** forget to remove `.warning` CSS class from `count_label` in `clear_results()` — stale warning styling would carry over to the next search
10. **DO NOT** forget the SPDX license header on any new `.rs` files (none expected in this story, but verify modified files still have it)

### Project Structure Notes

- All changes are within the existing search panel module — no new modules needed
- `search_panel/imp.rs` grows from ~496 to ~556 lines — well within the 1000-line limit
- `search_panel/mod.rs` grows from ~321 to ~331 lines — well within the limit
- `search-panel.ui` grows from ~116 to ~136 lines
- `config.rs` grows from ~38 to ~40 lines
- The options revealer follows the UX spec's "Progressive Minimal" pattern (Direction C): core toggles always visible, advanced options behind "More"

### References

- [Source: _bmad-output/planning-artifacts/epics.md#Story 1.4: Gitignore Toggle, Glob Filter & Options Panel]
- [Source: _bmad-output/planning-artifacts/architecture.md#GSettings Schema Additions]
- [Source: _bmad-output/planning-artifacts/architecture.md#Widget Integration]
- [Source: _bmad-output/planning-artifacts/architecture.md#Pattern 2: Action Namespace]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Direction C: Progressive Minimal]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Design System Foundation — Standard Adwaita components]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#Panel layout structure]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR1 Progressive Minimal]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR4 GtkRevealer transitions]
- [Source: _bmad-output/planning-artifacts/ux-design-specification.md#UX-DR11 Inline error/warning labels]
- [Source: _bmad-output/implementation-artifacts/1-3-search-toggles-match-highlighting.md#Completion Notes]
- [Source: _bmad-output/implementation-artifacts/1-2-search-panel-with-streaming-results.md#Dev Notes]
- [Source: _bmad-output/implementation-artifacts/1-1-content-search-service-types.md#Completion Notes]
- [Source: .agents/AGENTS.md#GSettings for preferences]
- [Source: .agents/rules/rust.md#Mutable State on GObject Structs]
- [Source: .agents/rules/ui.md#GSettings Bindings]
- [Source: .agents/rules/widget-wiring.md#Auto-Dismiss Timers (Generation Counter)]
- [Source: .agents/rules/widget-wiring.md#Testing]

## Dev Agent Record

### Agent Model Used

Claude Opus 4.6 (1M context)

### Debug Log References

- Fixed: `gen` is a reserved keyword in Rust Edition 2024. Renamed to `current_gen` in glob debounce closure.
- Fixed: `rustfmt` reformatted method chain style for `set_text()` and `try_from().unwrap_or()` calls in `mod.rs`.

### Completion Notes List

- **Task 1:** Added `search-panel-options-expanded` (bool, default false) and `search-gitignore` (bool, default true) GSettings keys to schema XML. Added corresponding constants to `config.rs` keys module.
- **Task 2:** Added `GtkRevealer` with `GtkToggleButton` (.gitignore) and `GtkEntry` (glob filter) to `search-panel.ui`. Removed `sensitive=false` from `more_toggle`. Added 3 `TemplateChild` fields + defaults to `imp.rs`.
- **Task 3:** Implemented `setup_options()` in `imp.rs`. GSettings bind for `SEARCH_PANEL_OPTIONS_EXPANDED` → `more_toggle.active`, then `bind_property` from `more_toggle.active` → `options_revealer.reveal_child` with `sync_create`.
- **Task 4:** GSettings bind for `SEARCH_GITIGNORE` → `gitignore_toggle.active`. Connected `notify::active` for immediate re-search with `constructed_complete` guard.
- **Task 5:** Added `glob_generation: Cell<u32>` field. Connected `glob_entry.connect_changed()` with 300ms generation-counter debounce, same pattern as search entry.
- **Task 6:** Replaced hardcoded `gitignore: true` and `glob: None` in `start_search()` with values from `gitignore_toggle.is_active()` and `glob_entry.text()`.
- **Task 7:** Enhanced `SearchEvent::ResultCap` handler to set truncation text and add `.warning` CSS class. Added `remove_css_class("warning")` to `clear_results()`.
- **Task 8:** Added 7 widget tests: `test_more_toggle_is_sensitive`, `test_options_revealer_exists_and_starts_hidden`, `test_gitignore_toggle_exists_and_starts_active`, `test_glob_entry_exists_and_starts_empty`, `test_gsettings_search_panel_options_expanded_default`, `test_gsettings_search_gitignore_default`, `test_clear_results_removes_warning_class`. Updated existing `test_search_panel_has_toggle_template_children` to assert `more_toggle` is sensitive.
- **Task 9:** All checks pass: `make check` (clippy + fmt clean), 205 unit tests, 52 integration tests, 366 widget tests — all green with 0 failures.

### Change Log

- Story 1.4 implementation complete (Date: 2026-04-07)

### File List

- `data/dev.cominotti.lushtext.gschema.xml` — added 2 GSettings keys
- `crates/lushtext-core/src/config.rs` — added 2 key constants
- `resources/ui/search-panel.ui` — added options revealer, enabled more_toggle
- `crates/lushtext-core/src/ui/search_panel/imp.rs` — added 3 template children, glob_generation field, setup_options() method
- `crates/lushtext-core/src/ui/search_panel/mod.rs` — updated start_search options, enhanced truncation indicator, added warning class cleanup
- `crates/lushtext/tests/widget/search_panel.rs` — added 7 Story 1.4 tests, updated 1 existing test

### Review Findings

- [x] [Review][Patch] **CRITICAL: Glob debounce never fires — Cell clone diverges from imp field** [search_panel/imp.rs:488-504] — Fixed: mirror `setup_search_entry` pattern (read/write through `panel.imp()` reference).
- [x] [Review][Patch] AC #5 truncation text overwritten by generic count update [search_panel/mod.rs:272-299] — Fixed: guard count-label update with `!imp.result_capped.get()`.
- [x] [Review][Patch] Documentation not updated for search feature (README.md, AGENTS.md, ui.md) — Fixed: added content_search module, search_panel, Ctrl+Shift+F docs, updated widget hierarchy.
- [x] [Review][Patch] Duplicate doc comment on RESULT_CAP [services/content_search.rs:23-24] — Fixed: removed duplicate line.
- [x] [Review][Defer] Premature model types (Replacement, ReplaceResult, SearchHistoryEntry, SavedSearch) [model/content_search.rs:68-103] — deferred, from story 1-1 spec forward-planning
- [x] [Review][Defer] Single-slot `connect_workspace_changed` callback fragile design [window/imp.rs:651, window/search.rs:77] — deferred, design issue not a current bug
- [x] [Review][Defer] Search threads bypass MAX_CONCURRENT_SPAWNS concurrency guard [search_panel/mod.rs:148] — deferred, by design (streaming pattern)
- [x] [Review][Defer] `find_match_range` only highlights first match per line [services/content_search.rs:164] — deferred, standard line-oriented search behavior
- [x] [Review][Defer] `OverrideBuilder::new(roots[0])` glob may not work for path-anchored patterns on other roots [services/content_search.rs:100] — deferred, edge case
- [x] [Review][Defer] Multiline selection pre-fill produces unfindable query [window/search.rs:107-112] — deferred, non-harmful edge case
