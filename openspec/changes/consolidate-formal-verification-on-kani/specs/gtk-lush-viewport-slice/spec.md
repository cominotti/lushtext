## MODIFIED Requirements

### Requirement: Slice geometry is a pure, property-tested policy
The slice decision SHALL be a GTK-free pure function in `gtk-lush-widgets` that maps (outer viewport top relative to the container, outer viewport height, content height, overscan) to (slice top, slice height). It MUST be covered by unit tests and bounded property tests, and its safety properties MUST be proved by Kani harnesses. The function MUST NOT panic for any `f64` input. Containment and exact request landing are guaranteed on whole-pixel inputs (integer values in the `i32` range, and overscan and reconfiguration shift within stated bounds). The rustdoc and this requirement state that domain; they do not claim the properties for arbitrary `f64`.

#### Scenario: Slice always lies inside the content
- **WHEN** any whole-pixel non-negative viewport top, viewport height, content height, and overscan are supplied
- **THEN** slice top is at least zero and slice top plus slice height does not exceed content height

#### Scenario: Slice covers the visible intersection
- **WHEN** the viewport band intersects the content
- **THEN** the slice contains the whole intersection

#### Scenario: Slice is stable when the viewport does not move
- **WHEN** the same inputs are supplied twice
- **THEN** the same slice is returned, and a viewport move smaller than the overscan margin returns a slice that still covers the new intersection

#### Scenario: No input panics
- **WHEN** any `f64` values are supplied, including NaN, infinities, and negatives
- **THEN** the slice and request decisions return without panicking, as proved by Kani

#### Scenario: A honoured request lands exactly
- **WHEN** a request is forwarded for whole-pixel published offset, child value, and viewport top
- **THEN** viewport top plus the forwarded delta equals the child value exactly, as proved by Kani

## ADDED Requirements

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

#### Scenario: Resting loop reaches a fixed point
- **WHEN** no external input occurs for any admissible child behaviour
- **THEN** the model shows the outer value, published offsets, and child values stop changing within two allocations

#### Scenario: The learning-frame residual is decided
- **WHEN** the model explores a request applied in the first allocation after inset learning
- **THEN** the result either proves the request is honoured or produces a counterexample, and the counterexample is fixed or recorded with a ledger-cited justification
