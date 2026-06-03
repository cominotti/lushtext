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
