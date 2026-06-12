# gtk-lush-viewport Specification

## Purpose
Define the reusable GTK Lush viewport observation helpers used by stock gtk-rs
applications and by LushText editor geometry workflows.

## Requirements
### Requirement: Leaf viewport observation crate
`gtk-lush-viewport` SHALL provide viewport observation helpers for stock
gtk-rs applications while remaining an independently adoptable GTK Lush leaf
crate. The crate MUST NOT depend on LushText crates, MUST NOT depend on another
GTK Lush family crate at runtime, MUST NOT subclass or own the consumer's app
shell, and MUST NOT replace GTK's allocation, scrolling, or event delivery
systems.

#### Scenario: Standalone application adopts only viewport
- **WHEN** `cargo test -p gtk-lush-viewport --examples` builds the crate's
  standalone example
- **THEN** the example uses stock gtk-rs plus `gtk-lush-viewport`
- **AND** no other GTK Lush crate or LushText crate is required

#### Scenario: Runtime family dependency is rejected
- **WHEN** `gtk-lush-viewport` declares another `gtk-lush-*` crate as a
  non-dev dependency
- **THEN** the family policy check fails until the dependency is removed

#### Scenario: GTK remains the source of geometry truth
- **WHEN** a consumer observes viewport changes through the crate
- **THEN** the observer reacts to GTK-owned adjustments and widget state
- **AND** it does not install a replacement layout manager or animation system

### Requirement: Adjustment page-size changes report viewport reflow
The crate SHALL observe viewport width and height changes through scrollable
adjustments, including `page-size` changes that occur when GTK reallocates
layout-manager widgets. It MUST expose axis-specific events and avoid reporting
unchanged dimensions as fresh reflow.

#### Scenario: Width-only reflow is observed
- **WHEN** a scrollable widget's horizontal adjustment page size changes while
  height remains unchanged
- **THEN** the observer emits a horizontal viewport-change event
- **AND** a consumer can run width-only repair logic from that event

#### Scenario: Height-only reflow is observed
- **WHEN** a scrollable widget's vertical adjustment page size changes while
  width remains unchanged
- **THEN** the observer emits a vertical viewport-change event
- **AND** a consumer can run height-only repair logic from that event

#### Scenario: Dead allocation-vfunc trap is documented
- **WHEN** a consumer reads the crate docs
- **THEN** the docs explain that layout-manager widget subclasses may not
  receive useful `size_allocate` overrides
- **AND** they show adjustment observation as the stock gtk-rs-compatible
  alternative

### Requirement: Rest state tracks user intent outside reflow bursts
The crate SHALL provide rest-state helpers for start-edge horizontal and
vertical scrolling. Rest state MUST be updated from adjustment values during
ordinary user or programmatic scroll events, but callers MUST be able to pause
or exclude updates during known reflow bursts so GTK-preserved values do not
masquerade as user intent.

#### Scenario: At-left state records ordinary scroll
- **WHEN** a horizontal adjustment value moves to or from its lower bound
  outside a paused reflow window
- **THEN** the rest-state helper records whether the viewport rests at the
  left edge

#### Scenario: At-top state ignores reflow preservation
- **WHEN** a vertical adjustment value changes while the caller has marked a
  reflow burst as pending
- **THEN** the rest-state helper does not treat that transient value as new
  user scroll intent

#### Scenario: Explicit user scroll can reveal held rendering
- **WHEN** a consumer receives an ordinary value-changed event after a reflow
  repair has entered a revealable state
- **THEN** the consumer can use the observer path to trigger an early reveal or
  equivalent user-scroll handling before recording new rest state

### Requirement: Anchor repair and overscroll hooks stay caller-owned
The crate SHALL expose enough viewport-change information for consumers to
schedule edge clamps, dynamic EOF overscroll refreshes, focus-mode geometry
refreshes, and minimap or preview repairs. The crate MUST NOT own those
workflow repairs; they remain callbacks or consumer code.

#### Scenario: Left-edge clamp is scheduled by consumer
- **WHEN** a width reflow occurs while rest state says the viewport was at the
  left edge
