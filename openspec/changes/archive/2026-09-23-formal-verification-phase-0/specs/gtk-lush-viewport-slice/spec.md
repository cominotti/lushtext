## MODIFIED Requirements

### Requirement: Adjustment synchronization is bidirectional and loop-free
The container SHALL re-slice when the outer vertical adjustment changes value or page size, and SHALL translate child-originated adjustment changes (keyboard focus moves, `scroll_to`) into outer adjustment changes so the focused row enters the outer viewport. Changes the container itself makes to the child adjustment MUST NOT be re-translated to the outer adjustment. The container SHALL recognise its own changes by comparing against the offset it published, not against the outer viewport's unclamped top edge, so that a container resting anywhere other than the viewport's top edge reports no request. A value the child settles on within a geometry correction the container's own publish provoked SHALL NOT be treated as a request in that allocation. When the container cannot tell a settle from a request because both fall within that correction, it SHALL NOT erase the divergence. It SHALL re-decide in a following allocation with stable geometry, so that a genuine child request is eventually honoured or proven to be a settle, and is never silently dropped. The decisions SHALL be GTK-free pure functions covered by unit and property tests.

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

#### Scenario: A request made while the geometry reconfigures is not dropped
- **WHEN** the child requests a row through `scroll_to` in the same allocation in which it reconfigures the adjustment's upper or page size, and the requested value lies within that correction of the published offset
- **THEN** the requested row ends fully inside the outer viewport once allocations stop
- **AND** the container still reaches rest with its allocation and correction counts no longer growing
