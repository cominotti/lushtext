## ADDED Requirements

### Requirement: Collapsing a sidebar directory fits the interaction budgets
Collapsing a sidebar directory by a route LushText owns SHALL keep the input dispatch and every step within `B_INTERACT_US`, and every dispatch until the listing is retired within `B_SETTLE_US`, at every size. The first frame after the input SHALL show the collapse. No frame SHALL bring more than `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows into the realized band. A toolkit cost that cannot fit SHALL be a recorded toolkit-floor exception.

#### Scenario: How the budgets are measured
- **WHEN** the verdict test measures a collapse
- **THEN** it runs on the optimized widget build with the sidebar presented, at host load below 4, interleaved with `main`
- **AND** a frame-paced step's dispatch, which also styles the rows that step brought into the band, is read against `B_SETTLE_US` as a whole, and the step itself against `B_INTERACT_US`
- **AND** at most one unit of bound rows is removed per frame across all the window's sections

#### Scenario: A toolkit floor
- **WHEN** a step cannot fit `B_INTERACT_US` at the smallest measured unit because of the toolkit's per-widget cost
- **THEN** it is recorded as a measured toolkit-floor exception citing a GTK axiom with a probe and a sample, and is not left unexplained

#### Scenario: Routes left to GTK
- **WHEN** a refresh splice or model reset removes an expanded directory, or an assistive technology invokes the expander's collapse action
- **THEN** GTK collapses directly, and the dispatch is measured and recorded as a residual

#### Scenario: A directory with rows after it
- **WHEN** the user collapses an expanded directory of 256 entries followed by 400 rows
- **THEN** the input dispatch and every step fit `B_INTERACT_US`, and every frame until it is collapsed fits `B_SETTLE_US`

#### Scenario: A huge directory
- **WHEN** the user collapses an expanded directory of 131,072 entries, with nothing after it and with 400 rows after it
- **THEN** the children no row widget is bound to are removed in chunks of at most `COLLAPSE_UNBOUND_CHUNK_ROWS` within the list turn slice, the bound children in units, and every dispatch fits its budget

#### Scenario: A collapse nobody draws
- **WHEN** a sliced collapse starts or continues while the sidebar is hidden, the section body is collapsed or the window is unmapped, or while the list stays mapped but its frame clock delivers no frame
- **THEN** its steps continue from a low-priority source within the same bounds, the collapse completes, and readiness becomes complete

#### Scenario: Several directories at once
- **WHEN** the folder header collapses two expanded roots of 4,096 entries each in one input
- **THEN** both rows show the collapse in the first frame, every frame takes at most one unit of bound rows across both collapses, and both complete

#### Scenario: A small directory
- **WHEN** the directory's subtree holds at most `DIRECT_COLLAPSE_ROWS` flattened rows
- **THEN** it collapses in the input dispatch, as on `main`, within `B_INTERACT_US`
- **AND** when one input collapses several such directories (the folder header), the input dispatch collapses them directly only until `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows are removed in total, and the rest are paced like a sliced collapse

#### Scenario: The collapse shows in the first frame
- **WHEN** a sliced collapse starts
- **THEN** the first frame after the input shows the collapsed arrow, and both the row and its expander report `expanded` false to assistive technologies, within `INPUT_TO_FRAME_US` of the input
- **AND** they keep reporting it until the collapse ends, including after the row is rebound

#### Scenario: Time to collapsed is recorded
- **WHEN** the verdict test collapses each measured case
- **THEN** it prints the time from the input until the row is collapsed and the frames that took, beside `main`'s collapse
- **AND** the figure is recorded in the design, not enforced

#### Scenario: The probe scan's publication is a named residual
- **WHEN** a dispatch after the collapse publishes the scan that GTK's `is_expandable()` probe started for the collapsed directory
- **THEN** the verdict test prints it separately, it is excluded from this requirement's pass bar, and it is recorded as owned by `stop-expander-bind-time-child-scans`

### Requirement: Small-directory expansion is measured against the settle budget
The sidebar expansion verdict SHALL also cover directories of 64, 256 and 1,024 entries, with and without 400 rows after them, on a first expansion and right after a collapse. A dispatch that applies a directory's rows and exceeds `B_SETTLE_US` SHALL be attributed with a profile before it is fixed, and fixed as the attribution shows.

#### Scenario: Measuring first
- **WHEN** this change starts implementation
- **THEN** quiet, interleaved `main` figures for these expansions are recorded before any fix, with the decision to cap or not

#### Scenario: Row-widget creation is the cost
- **WHEN** the attribution puts an over-budget expansion dispatch in the creation and styling of row widgets
- **THEN** a load's batches bring at most `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows into the realized band per frame

#### Scenario: Another cost
- **WHEN** the attribution puts it elsewhere
- **THEN** it is fixed as the attribution shows, or recorded as a toolkit-floor exception under the existing settle-budget requirement

#### Scenario: Expanding a 256-entry directory
- **WHEN** the user expands a directory of 256 entries after the decision above
- **THEN** every dispatch from the expansion to its terminal fits `B_SETTLE_US`, except a recorded toolkit-floor exception and the probe scan's publication that the collapse requirement names as a residual
