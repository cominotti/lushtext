## MODIFIED Requirements

### Requirement: Every workflow is enumerated before any workflow is migrated
The project SHALL maintain `docs/workflow-readability-matrix.md` as the completion
source of truth for workflow readability migration. Every LushText workflow MUST
have a row with a stable row id, its current file set, the roles it must gain to
satisfy the convention, its seam value object, its consumer count for any pure
policy it owns, its risk tier, and its migration status. The matrix MUST be
complete for all workflows before the first workflow is migrated, so that outliers
are classified before the convention becomes normative.

A row MUST NOT restate the target file set as a literal file list. The target file
set is fully derivable from the role naming convention, so duplicating it per row
creates a second source of truth that can drift from the convention.

A row's **measured** cells — its current size, its per-kind test seam counts, and
the consumer count for any pure policy it owns — are census estimates until the
row's migration re-derives them. A migrating workflow SHALL re-derive those cells
from the code and correct them in the same change, rather than inheriting the
census figures or reporting against them. Re-derivation MUST be **row-scoped**: it
counts only what the workflow owns, and it MUST NOT pool shared service files,
cross-cutting modules, or neighbouring files the workflow merely calls. Size
figures MUST count production lines, excluding `#[cfg(test)]` modules, and MUST
name any shared population that the census cell had pooled so a later slot reading
from the other side does not re-derive it as its own. A correction MAY move a
figure in either direction, and a change MUST NOT treat an unchanged cell as the
expected outcome.

#### Scenario: Census precedes exemplar migration
- **WHEN** the exemplar workflow migration begins
- **THEN** the matrix already contains a row for every workflow, including
  unmigrated ones
- **AND** each row names the roles it must gain, its seam value object, and its
  risk tier

#### Scenario: Workflow with no seam value object is still a complete row
- **WHEN** a workflow has no field bundle crossing two or more boundaries, because
  it is synchronous or delegates its seam to another workflow
- **THEN** the row records that no value object is required together with the
  evidence for that conclusion
- **AND** the row counts as complete rather than unresolved

#### Scenario: Known outlier is classified explicitly
- **WHEN** the census reaches a workflow that may not fit the convention, such as
  the minimap adapter, a policy module with several unrelated consumers, or a
  workflow decomposed by an earlier change
- **THEN** the row records it as conforming, exempt, or deferred with a stated
  reason
- **AND** a later migration change MUST NOT silently force an exempt workflow into
  the convention

#### Scenario: Migrated workflow without a row fails policy
- **WHEN** a workflow is migrated to the convention but has no matrix row, or its
  row claims evidence that does not exist
- **THEN** `make check-policy` fails
- **AND** the failure names the workflow and the missing row or evidence

#### Scenario: Migration re-derives its row's measured cells
- **WHEN** a workflow is migrated
- **THEN** the change re-derives the row's size, per-kind seam counts, and pure
  policy consumer count from the code and corrects the cells in the same change
- **AND** it does not report its migration against the census figures it replaced

#### Scenario: Re-derivation is row-scoped and excludes co-located tests
- **WHEN** a migration measures its row's size or seam population
- **THEN** it counts only the files the workflow owns, excluding shared services,
  cross-cutting modules, and neighbouring files the workflow only calls
- **AND** size figures count production lines rather than `#[cfg(test)]` modules

#### Scenario: A pooled census figure names the population it shared
- **WHEN** a corrected cell replaces a figure that had pooled seams or lines
  shared with another workflow
- **THEN** the correction names the shared population and the rows that share it
- **AND** a later migration of one of those rows can re-derive its own share
  without rediscovering the pooling

### Requirement: Pure workflow policy is co-located with its consumer and stays pure
The project SHALL place a workflow's pure decision logic in a `policy.rs` module
inside that workflow's own directory when the policy has a single owning workflow.
A `policy.rs` module MUST NOT import `gtk4`, `glib`, `gio`, `libadwaita`, or
`sourceview5`. Policy with several genuinely unrelated consumers MUST remain in a
shared location, and the matrix MUST record it as cross-cutting.

Relocation eligibility SHALL be decided by the number of **owning workflows**, not
by the number of consuming files. Pure policy whose only consumer is its own
coordination adapter is cross-cutting when that adapter serves several workflows,
and MUST NOT be relocated beneath any one of them. Shared coordination that encodes
LushText-specific budget, admission, or retirement policy MUST NOT be treated as
generic toolkit machinery for extraction into a separate reusable crate.

A workflow whose pure decision logic is **entirely** cross-cutting therefore owns
no `policy.rs`, and such a workflow SHALL still count as a complete migrated row.
Its matrix entry declares no pure policy role and names the cross-cutting module
and the other owning workflows that keep it shared. This mirrors the treatment of
a workflow with no qualifying seam bundle: the absence is a recorded conclusion
with its evidence, not an unmet obligation. A migration MUST NOT manufacture a
`policy.rs` for such a workflow by copying, forking, or re-implementing part of
the cross-cutting module beneath it, and MUST NOT duplicate a shared limit or
shared arithmetic to obtain a local policy module.

#### Scenario: Single consuming file is not sufficient grounds to relocate
- **WHEN** pure policy has exactly one consuming file, and that file is the
  coordination adapter that many workflows call
- **THEN** the policy is recorded as cross-cutting and stays in its shared location
- **AND** the matrix records the breadth of the adapter's consumers as the reason

#### Scenario: Single-consumer policy moves beside its workflow
- **WHEN** pure policy in `model/` has exactly one owning workflow
- **THEN** it moves to that workflow's `policy.rs`
- **AND** it remains free of GTK, GLib, GIO, Libadwaita, and SourceView imports

#### Scenario: Cross-cutting policy does not move
- **WHEN** pure policy is consumed by several unrelated workflows
- **THEN** it stays in its shared location
- **AND** the matrix records it as cross-cutting with its consumer list

#### Scenario: Workflow with only cross-cutting policy is still a complete row
- **WHEN** a migrated workflow's pure decision logic is entirely cross-cutting, so
  it owns no `policy.rs`
- **THEN** the row is complete with no pure policy role declared, naming the
  cross-cutting module and the other owning workflows that keep it shared
- **AND** the migration does not fork or re-implement part of that module beneath
  the workflow to manufacture a local policy file

#### Scenario: Shared arithmetic is called rather than duplicated
- **WHEN** two workflows need the same pure limit, threshold, or arithmetic that a
  cross-cutting module owns
- **THEN** both call the cross-cutting owner
- **AND** neither copies it into its own `policy.rs`, because a forked shared limit
  can drift while both copies still read as correct

#### Scenario: Policy purity is mechanically enforced
- **WHEN** a `policy.rs` module gains a GTK-family import
- **THEN** `make check-policy` fails
- **AND** the failure names the file and the disallowed import

#### Scenario: Domain layer keeps only domain concepts
- **WHEN** the census and exemplar are complete
- **THEN** modules remaining in `model/` name domain concepts or recorded
  cross-cutting policy
- **AND** no module is placed in `model/` solely to obtain test or mutation
  tooling reach
