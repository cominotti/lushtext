## Purpose

Define the GTK Lush adoption-validation phase that proves the extracted
functional crates can be adopted by real consumers before publication or
repository graduation work begins.

## Requirements

### Requirement: Adoption lab proves every functional GTK Lush crate as a real consumer
The project SHALL add and maintain an in-tree GTK Lush adoption lab that is a
real GTK application or gallery/demo consumer outside the GTK Lush family
crates and outside LushText app code. The lab MUST exercise every functional
GTK Lush crate in at least one realistic workflow: `gtk-lush-signals`,
`gtk-lush-settle`, `gtk-lush-tasks`, `gtk-lush-viewport`,
`gtk-lush-widgets`, `gtk-lush-proof-harness`, and `gtk-lush-proof-spine`.
The lab MAY depend on multiple family crates because it is a consumer, but it
MUST NOT be treated as a family crate and MUST NOT introduce runtime
dependencies between family crates.

#### Scenario: Lab builds as a consumer
- **WHEN** the adoption-lab build target runs
- **THEN** the lab builds as a normal workspace consumer outside
  `crates/gtk-lush/`
- **AND** it can depend on multiple GTK Lush crates without creating
  dependencies between those crates

#### Scenario: Every crate has a lab workflow
- **WHEN** maintainers inspect the adoption lab and adoption matrix
- **THEN** each functional GTK Lush crate has a named lab workflow using its
  public API in a realistic stock gtk-rs or Libadwaita context
- **AND** no workflow imports LushText application crates to satisfy GTK Lush
  behavior

#### Scenario: Lab UI covers state extremes
- **WHEN** the lab exposes interactive GTK surfaces
- **THEN** tests or proof evidence cover no required context, representative
  populated state, many or awkward items, and constrained geometry
- **AND** commands remain reachable, text remains readable, fixed headers or
  controls remain visible, and no unintended root-level scrollbars appear

### Requirement: Adoption matrix is the reviewable source of truth
The project SHALL maintain a crate-by-crate GTK Lush adoption matrix for this
phase. The matrix MUST list every functional family crate, the adoption-lab
workflow that uses it, the single-crate example or fixture that proves isolated
adoption, the relevant tests or proof evidence, adoption friction status, API
review decision, and follow-up issue or task when friction remains.

#### Scenario: Matrix covers all functional crates
- **WHEN** the adoption matrix check runs
- **THEN** it fails if any functional GTK Lush crate is missing from the
  matrix
- **AND** it fails if a crate row lacks workflow, evidence, friction, or API
  decision fields

#### Scenario: Matrix and implementation stay synchronized
- **WHEN** a GTK Lush crate is added, removed, renamed, or has its lab
  workflow changed
- **THEN** the matrix is updated in the same change
- **AND** policy tooling or review rejects stale matrix entries

### Requirement: Stock gtk-rs afternoon adoption is timed and journaled
The phase SHALL run a timed adoption exercise where a fresh-session agent or
maintainer adopts exactly one GTK Lush crate into a stock gtk-rs starter-style
application without restructuring the starter. The exercise MUST record the
selected crate, start and end time or elapsed time, starter shape, commands
run, code added at a summary or patch level, friction points, documentation
gaps, and the API decision that followed.

#### Scenario: Timed journal exists
- **WHEN** the phase is ready to archive
- **THEN** a bounded timed-adoption journal exists for at least one GTK Lush
  crate
- **AND** the journal records elapsed effort, commands, friction, and the
  resulting API or documentation decisions

#### Scenario: Starter remains stock-shaped
- **WHEN** the stock starter fixture is checked
- **THEN** it adopts exactly one GTK Lush crate through a path dependency
- **AND** it does not import LushText crates, generated LushText resources,
  LushText GSettings schemas, or another GTK Lush family crate

#### Scenario: Friction creates actionable work
- **WHEN** the timed adoption journal records a friction point
- **THEN** the point is classified as documentation, example, naming,
  type-shape, feature flag, missing helper, overreach, or accepted limitation
- **AND** every non-accepted friction point is linked to a task, code change,
  doc change, or follow-up issue before archive

### Requirement: Unrelated existing project adoption spike is recorded
The phase SHALL attempt to adopt at least one GTK Lush crate into an unrelated
existing gtk-rs or Libadwaita project. The project source MUST NOT be vendored
into this repository. The retained evidence MUST be bounded and include the
candidate project, license compatibility review, selected crate, source
version or commit, elapsed effort when available, patch summary or external
branch reference, friction, and decision.

#### Scenario: External adoption notes are bounded
- **WHEN** the external adoption spike completes
- **THEN** this repository contains a bounded note or journal naming the
  candidate, selected crate, source version, commands, patch summary, and
  friction
