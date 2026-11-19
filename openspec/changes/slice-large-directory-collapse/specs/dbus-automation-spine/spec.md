## ADDED Requirements

### Requirement: Automation snapshots report sidebar collapses in progress
The automation snapshot SHALL report, beside `refresh_blocks_readiness`, whether any workspace sidebar directory collapse is still being paced (`collapse_pending`), projected from the sidebar's evidence surface without making the toolkit do work. It SHALL carry no path or file name. The field SHALL be documented with a stable anchor in `docs/automation.md` and `docs/automation-reference.md`, and `make check-automation-docs` SHALL pass.

#### Scenario: A paced collapse in the snapshot
- **WHEN** a sliced collapse of a large sidebar directory is in progress
- **THEN** the snapshot reports `collapse_pending` true and `workspace-refresh-complete` is not satisfied
- **AND** both change together once the collapse ends

#### Scenario: No collapse
- **WHEN** no collapse is in progress
- **THEN** the snapshot reports `collapse_pending` false
