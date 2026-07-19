## ADDED Requirements

### Requirement: Incomplete manifest repair never becomes cleanup authority
The system SHALL distinguish a complete draft-body inventory from a bounded partial repair result. A repaired draft manifest MUST be persisted as authoritative only after directory traversal reaches a trusted terminal state and every discovered body is represented or conservatively classified. When completeness cannot be proven, every workflow that could replace the manifest MUST either complete a fresh reconciliation first or fail retryably without clearing dirty draft state, and orphan cleanup MUST remain disabled across later startups.

#### Scenario: Repair spans more than one directory page
- **WHEN** a missing or malformed manifest is repaired from more draft bodies than one bounded scan page can contain
- **THEN** repair continues through bounded pages until it proves a complete inventory
- **AND** any persisted repaired manifest represents every discovered body before cleanup becomes eligible

#### Scenario: Repair cannot prove completeness
- **WHEN** directory scanning, body classification, or manifest-capacity validation stops before a complete inventory is proven
- **THEN** the system preserves every draft body and reports a bounded partial-repair diagnostic
- **AND** it does not persist the partial subset as an authoritative clean manifest
- **AND** later autosave, session, deletion, and cleanup paths cannot forget the untrusted state by replacing the manifest with that subset

#### Scenario: Partial repair survives repeated startup
- **WHEN** startup encounters an incomplete repair state, exits, and starts again before a complete reconciliation succeeds
- **THEN** the later startup still treats orphan cleanup as untrusted
- **AND** draft bodies omitted from the earlier bounded page remain undeleted and eligible for recovery

#### Scenario: Complete reconciliation restores normal cleanup
- **WHEN** a later bounded repair pass reaches a trusted terminal inventory and durably writes the complete manifest
- **THEN** manifest writers may resume their normal serialized updates
- **AND** orphan cleanup may resume only from that complete latest manifest with its existing fingerprint and stable-target guards
