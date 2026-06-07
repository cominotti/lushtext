## ADDED Requirements

### Requirement: Workspace state uses the public v1 JSON envelope
The system SHALL persist `workspaces.json` as a supported v1 app-owned JSON envelope. Runtime loading MUST require the workspace document kind and supported version before reading workspace data.

#### Scenario: Persist workspace state as v1
- **WHEN** workspace state is saved after the format hardening change
- **THEN** `workspaces.json` is written as a pretty JSON envelope with the workspace document kind
- **AND** its payload stores the current workspace scope and single-root workspace list

#### Scenario: Load supported workspace state
- **WHEN** startup loads `workspaces.json` with the workspace document kind and supported version
- **THEN** the sidebar restores the workspace names, roots, and current scope from the payload
- **AND** missing selected-workspace targets still normalize to `All workspaces`

### Requirement: Unsupported workspace JSON is preserved before reset
The system SHALL treat pre-public bare workspace JSON, wrong-kind envelopes, unsupported versions, malformed files, and unsupported file kinds as recovery metadata problems. The system MUST preserve that metadata before writing a default v1 workspace file.

#### Scenario: Unsupported workspace file resets safely
- **WHEN** startup finds unsupported workspace metadata and preservation succeeds
- **THEN** the original metadata is available in quarantine or diagnostic evidence
- **AND** the app may continue with an empty v1 workspace state

#### Scenario: Workspace preservation failure blocks overwrite
- **WHEN** startup finds unsupported workspace metadata and preservation fails
- **THEN** the app does not overwrite the original workspace file
- **AND** it reports that workspace recovery could not safely replace the file

## REMOVED Requirements

### Requirement: Legacy persisted workspaces migrate safely to single-root form
**Reason**: The public-era format is a clean break from pre-public workspace JSON. Runtime app code must not keep legacy multi-root or standalone-file workspace readers.

**Migration**: If conversion of pre-public workspace data is useful, provide an optional one-shot helper under `scripts/migrations/` that writes the v1 workspace envelope. Otherwise unsupported pre-public workspace JSON is preserved through recovery diagnostics and reset to the default v1 workspace state only when replacement is safe.
