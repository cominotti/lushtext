## ADDED Requirements

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
