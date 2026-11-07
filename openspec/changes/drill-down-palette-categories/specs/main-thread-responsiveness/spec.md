## ADDED Requirements

### Requirement: Command palette grouped and category dispatches fit the interaction budgets
On the reference host, publishing grouped palette results with every category full and its "More" rows SHALL fit `B_INTERACT_US` and SHALL cost no more than the same publish on the base revision beyond measurement noise. In a category view, at any row count, every input dispatch (Enter on a "More" row, a keystroke, Up, Down, Page Up, Page Down, Escape, Backspace) SHALL fit `B_INTERACT_US`, and each publish, the frame after it, and the return to grouped results SHALL fit `B_SETTLE_US`. Ranking, counting and filtering SHALL run off GTK, and the GTK-side work of a publish SHALL not grow with the number of ranked rows. Measurements SHALL be interleaved with the base revision at a 1-minute load below 4. A dispatch that cannot meet its budget because of the toolkit SHALL be recorded as a toolkit-floor exception with a GTK axiom probe, its measured figures and a widget assertion bounding it, and SHALL NOT be left unexplained.

#### Scenario: The grouped publish keeps its cost
- **WHEN** All mode publishes 64 rows in every category plus their More rows
- **THEN** the publish dispatch fits `B_INTERACT_US` and stays within the base revision's measured range

#### Scenario: Opening and typing in a large category
- **WHEN** the user opens a category view over 131,072 files and types a query
- **THEN** each input dispatch fits `B_INTERACT_US` and each publish fits `B_SETTLE_US`, at the same cost as over 512 files

#### Scenario: Reaching the last row
- **WHEN** the user moves the selection to the last of 131,072 rows with Page Down
- **THEN** each key-press dispatch fits `B_INTERACT_US`
