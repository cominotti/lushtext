## ADDED Requirements

### Requirement: Viewport slice container hosts a scrollable child at full natural height
`gtk-lush-widgets` SHALL provide a single-child container (working name `ViewportSliceBin`, template type `GtkLushViewportSliceBin`) for a `GtkScrollable` child placed inside an outer scroller. The container MUST report the child's full content height as its own natural height so the outer scroller's range is exact, and MUST allocate the child only the band that intersects the outer viewport plus a bounded overscan, driving the child's vertical adjustment so the child realizes only that band.

#### Scenario: Natural height equals content height
- **WHEN** the container hosts a `GtkListView` whose measured natural height is 11,486 px
- **THEN** the container's vertical natural height is 11,486 px and its minimum height is the child's minimum height

#### Scenario: Child allocation is the visible slice
- **WHEN** the outer viewport is 665 px tall, its value is 8,000 px, the container starts 55 px below the outer content origin, and the overscan is one viewport
- **THEN** the child is allocated a height no greater than 665 px plus two overscans, positioned so it covers the outer viewport band, and its vertical adjustment value equals the child's offset within the container

#### Scenario: Content shorter than the viewport
- **WHEN** the child's content height is smaller than the outer viewport height
- **THEN** the child is allocated its full content height at offset zero and its adjustment value is zero

### Requirement: Slice geometry is a pure, property-tested policy
The slice decision SHALL be a GTK-free pure function in `gtk-lush-widgets` that maps (outer viewport top relative to the container, outer viewport height, content height, overscan) to (slice top, slice height). It MUST be covered by unit tests and bounded property tests.

#### Scenario: Slice always lies inside the content
- **WHEN** any non-negative viewport top, viewport height, content height, and overscan are supplied
- **THEN** slice top is at least zero and slice top plus slice height does not exceed content height

#### Scenario: Slice covers the visible intersection
- **WHEN** the viewport band intersects the content
- **THEN** the slice contains the whole intersection

#### Scenario: Slice is stable when the viewport does not move
- **WHEN** the same inputs are supplied twice
- **THEN** the same slice is returned, and a viewport move smaller than the overscan margin returns a slice that still covers the new intersection

### Requirement: Adjustment synchronization is bidirectional and loop-free
The container SHALL re-slice when the outer vertical adjustment changes value or page size, and SHALL translate child-originated adjustment changes (keyboard focus moves, `scroll_to`) into outer adjustment changes so the focused row enters the outer viewport. Changes the container itself makes to the child adjustment MUST NOT be re-translated to the outer adjustment.

#### Scenario: Outer scroll moves the slice
- **WHEN** the outer adjustment value changes
- **THEN** the container queues one allocation and the child adjustment value tracks the new slice offset

#### Scenario: Child scroll-to drives the outer scroller
- **WHEN** the child calls `scroll_to` for a row below the current slice
- **THEN** the outer adjustment value changes so that row lies inside the outer viewport, and the child adjustment is then re-derived from the outer value

#### Scenario: No re-entrant feedback
- **WHEN** the container sets the child adjustment during its own allocation
- **THEN** the resulting `value-changed` emission does not modify the outer adjustment

### Requirement: Viewport slice container is governed like other GTK Lush widgets
The widget SHALL ship with a doc comment, a README section, a CHANGELOG entry, a public-API snapshot update, a proof-harness example under `crates/gtk-lush/widgets/examples`, and adoption-lab evidence, and `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-doctests`, and `make gtk-lush-examples` MUST pass.

#### Scenario: Governance gates
- **WHEN** the widget is added
- **THEN** the GTK Lush policy, adoption, doctest, example, and public-API advisory targets pass without new waivers
