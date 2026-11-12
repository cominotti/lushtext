## ADDED Requirements

### Requirement: Realized list rows do not measure their text and are retired outside input dispatches
The rows of the Notes browser (its items and section headers) and of the command palette's grouped and category views SHALL show their text through a widget whose measured size does not depend on its text (`LushtextRowText`). It SHALL create its text layout only when it is drawn, so that changing the text of a realized row costs the next frame no layout, only the paint of the rows on screen (ledger A108). Only the palette's group headers MAY measure their text, for the separator that fills the rest of the header row. `GtkInscription` SHALL NOT be used for these rows, because its constructor lays out text and computes a style for a node with no parent yet (A109).

Tearing down a list's realized rows SHALL NOT happen inside an input dispatch, and SHALL NOT happen in one dispatch. When the Notes browser closes, or a palette category view is left, its rows SHALL be removed from the end of its model in time-sliced `G_PRIORITY_LOW` turns. Each turn removes at least one unit of rows and stays within `LIST_TURN_SLICE_US` (A110). Any publish to that model or its disposal SHALL cancel the retirement. The model's sources and payloads SHALL be released when the model is empty, through the existing disposal paths, and its evidence SHALL report the rows still being retired.

#### Scenario: A query that changes the rows on screen
- **WHEN** a Notes browser query over 16,384 listed rows changes the rows on screen
- **THEN** the frame that shows them spends no time in text layout of realized rows, its layout phase fits `B_INTERACT_US`, and the frame fits `B_SETTLE_US` under the GL renderer

#### Scenario: Closing the browser with realized rows
- **WHEN** the user closes the Notes browser while 205 rows are realized
- **THEN** the close input dispatch fits `B_INTERACT_US`
- **AND** the rows are torn down afterwards in turns, each within `LIST_TURN_SLICE_US` of row work
- **AND** the browser's sources are released once the rows are gone

#### Scenario: Leaving a category view
- **WHEN** the user leaves a palette category view over 131,072 rows
- **THEN** Escape fits `B_INTERACT_US`, every turn of the row retirement fits `B_INTERACT_US`, and the category view no longer holds sources once it completes

#### Scenario: Re-entering during a retirement
- **WHEN** the user enters a category view again before the previous retirement finished
- **THEN** the retirement stops, the new publish lists the new rows, and every model emission matches the length the model reports

#### Scenario: Rows look the same
- **WHEN** the Notes browser and the palette show Latin, CJK and emoji titles in both modes and both palette views, in light, dark and high contrast
- **THEN** row heights, text origins, ellipsis, fonts and colours match the base revision's in a same-session visual proof, and no glyph is clipped

## MODIFIED Requirements

### Requirement: Notes browser dispatches fit the interaction budgets
Every GTK dispatch of the Notes browser SHALL fit `B_SETTLE_US` on the reference host, whatever the number of rows. That covers activating `Browse Notes...` or `Browse Bookmarks...` until the first rows and the first preview are shown, the frame that first shows rows, each open-editor snapshot turn, each query publish and the frame after it, each selection change, and each turn of the row retirement after closing. The input dispatch of a keystroke in the browser's search field, and the input dispatch that closes the dialog, SHALL fit `B_INTERACT_US`. Work that grows with the number of rows (source construction, matching, category grouping) SHALL run off GTK, and GTK-side work that grows with data (the open-editor snapshot, the row retirement) SHALL run in turns bounded by the shared list turn slice. Frame verdicts SHALL be measured under the GL renderer in steady state (not the first dialog of a process, whose frames include the renderer's one-time pipeline setup), with the layout and paint phases recorded separately. The harness's cairo renderer SHALL be measured and recorded too, and its animation paint SHALL be attributed to the test renderer (A111), never to the rows. A dispatch that cannot meet its budget because of the toolkit SHALL be recorded as a toolkit-floor exception with a GTK axiom probe and its measured figures, and SHALL NOT be left unexplained.

#### Scenario: Opening a large browser
- **WHEN** the user opens `Browse Notes...` over 131,072 admitted rows
- **THEN** no dispatch from the activation until the first rows and the first preview are shown exceeds `B_SETTLE_US`

#### Scenario: Typing over a large source
- **WHEN** the user types in the search field while 131,072 rows are listed
- **THEN** each key-press dispatch fits `B_INTERACT_US` and each query publish fits `B_SETTLE_US`

#### Scenario: Many open-editor bookmarks
- **WHEN** one open editor holds 20,000 bookmarks and the user opens `Browse Bookmarks...`
- **THEN** the snapshot is captured in several turns, each within `B_SETTLE_US`, and every one of the 20,000 bookmarks is listed when the byte limit allows it

#### Scenario: The frame that first shows rows
- **WHEN** the browser first shows rows from a source of 16,384 rows, in a dialog opened after the process's first
- **THEN** that frame fits `B_SETTLE_US` under the GL renderer, or it is a recorded toolkit-floor exception whose figure, attribution and axiom are in the programme record and whose layout phase a widget test bounds at twice its measured worst

#### Scenario: Closing the dialog
- **WHEN** the user closes the browser with 16,384 rows listed
- **THEN** the close input dispatch fits `B_INTERACT_US` and no later dispatch of the browser exceeds `B_SETTLE_US` under the GL renderer

### Requirement: Command palette grouped and category dispatches fit the interaction budgets
On the reference host, publishing grouped palette results with every category full and its "More" rows SHALL fit `B_INTERACT_US` and SHALL cost no more than the same publish on the base revision beyond measurement noise, and the frame after it SHALL fit `B_SETTLE_US`. In a category view, at any row count, every input dispatch (Enter on a "More" row, a keystroke, Up, Down, Page Up, Page Down, Escape, Backspace) SHALL fit `B_INTERACT_US`. Each publish, the frame after it, the return to grouped results and each turn of the row retirement after leaving SHALL fit `B_SETTLE_US`. Ranking, counting and filtering SHALL run off GTK, and the GTK-side work of a publish SHALL not grow with the number of ranked rows. Measurements SHALL be interleaved with the base revision at a 1-minute load below 4, with frame verdicts under the GL renderer and the cairo figures recorded. A dispatch that cannot meet its budget because of the toolkit SHALL be recorded as a toolkit-floor exception with a GTK axiom probe, its measured figures and a widget assertion bounding it, and SHALL NOT be left unexplained.

#### Scenario: The grouped publish keeps its cost
- **WHEN** All mode publishes 64 rows in every category plus their More rows
- **THEN** the publish dispatch fits `B_INTERACT_US` and stays within the base revision's measured range

#### Scenario: Opening and typing in a large category
- **WHEN** the user opens a category view over 131,072 files and types a query
- **THEN** each input dispatch fits `B_INTERACT_US` and each publish fits `B_SETTLE_US`, at the same cost as over 512 files

#### Scenario: Reaching the last row
- **WHEN** the user moves the selection to the last of 131,072 rows with Page Down
- **THEN** each key-press dispatch fits `B_INTERACT_US`

#### Scenario: The frame after the first category publish
- **WHEN** the user enters a category view over 131,072 files
- **THEN** the frame after the first publish fits `B_SETTLE_US` under the GL renderer, and its layout phase fits `B_INTERACT_US`
