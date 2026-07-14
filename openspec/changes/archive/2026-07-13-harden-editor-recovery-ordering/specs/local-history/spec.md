## ADDED Requirements

### Requirement: Failed baseline capture remains safely retryable
The system MUST retain or recover the clean pre-edit baseline when baseline persistence fails. A retry MUST occur only while the same editor lifetime, saved-file identity, and editing cycle still own that baseline, and MUST NOT overwrite a newer clean baseline.

#### Scenario: Initial baseline write fails transiently
- **WHEN** the first baseline persistence attempt fails while the document remains modified on the same path
- **THEN** the pre-edit text remains available for bounded retry
- **AND** a later successful retry records the original pre-edit state

#### Scenario: Path changes before failed baseline returns
- **WHEN** a baseline write fails after Save As, rename, reload, or editor disposal changes its ownership facts
- **THEN** the old text is not restored into the new lineage
- **AND** no retry writes it under the newer path identity

#### Scenario: New clean baseline supersedes a failed attempt
- **WHEN** a later successful save establishes a newer clean baseline before the older failure completes
- **THEN** the older baseline cannot replace the newer baseline candidate
- **AND** future editing cycles use the latest clean state
