## MODIFIED Requirements

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

## ADDED Requirements

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
