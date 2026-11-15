## ADDED Requirements

### Requirement: Collapsing a sidebar directory fits the interaction budgets
Collapsing a workspace sidebar directory SHALL keep its input dispatch and every collapse step within `B_INTERACT_US`, and every dispatch until the row is collapsed and its listing retired within `B_SETTLE_US`, at every directory size, with or without rows after it. The first frame after the input SHALL show the collapse. No step SHALL bring more than `COLLAPSE_REALIZED_ROWS_PER_FRAME` rows into the realized band.

#### Scenario: A directory with rows after it
- **WHEN** the user collapses an expanded directory of 256 entries followed by 400 rows
- **THEN** the input dispatch and every step fit `B_INTERACT_US`, and every frame until it is collapsed fits `B_SETTLE_US`

#### Scenario: A huge directory
- **WHEN** the user collapses an expanded directory of 131,072 entries, with nothing after it and with 400 rows after it
- **THEN** the children no row widget is bound to are removed in chunks of at most `COLLAPSE_UNBOUND_CHUNK_ROWS` within the list turn slice, the bound children in units, and every dispatch fits its budget

#### Scenario: A collapse nobody draws
- **WHEN** a sliced collapse starts or continues while the sidebar is hidden, the section body is collapsed or the window is unmapped
- **THEN** its steps continue from a low-priority source within the same bounds, the collapse completes, and readiness becomes complete

#### Scenario: A small directory
- **WHEN** the directory's subtree holds at most `DIRECT_COLLAPSE_ROWS` flattened rows
- **THEN** it collapses in the input dispatch, as on `main`, within `B_INTERACT_US`

#### Scenario: The collapse shows in the first frame
- **WHEN** a sliced collapse starts
- **THEN** the first frame after the input shows the collapsed arrow and the row reports `expanded` false to assistive technologies, within `INPUT_TO_FRAME_US` of the input

#### Scenario: Time to collapsed is recorded
- **WHEN** the verdict test collapses each measured case
- **THEN** it prints the time from the input until the row is collapsed and the frames that took, beside `main`'s collapse
- **AND** the figure is recorded in the design, not enforced

#### Scenario: The probe scan's publication is a named residual
- **WHEN** a dispatch after the collapse publishes the scan that GTK's `is_expandable()` probe started for the collapsed directory
- **THEN** the verdict test prints it separately, it is excluded from this requirement's pass bar, and it is recorded as owned by `stop-expander-bind-time-child-scans`

### Requirement: Small-directory expansion is measured against the settle budget
The sidebar expansion verdict SHALL also cover directories of 64, 256 and 1,024 entries, with and without 400 rows after them. If any dispatch that applies a directory's rows exceeds `B_SETTLE_US`, a load's batches SHALL bring at most `COLLAPSE_REALIZED_ROWS_PER_FRAME` rows into the realized band per frame.

#### Scenario: Measuring first
- **WHEN** this change starts implementation
- **THEN** quiet, interleaved `main` figures for these expansions are recorded before any fix, with the decision to cap or not

#### Scenario: Expanding a 256-entry directory
- **WHEN** the user expands a directory of 256 entries after the decision above
- **THEN** every dispatch from the expansion to its terminal fits `B_SETTLE_US`