- **THEN** the consumer can schedule a GTK-main-loop clamp to the horizontal
  lower bound
- **AND** the crate does not directly mutate the consumer's scroll state unless
  asked through the documented callback

#### Scenario: Top-edge clamp is scheduled by consumer
- **WHEN** a height reflow occurs while rest state says the viewport was at the
  top edge
- **THEN** the consumer can schedule a GTK-main-loop clamp to the vertical
  lower bound
- **AND** explicit user scroll away from the top remains respected

#### Scenario: EOF overscroll remains app policy
- **WHEN** viewport height changes for an editor that has dynamic EOF
  overscroll
- **THEN** the observer can notify the app to refresh its bottom margin
- **AND** the crate does not encode LushText's overscroll percentage or editor
  margin policy as a generic rule

### Requirement: Public documentation and tests prove viewport behavior
Every public item in `gtk-lush-viewport` SHALL be documented under the GTK Lush
engineering bar. Observable behavior MUST have runnable doctests, unit tests,
or widget tests as appropriate. The crate MUST keep `#![forbid(unsafe_code)]`
and `#![deny(missing_docs)]`.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public observer, event, rest-state, guard, or helper type is added
  without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Tests prove page-size and value behavior
- **WHEN** `cargo test -p gtk-lush-viewport` runs
- **THEN** tests cover axis-specific page-size changes, unchanged dimensions,
  value-changed rest-state updates, paused reflow state, and dead-target
  cleanup without linking LushText

#### Scenario: README teaches the choosing rule
- **WHEN** the README is rendered
- **THEN** it explains when adjustment observation is appropriate, when a
  consumer should use ordinary GTK signals directly, and how the crate avoids
  becoming a layout framework

### Requirement: LushText overscroll observers migrate to the crate
LushText SHALL replace fitting viewport observation logic in
`editor_page/overscroll.rs` with `gtk-lush-viewport`. The migration MUST
preserve horizontal and vertical page-size detection, rest-state recording,
minimap reflow scheduling, dynamic EOF overscroll refresh, top/left clamps,
focus-mode geometry refresh, and user-scroll reveal behavior.

#### Scenario: Width-only sidebar transition remains anchored
- **WHEN** the workspace sidebar show/hide transition changes editor width
  while the editor was resting at the left edge
- **THEN** the migrated observer schedules the same left-edge clamp behavior
  as before
- **AND** the editor gutter and line starts remain visible after layout settles

#### Scenario: Top anchor remains stable
- **WHEN** a height-affecting layout change occurs while the editor was
  resting at the top edge
- **THEN** the migrated observer schedules the same top-edge clamp behavior as
  before
- **AND** the first visible line remains line one after layout settles

#### Scenario: Overscroll and minimap refresh still run
- **WHEN** the viewport dimensions change after migration
- **THEN** LushText refreshes dynamic EOF overscroll and minimap geometry with
  the same visible behavior as before
- **AND** expensive marker recomputation remains bounded or debounced according
  to the existing minimap contracts

### Requirement: Viewport migration preserves proof gates
The viewport extraction SHALL preserve LushText's adaptive geometry,
minimap, and warning-free contracts. The phase MUST pass family crate tests,
focused editor viewport/widget tests, relevant adaptive shell tests, visual
geometry scenarios for sidebar/minimap reflow, and delegated GTK internals and
responsiveness reviews before archive.

#### Scenario: Visual geometry verifies viewport-sensitive invariants
- **WHEN** the migrated viewport observer affects sidebar animation, editor
  width reflow, minimap top anchoring, or dynamic overscroll
- **THEN** visual-geometry proof verifies the affected pixel anchors and
  animation-frame invariants
- **AND** final-settle-only evidence is not counted for animation-sensitive
  minimap coverage

#### Scenario: Delegated reviews cover viewport risks
- **WHEN** the viewport migration is implementation-complete
- **THEN** focused delegated reviews examine GTK allocation assumptions,
  main-thread responsiveness, architecture boundaries, and comments
- **AND** actionable findings are fixed before the phase is marked complete
