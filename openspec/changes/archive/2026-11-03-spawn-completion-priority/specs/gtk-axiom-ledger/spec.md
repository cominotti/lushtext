## ADDED Requirements

### Requirement: Text validation's hold on lower main-loop priorities is a ledger axiom
The ledger SHALL record A80. While a `GtkTextView` has lines left to validate,
its incremental validation idle is ready again after every batch. So a
`G_PRIORITY_DEFAULT_IDLE` source does not dispatch until the whole buffer is
validated, whether the view is on screen or on the hidden page of a
`GtkStack`. A source at `GDK_PRIORITY_REDRAW + 4` dispatches before the first
incremental batch completes. A80 SHALL be pinned by a `gtk-lush-axioms` probe,
with a sample that has an interactive and a `--check` mode, and with a control
showing that the lower-priority source dispatches at once when nothing is left
to validate. The probe SHALL record the iterations and microseconds each source
waited as magnitudes, never as a verdict.

#### Scenario: A80 is probed
- **WHEN** `make gtk-axioms` runs
- **THEN** the A80 probe runs under the private headless session, in both the
  presented and the hidden-page fixture
- **AND** `make check-gtk-axioms` finds the ledger, catalogue, probe and sample
  in agreement

#### Scenario: GTK stops holding lower priorities back
- **WHEN** a GTK update makes a `G_PRIORITY_DEFAULT_IDLE` source dispatch
  before validation completes, or makes validation outrank priority 124
- **THEN** the probe fails
- **AND** the completion priority of `gtk-lush-tasks` and LushText's admission
  hand-offs are revisited before the probe is changed
