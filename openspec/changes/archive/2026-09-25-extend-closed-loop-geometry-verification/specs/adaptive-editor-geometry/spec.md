## ADDED Requirements

### Requirement: The adaptive-shell breakpoint loop is verified against the axiom ledger
The project SHALL keep a Kani-checked step model of the adaptive-shell
breakpoint feedback loop: allocated window width → derived layout →
properties breakpoint threshold and Adwaita breakpoint state → rendered
surfaces → allocated window width.

The model SHALL call the real `derive_adaptive_shell_layout` and the real
reconciliation decision the execution module applies. It SHALL model Adwaita
only through `kani::assume` clauses that cite ledger axiom ids.

For every workspace preset, every requested-visibility and compact-slot
combination, Focus Mode on and off, and window widths across the supported
range, the model SHALL prove the following.

- **Fixed point:** with stable inputs, the cached threshold, the installed
  breakpoint condition, the breakpoint's applied state, the properties layout
  name, the compact slot, and the rendered visibility of both surfaces stop
  changing within a stated bound of allocations. One further allocation at the
  same width then changes nothing.
- **No flapping:**
  - with stable inputs, the properties presentation changes at most once;
  - across a monotone width sweep, it changes at most once per threshold
    crossing.
- **Requested visibility is preserved:** no step writes the requested
  workspace or document-properties visibility. Whenever the width admits both
  surfaces, the rendered visibility equals the requested visibility.
- **Agreement:** at rest, the rendered properties layout equals the
  presentation the policy derives.
- **No persistence on the allocation path:** no step reachable from an
  allocation writes a width fraction to settings.

A property that fails SHALL be fixed starting from a failing real-GTK widget
test when real GTK can reach it. When it is not reachable, it SHALL be kept as
a named `#[kani::should_panic]` residual harness, with reachability evidence
in the programme record.

#### Scenario: Stable width settles
- **WHEN** the window width, preset, requested visibility, compact slot and Focus Mode are held fixed for any admissible Adwaita behaviour
- **THEN** the model reaches a state that a further allocation at the same width leaves unchanged, within the stated bound

#### Scenario: A width sweep does not flap
- **WHEN** the window width moves monotonically across the properties breakpoint threshold
- **THEN** the properties presentation changes at most once for that crossing and never alternates between pane and sheet

#### Scenario: Requested visibility survives compact layouts
- **WHEN** a compact width suppresses the workspace sidebar or document properties and the width then widens back to one that admits both
- **THEN** both surfaces render as requested and the requested flags were never written by the loop

### Requirement: The shell reconciliation decision is pure policy
The decision that reconciles the rendered shell surfaces with a derived layout
SHALL be a GTK-free pure function in the workflow's `policy.rs`. It covers:

- which properties layout name to set;
- which compact slot to store;
- which `show-sidebar` and bottom-sheet `open` values to write;
- whether the breakpoint condition must be re-installed.

The execution module SHALL apply the returned writes without re-deciding
them. The extraction SHALL be behaviour-preserving: characterization tests
written before the move SHALL pass unchanged after it.

#### Scenario: The model drives production logic
- **WHEN** the breakpoint-loop harness takes a reconciliation step
- **THEN** it calls the same pure function that `sync_secondary_surfaces` and `sync_properties_breakpoint` apply in production

#### Scenario: Extraction preserves behaviour
- **WHEN** the reconciliation decision moves into `policy.rs`
- **THEN** the existing shell-geometry widget tests and the new characterization tests pass unchanged, and allocation-time sync still performs no settings write

### Requirement: The reconciliation plan is the only writer of the properties layout
The properties breakpoint SHALL NOT carry a setter for the properties layout
name. Its `apply` and `unapply` signals SHALL run the shell reconciliation, so
the pure plan is the only writer of `layout-name`. A setter restores its
add-time value on unapply (ledger A16) and, under text scaling, disagrees with
the policy's px-against-sp comparison (ledger A14), so two writers made one
allocation change the layout twice.

#### Scenario: Large text does not flip the pane
- **WHEN** the text scale is 1.25 and the window is 1500 px wide
- **THEN** the properties layout does not change during an allocation at a fixed width

### Requirement: Every breakpoint that narrows the shell collapses the workspace
Because only the last-added matching breakpoint applies (ledger A18), every
breakpoint whose condition implies the workspace breakpoint's SHALL also set the
workspace split view `collapsed`, so switching between them at a fixed width
never uncollapses the workspace.

#### Scenario: Doubled text scale at the minimum width
- **WHEN** the text scale is 2.0 and the window is 700 px wide, and the text scale then switches between 2.0 and 1.5
- **THEN** the workspace split view stays collapsed throughout
