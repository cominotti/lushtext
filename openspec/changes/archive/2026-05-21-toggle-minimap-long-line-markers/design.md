## Context

The minimap currently collects four marker families in `crates/lushtext-core/src/ui/editor_page/minimap.rs`: bookmarks, active in-tab search matches, modified-since-save lines, and long lines above the warning threshold. Long-line markers are painted in a red warning lane, which is useful for code but noisy for Markdown or prose where long raw lines are normal.

Preferences already expose `Show Minimap` under `Editor > Behavior` and bind rows directly to GSettings in `crates/lushtext-core/src/ui/preferences/imp.rs`. This change adds one adjacent preference and uses the existing settings-driven UI pattern.

## Goals / Non-Goals

**Goals:**

- Add a persisted long-line marker visibility preference that defaults to off.
- Keep the existing minimap visibility preference and the new long-line marker preference together under `Editor > Minimap`.
- Preserve existing bookmark, search, and modified marker behavior.
- Keep the implementation inside the GTK UI adapter layer: settings, preferences rows, and minimap marker collection.
- Add robust widget coverage for default-off behavior, enabled behavior, and preference binding.

**Non-Goals:**

- Do not create a new top-level `Minimap` preferences page.
- Do not change the long-line threshold in this change.
- Do not add per-marker toggles for bookmarks, search matches, or modified lines.
- Do not change minimap width, navigation, viewport overlay, or availability behavior.

## Decisions

### Keep minimap controls on the Editor page

Use a new `AdwPreferencesGroup` titled `Minimap` inside the existing `Editor` page. Move the existing `Show Minimap` row into that group and add `Show Long-Line Markers` next to it.

Alternative considered: a new top-level `Minimap` page. That would overstate the feature's weight with only two visible controls and split an editor-surface setting away from the other editor preferences. A separate page can still make sense later if minimap controls grow to include marker categories, width, click behavior, viewport styling, or density.

### Add a dedicated GSettings key

Add a boolean key named `minimap-long-line-markers-visible` with default `false`, plus a matching `keys::MINIMAP_LONG_LINE_MARKERS_VISIBLE` constant. The name keeps the setting scoped to the minimap family and describes visibility rather than line-length policy.

Alternative considered: reuse `show-minimap` and hide long-line markers whenever the minimap is disabled. That does not solve the noisy-marker problem because it forces users to disable the whole minimap to remove one marker family.

### Gate only long-line marker collection

Keep `collect_markers()` as the single marker aggregation point, but call `collect_long_line_warnings()` only when `minimap-long-line-markers-visible` is enabled. Leave marker colors and lane widths unchanged for users who opt in.

Alternative considered: collect long-line markers as usual and suppress them only during drawing. That would make marker counts and tests lie about what is visible, and it would keep unnecessary scanning work active when the feature is off.

### Test at both settings and widget levels

Add preference tests proving the new row defaults off and binds to GSettings, and widget/minimap tests proving long-line markers are absent by default and appear after enabling the setting. Existing tests for bookmarks, search, and modified markers should continue to show those marker families are unaffected.

## Risks / Trade-offs

- Existing users who liked long-line markers will stop seeing them after upgrade because the new default is off → Mitigation: expose the toggle beside `Show Minimap` with explicit wording.
- Moving `Show Minimap` from `Behavior` to `Minimap` could affect brittle widget tests that assume row placement → Mitigation: update tests to assert semantic grouping instead of relying only on old ordering.
- Adding a GSettings key can break startup if schema, key constants, and bindings drift → Mitigation: include schema validation/build coverage plus preference binding tests.
- Long-line scanning should not run when disabled → Mitigation: test marker count and gate before `collect_long_line_warnings()`.
