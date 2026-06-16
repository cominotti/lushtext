## ADDED Requirements

### Requirement: Open Popover Recent Rows Reveal Full Paths On Hover
The Open popover SHALL expose the full absolute activation path for every visible recent-document row through row hover tooltip text. The tooltip MUST use the same path that row activation opens, MUST remain complete and unellipsized even when row title or subtitle text is visually ellipsized, and MUST NOT change GNOME-style row layout, scrolling, or remove-control behavior.

#### Scenario: Hovering a recent row shows the full activation path
- **WHEN** the Open popover shows a recent row for a local file-backed document
- **THEN** hovering the row's non-action surface exposes tooltip text equal to the full absolute path that activating the row opens
- **AND** the tooltip text is not shortened to the title, parent directory, canonical identity, or visually ellipsized row subtitle
- **AND** the row keeps the existing compact GNOME-style layout and no-horizontal-scrollbar behavior

#### Scenario: Awkward and deep paths remain inspectable
- **WHEN** a visible recent row represents a file with a long absolute path containing deep directories, spaces, symbols, or mixed-width text
- **THEN** the row hover tooltip exposes the complete absolute path string for that file
- **AND** the row title and subtitle may still ellipsize inside the popover width without changing the tooltip string

#### Scenario: Remove control keeps its action tooltip
- **WHEN** a visible recent row exposes its trailing remove control
- **THEN** hovering the remove control exposes the remove action tooltip
- **AND** the remove control tooltip is not replaced by the document path tooltip
- **AND** activating the remove control still removes only that recent row without opening the document or closing the Open popover

#### Scenario: Recycled recent rows refresh tooltip text
- **WHEN** the recent list contains enough rows, filtering, or scrolling to cause GTK to reuse row widgets for different recent documents
- **THEN** every rebound row exposes tooltip text for the currently bound row's full absolute activation path
- **AND** no visible row exposes a tooltip from a previously bound recent document

#### Scenario: Empty and filtered states do not create fake path tooltips
- **WHEN** the Open popover has no eligible recent rows or the current search filters every recent row out
- **THEN** no fake recent row is inserted to provide path tooltip behavior
- **AND** the search entry and file-chooser action remain visible and reachable