- **AND** it does not commit the unrelated project's checkout or private user
  content

#### Scenario: License compatibility is checked
- **WHEN** a candidate unrelated project is selected
- **THEN** the adoption note records the candidate license and why the spike
  can be documented or patched without creating license ambiguity for LushText
  or GTK Lush

#### Scenario: External friction feeds API review
- **WHEN** the unrelated project spike identifies a GTK Lush API or docs
  problem
- **THEN** the friction is included in the same API review classification as
  the adoption lab and timed stock starter friction
- **AND** unresolved friction is recorded as accepted limitation or follow-up
  work before archive

### Requirement: Friction-driven API review happens before phase completion
Before this adoption phase archives, maintainers SHALL review all friction from
the adoption lab, stock starter exercise, and unrelated-project spike. The
review MUST explicitly decide whether to keep, rename, reshape, document,
feature-gate, remove, or defer each affected API. Breaking changes to `0.0.0`
GTK Lush APIs are permitted when they improve independent adoption, reduce
LushText-shaped assumptions, or better satisfy the anti-framework
constitution.

#### Scenario: API review records decisions
- **WHEN** adoption friction has been collected
- **THEN** a review note, matrix entry, or tasks section records a decision for
  each friction item
- **AND** implementation tasks for accepted API changes are complete before
  archive

#### Scenario: Breaking changes update all consumers
- **WHEN** the adoption review changes a GTK Lush public API
- **THEN** LushText call sites, adoption-lab workflows, examples, doctests,
  CHANGELOGs, READMEs, and public API snapshots are updated in the same change
- **AND** the full GTK Lush and LushText verification gates are rerun

#### Scenario: Overreach is rejected
- **WHEN** a proposed API change would add a view DSL, component model,
  application state/message loop, custom runtime, Libadwaita replacement, or
  runtime dependency between family crates
- **THEN** the API review rejects or redesigns the change before archive

### Requirement: Adoption evidence is tested by bounded local gates
The phase SHALL add or update deterministic local gates that verify adoption
evidence without requiring crates.io publication or network access. The gates
MUST cover family policy, GTK Lush doctests, standalone examples, adoption-lab
build/tests, stock starter fixture checks, adoption matrix completeness,
public API advisory snapshots, and any widget or visual proof required by UI
or rendered-geometry changes.

#### Scenario: Adoption gates run locally
- **WHEN** the phase verification ladder runs
- **THEN** it includes the existing GTK Lush family gates plus adoption-lab,
  stock-fixture, and matrix checks
- **AND** those checks do not require functional crates.io publication

#### Scenario: UI proof uses the right evidence
- **WHEN** the adoption lab or crate changes include widget behavior
- **THEN** headless widget tests cover lifecycle, focus, state, and geometry
  behavior
- **AND** visual-sensitive rendered effects use same-session visual proof
  rather than app-owned rectangles alone

### Requirement: Adoption artifacts remain bounded and privacy-preserving
Adoption evidence SHALL remain bounded and privacy-preserving. It MUST NOT
include raw private user content, full external source trees, unbounded logs,
raw image data embedded in markdown, document text, note bodies, draft bodies,
local-history contents, complete search result text, or private persistence
identifiers.

#### Scenario: Journals use bounded evidence
- **WHEN** adoption journals are committed
- **THEN** they contain concise commands, summaries, links or patch excerpts,
  decisions, and safe metadata
- **AND** large logs, screenshots, external source trees, and private user data
  are omitted or kept as ignored/generated artifacts

#### Scenario: Artifact paths are reviewable
- **WHEN** adoption checks write generated artifacts
- **THEN** outputs live under documented artifact directories or ignored build
  paths
- **AND** committed fixtures are small enough for ordinary code review

### Requirement: Adoption phase exits without publication
The adoption-validation phase SHALL complete only when adoption evidence,
API review, docs, tests, policy checks, and specialist reviews are recorded.
It MUST NOT publish functional crates, prepare a `0.1.0` release, split the
repository, move LushText to published GTK Lush dependencies, or perform the
Phase 6 upstreaming round.

#### Scenario: Archive gate records non-publication status
- **WHEN** the adoption-validation phase is ready to archive
- **THEN** governance and roadmap docs state that adoption validation is
  complete but functional publication and repository graduation remain future
  work
- **AND** no release automation has published GTK Lush functional crates

#### Scenario: Later publishing can cite adoption evidence
- **WHEN** a later publication or graduation change is proposed
- **THEN** it can cite this phase's adoption lab, timed journal,
  unrelated-project spike, API review, and verification evidence as the
  pre-publication adoption gate
- **AND** it must still perform any publication-specific release, repository,
  docs.rs, changelog, credential, and versioning work separately
