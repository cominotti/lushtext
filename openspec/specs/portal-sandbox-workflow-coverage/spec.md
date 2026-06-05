# portal-sandbox-workflow-coverage Specification

## Purpose
Define LushText's portal, file chooser, and confined-runtime smoke coverage so
sandboxed workflows remain diagnosable and safe for user data.

## Requirements
### Requirement: File chooser workflows are covered through native or portal paths
The project SHALL provide smoke coverage for user-facing file chooser workflows
that cannot be proven by direct path injection alone.

#### Scenario: Open File chooser accepts a selected document
- **WHEN** the file chooser smoke lane selects a supported text document through
  the available native or portal-backed chooser path
- **THEN** LushText opens that document in a tab
- **AND** the active editor content matches the selected file bytes

#### Scenario: Save As chooser adopts only successful destinations
- **WHEN** the file chooser smoke lane saves an untitled or renamed document to a
  selected destination
- **THEN** the destination file is written successfully
- **AND** the editor adopts the destination identity only after that write
  succeeds

#### Scenario: Chooser cancellation preserves state
- **WHEN** the user cancels an Open File, Save As, or Add Workspace Folder
  chooser
- **THEN** no document identity, workspace list, modified flag, or draft state is
  changed because of the canceled chooser

### Requirement: Confined runtime workflows are smoke-tested
The project SHALL provide confined-runtime smoke coverage for Flatpak and Snap
artifacts when the relevant packaging lane can build or install them.

#### Scenario: Confined app launches and loads resources
- **WHEN** a confined Flatpak or Snap smoke test launches LushText
- **THEN** the app starts, loads its GResource bundle and GSettings schema, and
  presents an interactive window without runtime denials

#### Scenario: Accessible path opens inside confinement
- **WHEN** the confined smoke test opens a file from a path available to the
  confined app
- **THEN** the file opens in the editor
- **AND** save or save-as writes through the supported confined access path
  without silent data loss

#### Scenario: Inaccessible path fails gracefully
- **WHEN** the confined smoke test attempts to open or add a path outside the
  app's available permissions
- **THEN** LushText reports an access error or requests a supported grant
- **AND** it does not crash, hang, create a bogus workspace entry, or mark a
  document saved when no durable write occurred

### Requirement: Portal and sandbox smoke artifacts are diagnostic
Portal and sandbox smoke lanes SHALL preserve enough information to distinguish
application defects from host runtime limitations.

#### Scenario: Runtime identity is recorded
- **WHEN** a portal or sandbox smoke lane runs
- **THEN** it records the package type, runtime version, portal implementation,
  desktop session, granted permissions, and relevant environment variables

#### Scenario: Denials and portal errors are preserved
- **WHEN** a confined run emits AppArmor, seccomp, portal, GIO, or filesystem
  access errors
- **THEN** the smoke lane captures those errors as artifacts
- **AND** unexpected denials or access errors fail the lane

#### Scenario: Missing runtime support skips clearly
- **WHEN** Flatpak, Snap, portal services, or required platform runtimes are not
  available on the host
- **THEN** the smoke lane reports a clear skip reason and does not mark the
  unsupported workflow as verified

### Requirement: URI And Document-Portal Activation Diagnostics Are Covered
Portal and sandbox smoke coverage SHALL distinguish unsupported URI-shaped
activation inputs from silent application no-ops. When a portal or confined
runtime provides a `gio::File` without a local path, the workflow MUST capture
the user-visible error and the runtime context needed to diagnose whether the
issue is app behavior, portal behavior, or confinement.

#### Scenario: Non-path portal activation records diagnostic feedback
- **WHEN** a portal or sandbox smoke lane can deliver a URI-shaped document
  activation that does not expose a local path
- **THEN** the lane records LushText's visible unsupported-input feedback
- **AND** it preserves the URI form, portal implementation, runtime identity,
  and relevant access-denial logs as artifacts

#### Scenario: Portal activation continues to validate accessible local files
- **WHEN** the same smoke environment can also provide an accessible local file
  path
- **THEN** the lane verifies that LushText opens that local file successfully
- **AND** unsupported URI diagnostics do not replace the accessible-file success
  check

#### Scenario: Unsupported URI workflow skips clearly when host support is absent
- **WHEN** the host cannot provide a portal, confined runtime, or URI activation
  mechanism for the smoke lane
- **THEN** the lane reports a clear skip reason
- **AND** it does not mark unsupported URI handling as verified

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
