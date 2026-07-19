## ADDED Requirements

### Requirement: Recent-document metadata limits are enforced during ingestion
Recent-document loading SHALL use the filesystem boundary's bounded byte reader with MAX_RECENT_DOCUMENTS_BYTES. Metadata MAY reject an already oversized file early but MUST NOT be the allocation boundary; exact-limit input MUST reach ordinary JSON parsing, while content that exceeds the cap before or during the read MUST be rejected without allocating or parsing the enlarged body.

#### Scenario: Metadata file is exactly at the cap
- **WHEN** the recent-document file contains exactly MAX_RECENT_DOCUMENTS_BYTES
- **THEN** bounded ingestion accepts it for normal JSON validation
- **AND** size alone does not reset valid exact-limit content

#### Scenario: File grows after metadata inspection
- **WHEN** the file appears within the cap during metadata inspection but grows beyond it before or during ingestion
- **THEN** the read stops at the bounded limit without allocating the full enlarged body
- **AND** the oversized body is not passed to the JSON parser

#### Scenario: Oversized metadata is recovered
- **WHEN** recent-document persistence exceeds the cap
- **THEN** the service applies its existing reset or prune recovery policy and emits a bounded diagnostic
- **AND** the popover remains usable with an empty or recovered model

#### Scenario: Missing or malformed metadata is loaded
- **WHEN** the file is missing or bounded input is invalid JSON
- **THEN** the established missing-file and corruption-recovery behavior remains unchanged
- **AND** no raw filesystem read bypass is introduced
