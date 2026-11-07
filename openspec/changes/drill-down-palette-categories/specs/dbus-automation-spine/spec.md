## ADDED Requirements

### Requirement: Automation observes and drives the palette category view
The automation snapshot SHALL report, under `command_palette`, the shown category's id (`category`, empty in the grouped view), how many rows the category view lists (`category_result_count`), how many results the category matches (`category_total_count`), both saturating at `u32::MAX`, and whether the retained-byte budget cut it (`category_limited`). When no category view is shown, the counts SHALL be zero and `category_limited` false. `result_count` SHALL count the rows the visible list renders. The values SHALL be projected from the palette workflow's evidence surface and SHALL carry no result text. The window action `win.set-command-palette-category` SHALL take a string: a category id of the current mode opens that category's view on a visible palette, the empty string returns to the grouped view, and any other value is refused without changing state. The action, its ids and the fields SHALL be documented in `docs/automation.md` and `docs/automation-reference.md` and listed in the action catalog.

#### Scenario: A category view is shown
- **WHEN** automation activates `win.set-command-palette-category` with `workspace-files` on a visible palette whose workspace group is truncated
- **THEN** after `command-palette-search` is ready the snapshot reports `category` `workspace-files`, `category_result_count` equal to `category_total_count`, and `category_limited` false

#### Scenario: Returning to grouped results
- **WHEN** automation activates the action with the empty string
- **THEN** the snapshot reports `category` empty, both category counts zero, and `category_limited` false

#### Scenario: An unknown category is refused
- **WHEN** automation activates the action with `nonexistent`
- **THEN** the palette's view and query are unchanged
