## ADDED Requirements

### Requirement: A sliced collapse goes through one entry point and keeps the tree consistent
Every LushText route that collapses a directory row SHALL go through one section entry point. A sliced collapse SHALL end in `main`'s final state. While it runs, no load, scan or refresh SHALL add rows to its subtree, and readiness SHALL report the tree as changing. Selection and focus inside the subtree SHALL move to the directory row first.

#### Scenario: Every route reaches the entry point
- **WHEN** a directory is collapsed by an expander click, by `minus`, `KP_Subtract`, `slash`, `KP_Divide`, `Shift+Left`, `Ctrl+Shift+Left` or `Ctrl+Space` on its row (`Shift+Right` and `Ctrl+Shift+Right` in right-to-left), by activating the row, by the folder header's collapse or toggle, by a failed reveal, or by the reorder drag's defensive collapse
- **THEN** the collapse runs through the section entry point, and no other LushText code calls `set_expanded(false)` on a directory row

#### Scenario: Expanding keeps GTK's behaviour
- **WHEN** a collapsed directory is expanded by a click or by `plus`, `KP_Add`, `asterisk`, `Shift+Right` or `Ctrl+Space`
- **THEN** it expands and scrolls to the row exactly as `GtkTreeExpander` does on `main`

#### Scenario: Final state matches a direct collapse
- **WHEN** a sliced collapse of a directory of 4,096 entries holding an expanded subdirectory completes
- **THEN** the flattened rows, `expanded_paths`, watch targets, the accessible state and the retired stores equal those of GTK's direct collapse from the same state

#### Scenario: The directory row does not move
- **WHEN** the directory row is on screen during a sliced collapse
- **THEN** its rendered top stays fixed at every step, the viewport slice bin records no correction, and `outer_request_pending()` is false once collapsed

#### Scenario: Collapsing from inside the children
- **WHEN** the directory row is above the viewport and the folder header's collapse collapses it
- **THEN** the collapse ends with the same outer scroller value and rendered row placement as GTK's direct collapse from the same state

#### Scenario: Selection inside the subtree
- **WHEN** a child of the collapsing directory is selected or focused
- **THEN** selection and focus move to the directory row before any row is removed, and file peek follows only that one change

#### Scenario: Expanding again mid-collapse
- **WHEN** the user expands the directory again before its sliced collapse ends
- **THEN** the collapse stops, the arrow and accessible state show expanded at once, and the store is refilled through the existing refresh path until it lists every entry again

#### Scenario: An ancestor collapses
- **WHEN** an ancestor of an expanded directory of 16,385 entries is collapsed
- **THEN** the descendant is drained and collapsed first, then the ancestor, with every step within the per-frame bounds

#### Scenario: Loads and refreshes stop
- **WHEN** a directory is collapsed while its load or a refresh under it is in progress
- **THEN** no further batch is applied to any store in its subtree, and a re-expansion starts a fresh load

#### Scenario: Readiness and evidence
- **WHEN** a sliced collapse is in progress
- **THEN** `workspace-refresh-complete` is not satisfied, the automation snapshot reports `collapse_pending`, and the section evidence reports the steps taken and the rows left without making the toolkit do work

#### Scenario: Removal while collapsing
- **WHEN** a refresh splice, a section rebuild or a model reset removes a collapsing directory row
- **THEN** the collapse is cancelled, GTK removes what is left directly (a measured residual), and the drained stores are retired as before
