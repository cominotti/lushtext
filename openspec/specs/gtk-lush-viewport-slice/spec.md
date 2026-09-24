# gtk-lush-viewport-slice Specification

## Purpose
Specify the GTK Lush `ViewportSliceBin` container that hosts a `GtkScrollable` child at full natural height inside an outer scroller while allocating only the visible slice, its pure property-tested slice geometry, its two-way adjustment synchronization, and its governance as a `gtk-lush-widgets` API.
## Requirements
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
The slice decision SHALL be a GTK-free pure function in `gtk-lush-widgets` that maps (outer viewport top relative to the container, outer viewport height, content height, overscan) to (slice top, slice height). It MUST be covered by unit tests and bounded property tests, and its safety properties MUST be proved by Kani harnesses. The function MUST NOT panic for any `f64` input. Containment and exact request landing are guaranteed on whole-pixel inputs (integer values in the `i32` range, and overscan and reconfiguration shift within stated bounds). The rustdoc and this requirement state that domain; they do not claim the properties for arbitrary `f64`. The band SHALL NOT exceed the height the outer viewport actually leaves for the content after chrome drawn above it, because the child decides row visibility from its own allocation. The band SHALL NOT be reduced at the content's bottom edge, where it would collapse to zero height.

#### Scenario: Slice always lies inside the content
- **WHEN** any whole-pixel non-negative viewport top, viewport height, content height, and overscan are supplied
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

#### Scenario: No input panics
- **WHEN** any `f64` values are supplied, including NaN, infinities, and negatives
- **THEN** the slice and request decisions return without panicking, as proved by Kani

#### Scenario: A honoured request lands exactly
- **WHEN** a request is forwarded for whole-pixel published offset, child value, and viewport top
- **THEN** viewport top plus the forwarded delta equals the child value exactly, as proved by Kani

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

#### Scenario: A container below other content leaves the outer scroller alone
- **WHEN** other content in the same outer scroller is drawn above the container, so the published slice offset is clamped to zero while the outer viewport's top edge lies above the container's content
- **THEN** the container reports no scroll request, the outer scroller stays where the user left it, and that content above remains visible

#### Scenario: The user can scroll back to the top
- **WHEN** the outer scroller is returned to its top after being scrolled away
- **THEN** it stays there across subsequent allocations instead of returning to the container's first row

#### Scenario: Two containers in one scroller do not oscillate
- **WHEN** two containers share one outer scroller and it is scrolled to the end of the content
- **THEN** the scroll position settles at that end instead of alternating between positions requested by each container

#### Scenario: A geometry settle is not a request
- **WHEN** the child reconfigures the adjustment's upper or page size during its allocation and its value settles within that correction of the published offset
- **THEN** the outer adjustment is not moved

#### Scenario: A request made while the geometry reconfigures is not dropped
- **WHEN** the child requests a row through `scroll_to` in the same allocation in which it reconfigures the adjustment's upper or page size, and the requested value lies within that correction of the published offset
- **THEN** the requested row ends fully inside the outer viewport once allocations stop
- **AND** the container still reaches rest with its allocation and correction counts no longer growing

### Requirement: Viewport slice container is governed like other GTK Lush widgets
The widget SHALL ship with a doc comment, a README section, a CHANGELOG entry, a public-API snapshot update, a proof-harness example under `crates/gtk-lush/widgets/examples`, and adoption-lab evidence, and `make check-gtk-lush-policy`, `make check-gtk-lush-adoption`, `make gtk-lush-doctests`, and `make gtk-lush-examples` MUST pass.

#### Scenario: Governance gates
- **WHEN** the widget is added
- **THEN** the GTK Lush policy, adoption, doctest, example, and public-API advisory targets pass without new waivers

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

### Requirement: The slice-bin feedback loop is verified against the axiom ledger
The project SHALL keep a Kani-checked step model of the `ViewportSliceBin`
feedback loop. It SHALL call the real `viewport_slice` and
`classify_child_scroll`, and model the child as a nondeterministic reaction
constrained only by ledger axioms. For one to three bins over a bounded number
of allocations, the model SHALL prove:

- the loop reaches rest within two allocations when there is no input;
- bins never oscillate;
- a honoured request lands where the re-slice republishes the child's value;
- every request beyond epsilon is honoured or clamped within the bound;
- at rest, the drawn row position equals the intended position.

These properties are proved within the envelope the ledger currently pins.
Axiom A8 says a settle does not survive into a stable-geometry frame. A case
that falls outside that envelope and yields a counterexample SHALL be kept as
a named `#[kani::should_panic]` residual harness. The programme record SHALL
state the residual, the evidence on whether real consumers can reach it, and a
candidate fix. A residual MUST be fixed, starting from a failing real-GTK
test, as soon as any consumer can reach it. Two such residuals are known: the
learning-frame request, and two consecutive reconfiguring allocations with
settles.

#### Scenario: Resting loop reaches a fixed point
- **WHEN** no external input occurs for any admissible child behaviour
- **THEN** the model shows the outer value, published offsets, and child values stop changing within two allocations

#### Scenario: A residual becomes reachable
- **WHEN** a consumer, such as a variable-height list, is shown to reach a recorded residual through real GTK
- **THEN** a failing widget test is written first, the fix is applied, and the residual harness becomes a proof

#### Scenario: The learning-frame residual is decided
- **WHEN** the model explores a request applied in the first allocation after inset learning
- **THEN** the result either proves the request is honoured or produces a counterexample, and the counterexample is fixed or recorded with a ledger-cited justification
