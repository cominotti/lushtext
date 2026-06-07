## ADDED Requirements

### Requirement: Confined recovery metadata persistence is smoke-tested when available
Portal and sandbox smoke coverage SHALL verify recovery metadata persistence inside confined Flatpak or Snap app-data locations when the runtime can build, install, launch, and grant the required access.

#### Scenario: Confined app writes and reloads draft recovery metadata
- **WHEN** a confined smoke lane modifies a file-backed or untitled document and waits for recovery metadata
- **THEN** the metadata is written under the confined app-data location
- **AND** relaunching the confined app restores the expected work

#### Scenario: Confined session metadata survives restart
- **WHEN** a confined smoke lane opens multiple tabs and persists session state
- **THEN** relaunching the confined app restores the expected tab set when session metadata was durably written
- **AND** the artifact report identifies the confined data directory used

#### Scenario: Confined access denial does not masquerade as recovery success
- **WHEN** confinement denies access to a path or app-data write needed for recovery
- **THEN** LushText reports a visible or logged access diagnostic
- **AND** the smoke lane fails the recovery assertion instead of accepting missing metadata as success

### Requirement: Confined recovery diagnostics are preserved
Portal and sandbox smoke artifacts SHALL preserve enough recovery and runtime context to diagnose whether a recovery failure belongs to LushText, the runtime, portal mediation, or host setup.

#### Scenario: Runtime permissions and denials are recorded
- **WHEN** a confined recovery smoke run completes
- **THEN** the artifacts record package type, runtime version, app-data path, granted permissions, portal implementation, and relevant denials

#### Scenario: Quarantine diagnostics survive confinement
- **WHEN** malformed recovery metadata is quarantined or preserved inside a confined runtime
- **THEN** the artifacts record the diagnostic kind, metadata class, and app-data-relative quarantine path
- **AND** they avoid dumping unbounded document contents

#### Scenario: Unsupported confined recovery skips clearly
- **WHEN** Flatpak, Snap, portal services, or platform runtime support is unavailable for confined recovery testing
- **THEN** the lane reports a clear skip reason
- **AND** native recovery smoke remains distinct from confined recovery coverage

### Requirement: Portal-mediated recovery failures preserve user data
The system SHALL treat portal-mediated file access failures during recovery-sensitive workflows as partial failures rather than success. Recovery state MUST remain eligible for retry when a portal grant, file handle, or confined write target is temporarily unavailable.

#### Scenario: Save As portal failure keeps draft recovery eligible
- **WHEN** Save As through a portal fails after an untitled document has draft recovery data
- **THEN** the editor remains modified and draft recovery remains eligible
- **AND** the document does not adopt the failed destination identity

#### Scenario: Portal reopen failure preserves session diagnostics
- **WHEN** startup restore attempts to reopen a session file that is no longer reachable through the portal or confined permission set
- **THEN** the session entry is reported as unavailable
- **AND** unrelated session tabs and drafts still restore

#### Scenario: Confined recovery tests cover denied app-data cleanup
- **WHEN** confined cleanup of stale recovery metadata is denied or fails
- **THEN** the smoke lane records the denial and the app reports diagnostic state
- **AND** cleanup is retried later without blocking startup indefinitely
