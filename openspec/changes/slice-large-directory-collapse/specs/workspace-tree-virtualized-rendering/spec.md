## ADDED Requirements

### Requirement: A sliced collapse goes through one entry point and keeps the tree consistent
Every LushText route that collapses a directory row SHALL go through one section entry point. A sliced collapse SHALL end in `main`'s final state. While it runs, no load, scan or refresh SHALL add rows to its subtree, and readiness SHALL report the tree as changing. Selection and focus inside the subtree SHALL move to the directory row first.

#### Scenario: Every route reaches the entry point
- **WHEN** a directory is collapsed by a click on its expander arrow, by activating the row, by the folder header's collapse or toggle, by a failed reveal, or by the reorder drag's defensive collapse
- **OR** by one of the expander's collapse key bindings that the ledger's expander axiom shows reachable from LushText's rows on `main`, among `minus`, `KP_Subtract`, `slash`, `KP_Divide`, `Shift+Left`, `Shift+KP_Left`, `Ctrl+Shift+Left`, `Ctrl+Shift+KP_Left`, `Ctrl+Space` and `Ctrl+KP_Space` (the `Right` and `KP_Right` four in right-to-left)
- **THEN** the collapse runs through the section's collapse entry point, and no other LushText code calls `set_expanded(false)` on a directory row
- **AND** every LushText caller of `set_expanded(true)` goes through the expand entry point
- **AND** the key routes are exactly those GTK's expander honours for the focus LushText's rows have on `main`, so no key gains or loses a behaviour
- **AND** an AT-SPI action on the expander and GTK's drag-hover expansion keep GTK's own behaviour, and are recorded as residual routes

#### Scenario: Expanding keeps GTK's behaviour
- **WHEN** a collapsed directory is expanded by a click on its arrow, or by an expand key binding the ledger's expander axiom shows reachable, among `plus`, `KP_Add`, `asterisk`, `KP_Multiply`, `Shift+Right`, `Shift+KP_Right`, `Ctrl+Shift+Right`, `Ctrl+Shift+KP_Right`, `Ctrl+Space` and `Ctrl+KP_Space` (the `Left` and `KP_Left` four in right-to-left)
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
- **WHEN** a child of a directory being collapsed, on the sliced or the direct path, is selected or focused
- **THEN** selection and focus move to the directory row before any row is removed, and file peek follows only that one change
- **AND** when the subtree's rows cannot be located (a path shown twice by overlapping folders), selection and focus stay where they are

#### Scenario: Expanding again mid-collapse
- **WHEN** the directory is expanded again before its sliced collapse ends, by a click, an expand key, a reveal, the new-item flow, a restore or a folder toggle
- **THEN** the collapse stops, the arrow and accessible state show expanded at once, and the store is refilled through the existing refresh path until it lists every entry again
- **AND** a caller that needs the children waits on that refill as it waits on a first expansion

#### Scenario: An ancestor collapses
- **WHEN** an ancestor of an expanded directory of 16,385 entries is collapsed
- **THEN** the descendant is drained and collapsed first, then the ancestor, with every step within the per-frame bounds

#### Scenario: Loads and refreshes stop
- **WHEN** a directory is collapsed while its load or a refresh under it is in progress
- **THEN** no further batch is applied to any store in its subtree, and a re-expansion starts a fresh load

#### Scenario: Readiness and evidence
- **WHEN** one or more sliced collapses are in progress
- **THEN** `workspace-refresh-complete` is not satisfied, the automation snapshot reports `collapse_pending`, and the section evidence reports each queued collapse's steps taken and rows left without making the toolkit do work

#### Scenario: The folder header collapses every root
- **WHEN** the folder header collapses two expanded roots, the first of 4,096 entries and the second small enough to collapse directly
- **THEN** both roots stay collapsed after every deferred restore, and `expanded_paths` holds neither
- **AND** a LushText expansion or collapse records its intent whether or not GTK still holds the row object a widget once showed

#### Scenario: Keys typed into inline rename
- **WHEN** the user renames an expanded directory inline and types `-`, `+`, `*`, `/` or another character that is also an expander key binding
- **THEN** the entry receives the character as on `main`, and the directory is not collapsed by the collapse entry point

#### Scenario: Rows inside a collapsing subtree
- **WHEN** a directory is collapsing and an expanded descendant of it is still in the realized band
- **THEN** the descendant also shows collapsed and reports `expanded` false, and a click, key or activation on any row strictly inside the subtree does not expand or collapse anything
- **AND** a GTK expansion inside the subtree (a drag hover, an assistive technology's expand action) is collapsed again directly and records no expansion intent

#### Scenario: A taken-over descendant after its ancestor re-expands
- **WHEN** the user collapses a large directory, then its ancestor, then expands the ancestor again before either collapse ends
- **THEN** the descendant ends collapsed, as on `main`, and its arrow and accessible state agree with that

#### Scenario: New File inside a collapsing subtree
- **WHEN** New File or New Folder targets a directory inside a collapsing subtree
- **THEN** the target directory is expanded first through the expand entry point, the new item appears in its refilled listing, and inline rename starts on it

#### Scenario: Removal while collapsing
- **WHEN** a refresh splice, a section rebuild or a model reset removes a collapsing directory row
- **THEN** the collapse is cancelled, GTK removes what is left directly (a measured residual), and the drained stores are retired as before
