## ADDED Requirements

### Requirement: Showing or collapsing a directory row performs no directory I/O
The workspace sidebar SHALL NOT scan, admit a scan for, take a scan permit for, or register a child store for a directory whose child model the tree has not installed. Binding a collapsed directory row, rebinding it while the list recycles rows during scrolling, and collapsing an expanded row MUST NOT submit a child scan or register an orphan child store.

#### Scenario: Scrolling past collapsed directories starts no scans
- **WHEN** a workspace folder with hundreds of collapsed, non-empty subdirectories is shown and the user scrolls the sidebar to the end and back
- **THEN** no child scan is submitted and no child store is registered for any of those directories
- **AND** each directory row still shows its expander affordance
- **AND** the file tree's accessible busy state does not change

#### Scenario: Directories beyond the emptiness lookahead behave the same
- **WHEN** a collapsed directory row's emptiness is unknown because the parent scan's lookahead cap was exhausted
- **THEN** binding the row starts no scan and the row remains expandable

#### Scenario: Collapsing a directory does not rescan it
- **WHEN** the user collapses an expanded directory row
- **THEN** its children leave the tree and no child scan of that directory is submitted

#### Scenario: No orphan store answers for a collapsed directory
- **WHEN** a directory row has been shown and rebound but never expanded
- **THEN** no cached child store exists for its path, and a file operation targeting that directory reaches no store the tree does not show

### Requirement: Installed child models scan once with bounded readiness and inert evidence
A child scan SHALL start only for an installed directory model while preserving active-plus-latest, admission, generation, and bounded reconciliation contracts. Pending installed materializations SHALL block readiness without discarded probes blocking readiness or toggling accessible `busy`; the evidence surface SHALL read child-model creations, discarded probes, materializations, pending count, scan submissions, and registered stores inertly.

#### Scenario: Expanding a directory scans it exactly once
- **WHEN** the user expands a collapsed directory row
- **THEN** exactly one child scan of that directory is submitted through the existing scan admission
- **AND** its entries appear under the row, and workspace refresh readiness becomes terminal once that scan and its reconciliation complete

#### Scenario: Readiness does not report an expanded but unscanned directory as settled
- **WHEN** a directory row has just been expanded and its scan has not started yet
- **THEN** the section reports a child scan pending, and a workspace path reveal waiting on that ancestor does not classify a missing child before the scan's listing is complete

#### Scenario: A refresh reaching the installed store first does not double-scan it
- **WHEN** a refresh targets a directory whose child model was installed in the same main-loop turn
- **THEN** only one scan of that directory is active or pending at a time

#### Scenario: Existing sidebar affordances are unchanged
- **WHEN** directory rows are shown with the fix in place
- **THEN** rows the parent scan proved empty keep the `(Empty)` label, no expander, and a disabled Focus Folder action
- **AND** non-empty rows keep the expander and Focus Folder, auto-refresh watch targets still follow expanded directories, folder-reorder drag hover still materializes nothing, and every rendered-row and reveal contract of the viewport slice host still holds
