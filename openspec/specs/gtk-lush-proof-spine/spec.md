## Purpose

Define the GTK-free proof spine crate that carries reusable readiness,
snapshot, workflow, and artifact value objects while leaving app control flow
and transports with each consumer.

## Requirements

### Requirement: Proof spine crate is GTK-free and independently adoptable
`gtk-lush-proof-spine` SHALL be a GTK Lush family crate under
`crates/gtk-lush/proof-spine` with package name `gtk-lush-proof-spine`. The
crate MUST provide GTK-free value objects, traits, and serialization helpers
for readiness, snapshots, workflow events, and artifact envelopes. It SHALL NOT
register D-Bus objects, own GTK actions, depend on LushText crates, or depend
on another GTK Lush family crate.

#### Scenario: Spine builds without GTK runtime dependencies
- **WHEN** `cargo check -p gtk-lush-proof-spine --all-targets` runs
- **THEN** the crate builds without requiring GTK, Libadwaita, GtkSourceView,
  or a display server
- **AND** its public examples show an app-owned provider implementing the
  traits without importing LushText

#### Scenario: Spine stays out of app control flow
- **WHEN** a consumer implements the spine traits
- **THEN** the consumer owns how state is collected, how commands are exposed,
  and whether D-Bus, CLI, or another transport is used
- **AND** the crate provides protocol objects rather than a runtime loop,
  command dispatcher, widget tree wrapper, or state/message system

### Requirement: Spine defines versioned readiness predicates and blockers
The proof spine SHALL define stable readiness predicate identifiers, readiness
results, blocker summaries, timeout statuses, and version metadata suitable for
automation clients and proof tools. Unknown predicates MUST fail explicitly
instead of falling back to broad idle waits.

#### Scenario: Unknown predicate is explicit
- **WHEN** a proof client asks a provider for an unsupported readiness
  predicate
- **THEN** the provider result identifies the predicate as unknown
- **AND** callers can distinguish unknown-predicate from timeout,
  unsupported-host, and application-failure statuses

#### Scenario: Blocker summary is bounded
- **WHEN** readiness is blocked by background work, layout settling, visual
  geometry settling, or unavailable host support
- **THEN** the blocker summary names the workflow or surface class and bounded
  status detail
- **AND** it does not include document text, note bodies, draft bodies,
  local-history contents, or private persistence identifiers

### Requirement: Spine defines bounded snapshot envelopes
The proof spine SHALL provide serializable snapshot envelopes for app-owned
diagnostics. Snapshot envelopes MUST include schema/interface versioning,
capture time or sequence information, status, safe surface/workflow summaries,
and privacy classification. They MUST be bounded by design and must support
redaction or omission of app-private fields.

#### Scenario: Snapshot envelope carries safe visual state
- **WHEN** a provider records visual geometry state for proof tooling
- **THEN** the envelope can represent named surfaces, visibility, rectangles,
  allocation sizes, scroll anchors, scale factor, and readiness metadata
- **AND** it does not require arbitrary widget identifiers or user content

#### Scenario: Snapshot serialization is stable
- **WHEN** a snapshot envelope is serialized to JSON for artifacts or tests
- **THEN** required top-level fields are stable and documented
- **AND** additive optional fields do not break readers that understand the
  declared schema version

### Requirement: Spine defines workflow events
The proof spine SHALL define bounded workflow event value objects for
automation and smoke tooling. Events MUST include stable workflow identity,
phase, status, sequence or timestamp, bounded detail, and blocker information
when available.

#### Scenario: Async workflow is bracketed
- **WHEN** an app reports a long-running workflow such as load, save, search,
  replace, session restore, workspace refresh, or visual layout settling
- **THEN** the spine event model can represent start, progress, finish, skip,
  and failure states
- **AND** event detail stays bounded and privacy-preserving

#### Scenario: Event ordering is observable
- **WHEN** a proof client reads workflow events
- **THEN** each event includes enough sequence or timing metadata to preserve
  ordering in artifact summaries
- **AND** missing or truncated history is reported explicitly

### Requirement: Spine defines artifact result envelopes
The proof spine SHALL define reusable result envelopes for proof and automation
artifacts. Envelopes MUST include `ok`, stable status, command or scenario
identity, bounded detail, schema/tool versions, and safe data fields. Exit-code
classes for success, failure, usage error, automation unavailable,
unsupported-host, and skipped coverage MUST be documented by consumers that use
the envelope in a CLI.

#### Scenario: Error envelope is machine-readable
- **WHEN** a proof command fails with JSON output enabled
- **THEN** the result envelope includes `ok=false`, a stable status, command or
  scenario identity, bounded detail, and safe diagnostic data
- **AND** it does not print raw screenshots, unbounded logs, or user content

#### Scenario: Skip does not count as proof
- **WHEN** host tooling or runtime support is unavailable
- **THEN** the envelope can represent skipped or unsupported coverage
- **AND** policy consumers can distinguish that result from verified coverage

### Requirement: LushText Automation1 adapts to the proof spine without D-Bus drift
LushText SHALL use `gtk-lush-proof-spine` value objects and traits behind its
Automation1 implementation while preserving the documented Automation1 D-Bus
surface, action catalog behavior, readiness predicate names, snapshot field
meanings, workflow-event semantics, result statuses, and privacy guarantees.

#### Scenario: Introspection remains stable
- **WHEN** Automation1 introspection is captured before and after the spine
  migration
- **THEN** the object path, interface name, method names, property names,
  signal names, and existing type signatures are unchanged except for
  explicitly documented additive fields
- **AND** `make check-automation-docs` fails if the docs do not match

#### Scenario: Snapshot privacy is preserved
- **WHEN** LushText maps app state into spine snapshot envelopes
- **THEN** the resulting Automation1 snapshot remains bounded to documented
  diagnostic fields
- **AND** it does not expose document text, note bodies, draft bodies,
  local-history contents, complete search result text, or private persistence
  identifiers
