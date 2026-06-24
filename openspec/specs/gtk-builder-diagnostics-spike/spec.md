# gtk-builder-diagnostics-spike Specification

## Purpose

Define how LushText evaluates runtime GtkBuilder diagnostics for
Blueprint-generated templates without replacing existing Blueprint generation,
drift, lint, or widget validation gates.

## Requirements

### Requirement: Builder diagnostics spike runs in an initialized GTK application context
The system SHALL evaluate `GTK_DEBUG=builder` diagnostics through a runtime context that initializes GTK, Libadwaita, GResource templates, and LushText composite widget types.

#### Scenario: Runtime diagnostics are executed
- **WHEN** the builder diagnostics spike runs
- **THEN** it MUST launch a LushText runtime, widget, smoke, or equivalent harness with `GTK_DEBUG=builder,builder-objects`
- **AND** it MUST capture stdout, stderr, command line, GTK runtime version where available, and the instantiated templates or surfaces

#### Scenario: Debug environment variables are scoped
- **WHEN** `GTK_DEBUG` is used by the spike
- **THEN** it MUST be treated as a diagnostic-only environment variable and MUST NOT become an end-user preference, persisted setting, Flatpak permission change, or required runtime environment

### Requirement: Builder diagnostics complement Blueprint validation
The system SHALL keep existing Blueprint and template-validation gates authoritative while evaluating builder debug output as additional evidence.

#### Scenario: Existing template checks are available
- **WHEN** the spike evaluates generated templates
- **THEN** it MUST run or reference `make check-blueprint`, `make lint-blueprint`, and the template-contract check as the existing source-fidelity baseline

#### Scenario: Standalone GTK validation has a limitation
- **WHEN** `gtk4-builder-tool validate` cannot load a generated template because it requires Libadwaita types or app-registered composite widgets
- **THEN** the spike MUST record that limitation and use the initialized runtime diagnostic lane instead of treating the standalone failure as a template defect

#### Scenario: GTK-only templates validate standalone
- **WHEN** a generated template can be validated by `gtk4-builder-tool validate` without Libadwaita or app-specific type registration
- **THEN** the spike MUST record whether standalone validation adds useful evidence beyond existing Blueprint checks

### Requirement: Builder diagnostics output is classified before enforcement
The system SHALL classify builder diagnostic output before any finding is treated as actionable or promoted to a future gate.

#### Scenario: Diagnostic output contains findings
- **WHEN** `GTK_DEBUG=builder,builder-objects` emits deprecated feature, unused object, builder trace, template, or object-construction output
- **THEN** the spike MUST classify each finding as actionable defect, known standalone limitation, benign diagnostic noise, unsupported-host blocker, or candidate for future advisory or blocking enforcement

#### Scenario: Actionable defect is found
- **WHEN** the spike classifies a builder diagnostic as an actionable defect
- **THEN** it MUST record the affected template or surface, the exact diagnostic text, the likely owning source file, and whether the fix belongs in the current spike, a follow-up proposal, or existing Blueprint validation policy

#### Scenario: No actionable defects are found
- **WHEN** runtime builder diagnostics complete without actionable defects
- **THEN** the spike MUST record the covered templates and surfaces so the absence of findings is tied to explicit coverage rather than assumed global safety

### Requirement: Builder diagnostics coverage is explicit
The system SHALL document which templates and state extremes were instantiated during the builder diagnostics spike.

#### Scenario: Shell templates are instantiated
- **WHEN** the diagnostics lane instantiates the main shell or core composite widgets
- **THEN** it MUST record coverage for no-context startup, a representative open document, and any lazily loaded surfaces that were intentionally opened

#### Scenario: Lazy dialogs or popovers are not instantiated
- **WHEN** a template-backed dialog, popover, or secondary surface is not opened during the diagnostics run
- **THEN** the spike MUST list it as uncovered or separately covered by another command

#### Scenario: Builder diagnostics are proposed for automation
- **WHEN** the spike recommends keeping the diagnostic lane
- **THEN** it MUST recommend whether the lane belongs as a manual recipe, an advisory target, a widget-test mode, a smoke-test mode, or a future blocking check

### Requirement: Builder diagnostics spike hands off to automated diagnostics
The system SHALL treat the completed builder diagnostics spike as evidence for an automated diagnostics lane rather than as the final operating model.

#### Scenario: Future work consults the spike
- **WHEN** future planning or implementation work consults `gtk-builder-diagnostics-spike`
- **THEN** it MUST use the `automated-builder-diagnostics` capability for reusable runtime, CI, coverage, classification, and enforcement requirements
- **AND** it MUST treat the manual `GTK_DEBUG=builder,builder-objects` recipe as historical or focused-debugging evidence, not the complete diagnostics workflow

#### Scenario: Spike evidence remains useful
- **WHEN** a builder diagnostics implementation needs prior evidence
- **THEN** it MAY reference the spike's command lines, limitations, and findings
- **AND** it MUST still satisfy the automated diagnostics capability before claiming local or CI coverage
