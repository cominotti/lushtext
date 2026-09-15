# workspace-tree-virtualized-rendering Specification

## Purpose
Guarantee that every row in a workspace section's tree model is rendered and reachable regardless of row count, with row widgets bounded by the visible viewport rather than by the list length, so `GtkListView`'s realized-widget cap can never leave a blank band in the sidebar.

## Requirements

### Requirement: Every tree-model row is rendered regardless of row count
The workspace sidebar SHALL render every row present in a workspace section's `GtkTreeListModel`. The set of rows a user can reach by scrolling the workspace-section area MUST equal the set of rows in the model, for any row count up to and including the per-directory truncation placeholder. Rendering completeness MUST NOT depend on `GtkListView`'s realized-widget cap (`GTK_LIST_VIEW_MAX_LIST_ITEMS`, 200 plus extra items).

#### Scenario: Single directory above the GTK realized-widget cap
- **WHEN** an expanded directory holds 300 visible entries and the outer sidebar scroller is moved to its lower edge
- **THEN** the row for the last entry in sort order is realized, allocated a non-zero height, and its label text is the last entry's file name
- **AND** no vertical band inside the section's allocated list height is left without a realized row

#### Scenario: Boundary row counts around the cap
- **WHEN** an expanded directory holds exactly 199, 200, 201, 202, or 205 visible entries
- **THEN** for each count the last row in sort order is reachable and rendered after scrolling to the lower edge
- **AND** the number of rendered rows equals the number of model rows for the directory when the whole directory fits the viewport, or the last row is rendered when it does not

#### Scenario: Nested large directory
- **WHEN** a directory with 400 visible entries sits two levels below a workspace folder and both ancestors plus the directory are expanded
- **THEN** every one of the 400 rows is reachable by scrolling and the last one is rendered

#### Scenario: Large directory at one thousand rows
- **WHEN** an expanded directory holds 1,000 visible entries
- **THEN** the first, a middle, and the last row are each rendered when scrolled into view
- **AND** the outer scroller's upper bound equals the sum of the section's header height and all row heights

#### Scenario: Truncation placeholder is reachable
- **WHEN** an expanded directory exceeds the per-directory entry cap so the tree model ends with the truncation placeholder row
- **THEN** the placeholder row is rendered when the outer scroller reaches its lower edge

### Requirement: Row widgets are bounded by the viewport, not the model
The system SHALL realize row widgets only for the rows that intersect the outer viewport plus a bounded overscan band. The count of realized row widgets in a section MUST NOT grow with the number of rows in the model once the model exceeds the viewport.

#### Scenario: Realized widget count stays bounded
- **WHEN** an expanded directory holds 1,000 entries and the outer viewport is 800 px tall
- **THEN** the number of realized row widgets is at most the number of rows that fit the viewport plus the overscan allowance on each side
- **AND** it stays below the GTK realized-widget cap

#### Scenario: Realized set follows the scroll position
- **WHEN** the outer scroller moves from the top to the middle of a 1,000-row section
- **THEN** the realized rows are those intersecting the new viewport band, and rows far outside it are no longer realized

### Requirement: Multi-section totals above the cap render completely
The completeness guarantee SHALL hold when the total number of rows across all visible workspace sections exceeds the cap, whether the rows belong to one section or several.

#### Scenario: Two sections each above the cap
- **WHEN** two workspaces each expose an expanded directory of 250 entries
- **THEN** the last row of the first section and the last row of the second section are each rendered when scrolled into view
- **AND** the fixed workspace-scope row stays visible throughout

#### Scenario: Workspace filter hides a large section
- **WHEN** the workspace scope selector narrows to the second workspace while the first holds an expanded 300-row directory
- **THEN** the hidden section is allocated no height and realizes no rows
- **AND** re-selecting `All workspaces` renders the first section completely again

### Requirement: Navigation and scroll-to-row reach every row
Keyboard navigation, programmatic selection with scroll, focus-folder drilldown, pending-selection restore after refresh, and file peek SHALL work for rows beyond the cap exactly as they do for the first rows.

#### Scenario: Arrow-key traversal to the last row
- **WHEN** focus is on the first row of a 300-row directory and Down is pressed until the last row
- **THEN** the focused row is rendered and inside the outer viewport at every step
- **AND** the outer scroller follows the focused row without the inner list scrolling independently

#### Scenario: Programmatic select-and-scroll beyond the cap
- **WHEN** the sidebar selects a file that is row 280 of a 300-row directory and scrolls to it
- **THEN** that row is rendered, selected, and inside the outer viewport

#### Scenario: Pending selection survives a refresh while scrolled to the end
- **WHEN** row 290 of a 300-row directory is selected, the outer scroller rests at its lower edge, and a manual refresh reconciles 120 removed and 120 added rows in bounded batches
- **THEN** after `workspace-refresh-complete` readiness the selected row is still selected and rendered
- **AND** the outer scroller's value is clamped so no blank space appears below the last row

#### Scenario: File peek on a late row
- **WHEN** Space is pressed on row 270 of a 300-row directory
- **THEN** the peek popover anchors to that rendered row without resizing the split layout

#### Scenario: Focus Folder drilldown on a deep large directory
- **WHEN** Focus Folder is invoked on a directory holding 300 entries
- **THEN** the drilldown header is scrolled to the top of the outer scroller and the last entry remains reachable

### Requirement: Geometry stays consistent under resize and structural change
The section's advertised natural height SHALL equal the list's full content height, and re-slicing SHALL follow viewport height changes, collapse/expand, and reconciliation without allocation loops.

#### Scenario: Window height change
- **WHEN** a 300-row directory is scrolled to the lower edge and the window height changes from 800 px to 500 px and back
- **THEN** the last row remains rendered at each size and the outer scroller's value is clamped to the new range

#### Scenario: Collapse removes the rows and the blank space
- **WHEN** an expanded 300-row directory is collapsed
- **THEN** the section's height shrinks to its remaining rows and no empty band remains in the outer scroller

#### Scenario: No allocation feedback loop
- **WHEN** the outer scroller is dragged continuously across a 1,000-row section for two seconds
- **THEN** the number of size-allocate passes on the section per frame stays bounded and the GTK main loop emits no `Trying to measure`, allocation, or adjustment warnings

### Requirement: Existing sidebar contracts are preserved
The virtualized presentation SHALL preserve the no-horizontal-scrollbar contract, ellipsized labels, the fixed workspace-scope row, per-section refresh and readiness evidence, DnD, context menus, and accessibility metadata.

#### Scenario: Wide names in a large directory
- **WHEN** a 300-row directory contains entries with 200-character names
- **THEN** labels ellipsize, the sidebar exposes no horizontal scrollbar, and the last row is still reachable

#### Scenario: Accessibility state on rendered rows
- **WHEN** row 260 of a 300-row directory is scrolled into view
- **THEN** its rendered row exposes the same accessible label, role, and selected/expanded states as an early row does
- **AND** the tree's busy state and `workspace-refresh-complete` readiness are unchanged by scrolling
