## MODIFIED Requirements

### Requirement: Slice geometry is a pure, property-tested policy
The slice decision SHALL be a GTK-free pure function in `gtk-lush-widgets` that maps (outer viewport top relative to the container, outer viewport height, content height, overscan) to (slice top, slice height). It MUST be covered by unit tests and bounded property tests. The band SHALL NOT exceed the height the outer viewport actually leaves for the content after chrome drawn above it, because the child decides row visibility from its own allocation. The band SHALL NOT be reduced at the content's bottom edge, where it would collapse to zero height.

#### Scenario: Slice always lies inside the content
- **WHEN** any non-negative viewport top, viewport height, content height, and overscan are supplied
- **THEN** slice top is at least zero and slice top plus slice height does not exceed content height

#### Scenario: Slice covers the visible intersection
- **WHEN** the viewport band intersects the content
- **THEN** the slice contains the whole intersection

#### Scenario: Slice is stable when the viewport does not move
- **WHEN** the same inputs are supplied twice
- **THEN** the same slice is returned, and a viewport move smaller than the overscan margin returns a slice that still covers the new intersection

#### Scenario: Chrome above the content is deducted from the band
- **WHEN** the content starts below the outer viewport's top edge, so part of the viewport is occupied by chrome the host draws above it
- **THEN** the returned band is shorter than the viewport by that occupied height, so a child that places a row at its own bottom edge places it at the fold rather than behind the chrome

#### Scenario: A content end above the viewport bottom keeps a full band
- **WHEN** the content's end lies above the outer viewport's bottom edge, or the content has scrolled entirely past the viewport
- **THEN** the returned band still spans a full viewport height clamped to the content, never zero height

### Requirement: Adjustment synchronization is bidirectional and loop-free
The container SHALL re-slice when the outer vertical adjustment changes value or page size, and SHALL translate child-originated adjustment changes (keyboard focus moves, `scroll_to`) into outer adjustment changes so the focused row enters the outer viewport. Changes the container itself makes to the child adjustment MUST NOT be re-translated to the outer adjustment. The container SHALL recognise its own changes by comparing against the offset it published, not against the outer viewport's unclamped top edge, so that a container resting anywhere other than the viewport's top edge reports no request. The decision SHALL be a GTK-free pure function covered by unit and property tests.

#### Scenario: Outer scroll moves the slice
- **WHEN** the outer adjustment value changes
- **THEN** the container queues one allocation and the child adjustment value tracks the new slice offset

#### Scenario: Child scroll-to drives the outer scroller
- **WHEN** the child calls `scroll_to` for a row below the current slice
- **THEN** the outer adjustment value changes so that row lies inside the outer viewport, and the child adjustment is then re-derived from the outer value

#### Scenario: No re-entrant feedback
- **WHEN** the container sets the child adjustment during its own allocation
- **THEN** the resulting `value-changed` emission does not modify the outer adjustment

#### Scenario: A container below other content leaves the outer scroller alone
- **WHEN** other content in the same outer scroller is drawn above the container, so the published slice offset is clamped to zero while the outer viewport's top edge lies above the container's content
- **THEN** the container reports no scroll request, the outer scroller stays where the user left it, and that content above remains visible

#### Scenario: The user can scroll back to the top
- **WHEN** the outer scroller is returned to its top after being scrolled away
- **THEN** it stays there across subsequent allocations instead of returning to the container's first row

#### Scenario: Two containers in one scroller do not oscillate
- **WHEN** two containers share one outer scroller and it is scrolled to the end of the content
- **THEN** the scroll position settles at that end instead of alternating between positions requested by each container
