## ADDED Requirements

### Requirement: List row-widget churn and tree-expander disclosure are ledger axioms
The ledger SHALL record, each with a probe, a catalogue entry and a sample verified on GNOME 51 and the GNOME 50 floor: how a mapped `GtkListView` destroys and creates row widgets when rows are removed; `GtkTreeExpander`'s click, key, action and AT-SPI routes and how an application can own them; how a CSS class overrides the expander's checked arrow; that `GtkTreeListModel` hands out a new row object, without the data and handlers of the one that died, for a node nothing references.

#### Scenario: Removing rows from a mapped list
- **WHEN** the probe removes 16, 64 and 256 bound rows, with 0 and 400 rows after them, and separately the rows no widget is bound to after the band and before it, from a presented `GtkListView` over a `GtkTreeListModel`
- **THEN** it records that bound rows' widgets are destroyed inside `items-changed`, fresh widgets are created for the rows entering the band and never reused from removed items, and their styles are computed in the layout phase of the frame that follows or contains the removal
- **AND** it records that unbound removals on either side of the band destroy and create none and leave the band where it was
- **AND** it records the same head removal with 400 rows after it on an unmapped list

#### Scenario: Owning the expander's routes
- **WHEN** the probe adds a capture-phase click gesture to the expander that claims the sequence on a release inside its arrow icon, adds a capture-phase key controller on an ancestor, inserts a `listitem` action group, and reads the expander's AT-SPI action table
- **THEN** it records that the expander's own click gesture is on its arrow icon and not on the expander, that the claimed click toggles nothing even after the icon is re-created while the row's focus-on-click, selection and double-press activation are unchanged, which of the expander's key bindings reach it with focus on a focusable list row and on a button inside it, that the controller stops those bindings, and that the class actions still resolve first
- **AND** it records the AT-SPI actions the expander exposes, that `DoAction` on its collapse action collapses the row directly, and which accessible `expanded` state GTK keeps on the expander and on its icon

#### Scenario: Overriding the checked arrow
- **WHEN** the probe adds a CSS class to an expanded row's `GtkTreeExpander` under the shipped style
- **THEN** it records the arrow icon the class selects in both text directions, with the row and its `expander` node still expanded and `:checked`

#### Scenario: Row objects are not kept
- **WHEN** the probe sets object data and a `notify::expanded` handler on a `GtkTreeListRow`, drops every reference to it, and reads the same node back through `get_item`
- **THEN** it records whether the row object is the same, whether the data and the handler survive, and which object a `set_expanded` through the new one notifies

#### Scenario: A91 is corrected
- **WHEN** the ledger's A91 row is updated by this change
- **THEN** it states its node-teardown share of a collapse (about a tenth in the stage 1 profile) and points to the new row-widget axiom for the rest

### Requirement: Hiding a list row by opacity and targeting is a ledger axiom
The ledger SHALL record, with a probe, a catalogue entry and a sample verified on GNOME 51 and the GNOME 50 floor, whether hiding a realized `GtkListView` row widget (the list view's child) by opacity 0, `can-target` false and the accessible `hidden` state is paint-only, and how GTK's own map writes treat that state.

#### Scenario: Hiding a row is paint-only
- **WHEN** the probe sets opacity 0, `can-target` false and the accessible `hidden` state on the row widgets of a presented `GtkListView`'s realized rows over a `GtkTreeListModel`, on GNOME 51 and the GNOME 50 floor
- **THEN** it records whether the list runs any measure, factory setup or teardown, whether the band's position and the list's allocation change, whether the hidden rows draw any pixel while selected, focused or under the pointer, which widget the pointer picks over them, what a real press and double press over them select or activate, and whether the presented accessible tree keeps them or their descendants
- **AND** it records the `hidden` state of a row widget the list creates while the state is applied, read after its insertion, and of a hidden row after the list is unmapped and mapped again, and the time to hide and to clear the band's rows
