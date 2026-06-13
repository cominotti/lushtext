# persistent-json-format-contract Specification

## Purpose
Define the public app-owned JSON format contract so long-lived LushText metadata uses explicit versioned envelopes, unsupported pre-public shapes are preserved safely, and future storage changes stay deliberate.

## Requirements
### Requirement: Long-lived app-owned JSON uses a versioned envelope
The system SHALL persist long-lived app-owned JSON documents as pretty JSON envelopes with explicit document kind, integer format version, and data payload fields. The ordinary runtime loader MUST validate the kind and latest supported version for the requested document class before deserializing the payload.

#### Scenario: Save a v1 JSON document
- **WHEN** the system saves a long-lived app-owned JSON document
- **THEN** the written JSON contains a stable `kind` field identifying the document class
- **AND** it contains a supported integer `version`
- **AND** it contains the document payload under `data`

#### Scenario: Load a matching v1 JSON document
- **WHEN** the system loads a JSON document whose `kind` matches the requested metadata class and whose `version` is the latest supported version for that class
- **THEN** it deserializes the `data` payload into that metadata class
- **AND** unknown envelope or payload fields do not prevent loading when the supported schema can otherwise be read

### Requirement: Runtime JSON loading is a clean break from pre-public bare shapes
The system SHALL NOT include permanent old-format readers in ordinary runtime metadata loaders. A bare JSON value, wrong-kind envelope, unsupported future version, or otherwise unsupported shape MUST be treated as unsupported metadata by normal readers rather than migrated by ordinary app startup. Supported older versioned envelopes MAY be parsed only by the sealed format-upgrade workflow before normal metadata consumers run.

#### Scenario: Bare old JSON is unsupported at runtime
- **WHEN** a long-lived JSON metadata path contains a pre-public bare JSON value instead of a supported envelope
- **THEN** the runtime reports an unsupported-format recovery diagnostic
- **AND** it does not parse the bare value as a compatibility format

#### Scenario: Wrong document kind is unsupported
- **WHEN** a JSON metadata path contains an envelope whose `kind` does not match the loader's requested metadata class
- **THEN** the runtime reports an unsupported-format recovery diagnostic
- **AND** it does not deserialize that payload into the requested model

#### Scenario: Unsupported future version is preserved
- **WHEN** a JSON metadata path contains an envelope with a recognized kind but a version newer than this binary supports
- **THEN** the runtime reports an unsupported-version recovery diagnostic
- **AND** it does not overwrite the original metadata until preservation has succeeded or replacement is explicitly disallowed
- **AND** the format-upgrade workflow does not offer a downgrade conversion for that file

#### Scenario: Supported older version is parsed only by upgrade workflow
- **WHEN** a long-lived JSON metadata path contains a recognized kind with an older version for which this binary has a tested converter
- **THEN** ordinary latest-format readers still reject it as unsupported-version if it reaches them unconverted
- **AND** only the sealed format-upgrade workflow may deserialize it as an upgrade input

### Requirement: Unsupported JSON is preserved before replacement
The system SHALL preserve unsupported, malformed, wrong-kind, or unsupported-version JSON metadata before writing a latest-format replacement for the same logical state. If preservation cannot be completed, the system MUST leave the original metadata untouched and MUST report that replacement is unsafe.

#### Scenario: Unsupported old JSON is quarantined before reset
- **WHEN** a metadata file contains unsupported pre-public JSON and the app needs to continue with default latest-format state
- **THEN** the original metadata is moved or copied into app-owned quarantine storage first
- **AND** only then may the app write a latest-format replacement

#### Scenario: Preservation failure blocks replacement
- **WHEN** unsupported JSON cannot be quarantined or otherwise preserved
- **THEN** the app does not overwrite the original metadata with a default latest-format file
- **AND** the recovery diagnostic marks replacement as unsafe

### Requirement: App-owned format upgrades stay sealed from ordinary readers
The project SHALL provide app-owned format upgrade support through a dedicated runtime service when a supported older version can be converted to the latest version. The service MUST be separate from ordinary latest-format metadata readers and MUST write only latest-format envelopes.

#### Scenario: Upgrade workflow writes latest envelopes
- **WHEN** the format-upgrade workflow converts a supported older JSON metadata file
- **THEN** the resulting file is written with the latest supported envelope version and stable document kind
- **AND** future ordinary loads use the latest-format reader without consulting old-version code

#### Scenario: Converter coverage is explicit per version step
- **WHEN** the project introduces a new metadata version after v1
- **THEN** it adds tested converter coverage for every supported older version step needed to reach the latest format
- **AND** no Convert action is exposed for a version step without a deterministic converter

#### Scenario: Current v1 baseline is a no-op
- **WHEN** the current public v1 metadata format is the latest supported format
- **THEN** the format-upgrade inventory reports v1 metadata as current
- **AND** no conversion write is required for existing v1 files

### Requirement: JSON format fixtures define the public contract
The project SHALL maintain deterministic fixture coverage for the public JSON format contract. Fixtures MUST cover valid latest-format documents, missing optional fields, unknown fields, malformed inputs, unsupported old-shape inputs, wrong-kind inputs, unsupported versions, oversized metadata where applicable, and supported older-version conversion fixtures when such converters exist.

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
