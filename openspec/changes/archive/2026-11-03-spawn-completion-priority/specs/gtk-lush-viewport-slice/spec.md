## MODIFIED Requirements

### Requirement: The slice bin reports a pending forwarded request as evidence

`ViewportSliceBin` SHALL expose a read-only accessor reporting whether a child-originated scroll request has been accumulated for the outer scroller and is not yet at rest: not yet applied by its idle, or applied by the idle but not yet consumed by the bin's next allocation, which re-slices from the outer scroller's new value. Between the idle and that allocation the outer holds its new value while the rows are still drawn where the old band put them, so the accessor SHALL keep reporting the request pending there. An idle whose move the outer scroller clamps to no change leaves nothing pending. The accessor SHALL NOT allocate, queue a layout, or change any adjustment, and SHALL be governed like `allocation_count()` and `correction_count()`: documented, covered by the GTK Lush public-API snapshot, and exercised by a widget test in both LushText and the GTK Lush adoption lab. The Kani slice-loop model SHALL model the accessor and prove that, polled after each frame's idles, a bin reporting nothing pending holds the band the current outer value gives it, with a `should_panic` harness pinning the pre-fix accessor that ended with the idle.

#### Scenario: A forwarded request is pending until its idle runs
- **WHEN** the child applies a `scroll_to` whose target lies outside the published band
- **THEN** the accessor reports a pending request until the idle has moved the outer adjustment and the bin's next allocation has re-sliced from it, and reports none afterwards
- **AND** polled right after the idle, before any frame, it still reports the request pending (the adoption-lab test failed first against the accessor that ended with the idle)

#### Scenario: A resting bin reports no pending request
- **WHEN** the bin has been re-allocated repeatedly with no input
- **THEN** the accessor reports no pending request and `allocation_count()` is the only counter that grows
