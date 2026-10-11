## ADDED Requirements

### Requirement: Tree-expander expandability queries are ledger axioms
Every behaviour of `GtkTreeExpander` and `GtkTreeListRow` expandability that the workspace sidebar's deferred child-model materialization relies on SHALL be a ledger entry with a new stable id. Existing ids SHALL NOT be reused. At minimum the ledger SHALL cover:

- **Query on bind.** `GtkTreeExpander` asks the row whether it is expandable synchronously when a row is set on it, and again on row property notifications other than `expanded` and `item`, whatever its `hide-expander` value.
- **No positive cache.** A query on a collapsed row that has never answered "not expandable" calls the create function again on every query.
- **Probe models are discarded.** A model the create function returned to a query, held by nothing else, is finalized by the time the query returns.
- **Installed models are kept.** A model installed by expanding the row stays alive while the row is expanded and is the row's children model. Collapsing finalizes it, and a bound expander's follow-up query calls the create function again.
- **Negative answers are final.** After the create function returns no model, further queries and rebinds call it no more.

Each entry SHALL be pinned by an isolated headless probe in the `gtk-lush-axioms` crate that uses no LushText or GTK Lush widget and needs no window. Each SHALL have a catalogue entry and a sample with an interactive mode and a headless `--check` mode. Each entry's statement SHALL be written from what its probe observes. The ledger row SHALL name the sidebar's deferred-materialization design and its consumer widget tests as dependents.

#### Scenario: The probe observes a query per bind
- **WHEN** the probe binds a collapsed, never-probed row to a tree expander, unbinds it, and binds it again, once with `hide-expander` set
- **THEN** each bind calls the create function once and each returned model is finalized on return

#### Scenario: The probe observes collapse leading to a fresh query
- **WHEN** the probe expands and then collapses a row bound to a tree expander
- **THEN** the installed model is alive and equal to the row's children while expanded, is finalized after the collapse, and exactly one further create call follows the collapse

#### Scenario: A toolkit update changes the behaviour
- **WHEN** the probe fails after a GTK update
- **THEN** the ledger entry, the sidebar's deferred-materialization design, and its consumer tests are revisited before the failure is resolved
