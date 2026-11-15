## ADDED Requirements

### Requirement: List row-widget churn and tree-expander disclosure are ledger axioms
The ledger SHALL record, each with a probe, a catalogue entry and a sample verified on GNOME 51 and the GNOME 50 floor: how a mapped `GtkListView` destroys and creates row widgets when rows are removed; `GtkTreeExpander`'s click, key and action routes and how an application can own them; and how a CSS class overrides the expander's checked arrow.

#### Scenario: Removing rows from a mapped list
- **WHEN** the probe removes 16, 64 and 256 bound rows, and separately rows no widget is bound to, from a presented `GtkListView` over a `GtkTreeListModel`
- **THEN** it records that bound rows' widgets are destroyed inside `items-changed`, fresh widgets are created for the rows entering the band and never reused from removed items, their styles are computed in the next frame, and unbound removals destroy and create none

#### Scenario: Owning the expander's routes
- **WHEN** the probe disables the expander's click gesture, adds a capture-phase key controller on an ancestor, and inserts a `listitem` action group
- **THEN** it records that the click toggles nothing, that the controller stops the collapse key bindings, that the class actions still resolve first, and that the expander has no AT-SPI action

#### Scenario: Overriding the checked arrow
- **WHEN** the probe adds a CSS class to an expanded row's `GtkTreeExpander` under the shipped style
- **THEN** it records the arrow icon the class selects in both text directions, with the row and its `expander` node still expanded and `:checked`

#### Scenario: A91 is corrected
- **WHEN** the ledger's A91 row is updated by this change
- **THEN** it states its node-teardown share of a collapse (about a tenth in the stage 1 profile) and points to the new row-widget axiom for the rest
