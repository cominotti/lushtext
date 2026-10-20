## ADDED Requirements

### Requirement: Toplevel presentation, visibility, and late-composition behaviours are ledger axioms
The ledger SHALL record, with the next free ids (A46–A50 at the time of writing; A42–A45 belong to
`document-scale-policy`), the GTK and Libadwaita behaviours that the staged window composition
depends on:
- that the first realize of a toplevel in a process carries a one-time cost that later toplevels
  do not pay, recording the measured magnitudes of both;
- that `gtk_window_show` validates CSS only for visible nodes and realizes the surface, and that
  the toplevel's size computation and first measure run in the first frame's layout phase, not
  inside `present`;
- that a subtree made visible or parented in an already mapped window is styled and measured in
  the next frame's layout phase, not at the call;
- that a concealed vertical-slide `GtkRevealer` measures a visible child horizontally at full
  scale and returns zero for an invisible child;
- that `AdwBreakpointBin` applies a breakpoint matching on its first allocation after `map` before
  allocating its child, while a breakpoint that starts matching on a later allocation, including
  one added after `map`, swaps through a one-frame snapshot of the old content.

Each entry SHALL be pinned by an isolated `gtk-lush-axioms` probe with a sample that has an
interactive and a `--check` mode. The composition design SHALL cite the ids.

#### Scenario: Presentation axioms are probed
- **WHEN** `make gtk-axioms` runs
- **THEN** each presentation axiom's probe asserts its recorded behaviour in isolation, and `make check-gtk-axioms` finds the ledger, catalogue, probes, and samples in agreement

#### Scenario: A toolkit update removes the one-time realize cost
- **WHEN** the first-realize probe reports that a first and a later toplevel realize cost the same after a GTK update
- **THEN** the change is reviewed against the composition stages before the probe is edited, and the realize stage is reconsidered

#### Scenario: A toolkit update changes the first-allocation breakpoint path
- **WHEN** the breakpoint probe reports that a breakpoint matching on the first allocation after map no longer applies before the child is allocated
- **THEN** the composition's first-frame geometry guarantee and the breakpoint-loop model are reviewed before the probe is edited
