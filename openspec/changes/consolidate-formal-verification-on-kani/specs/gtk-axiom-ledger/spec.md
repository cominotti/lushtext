## ADDED Requirements

### Requirement: GTK behavioural axioms are recorded in one normative ledger
The project SHALL keep one ledger of the GTK, GtkListBase, GtkScrollable, and
Adwaita behaviours that LushText and GTK Lush geometry designs depend on. Each
entry SHALL have a stable id (A1, A2, …), a precise statement, the designs
that depend on it, and its pinning status. The ledger SHALL live with the
gtk4-libadwaita-internals references, and the programme record SHALL link to
it.

#### Scenario: A design relies on a new GTK behaviour
- **WHEN** a geometry change starts depending on a GTK behaviour not yet in the ledger
- **THEN** the same change adds a ledger entry for it

### Requirement: Every axiom is pinned by an isolated headless probe or declared unpinnable
Each axiom SHALL be pinned by a headless widget probe that asserts the
behaviour in isolation against real GTK, not only indirectly through a
consumer test. An axiom that cannot be pinned SHALL record why. A probe
failure after a GTK update MUST be treated as an axiom change and reviewed
against every dependent design.

#### Scenario: Previously unpinned axioms gain probes
- **WHEN** this change completes
- **THEN** axioms A5, A9, A11, and A13 each have an isolated probe
- **AND** A8 records the phase-0 instrumented evidence

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
