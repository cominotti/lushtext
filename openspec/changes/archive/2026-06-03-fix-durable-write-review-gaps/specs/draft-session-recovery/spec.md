## ADDED Requirements

### Requirement: Startup draft preload is bounded and non-destructive
The system SHALL bound eager draft-content preloading during startup restore. Before loading draft bodies into memory, the system MUST inspect draft file sizes and enforce a total preload cap of `64 * 1024 * 1024` bytes. Drafts skipped because of the cap MUST NOT be deleted or removed from the manifest solely because eager preload was bounded.

#### Scenario: Oversized draft is skipped without deletion
- **WHEN** startup restore finds a draft file whose size would exceed the eager preload limit
- **THEN** the draft body is not loaded into the preload map
- **AND** the draft file and manifest entry remain available for later recovery handling

#### Scenario: Total preload cap prevents startup memory spike
- **WHEN** startup restore finds multiple draft files whose combined size exceeds the total preload cap
- **THEN** the system preloads only drafts within the cap
- **AND** the remaining drafts are reported as skipped without deleting recovery data

#### Scenario: Skipped draft does not block session tab restore
- **WHEN** a session tab has draft recovery data that was not eagerly preloaded because of the cap
- **THEN** startup restore still attempts to recreate the tab
- **AND** the absence of preloaded body text does not erase the tab or draft identity
