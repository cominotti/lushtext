## ADDED Requirements

### Requirement: Session and draft manifests use the public v1 JSON envelope
The system SHALL persist `session.json` and `drafts/manifest.json` as supported v1 app-owned JSON envelopes. Runtime loading MUST require the correct document kind and supported version before reading session or draft-manifest payloads.

#### Scenario: Save session as v1
- **WHEN** the app persists session state
- **THEN** `session.json` is written as a pretty JSON envelope with the session document kind
- **AND** the payload stores file paths, untitled draft IDs, cursor position, scroll position, pinned state, and selected tab index

#### Scenario: Save draft manifest as v1
- **WHEN** the app persists draft manifest state
- **THEN** `drafts/manifest.json` is written as a pretty JSON envelope with the draft-manifest document kind
- **AND** the payload maps draft IDs to original paths, backing-file mtimes, and saved timestamps

### Requirement: Path-backed draft IDs use explicit stable hashing in v1
The system SHALL derive path-backed draft IDs in the v1 draft format with an explicit stable hashing algorithm rather than an implementation-dependent hasher. The algorithm MUST be documented in code and covered by deterministic tests.

#### Scenario: Same path yields same v1 draft ID
- **WHEN** the v1 draft ID helper receives the same absolute file path across process launches
- **THEN** it returns the same draft ID
- **AND** the result does not depend on process-randomized hash seeds

#### Scenario: Unsupported old draft manifest is preserved
- **WHEN** startup finds an unsupported pre-public draft manifest
- **THEN** the manifest is preserved through recovery diagnostics before replacement is allowed
- **AND** the runtime does not parse it through a permanent legacy manifest reader
