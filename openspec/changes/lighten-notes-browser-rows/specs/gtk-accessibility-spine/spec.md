## ADDED Requirements

### Requirement: Measure-free row text keeps the accessible text of the labels it replaces
A list row text shown through `LushtextRowText` SHALL expose the `label` accessible role with an accessible label equal to its current text. It SHALL update that label on every bind and retarget, and clear it on unbind, through `crate::ui::accessibility`. The Notes browser rows and section headers, and the command palette's grouped, header, More and category rows, SHALL keep their row-level accessible names, descriptions, selection, position and set-size metadata unchanged. Every AT-SPI anchor for these surfaces in `docs/automation-reference.md` (role `label`, name equal to the row text) SHALL be unchanged.

#### Scenario: Orca reads a notes row
- **WHEN** a screen reader moves through the Notes browser's rows
- **THEN** it hears the same row name and description (title, section and subtitle) as on the base revision

#### Scenario: AT-SPI anchors still resolve
- **WHEN** `make accessibility-smoke` runs its notes browser and command palette cases
- **THEN** every `label` anchor for those surfaces resolves by role and name as before

#### Scenario: A recycled row text does not leak
- **WHEN** a realized row is rebound to another item or retargeted by a publish
- **THEN** its row text's accessible label is the new item's text, and an unbound row exposes none
