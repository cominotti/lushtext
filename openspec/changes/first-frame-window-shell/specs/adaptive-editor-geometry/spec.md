## ADDED Requirements

### Requirement: The first-frame shell geometry is a pure plan that equals the composed geometry
The system SHALL derive the geometry of the first-frame shell's body placeholder from a pure,
whole-pixel, float-free plan in `ui/window/geometry/policy.rs`: the visible workspace rail width
or its absence, and the visible document-properties pane width or its absence. The plan SHALL take
the same facts as the split-view restore plan, plus the restored window size. It SHALL reuse the
same rules as the shell reconciliation decision and SHALL NOT copy them.

A Kani harness SHALL prove, for every `i32` window width, every workspace width preset, and every
combination of requested workspace visibility, requested document-properties visibility, and
compact layout, that the plan's rail and pane widths equal those that the composed shell's first
reconciliation renders. The window SHALL restore its size and maximized state before the shell
surface is realized.

#### Scenario: Shell rail matches the composed sidebar
- **WHEN** a window is restored at 1,200 sp with the workspace sidebar requested and the `Comfy` preset
- **THEN** the shell's rail placeholder has the visible width that the composed workspace sidebar is allocated after the body stage

#### Scenario: Compact restore shows no rail
- **WHEN** the restored window width is at or below the breakpoint that collapses the workspace sidebar
- **THEN** the shell shows no rail placeholder, and the composed shell renders no docked workspace sidebar

#### Scenario: The proof covers every width
- **WHEN** the Kani harness for the shell plan runs
- **THEN** it verifies the agreement for every `i32` width, preset, and intent combination, and covers each branch with a `kani::cover!`
