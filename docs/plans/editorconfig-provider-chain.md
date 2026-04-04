# Plan: EditorConfig Provider Chain

**Status:** Implemented  
**Date:** 2026-04-04  
**Branch:** `feat/command-palette`  
**Commits:** `1327bcc`, `d0eba76`

## Problem Statement

LushText used a flat GSettings architecture where every editor tab bound
directly to a single application-wide `gio::Settings` instance. There was
no mechanism for per-file, per-project, or per-language formatting overrides.
A Python file and a Makefile in the same workspace used the same tab width
and indentation style.

## Research: GNOME Text Editor's Provider Chain

Before designing, we studied GNOME Text Editor's (GTE) settings architecture
to understand the state of the art in the GNOME ecosystem.

GTE uses a **Chain of Responsibility** pattern with four providers resolved
in priority order:

| Priority | Provider | Source |
|----------|----------|--------|
| 1 (highest) | Modeline | `vim:`, `emacs -*-`, `kate:` in file lines |
| 2 | EditorConfig | `.editorconfig` files in parent directories |
| 3 | Language Defaults | Static table (Python→4 spaces, Makefile→tabs) |
| 4 (lowest) | GSettings | User global preferences |

Key design elements from GTE:
- `EditorPageSettingsProvider` GObject interface with tristate getters
  (`TRUE` = "I have a value", `FALSE` = "skip me")
- Per-document `EditorPageSettings` aggregator with first-wins resolution
- `GBindingGroup` for live property propagation
- `*_set` override flags for per-tab manual overrides
- `libeditorconfig` C library dependency

## Alternatives Evaluated

### Option A: Full Provider Chain (GTE clone)

Replicate all four providers. ~2000 LOC, requires `libeditorconfig` C
dependency, modeline parsing (vim + emacs + kate syntaxes), language defaults
table.

**Rejected:** Too large for initial implementation. Modelines are niche.

### Option B: EditorConfig-Only (no trait abstraction)

Add EditorConfig as a single override layer with a flat `HashMap` of
overrides. ~600 LOC but no extensibility — adding modelines later would
require a rewrite.

**Rejected:** Creates refactoring debt when the second provider is added.

### Option C: Resolved Settings Layer (chosen)

Build the extensible architecture (trait-ready design with `Option<T>`
composition) but ship only EditorConfig + GSettings. ~800 LOC. The
`Option::unwrap_or_else` chain is the Rust-idiomatic "first-wins" pattern.

**Chosen because:**
1. Right abstraction at right scope — extensible without over-engineering
2. `Option<T>` is Rust's native tristate (vs GTE's `gboolean` + out-param)
3. Pure Rust `editorconfig-parser` crate — no C dependency
4. Adding modelines later is just a new `FormattingOverrides` source + merge

### Option D: Do Nothing

**Rejected:** Per-file settings is a significant gap vs every other editor.

## Design Decisions

### 1. Which EditorConfig properties to support initially

Supported: `indent_style`, `tab_width`, `indent_size` — the three that map
directly to existing GtkSourceView properties.

Deferred: `end_of_line`, `charset`, `trim_trailing_whitespace`,
`insert_final_newline`, `max_line_length` — each requires new features
beyond the settings layer. Documented in `docs/next/editorconfig-future.md`.

### 2. Settings toggle

Added `use-editorconfig` GSettings key (default: `true`) with a Preferences
switch. Users working on projects without `.editorconfig` shouldn't pay the
lookup cost.

### 3. Which settings are overrideable

Only formatting settings (`tab-width`, `insert-spaces`, `indent-width`)
participate in the provider chain. Visual-only settings (`show-line-numbers`,
`highlight-current-line`, `style-scheme`, `font`) stay as direct GSettings
bindings — they don't vary per-file.

### 4. Status bar indicator

"EditorConfig" label in the metadata area, before the encoding label.
Visible only when overrides are active for the current tab.

### 5. Replacing `Settings::bind(GET)`

The critical architectural change. `Settings::bind(GET)` creates a live
one-way pipe from GSettings to the widget property — any GSettings change
overwrites per-file overrides. Replaced with manual `connect_changed`
handlers that consult `FormattingOverrides` before falling back to GSettings.

### 6. `Unset` vs `None` handling

The `editorconfig-parser` crate returns three variants: `Value(T)`,
`Unset`, `None`. During multi-file merge (closest to farthest):
- `Value(T)` → store the override, mark resolved
- `Unset` → mark resolved with no override (prevents inheritance)
- `None` → not mentioned, keep looking at parent files

### 7. No trait abstraction (yet)

A `SettingsProvider` trait was considered but deferred. With only two
sources (EditorConfig + GSettings), the `Option::unwrap_or_else` pattern
is simpler and equally testable. The trait becomes worthwhile when a third
provider is added.

