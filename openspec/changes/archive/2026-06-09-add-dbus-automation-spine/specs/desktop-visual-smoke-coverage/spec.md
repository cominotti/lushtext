## ADDED Requirements

### Requirement: Visual smoke scenarios SHALL use automation state before capture
The visual smoke lane SHALL use the automation spine to verify intended app state before screenshots whenever the state is exposed through actions or the read-only automation snapshot. Screenshots MUST remain visual proof, not the only proof that the app reached the intended workflow.

#### Scenario: Search/minimap capture verifies state first
- **WHEN** the visual smoke lane captures a search and minimap state
- **THEN** it first verifies the active document, search query, match count or bounded match summary, and minimap visibility through actions or automation state
- **AND** it then captures the screenshot and nonblank/window-bounds assertions

#### Scenario: Preview and properties captures verify state first
- **WHEN** the visual smoke lane captures Markdown preview or document-properties states
- **THEN** it first verifies preview mode, active document identity, and requested/rendered secondary-surface state through the automation snapshot or stateful actions
- **AND** compact and wide presentations remain distinguishable in artifacts

#### Scenario: State mismatch fails before accepting screenshot
- **WHEN** a scenario screenshot exists but the automation state does not match the requested scenario
- **THEN** the lane fails with the state mismatch, logs, and screenshot preserved
- **AND** it does not report the screenshot as proof of the intended state

### Requirement: Visual smoke SHALL include an automation-backed scenario matrix
The visual smoke lane SHALL support an automation-backed matrix that covers representative user workflows and UI state extremes without relying on coordinate input.

#### Scenario: Empty and no-context states are captured
- **WHEN** the matrix captures no-document, empty workspace, empty notes, empty bookmarks, empty search results, or no-required-context surfaces
- **THEN** the automation snapshot records the empty-state kind
- **AND** screenshots show readable empty states with reachable persistent commands and no fake rows

#### Scenario: Dense and awkward states are captured
- **WHEN** the matrix captures many tabs, long file names, dense workspace rows, many notes/bookmarks, or long search results
- **THEN** automation state records counts and selected identity
- **AND** screenshots show item-region-only scrolling, preserved headers/close/actions, and no unintended horizontal scrollbars or clipped primary controls

#### Scenario: Constrained geometry is captured
- **WHEN** the matrix captures narrow, compact, or short-window geometry
- **THEN** automation state records requested and rendered surfaces
- **AND** screenshots prove persistent chrome remains visible unless the tested mode intentionally hides it

### Requirement: Visual smoke automation docs SHALL stay synchronized
The project SHALL document each visual smoke scenario, its fixture data, actions, state predicates, screenshots, artifacts, and host requirements. Scenario definitions and documentation MUST change together.

#### Scenario: Scenario documentation explains proof chain
- **WHEN** maintainers read visual smoke or automation documentation
- **THEN** each scenario explains which action, D-Bus, AT-SPI, screenshot, warning-scan, and artifact assertions prove the workflow

#### Scenario: Scenario drift fails validation
- **WHEN** a scenario name, helper flag, fixture contract, state predicate, or expected artifact changes
- **THEN** the scenario documentation or generated reference check fails until it is updated
