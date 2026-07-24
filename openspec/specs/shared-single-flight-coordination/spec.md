# shared-single-flight-coordination Specification

## Purpose
TBD - created by archiving change close-consistency-and-decomposition-gaps. Update Purpose after archive.
## Requirements
### Requirement: One-active/one-latest coordination has one shared primitive
Workflows that keep at most one active background request and at most one
latest superseding request SHALL use one shared coordinator primitive with a
workflow-neutral name and home, parameterized by the workflow's request
type. Command-palette search, notes browsing, bookmark excerpt previews,
local-history preview selection, and workspace content search MUST consume
the shared primitive (directly or through type aliases) rather than parallel
per-workflow reimplementations of submit/finish/supersede generation
semantics.

#### Scenario: Workspace search flight uses the shared coordinator
- **WHEN** workspace content search submits, supersedes, or finishes a
  search request
- **THEN** the one-active/one-latest and generation semantics come from the
  shared coordinator primitive
- **AND** the existing supersession observability (active-generation
  evidence) is preserved through the shared surface or a thin
  workflow-owned wrapper

#### Scenario: Consolidation preserves each workflow's semantics
- **WHEN** the existing coordinator consumers (palette, notes browser,
  bookmark excerpts) and the migrated consumers run their existing
  supersession, cancellation-observability, and stale-completion tests
- **THEN** all tests pass without weakened assertions
- **AND** no workflow gains or loses queued requests, generations, or
  cancellation signals as a result of the consolidation

### Requirement: Cancellation tokens are aliased, not copied
Workflows that need the shared cooperative cancellation token SHALL alias or
re-export the shared token type rather than maintaining structural copies.
The shared token and coordinator types SHALL carry workflow-neutral names so
non-palette consumers do not depend on palette-named primitives.

#### Scenario: Local-history preview uses the shared token
- **WHEN** local-history preview selection needs cooperative cancellation
- **THEN** it uses the shared token via alias or import
- **AND** no structurally duplicated token type remains in the service

#### Scenario: Shared primitives are workflow-neutrally named
- **WHEN** a non-palette workflow imports the shared coordinator or token
- **THEN** the imported type's name and module do not claim a
  palette-specific identity
- **AND** existing consumers compile against the renamed home without
  behavior change

### Requirement: Guarded worker-outcome adaptation shares one shape
The UI-side adapters that convert a worker outcome into a guarded payload
(compute retained weight, shrink the reservation, take ownership, classify
into a workflow outcome) SHALL either share one helper for the common
weight-then-own sequence or document, at each hand-rolled site, why the
workflow's freshness or weight semantics cannot use the shared helper.

#### Scenario: Shared adapter centralizes the weight-then-own sequence
- **WHEN** a UI workflow adapts a guarded worker outcome for file-index
  builds, note-source loads, or local-history preview loads
- **THEN** the reservation-shrink and ownership-take sequence flows through
  the shared helper or a documented site-local exception
- **AND** each workflow keeps its own outcome vocabulary and freshness
  checks explicit at the call site

