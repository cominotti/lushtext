## ADDED Requirements

### Requirement: Tree-expander expandability queries are ledger axioms
Every behaviour of `GtkTreeExpander` and `GtkTreeListRow` expandability relied upon by deferred child-model materialization SHALL be pinned under a new ledger id: query on bind/rebind/`hide-expander`/non-`expanded`/`item` notify, no positive cache, discarded probe models finalized on return, installed models alive until collapse, and `None` answers cached as empty.

#### Scenario: The probe observes a query per bind
- **WHEN** the probe binds a collapsed, never-probed row to a tree expander, unbinds it, and binds it again, once with `hide-expander` set
- **THEN** each bind calls the create function once and each returned model is finalized on return

#### Scenario: The probe observes collapse leading to a fresh query
- **WHEN** the probe expands and then collapses a row bound to a tree expander
- **THEN** the installed model is alive and equal to the row's children while expanded, is finalized after the collapse, and exactly one further create call follows the collapse

### Requirement: Tree-expander ledger entries carry probes and dependents
Each new ledger entry SHALL have an isolated headless probe in `gtk-lush-axioms` (requiring no LushText or GTK Lush widget and no window), a catalogue entry, and an interactive/`--check` sample. Its statement SHALL record what the probe observes, and its row SHALL name the deferred-materialization design and its consumer widget tests as dependents.

#### Scenario: A toolkit update changes the behaviour
- **WHEN** the probe fails after a GTK update
- **THEN** the ledger entry, the sidebar's deferred-materialization design, and its consumer tests are revisited before the failure is resolved
