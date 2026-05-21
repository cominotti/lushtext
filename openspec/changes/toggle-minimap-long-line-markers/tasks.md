## 1. Grounding

- [x] 1.1 Inspect `openspec/specs/editor-minimap/spec.md`, `resources/ui/preferences.ui`, `data/dev.cominotti.lushtext.gschema.xml`, `crates/lushtext-core/src/ui/preferences/imp.rs`, and `crates/lushtext-core/src/ui/editor_page/minimap.rs` to confirm the current minimap setting, marker, and Preferences patterns.
- [x] 1.2 Inspect existing preference and minimap widget tests in `crates/lushtext/tests/widget/preferences.rs` and `crates/lushtext/tests/widget/window.rs` so new coverage follows local harness conventions.

## 2. Settings and Preferences

- [x] 2.1 Add a boolean GSettings key `minimap-long-line-markers-visible` with default `false`, summary, description, and range-free boolean schema wiring.
- [x] 2.2 Add `keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE` and bind it consistently wherever preferences or editor pages need live access.
- [x] 2.3 Reorganize `resources/ui/preferences.ui` so `Show Minimap` moves from `Editor > Behavior` into a new `Editor > Minimap` preferences group.
- [x] 2.4 Add a `Show Long-Line Markers` switch row in the new `Editor > Minimap` group with wording that makes the opt-in nature clear.
- [x] 2.5 Update `LushtextPreferences` template children and constructed-time bindings so both minimap rows reflect and update GSettings.

## 3. Minimap Behavior

- [x] 3.1 Gate `collect_long_line_warnings()` behind `minimap-long-line-markers-visible` so long-line marker scanning and marker creation do not run while the preference is disabled.
- [x] 3.2 Add live settings-change refresh wiring so toggling `Show Long-Line Markers` updates already-open editor pages without requiring tab reload or app restart.
- [x] 3.3 Preserve existing bookmark, active-search, modified-since-save, minimap visibility, viewport overlay, navigation, and high-cost document behavior.

## 4. Tests

- [x] 4.1 Add or update preference widget tests proving `Show Long-Line Markers` defaults off and is bound to `minimap-long-line-markers-visible`.
- [x] 4.2 Add or update preference widget tests proving minimap controls are grouped under `Editor > Minimap` and no new top-level `Minimap` page is required.
- [x] 4.3 Add minimap widget coverage proving long-line markers are absent by default even when the minimap is enabled and the document has long lines.
- [x] 4.4 Add minimap widget coverage proving long-line markers appear when the new preference is enabled.
- [x] 4.5 Add minimap widget coverage proving disabling the preference removes existing long-line markers while bookmark, search-match, and modified markers retain their existing behavior.

## 5. Verification

- [x] 5.1 Run `openspec validate toggle-minimap-long-line-markers --strict`.
- [x] 5.2 Run `cargo fmt --check`.
- [x] 5.3 Run `cargo check --workspace`.
- [x] 5.4 Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [x] 5.5 Run targeted preference and minimap widget tests through `./scripts/run-widget-tests.sh --auto`.
- [x] 5.6 Run the full widget suite with `./scripts/run-widget-tests.sh --auto`.
- [x] 5.7 Run `git diff --check`.
