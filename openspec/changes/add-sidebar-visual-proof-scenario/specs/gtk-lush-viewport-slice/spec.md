## ADDED Requirements

### Requirement: The slice bin reports a pending forwarded request as evidence

`ViewportSliceBin` SHALL expose a read-only accessor reporting whether a child-originated scroll request has been accumulated for the outer scroller and not yet applied by its idle. The accessor SHALL NOT allocate, queue a layout, or change any adjustment, and SHALL be governed like `allocation_count()` and `correction_count()`: documented, covered by the GTK Lush public-API snapshot, and exercised by a widget test in both LushText and the GTK Lush adoption lab.

#### Scenario: A forwarded request is pending until its idle runs
- **WHEN** the child applies a `scroll_to` whose target lies outside the published band
- **THEN** the accessor reports a pending request until the idle has moved the outer adjustment, and reports none afterwards

#### Scenario: A resting bin reports no pending request
- **WHEN** the bin has been re-allocated repeatedly with no input
- **THEN** the accessor reports no pending request and `allocation_count()` is the only counter that grows
