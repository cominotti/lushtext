## MODIFIED Requirements

### Requirement: Adjustment synchronization is bidirectional and loop-free
The container SHALL re-slice when the outer vertical adjustment changes value or page size, and SHALL translate child-originated adjustment changes (keyboard focus moves, `scroll_to`) into outer adjustment changes so the focused row enters the outer viewport. Changes the container itself makes to the child adjustment MUST NOT be re-translated to the outer adjustment. The container SHALL recognise its own changes by comparing against the offset it published, not against the outer viewport's unclamped top edge, so that a container resting anywhere other than the viewport's top edge reports no request. A value the child settles on within a geometry correction the container's own publish provoked SHALL NOT be treated as a request. The decisions SHALL be GTK-free pure functions covered by unit and property tests.

#### Scenario: Outer scroll moves the slice
- **WHEN** the outer adjustment value changes
- **THEN** the container queues one allocation and the child adjustment value tracks the new slice offset

#### Scenario: Child scroll-to drives the outer scroller
- **WHEN** the child calls `scroll_to` for a row below the current slice
- **THEN** the outer adjustment value changes so that row lies inside the outer viewport, and the child adjustment is then re-derived from the outer value

#### Scenario: A clipped row is still revealed and the scroller then rests
- **WHEN** keyboard focus moves to a row clipped by a few pixels at the slice edge
- **THEN** the outer adjustment moves so the row is fully inside the outer viewport and then does not move again; the outer lands where the re-slice republishes exactly the value the child chose, because the child drops a pending request whenever its adjustment value changes under it

#### Scenario: No re-entrant feedback
- **WHEN** the container sets the child adjustment during its own allocation
- **THEN** the resulting `value-changed` emission does not modify the outer adjustment

#### Scenario: A geometry settle is not a request
- **WHEN** the child reconfigures the adjustment's upper or page size during its allocation and its value settles within that correction of the published offset
- **THEN** the outer adjustment is not moved

## ADDED Requirements

### Requirement: Rendered rows are stable while the outer scroller rests
The container SHALL publish the child adjustment's upper and page size in the child's own CSS content-box frame, deriving the inset from what the child reports rather than from style queries, so that an ordinary allocation leaves the child nothing to overwrite; the value SHALL remain the slice offset, unadjusted. Between two allocations with the same content height, the difference between the offset the container allocates the child at and the child adjustment's value SHALL be unchanged, so every row's rendered bounds are pixel-identical. A value the child settles on that is neither the published offset nor a request SHALL be written back to the published offset within the same allocation. A correction the container applies SHALL provoke at most one further allocation and none once settled. The container SHALL expose its allocation and correction counts as test evidence and SHALL NOT assert the invariant with a panic inside allocation.

#### Scenario: Rows do not move across selection and focus changes
- **WHEN** the outer adjustment value is unchanged, the content height is unchanged, and selection or keyboard focus moves across rows that are already fully visible, at the top of the content or mid-content
- **THEN** a row's rendered bounds relative to a fixed sibling above the container are pixel-identical across every sample, with layout forced between samples

#### Scenario: Rows settle after a content change
- **WHEN** the outer adjustment value is unchanged and the host changes the child's model or content height
- **THEN** once the container's allocation count stops growing, a row's rendered bounds are pixel-identical across every further sample

#### Scenario: Ordinary allocations leave the published geometry standing
- **WHEN** the container allocates the child after the inset is known, with the content height unchanged
- **THEN** the child adjustment's upper and page size after the allocation equal the values the container published, and the published offset is unchanged by the allocation

#### Scenario: Corrections terminate
- **WHEN** the container is at rest after the inset is known, or an already visible row is selected
- **THEN** the container's allocation count does not grow across idle passes and its correction count does not grow
