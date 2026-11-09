## ADDED Requirements

### Requirement: The slice bin never measures or allocates its list below the list's minimum
On every supported toolkit (the GNOME 50 floor and the shipped GNOME 51 runtime), `ViewportSliceBin` SHALL NOT measure or allocate its child `GtkListView` with a height below the minimum the list reports for that measurement. GTK SHALL never log `Trying to measure GtkListView … for height of 0, but it needs at least N` for a slice-bin list, including empty, collapsed, hidden, and just-constructed sections. If a toolkit change alters the list's minimum-height contract, the change that adopts that toolkit SHALL pin the new contract as a ledger axiom with a probe, fix the bin at the cause, and rerun the Kani shards that cover any changed slice geometry or scroll-request code. The rendered-row contract (`workspace-tree-rendered-row-anchors`) SHALL keep holding.

#### Scenario: Workspace sections on GTK 4.24
- **WHEN** the `workspace_section` widget tests run on GTK 4.24
- **THEN** no `GtkListView` measure warning appears, and the rendered-bounds slice-bin tests still pass

#### Scenario: Toggling a section's folders
- **WHEN** a workspace section's folder set is collapsed and expanded again on either supported toolkit
- **THEN** the list is never measured for a height below its reported minimum
