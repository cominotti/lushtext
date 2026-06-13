# recovery-metadata-integrity Specification

## Purpose
Define the integrity contract for app-owned recovery metadata so malformed, partial, and repairable state is diagnosed, preserved, and safely replaced without data loss.

## Requirements
### Requirement: App-owned recovery metadata is loaded with explicit integrity outcomes
The system SHALL distinguish valid, missing, malformed, unreadable, non-file, oversized, unsupported-format, unsupported-version, and partially repairable app-owned recovery metadata. Recovery metadata includes workspace state, session state, draft manifests, note and bookmark sidecars, local-history indexes, saved-search state, Replace All undo journals, and migration ledgers. A caller that loads recovery metadata MUST receive structured diagnostics instead of silently treating every failure as empty default state.

#### Scenario: Missing metadata uses a default without a warning
- **WHEN** a recovery metadata file does not exist
- **THEN** the loader returns the documented default state for that metadata kind
- **AND** no corruption or quarantine warning is reported

#### Scenario: Malformed metadata returns diagnostics
- **WHEN** a recovery metadata file exists but cannot be parsed
- **THEN** the loader returns a diagnostic that identifies the metadata kind and failure category
- **AND** the caller does not silently treat the failure as ordinary missing state

#### Scenario: Non-file metadata path is rejected
- **WHEN** a recovery metadata path is a directory or other unsupported file kind
- **THEN** the loader reports an integrity diagnostic
- **AND** the unsupported path is not overwritten by default metadata during the same load

### Requirement: Recovery metadata classifies unsupported JSON format state
The system SHALL classify unsupported JSON format state separately from ordinary malformed JSON. Unsupported format state includes bare pre-public JSON, wrong-kind envelopes, recognized-kind envelopes with no supported upgrade path, recognized-kind envelopes with future versions, and envelopes whose payload cannot be read under either the latest schema or an explicitly supported upgrade schema. Supported older envelopes discovered by the format-upgrade preflight SHALL be classified as upgradeable before ordinary recovery/default handling is allowed to consume them.

#### Scenario: Bare JSON produces unsupported-format diagnostics
- **WHEN** recovery metadata loading finds syntactically valid JSON that is not a supported envelope for that metadata class
- **THEN** the diagnostic identifies the metadata kind and unsupported-format category
- **AND** the caller does not silently treat the file as ordinary missing state

#### Scenario: Unsupported future version produces version diagnostics
- **WHEN** recovery metadata loading finds a supported document kind with a version newer than this binary supports
- **THEN** the diagnostic identifies the metadata kind and unsupported version category
- **AND** the original metadata is preserved before replacement is allowed
- **AND** no downgrade conversion is offered

#### Scenario: Upgradeable older version is gated before defaulting
- **WHEN** preflight finds a recognized metadata kind with an older version that has a tested converter to the latest format
- **THEN** the metadata is reported as upgradeable rather than ordinary damaged recovery state
- **AND** normal startup recovery/defaulting for that metadata waits for the user's Convert, Start Fresh, or Quit decision

#### Scenario: Older version without converter remains unsupported
- **WHEN** preflight or recovery loading finds an older recognized version without a tested converter path to the latest format
- **THEN** the metadata is classified as unsupported format or unsupported version
- **AND** the original metadata is preserved or left untouched according to the recovery preservation rules

### Requirement: Format upgrade preflight preserves recovery safety boundaries
The system SHALL run format-upgrade preflight and recovery metadata handling as adjacent but distinct workflows. Preflight SHALL identify upgradeable app-owned metadata before normal consumers run, while recovery metadata handling SHALL continue to own malformed, unreadable, oversized, wrong-kind, unsafe-to-replace, and non-upgradeable unsupported data.

#### Scenario: Upgradeable metadata is not quarantined before user choice
- **WHEN** preflight finds supported older metadata that can be converted
- **THEN** the system does not quarantine or replace that metadata as ordinary unsupported recovery state before the user chooses an action
- **AND** Convert preserves the original bytes through the format-upgrade backup path before writing latest state

#### Scenario: Damaged metadata still follows recovery preservation
- **WHEN** preflight finds malformed, unreadable, non-file, oversized, wrong-kind, or otherwise damaged metadata
- **THEN** the existing recovery metadata preservation and diagnostics rules apply
- **AND** the format-upgrade workflow does not invent a conversion for damaged data

#### Scenario: Mixed upgrade and recovery issues are summarized without data loss
- **WHEN** startup finds both upgradeable metadata and damaged non-upgradeable metadata
- **THEN** the compatibility dialog or grouped recovery feedback distinguishes upgradeable items from damaged items
- **AND** unaffected or recoverable metadata is not discarded solely because another item requires conversion or recovery

### Requirement: Workspaces and saved searches participate in recovery metadata handling
The system SHALL treat workspace state and saved-search state as app-owned recovery metadata for the purposes of unsupported-format preservation, malformed-file preservation, grouped diagnostics, and replacement safety.

#### Scenario: Corrupt workspace metadata is not silently defaulted
- **WHEN** `workspaces.json` exists but cannot be loaded as supported workspace metadata
- **THEN** the original metadata is preserved or left untouched when preservation fails
- **AND** the window receives a grouped recovery diagnostic instead of treating the sidebar as ordinarily empty

