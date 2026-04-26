## ADDED Requirements

### Requirement: Focus Mode enters a reversible focused shell
The system SHALL provide a per-window Focus Mode that enters fullscreen, suppresses persistent non-writing chrome, and restores the previous shell presentation when Focus Mode exits. Focus Mode active state MUST NOT persist across application launches.

#### Scenario: Enter Focus Mode from normal editing
- **WHEN** a document window is open and the user activates Focus Mode
- **THEN** the window enters fullscreen
- **AND** the ordinary header bar, tab bar, status bar, workspace sidebar, and document-properties surface are not rendered
- **AND** the active editor remains the primary focused surface

#### Scenario: Exit Focus Mode restores the previous shell
- **WHEN** Focus Mode is active after being entered from a non-fullscreen window with the workspace sidebar requested visible
- **AND** the user exits Focus Mode
- **THEN** the window leaves fullscreen
- **AND** the workspace sidebar renders according to its previously requested state
- **AND** the ordinary header bar, tab bar, and status bar are visible again

#### Scenario: Existing fullscreen state is preserved
- **WHEN** the user enters Focus Mode from an already fullscreen window
- **AND** the user exits Focus Mode
- **THEN** the window remains fullscreen
- **AND** only the Focus Mode chrome suppression is removed

### Requirement: Focus Mode uses dedicated actions and shortcuts
The system SHALL expose Focus Mode through a stateful `win.toggle-focus-mode` action and the `Ctrl+Shift+F11` shortcut. The existing `F11` fullscreen shortcut MUST remain ordinary fullscreen and MUST NOT become Focus Mode.

#### Scenario: Toggle Focus Mode with shortcut
- **WHEN** the main window has focus and the user presses `Ctrl+Shift+F11`
- **THEN** Focus Mode toggles on or off for that window

#### Scenario: Fullscreen shortcut remains separate
- **WHEN** the main window has focus and the user presses `F11`
- **THEN** the existing fullscreen action toggles fullscreen
- **AND** Focus Mode state does not change

### Requirement: Focus Mode preserves Markdown preview-only behavior
The system SHALL keep `Alt+P` assigned to Markdown preview-only mode while Focus Mode is active. Activating `Alt+P` in Focus Mode MUST switch between focused source editing and focused rendered Markdown without exiting Focus Mode.

#### Scenario: Preview-only mode opens while focused
- **WHEN** Focus Mode is active on a Markdown document in source editing view
- **AND** the user presses `Alt+P`
- **THEN** the rendered Markdown preview fills the focused content area
- **AND** Focus Mode remains active

#### Scenario: Preview-only mode closes while focused
- **WHEN** Focus Mode is active and Markdown preview-only mode is visible
- **AND** the user presses `Alt+P`
- **THEN** the source editor fills the focused content area again
- **AND** Focus Mode remains active

#### Scenario: Side-by-side preview is suppressed on entry
- **WHEN** side-by-side Markdown preview is visible
- **AND** the user enters Focus Mode
- **THEN** side-by-side preview is temporarily hidden so `Alt+P` can operate as preview-only mode
- **AND** the previous side-by-side preview state is remembered for restoration when Focus Mode exits unless the user changes preview state while focused

### Requirement: Focus Mode preserves shortcut priority for transient surfaces
The system SHALL let higher-priority transient surfaces handle `Escape` before Focus Mode exits. Focus Mode MUST exit on `Escape` only when no command palette, in-tab search, workspace search panel, dialog, popover, or other transient surface is active.

#### Scenario: Escape closes command palette first
- **WHEN** Focus Mode is active and the command palette is open
- **AND** the user presses `Escape`
- **THEN** the command palette closes
- **AND** Focus Mode remains active

#### Scenario: Escape exits Focus Mode when no overlay is active
- **WHEN** Focus Mode is active and no higher-priority transient surface is active
- **AND** the user presses `Escape`
- **THEN** Focus Mode exits

### Requirement: Focus Mode provides a minimal reveal affordance
The system SHALL provide a minimal overlaid Focus Mode affordance that lets users discover or activate Leave Focus Mode without restoring the ordinary full header bar. The affordance MUST remain visually minimal, MUST NOT obscure the readable writing column during ordinary typing, and MUST be reachable without a pointer-only interaction.

#### Scenario: Reveal focus affordance near the top edge
- **WHEN** Focus Mode is active
- **AND** the user moves the pointer near the top edge of the window
- **THEN** the system reveals a small Focus Mode affordance with a Leave Focus Mode action

#### Scenario: Affordance does not cover active writing content while hidden
- **WHEN** Focus Mode is active and the user is typing normally
- **THEN** the overlaid affordance is hidden or visually out of the writing column
- **AND** the source editor or rendered Markdown content remains readable

