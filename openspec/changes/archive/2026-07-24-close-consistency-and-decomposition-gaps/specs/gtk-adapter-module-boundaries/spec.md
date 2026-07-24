## ADDED Requirements

### Requirement: Markdown preview is organized by rendering workflows
The `ui/markdown_preview` widget SHALL separate its rendering workflows —
image admission and decode, table building, code-block theming and repair,
footnote and link handling, and render-plan orchestration/projection — into
focused sibling modules under the existing widget folder, keeping the public
widget wrapper, template contract, and automation surface unchanged. The
resulting `mod.rs` MUST no longer mix these workflows in one multi-thousand
line implementation block, and extracted modules MUST follow the existing
decomposition rules: no new crates, no generic manager or controller layers,
widget-owned state stays on the existing `imp` struct, and services/models
remain GTK-free.

#### Scenario: Image pipeline is a focused module
- **WHEN** Markdown preview admits, decodes, applies, or retires an embedded
  image
- **THEN** that workflow lives in a dedicated sibling module rather than the
  render-orchestration file
- **AND** bounded admission, worker handoff, generation rejection, and
  disposal behavior are unchanged

#### Scenario: Table and code-block rendering are focused modules
- **WHEN** Markdown preview builds table cells or themes and repairs code
  blocks
- **THEN** each workflow lives in its own sibling module with the same
  observable buffer output
- **AND** the documented idle-plus-timeout code-block repair exception keeps
  its existing mechanism and timing

#### Scenario: Decomposition is behavior- and pixel-neutral
- **WHEN** the existing markdown-preview widget tests, visual lanes, and
  smoke coverage run against the decomposed widget
- **THEN** they pass without weakened assertions
- **AND** render output, sliced GTK application budgets, readiness
  reporting, and accessibility metadata are byte- and behavior-identical
