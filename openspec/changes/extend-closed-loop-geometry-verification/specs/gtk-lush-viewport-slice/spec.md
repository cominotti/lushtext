## ADDED Requirements

### Requirement: Simultaneous requests from several bins do not add up
When several `ViewportSliceBin` containers share one outer scroller and more
than one of them forwards a child request before the outer scroller has
moved, the container SHALL NOT apply the sum of their deltas.

Each forwarded request SHALL be anchored to the outer value its delta was
measured against. Applying requests SHALL be idempotent with respect to that
anchor, so the outer scroller lands either:

- where the last-applied request's re-slice republishes exactly that child's
  value; or
- at an end of the outer range (clamped).

Requests accumulated by one bin before its idle runs SHALL keep their existing
accumulated-delta semantics. A request whose outer anchor is superseded SHALL
end quietly: its child's pending `scroll_to` is dropped by the re-slice (ledger
A9), and it SHALL NOT be re-issued by the container. The loop SHALL reach rest,
and the bins SHALL NOT oscillate.

The property SHALL be decided by a Kani harness over the slice-loop model in
which two bins request in the same frame. A counterexample SHALL be reproduced
by a failing real-GTK widget test before the fix is applied, and the harness
SHALL then pass as a proof.

#### Scenario: Two bins request in the same frame
- **WHEN** two bins in one outer scroller each receive a child `scroll_to` request that is applied inside the same frame, before either forwarding idle has run
- **THEN** once allocations stop, the outer value equals the value the last-applied request asked for, or an end of the outer range, and never the sum of both deltas
- **AND** the requested row of the last-applied request lies fully inside the outer viewport

#### Scenario: One bin's accumulated requests are unchanged
- **WHEN** a single bin forwards several requests before its idle runs and no other bin forwards a request
- **THEN** the outer scroller moves by the accumulated delta exactly as before this change, and the keyboard-traversal and rendered-bounds widget tests still pass

#### Scenario: The superseded request comes to rest
- **WHEN** a bin's forwarded request is superseded by another bin's request against the same outer anchor
- **THEN** the superseded bin re-slices from the new outer value, reports no further request, and its allocation and correction counts stop growing

### Requirement: Slice-loop claims are extended beyond the bounded scope in Kani
The project SHALL attempt to extend the slice-bin loop's rest, no-oscillation
and request-fidelity claims beyond one to three bins and a bounded number of
allocations, using Kani only. There are two attempts.

The first attempt SHALL use Kani loop contracts: a loop invariant and, where
termination is claimed, a decreases clause. It SHALL target an arbitrary number
of allocations.

The second attempt SHALL use a compositional bin-independence argument. It has
three parts:

- harnesses proving that one bin's allocation step reads only its own state,
  the shared outer value and its own origin, and never writes another bin's
  state;
- a harness proving that the outer value is written only by the forwarding
  idle;
- one-bin harnesses quantified over any origin and any outer jump, plus the
  pairwise simultaneous-request harness.

A short proof note SHALL state how these harnesses together cover any number
of bins.

Each attempt's outcome SHALL be recorded in the programme record: proved,
proved with a stated domain, or failed with the Kani limitation or
counterexample that stopped it. An attempt that fails SHALL NOT be presented as
evidence for an unbounded claim.

#### Scenario: Loop-contract attempt
- **WHEN** the loop-contract harness runs with Kani's loop-contract feature enabled
- **THEN** it either proves the invariant for an arbitrary number of allocations or its failure and cause are recorded in the programme record

#### Scenario: Independence argument covers any number of bins
- **WHEN** the non-interference, outer-writer, quantified one-bin and pairwise harnesses all pass
- **THEN** the programme record's proof note states the any-N rest and no-oscillation claim and names each harness it rests on

#### Scenario: A failed attempt is not overstated
- **WHEN** an attempt does not verify
- **THEN** no rustdoc, spec, README or programme-record text claims the unbounded property on its strength

### Requirement: A viewport height change never scrolls the outer scroller
After the outer scroller has moved, a change of the outer viewport's height (a
window resize or maximize) SHALL NOT move the outer scroller. The bin SHALL
announce a value it moved the child to once more after the child's allocation,
so the child anchors inside its view (ledger A20), and SHALL write back, never
forward, a divergence the child shows in an allocation whose publish emitted
`value-changed` (ledger A9: no request can follow it) or in which the bin
changed the page or content height under an anchor the bin set (ledger A7).
The decision SHALL be a crate-private pure function beside
`classify_child_scroll`, whose public contract is unchanged. A child request
applied in the very allocation of such a page change is erased; this residual
SHALL be pinned by a `should_panic` Kani harness and recorded.

#### Scenario: Maximize after an outer scroll
- **WHEN** the outer scroller has been scrolled and the window is then resized or maximized
- **THEN** the outer value and the row at the top of the viewport are unchanged once allocations stop

#### Scenario: The pre-fix rule is pinned
- **WHEN** the Kani slice loop classifies with the magnitude rule alone after a viewport height change
- **THEN** a harness finds the outer scroller moved, and it is kept as a `should_panic` counterexample

### Requirement: The band is what the viewport shows
The band a bin allocates its child SHALL be the part of its content the outer
viewport shows, deducted at the bottom edge as at the top, plus any overscan.
Content entirely off screen SHALL keep a band one pixel plus the child's
learned inset tall at its nearest edge, never zero (ledger A5). A request for a
row of a bin the viewport has scrolled past, other than the row covering that
pixel (a recorded residual), SHALL be honoured.

#### Scenario: A request in a scrolled-past bin
- **WHEN** a child `scroll_to`s a row near the end of a bin the viewport has scrolled past
- **THEN** the outer scroller brings that row on screen and comes to rest

### Requirement: An outer page change re-slices outside layout
The bin SHALL NOT queue its allocation from the outer adjustment's
`notify::page-size`, which `GtkViewport` delivers after it has allocated its
child (ledger A19); it SHALL re-slice from an idle, so GTK never draws the bin
with an allocation pending.

#### Scenario: No stale allocation on maximize
- **WHEN** the window is maximized or unmaximized while a sidebar with slice bins is scrolled
- **THEN** GTK prints no `Trying to snapshot ... without a current allocation` warning for a slice bin
