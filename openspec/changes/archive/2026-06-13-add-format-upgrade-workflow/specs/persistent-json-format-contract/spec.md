## MODIFIED Requirements

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

## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Optional old-data migration stays outside runtime app code
**Reason**: LushText needs a user-facing, app-owned one-click upgrade workflow before normal startup restore consumes old but supported app-owned metadata. Keeping conversion only in optional scripts would not protect users who launch the app with upgradeable session, draft, workspace, sidecar, history, or undo state.

**Migration**: Replace script-only migration guidance with the sealed `services::format_upgrade` workflow. Any helper script may remain as developer tooling, but the supported user path is the startup gate and `Preferences > Data`, both backed by the same GTK-free upgrade service.
