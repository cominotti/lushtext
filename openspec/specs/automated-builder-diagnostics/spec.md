# automated-builder-diagnostics Specification

## Purpose

Define LushText's reusable automated builder diagnostics lane, including its debug-enabled GTK runtime, smoke target, coverage accounting, output classification, CI staging, and documentation contract.

## Requirements

### Requirement: Builder diagnostics run through a reusable debug GTK runtime
The system SHALL provide a builder diagnostics runtime provider that can run locally and in CI with GTK builder debug channels enabled, without building GTK during each diagnostics invocation.

#### Scenario: CI diagnostics use a prebuilt runtime
- **WHEN** the builder diagnostics lane runs in CI
- **THEN** it MUST use a pinned prebuilt debug-enabled GTK runtime provider such as an OCI image
- **AND** it MUST NOT compile GTK from source as part of the diagnostics job itself
- **AND** it MUST record the runtime provider identity, tag or digest where available, GTK version, Libadwaita version, GtkSourceView version, and operating-system/container metadata in the diagnostics artifacts

#### Scenario: Local diagnostics reuse the same runtime
- **WHEN** a developer runs the builder diagnostics lane locally
- **THEN** the system MUST support running through the same reusable runtime provider when `podman`, `docker`, or an equivalent configured container runner is available
- **AND** it MUST document the command or environment variables needed to select the container provider

#### Scenario: Host runtime is used directly
- **WHEN** the diagnostics lane is configured to run on the host runtime
- **THEN** it MUST first prove that the host GTK honors `GTK_DEBUG=builder,builder-objects`
- **AND** it MUST record a clear unsupported-runtime result when GTK reports that debug channels are ignored or unavailable

#### Scenario: CI runtime lacks debug support
- **WHEN** the CI diagnostics lane uses the configured debug runtime provider
- **AND** the capability probe shows that GTK builder debug channels are unavailable
- **THEN** the lane MUST fail as a runtime setup failure rather than reporting successful builder coverage

### Requirement: Builder diagnostics are exposed as a stable smoke target
The system SHALL expose automated builder diagnostics through a stable local command and preserve diagnostics artifacts under the smoke artifact tree.

#### Scenario: Developer runs diagnostics target
- **WHEN** a developer runs the builder diagnostics smoke target
- **THEN** the command MUST initialize an isolated GTK runtime suitable for LushText UI construction
- **AND** it MUST run with `GTK_DEBUG=builder,builder-objects` scoped only to the diagnostics process
- **AND** it MUST write artifacts under `build/smoke/builder-diagnostics` by default

#### Scenario: Diagnostics artifacts are written
- **WHEN** the diagnostics target completes, fails, or skips for an unsupported runtime
- **THEN** it MUST preserve raw stdout and stderr logs, the executed command lines, environment/runtime metadata, standalone validation results, runtime probe results, classifier output, and a human-readable summary

#### Scenario: Debug environment remains scoped
- **WHEN** builder diagnostics are added
- **THEN** `GTK_DEBUG` MUST NOT become an end-user preference, persisted setting, Flatpak permission change, default `make run` environment, or required production runtime environment

### Requirement: Builder diagnostics complement existing Blueprint validation
The system SHALL keep existing Blueprint and template-validation gates authoritative while using builder diagnostics as additional runtime evidence.

#### Scenario: Existing Blueprint gates remain required
- **WHEN** builder diagnostics are implemented
- **THEN** `make check-blueprint`, Blueprint drift checking, generated UI template-contract checking, and classified Blueprint lint policy MUST remain available and authoritative for source-fidelity validation

#### Scenario: Standalone builder validation is useful
- **WHEN** `gtk4-builder-tool` can validate a generated UI file without Libadwaita or app composite type registration
- **THEN** the diagnostics lane MUST record that standalone result as supporting evidence for that template

#### Scenario: Standalone builder validation is insufficient
- **WHEN** `gtk4-builder-tool` cannot load a generated UI file because it needs Libadwaita types or app-registered composite widget classes
- **THEN** the diagnostics lane MUST classify that result as a known standalone-tool limitation
- **AND** it MUST rely on an initialized runtime probe for actionable evidence about that template

### Requirement: Builder diagnostics coverage is explicit and template-accountable
The system SHALL account for every committed generated GtkBuilder template in the diagnostics coverage report.

