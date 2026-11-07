## ADDED Requirements

### Requirement: Notes browser dispatches fit the interaction budgets
Every GTK dispatch of the Notes browser SHALL fit `B_SETTLE_US` on the reference host, whatever the number of rows. That covers activating `Browse Notes...` or `Browse Bookmarks...` until the first rows and the first preview are shown, each open-editor snapshot turn, each query publish, each selection change, and closing the dialog. The input dispatch of a keystroke in the browser's search field SHALL fit `B_INTERACT_US`. Work that grows with the number of rows (source construction, matching, category grouping) SHALL run off GTK, and GTK-side work that grows with data (the open-editor snapshot) SHALL run in turns bounded by the shared list turn slice. A dispatch that cannot meet its budget because of the toolkit SHALL be recorded as a toolkit-floor exception with a GTK axiom probe and its measured figures, and SHALL NOT be left unexplained.

#### Scenario: Opening a large browser
- **WHEN** the user opens `Browse Notes...` over 131,072 admitted rows
- **THEN** no dispatch from the activation until the first rows and the first preview are shown exceeds `B_SETTLE_US`

#### Scenario: Typing over a large source
- **WHEN** the user types in the search field while 131,072 rows are listed
- **THEN** each key-press dispatch fits `B_INTERACT_US` and each query publish fits `B_SETTLE_US`

#### Scenario: Many open-editor bookmarks
- **WHEN** one open editor holds 20,000 bookmarks and the user opens `Browse Bookmarks...`
- **THEN** the snapshot is captured in several turns, each within `B_SETTLE_US`, and every one of the 20,000 bookmarks is listed when the byte limit allows it
