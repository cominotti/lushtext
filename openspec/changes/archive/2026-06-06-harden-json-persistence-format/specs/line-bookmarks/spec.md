## ADDED Requirements

### Requirement: Bookmark sidecars use the public v1 JSON envelope
The system SHALL persist bookmark sidecars as supported v1 app-owned JSON envelopes under `$XDG_DATA_HOME/lushtext/bookmarks/`. Runtime loading MUST require the bookmark-sidecar document kind and supported version before reading bookmark records.

#### Scenario: Save bookmark sidecar as v1
- **WHEN** a saved document's bookmarks are persisted
- **THEN** the bookmark sidecar is written as a pretty JSON envelope with the bookmark-sidecar document kind
- **AND** the payload stores the document identity and bookmark records

#### Scenario: Unsupported bookmark sidecar is isolated
- **WHEN** a bookmark sidecar is bare pre-public JSON, wrong-kind JSON, unsupported-version JSON, or malformed JSON
- **THEN** that sidecar is preserved through recovery diagnostics before replacement is allowed
- **AND** unrelated valid bookmark sidecars continue to load
