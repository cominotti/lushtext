## MODIFIED Requirements

### Requirement: Automation snapshot mapping remains bounded
The Automation1 adapter SHALL map LushText app state into proof-spine snapshot
objects without broadening the exposed data surface. Snapshot serialization
MUST remain bounded to documented diagnostics and MUST preserve existing
redaction or omission behavior for private state. For workflows that expose a
typed evidence surface, the adapter SHALL project snapshot fields from that
evidence surface rather than independently re-deriving the same state from
widgets, and the externally visible snapshot contract MUST remain unchanged by
that projection.

#### Scenario: Visual geometry fields remain safe
- **WHEN** a visual proof tool reads an Automation1 snapshot after layout
  settles
- **THEN** it can access documented safe surface names, rectangles,
  visibility, allocation sizes, scroll anchors, scale factor, and readiness
  detail
- **AND** it cannot access arbitrary widget pointers, document contents, note
  bodies, draft bodies, local-history contents, or private persistence IDs

#### Scenario: Snapshot field meanings do not change
- **WHEN** a smoke test compares representative pre-migration and
  post-migration snapshots for the same app state
- **THEN** fields such as active tab metadata, visible surfaces, search state,
  minimap state, preview state, workflow readiness, and recent notifications
  retain their documented meanings
- **AND** any intentionally additive field is optional for older clients

#### Scenario: Migrated workflow state is projected, not re-derived
- **WHEN** a workflow exposes a typed evidence surface and an automation snapshot
  reports that workflow's state
- **THEN** the adapter reads the evidence surface and projects the documented
  snapshot fields from it
- **AND** it does not maintain a second independent derivation of the same state
  from widget properties

#### Scenario: Projection does not widen the external surface
- **WHEN** an evidence surface exposes internal fields that are not part of the
  documented automation contract
- **THEN** those fields are not serialized into the snapshot
- **AND** existing redaction and omission behavior for private state is preserved

#### Scenario: Evidence-to-snapshot drift is detected
- **WHEN** an evidence surface gains, removes, or renames a field that a snapshot
  projects
- **THEN** `make check-automation-docs` fails until the automation documentation is
  updated
- **AND** the failure names both the evidence field and the affected snapshot field
