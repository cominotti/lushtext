## ADDED Requirements

### Requirement: URI And Document-Portal Activation Diagnostics Are Covered
Portal and sandbox smoke coverage SHALL distinguish unsupported URI-shaped activation inputs from silent application no-ops. When a portal or confined runtime provides a `gio::File` without a local path, the workflow MUST capture the user-visible error and the runtime context needed to diagnose whether the issue is app behavior, portal behavior, or confinement.

#### Scenario: Non-path portal activation records diagnostic feedback
- **WHEN** a portal or sandbox smoke lane can deliver a URI-shaped document activation that does not expose a local path
- **THEN** the lane records LushText's visible unsupported-input feedback
- **AND** it preserves the URI form, portal implementation, runtime identity, and relevant access-denial logs as artifacts

#### Scenario: Portal activation continues to validate accessible local files
- **WHEN** the same smoke environment can also provide an accessible local file path
- **THEN** the lane verifies that LushText opens that local file successfully
- **AND** unsupported URI diagnostics do not replace the accessible-file success check

#### Scenario: Unsupported URI workflow skips clearly when host support is absent
- **WHEN** the host cannot provide a portal, confined runtime, or URI activation mechanism for the smoke lane
- **THEN** the lane reports a clear skip reason
- **AND** it does not mark unsupported URI handling as verified
