## ADDED Requirements

### Requirement: Workspace-note sidecars use the public v1 JSON envelope
The system SHALL persist workspace-note sidecars as supported v1 app-owned JSON envelopes under `$XDG_DATA_HOME/lushtext/workspace-notes/`. Runtime loading MUST require the workspace-note sidecar kind and supported version before reading the note payload.

#### Scenario: Save workspace note as v1
- **WHEN** a workspace root's note is persisted
- **THEN** the workspace-note sidecar is written as a pretty JSON envelope with the workspace-note document kind
- **AND** the payload stores the workspace-root identity and rich note body

#### Scenario: Unsupported workspace-note sidecar is isolated
- **WHEN** a workspace-note sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid workspace notes continue to load
