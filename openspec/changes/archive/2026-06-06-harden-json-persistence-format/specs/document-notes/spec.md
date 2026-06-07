## ADDED Requirements

### Requirement: Document-note sidecars use the public v1 JSON envelope
The system SHALL persist document-note sidecars as supported v1 app-owned JSON envelopes under `$XDG_DATA_HOME/lushtext/document-notes/`. Runtime loading MUST require the document-note sidecar kind and supported version before reading the note payload.

#### Scenario: Save document note as v1
- **WHEN** a saved document's document note is persisted
- **THEN** the document-note sidecar is written as a pretty JSON envelope with the document-note document kind
- **AND** the payload stores the document identity and rich note body

#### Scenario: Unsupported document-note sidecar is isolated
- **WHEN** a document-note sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid document notes continue to load
