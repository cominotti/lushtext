## ADDED Requirements

### Requirement: Full Flatpak filesystem permission remains intentional
This change SHALL preserve the current Flatpak full filesystem permission posture. Portal and sandbox work in this change MUST be limited to diagnostics, chooser/screenshot support, smoke evidence, and documentation; it MUST NOT migrate LushText to portals-only access or narrow the Flatpak filesystem permission.

#### Scenario: Flatpak manifest keeps broad filesystem access
- **WHEN** the Flatpak manifest is inspected after this change
- **THEN** it still includes the current broad filesystem permission required by LushText's workspace, monitoring, search, replace, notes, history, and session behavior
- **AND** no task in this change treats removing that permission as implementation-complete work

#### Scenario: Permission rationale is documented
- **WHEN** maintainers read portal, sandbox, Flatpak, or automation documentation
- **THEN** it explains that LushText intentionally keeps full filesystem permission for this change
- **AND** it identifies portal diagnostics as observability work rather than a permission migration

#### Scenario: Permission drift fails validation
- **WHEN** a packaging change narrows Flatpak filesystem access during this change
- **THEN** validation fails unless a separate explicit proposal and proof set covers the permission migration

### Requirement: Portal diagnostics SHALL be harmless and artifact-rich
Portal-related smoke additions SHALL record runtime state and diagnostic evidence without changing LushText's access model or claiming portal parity.

#### Scenario: Portal services are reported
- **WHEN** portal/sandbox smoke runs
- **THEN** artifacts record available portal bus names, portal implementation details when discoverable, Flatpak/Snap runtime identity, granted permissions, and relevant environment variables
- **AND** absence of portal support produces a clear skip or diagnostic note rather than a false pass

#### Scenario: Portal screenshot or chooser paths stay optional
- **WHEN** screenshot, file chooser, or document portal support is available
- **THEN** smoke helpers may use it for diagnostic capture or chooser proof
- **AND** failure or denial is reported as portal/runtime diagnostic evidence, not as a reason to narrow permissions

#### Scenario: Host-side portal limitations remain visible
- **WHEN** portal behavior is known or observed to degrade file identity, external-change detection, locking, or project-context access
- **THEN** smoke artifacts and documentation preserve that limitation
- **AND** the change does not claim portal-first parity

### Requirement: Portal and sandbox documentation SHALL stay synchronized
The project SHALL document which portal/sandbox details automation exposes, which are diagnostic-only, and which permissions remain intentionally broad. Documentation drift MUST fail when helper output, artifact names, permission assumptions, or runtime checks change.

#### Scenario: Documentation describes diagnostic-only portal exposure
- **WHEN** users or maintainers read automation or portal/sandbox documentation
- **THEN** it explains which portal state is collected, why it is collected, which artifacts contain it, and why full filesystem permission remains unchanged

#### Scenario: Portal artifact documentation matches helper output
- **WHEN** portal/sandbox helper artifact names, summary fields, skip reasons, or permission checks change
- **THEN** documentation and tests are updated in the same change
- **AND** stale documentation fails the relevant validation check
