## ADDED Requirements

### Requirement: Adwaita breakpoint and split-view behaviours are ledger axioms
Every Adwaita behaviour that the breakpoint-loop envelope relies on SHALL be a
ledger entry with a new stable id. Existing ids SHALL NOT be reused. At
minimum the ledger SHALL cover:

- **Condition units.** How an `AdwBreakpoint` `max-width` condition in `sp`
  is evaluated against the window width, including any dependence on the text
  scale factor.
- **Re-evaluation timing.** When a changed condition, set with
  `set_condition`, is re-evaluated: synchronously, or at the next allocation.
- **Setter semantics.** When setters are applied during allocation, and what
  value an unapplied breakpoint restores to a property the application wrote
  while it was applied.
- **Split-view allocation.** How `AdwOverlaySplitView` allocates its sidebar
  from `sidebar-width-fraction` and its minimum and maximum widths, and
  whether `show-sidebar` or `collapsed` changes alter the toplevel's allocated
  width.
- **Minimum width.** How the set of breakpoints affects the window's minimum
  width.

Each entry SHALL be pinned by an isolated headless probe in
`crates/lushtext/tests/widget/gtk_axioms.rs`, a minimal Adwaita fixture that
uses no LushText or GTK Lush widget. If an entry cannot be pinned, it SHALL
record why. Each entry's statement SHALL be written from what its probe
observes, not from belief.

#### Scenario: Breakpoint axioms gain probes
- **WHEN** this change completes
- **THEN** every axiom cited by the breakpoint-loop model has a ledger row with an isolated probe or a recorded reason it is not pinned
- **AND** each probe passes against the platform floor recorded in the ledger

#### Scenario: The breakpoint model cites its envelope
- **WHEN** the breakpoint-loop harness restricts Adwaita's behaviour
- **THEN** each `kani::assume` clause names the ledger id that justifies it, and any narrower assumption is recorded in the model and the programme record
