## ADDED Requirements

### Requirement: Protected View content is reachable through an accessible path
The main protected view's accessible text SHALL be documented as the folded view. Its accessible description SHALL state that long lines are shortened and SHALL name the Inspect Line shortcut. The Line Inspector SHALL be the accessible path to folded content. It SHALL have:
- an accessible label with the line number and the window's column range;
- labelled Previous, Next, Go to Column and Copy controls;
- a polite announcement when the window moves.

The fold-marker gutter SHALL expose an accessible label with the hidden-character count. If the lane's AT case (ledger P4) shows that an assistive technology's text queries or caret writes on a folded paragraph exceed `B_interact`, the main view SHALL be made non-focusable, its accessible description SHALL name the Line Inspector as the reading path, and `docs/accessibility.md` SHALL record the limitation. `docs/accessibility.md` and `docs/accessibility-matrix.md` SHALL gain Protected View and Line Inspector rows. `docs/accessibility-orca-checklist.md` SHALL gain a manual entry that walks through opening a protected document, hearing the notice, opening the inspector, moving windows and copying a window. Accessibility smoke SHALL cover the inspector's keyboard path.

#### Scenario: Keyboard-only reading of a giant line
- **WHEN** a keyboard-only user opens a protected document, presses the Inspect Line shortcut, accepts the default line and presses Next Window
- **THEN** focus moves into the inspector, the window text is exposed to AT-SPI, and the window change is announced once

#### Scenario: Orca checklist entry exists
- **WHEN** the release Orca checklist is prepared
- **THEN** it contains the Protected View entry with fields for the notice, the inspector navigation and the copy confirmation, and it stores no document text
