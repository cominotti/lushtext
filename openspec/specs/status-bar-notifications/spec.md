## Purpose

Define the persistent bottom status bar's notification presentation contract.

## Requirements

### Requirement: Status-bar notification updates flash the full message area
The system SHALL briefly flash the entire horizontal status-bar message area whenever the visible status-bar notification is newly published or meaningfully updated. The message area is the full space reserved for feedback between the workspace-sidebar toggle and the document metadata controls; the flash MUST cover empty horizontal space in that area and MUST NOT be limited to the text glyphs.

#### Scenario: Repeated save flashes the full message area
- **WHEN** the user saves the active document twice quickly
- **AND** each save publishes the same visible `File saved` status-bar notification
- **THEN** the status-bar message area flashes for the second save
- **AND** the flash spans the full available message area between the workspace toggle and document metadata controls
- **AND** the visible text remains `File saved`

#### Scenario: Message-area flash excludes status-bar controls
- **WHEN** a status-bar notification update flashes
- **THEN** the workspace-sidebar toggle is not included in the flash background
- **AND** the document metadata controls are not included in the flash background

### Requirement: Status-bar notification flashes use severity colors with readable contrast
The system SHALL style status-bar notification flashes according to the visible notification severity. Info flashes MUST use an accent or blue-hued treatment, warning flashes MUST use a warning or yellow-hued treatment, and error flashes MUST use an error or red-hued treatment. During each flash, message text MUST remain readable against the flash background in light and dark themes.

#### Scenario: Info notification uses info flash treatment
- **WHEN** an info status-bar notification becomes the visible message
- **THEN** the message area flashes with the info treatment
- **AND** the message text remains readable during the flash

#### Scenario: Warning notification uses warning flash treatment
- **WHEN** a warning status-bar notification becomes the visible message
- **THEN** the message area flashes with the warning treatment
- **AND** the message text remains readable during the flash

#### Scenario: Error notification uses error flash treatment
- **WHEN** an error status-bar notification becomes the visible message
- **THEN** the message area flashes with the error treatment
- **AND** the message text remains readable during the flash

### Requirement: Status-bar notification flashes restart for rapid visible updates
The system SHALL restart the status-bar message-area flash for each visible notification publication or meaningful visible update, even when the new visible message has the same text and severity as the previous one. Maintenance renders, expiry sweeps, resolve events, and progress heartbeats that do not change the visible user-facing message MUST NOT start a new flash.

#### Scenario: Identical visible notifications restart the flash
- **WHEN** the same status-bar notification text and severity are published twice in rapid succession
- **THEN** the second publication restarts the message-area flash

#### Scenario: Progress heartbeat does not flash
- **WHEN** a progress notification is visible in the status bar
- **AND** the progress owner renews its heartbeat without changing the visible text or severity
- **THEN** the status-bar message area does not start a new flash

#### Scenario: Hidden progress update does not flash over a transient message
- **WHEN** a transient status-bar notification is the visible message
- **AND** an underlying progress notification updates without becoming visible
- **THEN** the visible transient message remains unchanged
- **AND** the status-bar message area does not start a new flash for the hidden progress update

### Requirement: Status-bar message area is visually separated from the workspace toggle
The system SHALL keep a small horizontal gap between the workspace-sidebar toggle and the status-bar message area. The gap MUST be outside the flashing message-area background, so visible notification flashes begin after the gap and do not visually merge with the workspace toggle.

#### Scenario: Info flash starts after the workspace toggle gap
- **WHEN** an info status-bar notification becomes visible
- **THEN** the message-area flash begins after a small horizontal gap from the workspace-sidebar toggle
- **AND** the gap does not use the message-area flash background
- **AND** the workspace-sidebar toggle remains outside the flash background

#### Scenario: Severity flashes preserve the left gap
- **WHEN** warning and error status-bar notifications become visible
- **THEN** each message-area flash preserves the same small horizontal gap from the workspace-sidebar toggle
- **AND** the document metadata controls remain outside the flash background

#### Scenario: Message text keeps its compact alignment
- **WHEN** a status-bar notification is rendered
- **THEN** the message text keeps its existing compact offset inside the message area
- **AND** the added gap does not increase the status-bar height

### Requirement: Status bar remains readable while visually subordinate
The system SHALL render the persistent bottom status bar with enough vertical comfort for status messages and compact document metadata controls to be readable in normal use. The status bar MUST remain visually subordinate to the primary header bar: it MUST stay a one-row status strip, MUST NOT use header-scale title treatment, and MUST NOT match or exceed the header bar's visual prominence by default.

#### Scenario: Populated status bar is easier to scan
- **WHEN** an active document displays a status-bar message with line-ending and encoding metadata
- **THEN** the message text and metadata labels are vertically centered and readable
- **AND** the bar remains a single-row bottom status strip
- **AND** the status bar appears less prominent than the primary header bar

#### Scenario: Empty document state does not look overbuilt
- **WHEN** no active document is open and document metadata is hidden
- **THEN** the status bar remains visually balanced with the workspace-sidebar toggle and empty message lane
- **AND** the empty status bar does not look like a second header bar or toolbar

#### Scenario: Readability holds in light, dark, and high contrast styles
- **WHEN** the application is shown in light, dark, or high contrast style
- **THEN** status-bar messages and compact metadata controls remain readable
- **AND** the status bar keeps lower visual prominence than the primary header bar

### Requirement: Status bar readability changes preserve layout boundaries
The system SHALL preserve the existing status-bar structure while improving readability. The workspace-sidebar toggle, the non-flashing left gap, the full-width message area, and the compact metadata controls MUST remain distinct. Message text MUST continue to ellipsize rather than force the status bar into multiple rows or create an unintended horizontal scrollbar.

#### Scenario: Long message remains bounded
- **WHEN** a long status-bar notification is visible
- **THEN** the message text ellipsizes within the message area
- **AND** the status bar remains one row tall
- **AND** no unintended horizontal scrollbar appears
- **AND** the document metadata controls remain reachable

#### Scenario: Notification flash boundaries are unchanged
- **WHEN** a visible status-bar notification flashes after the readability update
- **THEN** the flash remains scoped to the message area
- **AND** the workspace-sidebar toggle, non-flashing left gap, and document metadata controls remain outside the flash background

#### Scenario: Constrained height preserves the status strip contract
- **WHEN** the window is short enough that the central editor/sidebar shell must shrink or clip
- **THEN** the status bar remains visible and readable
- **AND** the status bar's height does not push itself below the visible window
- **AND** the status bar does not consume so much height that the central editing area becomes unusable
