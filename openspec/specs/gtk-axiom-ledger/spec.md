# gtk-axiom-ledger Specification

## Purpose
Record the GTK, GtkListBase, GtkScrollable, and Adwaita behaviours that LushText and GTK Lush geometry designs depend on in one normative ledger, pin each axiom with an isolated headless probe, and derive verification envelopes from it.
## Requirements
### Requirement: GTK behavioural axioms are recorded in one normative ledger
The project SHALL keep one ledger of the GTK, GtkListBase, GtkScrollable, and
Adwaita behaviours that LushText and GTK Lush geometry designs depend on. Each
entry SHALL have a stable id (A1, A2, …), a precise statement, the designs
that depend on it, its pinning status, the GTK and Libadwaita versions it was
last verified against, and its `gtk-lush-axioms` sample or the reason it has
none. The ledger SHALL live with the gtk4-libadwaita-internals references, and
the programme record SHALL link to it. A policy check SHALL fail when the
ledger's ids and the `gtk-lush-axioms` catalogue, probes and samples disagree.

#### Scenario: A design relies on a new GTK behaviour
- **WHEN** a geometry change starts depending on a GTK behaviour not yet in the ledger
- **THEN** the same change adds a ledger entry for it and a `gtk-lush-axioms` catalogue entry, with a probe and a sample or a recorded reason it cannot be isolated

#### Scenario: The ledger and the catalogue drift
- **WHEN** a ledger id has no catalogue entry, a catalogue entry has no ledger id, a probed entry lacks its sample, or a "Pinned by" cell names a probe that no longer exists
- **THEN** `make check-policy` fails naming the id

#### Scenario: A verified-against version is recorded
- **WHEN** a ledger entry's verified toolkit versions are updated
- **THEN** the versions are copied from the probe's printed observation, with where it ran, not from an assumption

### Requirement: Every axiom is pinned by an isolated headless probe or declared unpinnable
Each axiom SHALL be pinned by a headless probe in the `gtk-lush-axioms` crate
that asserts the behaviour in isolation against real GTK, not only indirectly
through a consumer test. An axiom that cannot be pinned SHALL record why.
LushText consumer tests that depend on an axiom SHALL remain, as evidence that
LushText stays inside the conditions the probe isolates. A probe failure after
a GTK update MUST be treated as an axiom change and reviewed against every
dependent design.

#### Scenario: Isolated probes run outside LushText
- **WHEN** `make gtk-axioms` runs
- **THEN** axioms A1, A4, A5, A9, A11, and A13 each have an isolated `gtk-lush-axioms` probe, and none runs from LushText's widget-test binary
- **AND** A8 records the phase-0 instrumented evidence, and every other unprobed axiom records why it has no probe

#### Scenario: A GTK update changes a pinned behaviour
- **WHEN** a probe fails after a toolkit update
- **THEN** the ledger entry and every dependent design and verification envelope are revisited before the failure is resolved

### Requirement: Verification envelopes derive from the ledger
A verification model that treats GTK as nondeterministic SHALL constrain it
only by ledger axioms, and SHALL cite each axiom id it uses. Using an
envelope narrower than the ledger's pinned statement MUST be recorded as an
explicit assumption.

#### Scenario: The slice-bin loop model cites its envelope
- **WHEN** the closed-loop harness restricts the child's reaction to an allocation
- **THEN** each restriction names the ledger axiom that justifies it

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

Each entry SHALL be pinned by an isolated headless probe in the
`gtk-lush-axioms` crate, a minimal Adwaita fixture that uses no LushText or GTK
Lush widget, with a `gtk-lush-axioms` catalogue entry and sample. If an entry
cannot be pinned, it SHALL record why. Each entry's statement SHALL be written from what its probe
observes, not from belief.

#### Scenario: Breakpoint axioms gain probes
- **WHEN** this change completes
- **THEN** every axiom cited by the breakpoint-loop model has a ledger row with an isolated probe or a recorded reason it is not pinned
- **AND** each probe passes against the platform floor recorded in the ledger

#### Scenario: The breakpoint model cites its envelope
- **WHEN** the breakpoint-loop harness restricts Adwaita's behaviour
- **THEN** each `kani::assume` clause names the ledger id that justifies it, and any narrower assumption is recorded in the model and the programme record

