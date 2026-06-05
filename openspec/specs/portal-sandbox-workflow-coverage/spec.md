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