## Implementation Phases

### Phase 1: Model + Schema + Dependencies
- Created `model/formatting_overrides.rs` — `FormattingOverrides` struct
  (`Copy`, `Default`, `PartialEq`, zero GTK deps)
- Added `use-editorconfig` key to GSettings schema
- Added `editorconfig-parser = "0.0.3"` to workspace dependencies
- Ran `cargo hakari generate`

### Phase 2: EditorConfig Service
- Created `services/editorconfig.rs` — `resolve_for_path()` function
- Walks parent directories, parses each `.editorconfig`, merges results
- Pure I/O, no GTK deps, designed for background thread execution
- 11 unit tests covering: basic resolution, multi-level merge,
  `root = true` stops walk, section matching, value clamping, `Unset`

### Phase 3: EditorPage Wiring
- Removed `Settings::bind(GET)` for `tab-width` and `insert-spaces`
- Added `Cell<FormattingOverrides>` and two `connect_changed` handlers
- Added `apply_formatting_settings()` free function — single resolution
  point for EditorConfig overrides vs GSettings fallbacks
- Added `apply_editorconfig_overrides()`, `clear_editorconfig_overrides()`,
  `formatting_overrides()` public methods

### Phase 4: Status Bar Indicator
- Added `editorconfig_label` to status bar template (hidden by default)
- Added `set_editorconfig_active(bool)` method

### Phase 5: Window Integration
- Added `resolve_editorconfig_for_editor()` — spawns background resolution
  via `spawn_blocking_then`, applies result on main thread
- Wired into `open_document()` and `show_save_as_dialog()`
- Added `on_use_editorconfig_changed()` — iterates all tabs to re-resolve
  or clear overrides when the toggle changes
- Extended `refresh_status_bar()` with EditorConfig indicator

### Phase 6: Preferences UI
- Added `AdwSwitchRow` for "Use EditorConfig" with subtitle
- Two-way GSettings bind (same pattern as other preference rows)

### Phase 7: Testing
- 3 model unit tests (default, non-empty, copy semantics)
- 11 service unit tests (all resolution scenarios)
- 8 integration tests (end-to-end with real `.editorconfig` files)
- All 301 tests pass (22 new)

## Quality Reviews

### Hex-Arch Review
- 0 FLAGs (no architectural violations)
- 0 RECOMMENDs
- 1 CONSIDER (staggered tab update on toggle — acceptable)
- 6 GOODs (clean layer separation, correct dependency direction, CQS)

### Simplify Review
- Consolidated duplicate `connect_changed` closures into a loop
  (12 lines saved)
- No other reuse, quality, or efficiency issues found

## Files Changed

**New files (4):**
- `crates/lushtext-core/src/model/formatting_overrides.rs`
- `crates/lushtext-core/src/services/editorconfig.rs`
- `crates/lushtext/tests/integration/editorconfig.rs`
- `docs/next/editorconfig-future.md`

**Modified files (20):**
- `Cargo.toml`, `Cargo.lock`, `crates/lushtext-core/Cargo.toml`,
  `workspace-hack/Cargo.toml` — dependency addition
- `data/dev.cominotti.lushtext.gschema.xml` — new GSettings key
- `crates/lushtext-core/src/config.rs` — key constant
- `crates/lushtext-core/src/model/mod.rs`,
  `crates/lushtext-core/src/services/mod.rs` — module registration
- `crates/lushtext-core/src/ui/editor_page/imp.rs` — binding replacement
- `crates/lushtext-core/src/ui/editor_page/mod.rs` — public API
- `crates/lushtext-core/src/ui/status_bar/imp.rs`,
  `crates/lushtext-core/src/ui/status_bar/mod.rs` — indicator
- `crates/lushtext-core/src/ui/window/imp.rs`,
  `crates/lushtext-core/src/ui/window/mod.rs`,
  `crates/lushtext-core/src/ui/window/dialogs.rs` — integration
- `crates/lushtext-core/src/ui/preferences/imp.rs` — toggle row
- `resources/ui/status-bar.ui`, `resources/ui/preferences.ui` — templates
- `crates/lushtext/tests/integration.rs` — test module
- `.claude/CLAUDE.md` — architecture documentation

**Total diff:** +804 / -22 lines

## Future Work

1. **Modeline provider** — scan buffer lines for `vim:` / `emacs -*-`
   modeline markers. Higher priority than EditorConfig.
2. **Language defaults provider** — static table (Python→4 spaces,
   Makefile→tabs). Lower priority than EditorConfig.
3. **Deferred EditorConfig properties** — `insert_final_newline`,
   `trim_trailing_whitespace`, `max_line_length`, `end_of_line`, `charset`.
4. **Per-tab manual override** — GTE's `*_set` flags pattern for
   menu-driven overrides that beat all providers.
