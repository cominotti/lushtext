## ADDED Requirements

### Requirement: Long-lived app-owned JSON uses a versioned envelope
The system SHALL persist long-lived app-owned JSON documents as pretty JSON envelopes with explicit document kind, integer format version, and data payload fields. The runtime loader MUST validate the kind and version before deserializing the payload.

#### Scenario: Save a v1 JSON document
- **WHEN** the system saves a long-lived app-owned JSON document
- **THEN** the written JSON contains a stable `kind` field identifying the document class
- **AND** it contains a supported integer `version`
- **AND** it contains the document payload under `data`

#### Scenario: Load a matching v1 JSON document
- **WHEN** the system loads a JSON document whose `kind` matches the requested metadata class and whose `version` is supported
- **THEN** it deserializes the `data` payload into that metadata class
- **AND** unknown envelope or payload fields do not prevent loading when the supported schema can otherwise be read

### Requirement: Runtime JSON loading is a clean break from pre-public bare shapes
The system SHALL NOT include permanent runtime readers for pre-public bare JSON shapes. A bare JSON value, wrong-kind envelope, unsupported version, or otherwise unsupported old shape MUST be treated as unsupported metadata rather than migrated by normal app startup.

#### Scenario: Bare old JSON is unsupported at runtime
- **WHEN** a long-lived JSON metadata path contains a pre-public bare JSON value instead of a supported envelope
- **THEN** the runtime reports an unsupported-format recovery diagnostic
- **AND** it does not parse the bare value as a compatibility format

#### Scenario: Wrong document kind is unsupported
- **WHEN** a JSON metadata path contains an envelope whose `kind` does not match the loader's requested metadata class
- **THEN** the runtime reports an unsupported-format recovery diagnostic
- **AND** it does not deserialize that payload into the requested model

#### Scenario: Unsupported version is preserved
- **WHEN** a JSON metadata path contains an envelope with a recognized kind but unsupported version
- **THEN** the runtime reports an unsupported-version recovery diagnostic
- **AND** it does not overwrite the original metadata until preservation has succeeded or replacement is explicitly disallowed

### Requirement: Unsupported JSON is preserved before replacement
The system SHALL preserve unsupported, malformed, wrong-kind, or unsupported-version JSON metadata before writing a v1 replacement for the same logical state. If preservation cannot be completed, the system MUST leave the original metadata untouched and MUST report that replacement is unsafe.

#### Scenario: Unsupported old JSON is quarantined before reset
- **WHEN** a metadata file contains unsupported pre-public JSON and the app needs to continue with default v1 state
- **THEN** the original metadata is moved or copied into app-owned quarantine storage first
- **AND** only then may the app write a v1 replacement

#### Scenario: Preservation failure blocks replacement
- **WHEN** unsupported JSON cannot be quarantined or otherwise preserved
- **THEN** the app does not overwrite the original metadata with a default v1 file
- **AND** the recovery diagnostic marks replacement as unsafe

### Requirement: Optional old-data migration stays outside runtime app code
The project MAY provide one-shot conversion tooling for pre-public app data, but such tooling MUST live under `scripts/migrations/` and MUST NOT be required by normal runtime loading.

#### Scenario: Migration helper is optional
- **WHEN** the repository includes a helper that converts pre-public JSON into v1 envelopes
- **THEN** the helper lives under `scripts/migrations/`
- **AND** the application runtime does not call it automatically during startup

#### Scenario: Runtime remains clean without migration helper
- **WHEN** no migration helper exists for a pre-public JSON shape
- **THEN** the runtime still handles that file as unsupported metadata through diagnostics and preservation
- **AND** it continues with the documented default state when replacement is safe

### Requirement: JSON format fixtures define the public contract
The project SHALL maintain deterministic fixture coverage for the public JSON format contract. Fixtures MUST cover valid v1 documents, missing optional fields, unknown fields, malformed inputs, unsupported old-shape inputs, wrong-kind inputs, unsupported versions, oversized metadata where applicable, and optional migration-script output when such a script exists.

#### Scenario: Valid fixture proves stable shape
- **WHEN** a valid v1 fixture is loaded in tests
- **THEN** it produces the expected domain value
- **AND** re-saving that value preserves the envelope contract

#### Scenario: Unsupported old-shape fixture stays unsupported
- **WHEN** an unsupported pre-public JSON fixture is loaded by runtime tests
- **THEN** the loader returns a recovery diagnostic rather than a migrated domain value
- **AND** the fixture is preserved before replacement is allowed

### Requirement: SQLite remains a future index or cache only
The system SHALL NOT introduce SQLite as the source of truth for this JSON hardening change. SQLite MAY be reconsidered later only for index or cache workloads that need cross-document querying, global notes or history views, persistent file indexing, sync metadata, or similarly database-shaped behavior.

#### Scenario: Current JSON state remains source of truth
- **WHEN** the JSON hardening change is implemented
- **THEN** long-lived app-owned state remains persisted as pretty JSON or plain text bodies as appropriate
- **AND** no SQLite runtime dependency is required

#### Scenario: Future SQLite use is index-shaped
- **WHEN** a later feature needs global querying across many records
- **THEN** SQLite may be proposed as an index or cache layer
- **AND** that later proposal must define how it preserves inspectable source-of-truth data or intentionally replaces it