### Requirement: Focus Mode centers source editing in a readable column
The system SHALL center source editor content in a readable column while Focus Mode is active. The target column width MUST be preference-backed, default to 80 columns, use the active editor font metrics, and restore normal editor margins when Focus Mode exits.

#### Scenario: Editor column centers on a wide window
- **WHEN** Focus Mode is active on a wide window
- **THEN** the source editor text is centered in a readable column near the configured target width
- **AND** the text does not stretch across the full window width

#### Scenario: Narrow windows remain usable
- **WHEN** Focus Mode is active on a narrow window
- **THEN** the source editor keeps usable left and right margins
- **AND** the configured column width does not force horizontal clipping beyond the editor's normal wrapping behavior

#### Scenario: Normal margins restore after exit
- **WHEN** Focus Mode has changed the source editor margins
- **AND** the user exits Focus Mode
- **THEN** the source editor margins return to their normal-mode values

### Requirement: Focus Mode shows a source text-origin guide
The system SHALL show a subtle, non-interactive vertical guide at the source editor text origin while Focus Mode is active. The guide MUST mark the left bound of the document content column, MUST move with the active Focus Mode source-editor margin, and MUST remain hidden outside Focus Mode. The guide MUST NOT alter document content, cursor behavior, selection behavior, scrolling, editor margins, or persisted preferences.

#### Scenario: Text-origin guide appears while source editing is focused
- **WHEN** Focus Mode is active in source editing view
- **THEN** a gentle vertical guide is visible at the source editor's column-zero text origin
- **AND** indentation inside the document appears to the right of that guide

#### Scenario: Text-origin guide tracks readable-column changes
- **WHEN** Focus Mode is active in source editing view
- **AND** the readable-column margin changes because the window is resized or the target column preference changes
- **THEN** the text-origin guide moves to the updated source editor text origin

#### Scenario: Text-origin guide hides outside Focus Mode
- **WHEN** the source editor is visible outside Focus Mode
- **THEN** the text-origin guide is not visible

#### Scenario: Rendered Markdown is not given a technical origin guide
- **WHEN** Focus Mode is active and Markdown preview-only mode is visible
- **THEN** the source editor text-origin guide is not shown over the rendered Markdown preview

### Requirement: Focus Mode centers rendered Markdown in a readable column
The system SHALL apply a matching readable-column policy to rendered Markdown preview while Focus Mode preview-only mode is active. The rendered Markdown column MUST remain readable on wide windows and MUST keep normal preview margins outside Focus Mode.

#### Scenario: Rendered Markdown stays readable while focused
- **WHEN** Focus Mode is active on a Markdown document
- **AND** the user enters Markdown preview-only mode
- **THEN** the rendered Markdown content is centered in a readable column
- **AND** the preview does not stretch prose across the full window width

#### Scenario: Preview margins restore outside Focus Mode
- **WHEN** rendered Markdown has used Focus Mode column margins
- **AND** the user exits Focus Mode or leaves preview-only mode
- **THEN** the Markdown preview returns to its normal preview margins

### Requirement: Focus Mode provides optional typewriter scrolling
The system SHALL provide a Focus Mode typewriter scrolling preference that defaults off. When enabled and Focus Mode is active in source editing view, cursor movement or text insertion MUST keep the cursor line near the vertical center of the editor viewport without changing saved session cursor or scroll semantics.

#### Scenario: Typewriter scrolling defaults off
- **WHEN** the user has not changed Focus Mode preferences
- **THEN** entering Focus Mode does not force typewriter scrolling

#### Scenario: Enabled typewriter scrolling follows the cursor
- **WHEN** Focus Mode is active and typewriter scrolling is enabled
- **AND** the user types or moves the cursor in the source editor
- **THEN** the editor scrolls so the cursor line remains near the vertical center of the visible editor area

#### Scenario: Typewriter scrolling does not affect rendered preview
- **WHEN** Focus Mode is active and Markdown preview-only mode is visible
- **THEN** typewriter scrolling does not attempt to move a source-editor cursor inside the rendered preview

### Requirement: Focus Mode preferences live in Preferences
The system SHALL expose Focus Mode preferences in the application Preferences UI. The preferences MUST include the target column width and typewriter scrolling toggle, and changing them MUST affect future and currently active Focus Mode rendering without requiring an app restart.

#### Scenario: Change column width preference
- **WHEN** the user changes the Focus Mode column width preference while Focus Mode is active
- **THEN** the active source editor or rendered Markdown preview recalculates its readable column using the new target

#### Scenario: Toggle typewriter scrolling preference
- **WHEN** the user toggles the Focus Mode typewriter scrolling preference
- **THEN** newly focused editor interactions use the updated typewriter scrolling behavior
