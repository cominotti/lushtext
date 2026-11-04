## ADDED Requirements

### Requirement: The lane measures a switch to a never-shown large tab
`eviction/reselect` SHALL end with a `switch-hidden` interaction phase: select
a tab the run never selected before, wait for `file-open-complete`, and scroll
it, recording the phase's stalls and input-to-frame latencies like any other
interaction phase.

#### Scenario: The switch is measured
- **WHEN** `eviction/reselect` runs and its earlier phases succeeded
- **THEN** the row carries a `switch-hidden` phase of kind `interaction`

### Requirement: The lane switches to never-shown giant and dense documents
The lane SHALL run `eviction/switch-hidden-giant` (a 1.9M-character paragraph)
and `eviction/switch-hidden-dense` (an 11 MiB document of 2,000-character
lines): the large document opened first and a small one selected after it, a
background phase, then a `switch-hidden` phase that selects the large document
for its first layout and scrolls it.

#### Scenario: The never-shown documents are measured
- **WHEN** either case runs
- **THEN** its row carries `background` and `switch-hidden` phases

### Requirement: A wheel is credited only with a frame after it moved the view
The probe SHALL record an `adjustment_value` mark on every change of an editor
view's vertical adjustment value, and the lane SHALL report a wheel after
which no such mark precedes the frame it would be credited with as
`no-visible-change`, not as that frame's latency. A log without the mark SHALL
keep the earlier attribution.

#### Scenario: A wheel at the end of a document
- **WHEN** a wheel moves no view and a frame follows later for another reason
- **THEN** the wheel is reported as `no-visible-change`

### Requirement: The lane measures a hidden laid-out view's detach
The lane SHALL run `eviction/detach` and `eviction/detach-minimap`: one 8 MiB
document of short lines shown (with the minimap off and on), then a small
document selected and a zoom step, recording the `view_detach` span of the
hidden large view in a `detach` phase.

#### Scenario: The detach is measured with and without a minimap
- **WHEN** either case runs
- **THEN** its probe log carries the hidden view's `view_detach` span
