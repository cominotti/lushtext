# Delta Specification: gtk-lush-viewport

## MODIFIED Requirements

### Requirement: Adjustment page-size changes report viewport reflow
`gtk-lush-viewport` SHALL observe horizontal and vertical scrollable adjustments to report axis-specific `page-size` reflow beyond epsilon while rejecting non-finite/`NaN` `page_size` values from poisoning future comparisons. `ViewportObserver::for_scrollable` MUST document that callers invoke it after the scrollable is attached to its `GtkScrolledWindow` so replaced adjustments are not missed.

#### Scenario: Width-only reflow is observed
- **WHEN** a scrollable widget's horizontal adjustment page size changes while height remains unchanged
- **THEN** the observer emits a horizontal viewport-change event
- **AND** a consumer can run width-only repair logic from that event

#### Scenario: Height-only reflow is observed
- **WHEN** a scrollable widget's vertical adjustment page size changes while width remains unchanged
- **THEN** the observer emits a vertical viewport-change event
- **AND** a consumer can run height-only repair logic from that event

#### Scenario: Dead allocation-vfunc trap is documented
- **WHEN** a consumer reads the crate docs
- **THEN** the docs explain that layout-manager widget subclasses may not receive useful `size_allocate` overrides and that `for_scrollable` must be called after parenting into `GtkScrolledWindow`
- **AND** they show adjustment observation as the stock gtk-rs-compatible alternative

#### Scenario: Non-finite page size does not poison filter state
- **WHEN** an adjustment emits `changed` with a `NaN` or non-finite `page_size`
- **THEN** the observer ignores the non-finite sample without storing `NaN` as the baseline size

### Requirement: Public documentation and tests prove viewport behavior
Every public item in `gtk-lush-viewport` SHALL keep `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, accurate registration count docs on `len()`, and `Debug` implementations. Display-dependent observer tests MUST execute in the headless widget lane rather than sitting unexecuted behind `#[ignore]` attributes in `src/`.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public observer, event, rest-state, guard, or helper type is added without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Tests prove page-size and value behavior
- **WHEN** `cargo test -p gtk-lush-viewport` and the headless `gtk_lush_adoption` widget lane run
- **THEN** unit tests cover `RestState` and epsilon math without a display, and the headless widget lane executes all observer filter, accumulation, axis, drop-disconnect, and `RestState` tests without `#[ignore]` stubs

#### Scenario: README teaches the choosing rule
- **WHEN** the README is rendered
- **THEN** it explains when adjustment observation is appropriate, when a consumer should use ordinary GTK signals directly, and how the crate avoids becoming a layout framework