#### Scenario: Corrupt saved searches are not silently discarded
- **WHEN** `saved-searches.json` exists but cannot be loaded as supported saved-search metadata
- **THEN** the original metadata is preserved or left untouched when preservation fails
- **AND** the search panel remains usable without claiming the saved searches were simply absent

### Requirement: Malformed recovery metadata is preserved before replacement
The system SHALL preserve malformed or unreadable recovery metadata before writing replacement metadata for the same logical state. Preservation MUST use an app-owned quarantine location or, when quarantine cannot be completed, MUST leave the original file untouched and report that replacement is unsafe.

#### Scenario: Malformed metadata is quarantined before default replacement
- **WHEN** a malformed recovery metadata file is present during startup
- **THEN** the system moves or copies the original metadata into app-owned quarantine storage
- **AND** only then may it write replacement default metadata for that logical state

#### Scenario: Quarantine failure prevents destructive replacement
- **WHEN** malformed recovery metadata cannot be quarantined or otherwise preserved
- **THEN** the system does not overwrite that metadata with a default file
- **AND** it reports that recovery metadata preservation failed

#### Scenario: Quarantine records include enough diagnostic context
- **WHEN** the system quarantines a recovery metadata file
- **THEN** the quarantine record preserves the original bytes when readable
- **AND** the record identifies the original path, metadata kind, timestamp, and failure category

### Requirement: Startup recovery remains non-destructive under partial failure
The system SHALL continue startup with the safest recoverable subset of state when recovery metadata is partially damaged. Startup MUST NOT delete surviving drafts, sidecars, local-history snapshots, Replace All undo entries, or session evidence solely because another related metadata file failed to load.

#### Scenario: Corrupt draft manifest does not delete draft files
- **WHEN** the draft manifest is malformed but draft files remain readable
- **THEN** startup preserves the draft files
- **AND** recovery diagnostics explain that manifest state could not be trusted

#### Scenario: Corrupt sidecar does not hide unrelated sidecars
- **WHEN** one bookmark or note sidecar is malformed
- **THEN** browse and restore workflows still load unrelated valid sidecars
- **AND** the malformed sidecar is reported separately

#### Scenario: Corrupt local-history index does not delete snapshot text
- **WHEN** a local-history lineage index cannot be parsed but snapshot text files remain present
- **THEN** the snapshot files are preserved
- **AND** the lineage is either repaired conservatively or reported as unavailable without deletion

### Requirement: Recovery repair is conservative and auditable
The system SHALL repair recovery metadata only when the surviving data is sufficient to reconstruct a deterministic state. Repaired metadata MUST be written through the durable filesystem boundary and MUST produce diagnostics that describe what was repaired and what, if anything, was skipped.

#### Scenario: Deterministic repair succeeds
- **WHEN** recovery metadata can be rebuilt from surviving draft files, sidecar identities, or local-history snapshots without guessing user intent
- **THEN** the system writes the repaired metadata durably
- **AND** it reports a repair diagnostic for logs and smoke artifacts

#### Scenario: Ambiguous repair is skipped
- **WHEN** surviving recovery data cannot determine one valid repaired state
- **THEN** the system preserves the evidence without inventing replacement metadata
- **AND** it reports that manual inspection may be required

#### Scenario: Repair output stays bounded
- **WHEN** repair scans a recovery directory containing many entries
- **THEN** the system applies documented bounds to scanning and diagnostics
- **AND** startup remains responsive enough for the user to reach the editor

### Requirement: Recovery diagnostics are visible without overwhelming the user
The system SHALL surface recovery metadata problems in grouped user-visible feedback while preserving detailed paths and error causes in logs or smoke artifacts. A single startup with many related diagnostics MUST produce a concise user-facing summary rather than one alert per file.

#### Scenario: Startup shows grouped recovery warning
- **WHEN** startup encounters one or more recovery metadata diagnostics
- **THEN** the window shows a grouped warning that some recovery data could not be loaded or was repaired
- **AND** the warning does not block opening unaffected documents

#### Scenario: Detailed diagnostics are preserved for investigation
- **WHEN** recovery metadata diagnostics are emitted
- **THEN** logs or smoke artifacts include the affected metadata kind, original path, quarantine path when available, and failure category
- **AND** the user-facing summary does not need to expose every full path inline

#### Scenario: Successful later load clears stale warning state
- **WHEN** a later startup or retry loads the affected recovery metadata without diagnostics
- **THEN** stale warning state from the earlier failure is not shown again

### Requirement: Recovery integrity behavior is covered by deterministic tests
The project SHALL test recovery metadata integrity behavior with deterministic service, integration, widget, and generated-input coverage appropriate to each workflow.

#### Scenario: Service tests cover loader outcomes
- **WHEN** recovery metadata loaders are tested with valid, missing, malformed, non-file, unreadable, and oversized fixtures
- **THEN** each fixture produces the expected load outcome, quarantine behavior, and diagnostics

#### Scenario: Generated malformed bytes do not panic loaders
- **WHEN** generated malformed JSON bytes are passed to recovery metadata loaders within bounded cases
- **THEN** the loaders return errors or diagnostics without panicking

#### Scenario: Widget tests cover grouped startup warning
- **WHEN** startup encounters recoverable metadata diagnostics in the widget harness
- **THEN** the visible warning is grouped and unaffected state remains available
