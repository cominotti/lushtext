## MODIFIED Requirements

### Requirement: Preferences exposes a Data page for status and retry
The system SHALL add a `Preferences > Data` page that lets the user inspect current app-owned metadata format status, manually rescan, retry applicable upgrades, and see concise recovery/backup state. The page SHALL render correctly for empty/current state, representative upgradeable state, many or awkward items, failed conversion state, future-version state, active verification state, and constrained dialog geometry. Empty or irrelevant action groups MUST NOT be shown, and a completed current-state scan SHALL provide an explicit verified-current visual affordance alongside the manual rescan control.

#### Scenario: Current data shows a quiet status
- **WHEN** the user opens `Preferences > Data` with no app data or only latest-format metadata
- **THEN** the page reports that the data format is current
- **AND** no destructive, irrelevant, or empty action section is presented
- **AND** the Data Format row shows a verified-current affordance near the manual rescan control

#### Scenario: Manual rescan gives visible verification feedback
- **WHEN** the user activates the manual Data Format rescan control
- **THEN** the Data Format row reports that app data formats are being verified
- **AND** the rescan control is disabled while verification is active
- **AND** a fast no-op scan remains visibly in the verification state for at least one short perceptible dwell interval before returning to the completed state
- **AND** a completed current scan restores the quiet current status with the verified-current affordance visible

#### Scenario: Upgradeable data can be converted from Preferences
- **WHEN** the manual scan finds supported older metadata after startup
- **THEN** the page shows a grouped summary and a Convert action
- **AND** activating Convert uses the same format-upgrade service path as startup conversion

#### Scenario: Future data has no Convert control
- **WHEN** the manual scan finds metadata from a newer LushText format
- **THEN** the page explains that this app cannot convert it
- **AND** no Convert action is visible or enabled for that item
- **AND** no verified-current affordance is shown for the non-current state

#### Scenario: Many details scroll without hiding commands
- **WHEN** the page displays many metadata items or long app-data-relative paths
- **THEN** the summary and primary actions remain reachable
- **AND** only the item detail region scrolls without horizontal scrolling, fake rows, or clipped commands

#### Scenario: Failure status remains actionable
- **WHEN** a previous conversion attempt failed
- **THEN** the page reports a concise failure status with retry or quit/start-fresh guidance as appropriate
- **AND** detailed paths remain available through logs, backup manifests, or an explicit details area rather than overwhelming the main page
- **AND** no verified-current affordance is shown for the failed state
