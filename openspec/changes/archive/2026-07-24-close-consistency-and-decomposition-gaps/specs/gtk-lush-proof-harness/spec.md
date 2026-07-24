## ADDED Requirements

### Requirement: Widget-test window presentation is a single shared helper
Widget tests SHALL present test windows through one shared presentation
helper that presents the window, waits for realization/allocation with an
async-scale budget, and drains pending main-loop work. Per-module private
copies of the presentation helper MUST NOT exist, and the widget-wiring
documentation MUST name the helper's real home so the shared-helper claim
stays true. The helper MAY live in the proof-harness crate or in the
LushText widget-test common module; wherever it lives, all widget-test
modules import it from that one place.

#### Scenario: All widget-test modules share one presentation path
- **WHEN** a widget-test module needs a presented, realized window
- **THEN** it calls the shared presentation helper
- **AND** no module-local `present_window` (or equivalent divergent copy)
  remains in the widget-test tree

#### Scenario: Presentation waits on realization, not on luck
- **WHEN** the shared helper presents a window under the headless harness
- **THEN** it waits on an allocation/realization predicate with a generous
  async budget before returning
- **AND** a loaded machine delays completion without flaking the caller

#### Scenario: Documentation matches the helper's real home
- **WHEN** the widget-wiring rules describe shared widget-test wait and
  presentation helpers
- **THEN** the named module actually exports the presentation helper
- **AND** a future divergent copy is caught by review against the documented
  single-home contract
