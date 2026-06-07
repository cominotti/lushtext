## ADDED Requirements

### Requirement: Test status probes use lightweight filesystem helpers
Tests, property tests, widget tests, integration tests, and benchmarks SHALL use lightweight filesystem status helpers when they only need to assert existence, absence, or path kind. Rich filesystem fact helpers such as `file_facts()` MUST remain reserved for assertions that inspect canonical identity, byte size, modified time, or multiple facts together.

#### Scenario: Existence assertion avoids rich facts
- **WHEN** a test only needs to assert that a path exists or is absent
- **THEN** it uses `services::filesystem::metadata::exists` or `path_status`
- **AND** it does not call `file_facts()` solely to check whether metadata can be read

#### Scenario: Rich fact assertion remains explicit
- **WHEN** a test needs canonical path, byte size, modified time, or kind facts as part of the assertion
- **THEN** it may call `file_facts()`
- **AND** the returned facts are inspected rather than used only as an existence proxy

### Requirement: Sidecar filesystem helper cleanup is intentional
Bookmark, document-note, workspace-note, and local-history workflows SHALL either share a small helper for repeated sidecar filesystem mechanics or keep workflow-specific helpers when that is clearer. Any shared helper MUST have active callers and MUST only own filesystem mechanics such as listing candidate JSON sidecars, removing stale sidecar paths, or applying common directory-scan policy; workflow identity, filtering, merge, retention, and empty-document rules MUST remain in the owning service.

#### Scenario: Shared sidecar helper has active callers
- **WHEN** implementation extracts a shared sidecar filesystem helper
- **THEN** bookmark, document-note, workspace-note, or local-history code uses it for repeated filesystem mechanics
- **AND** the no-leftovers audit or final search evidence confirms the helper is not an unused public surface

#### Scenario: Workflow-specific sidecar helpers remain clear
- **WHEN** implementation determines a shared sidecar helper would obscure domain-specific rules
- **THEN** existing workflow-specific helper code remains in the owning services
- **AND** no new unused sidecar helper module, export, or function remains after the cleanup

### Requirement: No-leftovers audit covers polish-level filesystem drift
The deterministic filesystem-boundary audit SHALL catch polish-level leftovers after the completed rustix migration, including status-only `file_facts()` probes in tests, newly introduced local status wrappers, and unused sidecar helper surfaces created during cleanup.

#### Scenario: Test status-probe drift is caught
- **WHEN** a test or benchmark calls `file_facts(...).is_ok()` or `file_facts(...).is_err()` only as an existence probe
- **THEN** the no-leftovers audit reports the file and line
- **AND** implementation is not considered complete until the assertion uses a lightweight status helper or inspects rich facts

#### Scenario: New sidecar helper surface cannot linger unused
- **WHEN** cleanup introduces a new sidecar helper module, export, or function
- **THEN** the no-leftovers audit or final completion evidence confirms it has call sites
- **AND** implementation is not considered complete while an unused helper surface remains
