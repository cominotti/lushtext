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

**A decision recorded for one row does not propagate into another row's cells.**
Where a change resolves a census gap by *assigning* files to a row it does not
otherwise touch, the receiving row's measured cells become stale at that moment.
The assigning change SHALL either re-derive the receiving row's affected cells or
state in the row that they are now stale and why, so the receiving row's own
migration does not read them as measurements.

**Every row SHALL carry a terminal status once the programme's final migration
slot lands.** The terminal statuses are `migrated`, `exempt`, and `cross-cutting`.
`pending`, `deferred`, and `partially-conforming` are transitional and MUST NOT
survive the closing change, because a transitional status in a completed programme
tells a later reader that planned work was abandoned. A row resolved to a
non-migrating terminal status by the closing change SHALL record the **probe
evidence** for that conclusion — derived from the row's own adapter, in the same
form the pure-policy requirement demands of a workflow that concludes it owns no
`policy.rs` — rather than asserting the conclusion. Where the matrix and the
programme record's slot ledger disagree about a row's slot or status, the closing
change SHALL make them agree rather than leaving a reader to pick.

**A census row that groups several surfaces sharing one adapter is a provisional
grouping, not a workflow.** Such a row's migration SHALL first derive its ordered
stage orders, resumption points, shared coordination state, and external entry
surface from the code, and SHALL then decide whether the grouping is one workflow.
Where it is not, the row SHALL be replaced by rows that each name one workflow,
plus entries in the matrix's no-coordination-tier list for surfaces that have no
ordered stages, and the matrix's census **coverage proof** SHALL be re-derived in
the same change so that no file loses attribution. Sharing one GTK subclass's
`imp` struct is NOT evidence that surfaces belong to one workflow.

That split is licensed by the grouping being provisional and by the stage-order
evidence, and by nothing else. A split whose justification is the row's line count,
or the difficulty of fitting one facade inside the declared budget, is the
forbidden budget response wearing the grouping clause as cover: the facade budget
requirement already states that splitting a census row to make two smaller facades
is not an available answer to an over-budget facade.

**The programme record SHALL carry a completion section when its final slot
lands.** That section SHALL state the programme's measured outcomes against its own
recorded baseline, and SHALL list every remaining deferral in one place with its
gating condition and its owner — including deferrals recorded only inside archived
change directories. A deferral whose only home is an archived change directory is
not inventoried, because an archived directory is not where a later session looks
for outstanding work. The completion section MUST NOT record an unmet acceptance
gate as accepted on the change's own authority; where a gate requires proof the
change could not obtain, the section states the gap and whose decision it awaits.

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

#### Scenario: Assigning files to another row stales that row's cells
- **WHEN** a change resolves a census gap by assigning files to a row whose
  workflow it is not migrating
- **THEN** it re-derives the receiving row's affected measured cells, or records in
  that row that they are stale and why
- **AND** the receiving row's own migration does not read the unadjusted cells as
  measurements

#### Scenario: No row carries a transitional status after the final slot
- **WHEN** the programme's final migration slot lands
- **THEN** every matrix row carries `migrated`, `exempt`, or `cross-cutting`
- **AND** no row carries `pending`, `deferred`, or `partially-conforming`

#### Scenario: Non-migrating terminal resolution records its probe
- **WHEN** the closing change resolves a row to `exempt` or `cross-cutting` rather
  than migrating it
- **THEN** the row records the probe of its own adapter that supports the
  conclusion, including any separable pure decisions the probe found
- **AND** the conclusion is not asserted from the row's earlier census text

#### Scenario: Matrix and slot ledger are reconciled by the closing change
- **WHEN** the matrix and the programme record's slot ledger disagree about a row's
  slot or status
- **THEN** the closing change makes them agree
- **AND** it does not leave two sources of truth for the same row

#### Scenario: Provisional grouping row is tested against its stage orders
- **WHEN** a census row groups several surfaces that share one adapter and that row
  is migrated
- **THEN** the migration derives the stage orders, resumption points, shared
  coordination state, and external entry surface first, and decides on that
  evidence whether the grouping is one workflow
- **AND** sharing one `imp` struct is not treated as evidence of one workflow

#### Scenario: Grouping that is not one workflow is replaced by named rows
- **WHEN** the stage-order evidence shows a provisional grouping holds more than
  one story
- **THEN** the row is replaced by rows that each name one workflow, plus
  no-coordination-tier entries for surfaces with no ordered stages
- **AND** the matrix's census coverage proof is re-derived so no file loses
  attribution

#### Scenario: A split justified by line count is rejected
- **WHEN** a change proposes splitting a census row because one facade cannot fit
  the declared line budget
- **THEN** the split is rejected as the forbidden budget response
- **AND** the change delegates harder or amends the budget through the retroactive
  amendment rule instead

#### Scenario: Programme completion is recorded with its deferrals inventoried
- **WHEN** the programme's final slot lands
- **THEN** the programme record gains a completion section stating measured
  outcomes against its recorded baseline and listing every remaining deferral in
  one place with its gating condition and owner
- **AND** deferrals previously recorded only inside archived change directories
  appear in that inventory

#### Scenario: An unmet acceptance gate is not self-accepted
- **WHEN** the closing change cannot obtain proof that an acceptance gate requires
- **THEN** the completion section records the gap and whose decision it awaits
- **AND** the change does not record the gate as accepted on its own authority
