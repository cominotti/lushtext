## ADDED Requirements

### Requirement: Toplevel presentation and late-composition behaviours are ledger axioms
The ledger SHALL record, with the next free ids, the GTK behaviours that the staged window
composition depends on:
- that the first realize of a toplevel in a process carries a one-time cost that later toplevels
  do not pay, recording the measured magnitudes of both;
- that `gtk_window_show` validates CSS for the whole widget tree and realizes the surface, and that
  the toplevel's size computation and first measure run in the first frame's layout phase, not
  inside `present`;
- that a subtree parented into an already mapped window is styled and measured in the next frame's
  layout phase, not at insertion.

Each entry SHALL be pinned by an isolated `gtk-lush-axioms` probe with a sample that has an
interactive and a `--check` mode. The composition design SHALL cite the ids.

#### Scenario: Presentation axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** each presentation axiom's probe asserts its recorded behaviour in isolation, and `make check-gtk-axioms` finds the ledger, catalogue, probes, and samples in agreement

#### Scenario: A toolkit update removes the one-time realize cost
- **WHEN** the first-realize probe reports that a first and a later toplevel realize cost the same after a GTK update
- **THEN** the change is reviewed against the composition stages before the probe is edited, and the realize stage is reconsidered
