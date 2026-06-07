## ADDED Requirements

### Requirement: Recovery metadata classifies unsupported JSON format state
The system SHALL classify unsupported JSON format state separately from ordinary malformed JSON. Unsupported format state includes bare pre-public JSON, wrong-kind envelopes, recognized-kind envelopes with unsupported versions, and envelopes whose payload cannot be read under the supported schema.

#### Scenario: Bare JSON produces unsupported-format diagnostics
- **WHEN** recovery metadata loading finds syntactically valid JSON that is not a supported envelope for that metadata class
- **THEN** the diagnostic identifies the metadata kind and unsupported-format category
- **AND** the caller does not silently treat the file as ordinary missing state

#### Scenario: Unsupported version produces version diagnostics
- **WHEN** recovery metadata loading finds a supported document kind with an unsupported version
- **THEN** the diagnostic identifies the metadata kind and unsupported version category
- **AND** the original metadata is preserved before replacement is allowed

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