#### Scenario: Template coverage is summarized
- **WHEN** the diagnostics lane completes
- **THEN** the coverage artifact MUST list each generated template under `resources/ui/`
- **AND** each template MUST be classified as runtime-instantiated, standalone-validated, intentionally skipped, unsupported by the current runtime, or uncovered

#### Scenario: Runtime probe instantiates a surface
- **WHEN** a runtime probe constructs a LushText surface
- **THEN** the coverage artifact MUST record the probe name, command, template or surface covered, and whether the probe represented no-context startup, representative content, dense or awkward content, constrained geometry, or a narrower template-only construction

#### Scenario: Lazy surface is not opened
- **WHEN** a template-backed dialog, popover, or secondary surface is not opened by the diagnostics lane
- **THEN** the coverage artifact MUST mark that template or surface as uncovered or intentionally deferred
- **AND** it MUST NOT imply that the absence of builder diagnostics proves that surface is clean

### Requirement: Builder diagnostic output is classified before enforcement
The system SHALL classify runtime and standalone builder diagnostics before treating any line as actionable or promoting the lane to a stronger gate.

#### Scenario: Diagnostic output is parsed
- **WHEN** runtime or standalone diagnostics emit output
- **THEN** the classifier MUST categorize findings as actionable, known standalone-tool limitation, benign diagnostic noise, unsupported runtime, or future-gate candidate
- **AND** unclassified diagnostics MUST appear in the summary for manual review

#### Scenario: Actionable diagnostics are found
- **WHEN** the classifier identifies an actionable builder diagnostic
- **THEN** the summary MUST record the diagnostic text, owning template or surface where known, likely source file, probe command, and recommended fix path
- **AND** the scheduled/manual smoke lane MAY fail after preserving artifacts

#### Scenario: No actionable diagnostics are found
- **WHEN** no actionable diagnostics are found
- **THEN** the summary MUST state that the result applies only to the templates and surfaces listed in the coverage artifact

#### Scenario: Unsupported runtime is found locally
- **WHEN** a local run cannot access a debug-enabled GTK runtime
- **THEN** the command MUST report the unsupported runtime clearly and explain how to run with the reusable runtime provider
- **AND** the skip MUST NOT count as builder diagnostics coverage

### Requirement: Builder diagnostics run in scheduled or manual CI first
The system SHALL integrate builder diagnostics into CI as an artifact-preserving scheduled or manually dispatched smoke lane before any pull-request blocking promotion.

#### Scenario: Scheduled smoke workflow runs builder diagnostics
- **WHEN** the end-user smoke workflow runs its matrix
- **THEN** it MUST include a builder diagnostics lane that uses the reusable debug GTK runtime provider
- **AND** it MUST upload the builder diagnostics artifact directory regardless of success, failure, or runtime setup failure

#### Scenario: Smoke workflow drift is checked
- **WHEN** the scheduled/manual smoke workflow matrix changes for builder diagnostics
- **THEN** the workflow drift check MUST be updated so the Make target, artifact path, and documentation remain synchronized

#### Scenario: Pull-request CI promotion is considered
- **WHEN** maintainers consider making builder diagnostics part of default pull-request CI
- **THEN** a later change MUST prove that the runtime provider, classifier, execution time, and false-positive rate are stable enough for a bounded deterministic gate

### Requirement: Builder diagnostics documentation is maintained
The system SHALL document how builder diagnostics fit with Blueprint validation, end-user smoke coverage, local setup, CI setup, and GTK/Libadwaita agent guidance.

#### Scenario: Contributor documentation is updated
- **WHEN** the automated builder diagnostics lane is added
- **THEN** `docs/blueprint-validation.md` MUST describe when to use it, what artifacts it produces, and how it differs from Blueprint checks
- **AND** `docs/end-user-coverage.md` MUST list it as a scheduled/manual smoke lane

#### Scenario: Runtime guidance is updated
- **WHEN** the debug runtime provider is added
- **THEN** documentation MUST explain how the provider is built or refreshed outside normal diagnostics runs
- **AND** it MUST explain how local developers can select host, container, or automatic runtime-provider modes

#### Scenario: Agent guidance is updated
- **WHEN** builder diagnostics automation is implemented
- **THEN** GTK/Libadwaita agent guidance MUST point agents to the automated lane before relying on ad hoc `GTK_DEBUG=builder` commands
