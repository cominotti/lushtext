# typed-payload-ownership-contracts Specification

## Purpose
Define type-shape contracts for cross-thread payload wrappers, service results,
and byte-admission charges so ownership rules are enforced by construction
instead of by panic arms, dead passenger fields, or manual release calls.

## Requirements
### Requirement: Guarded payload wrappers make illegal ownership states unrepresentable
Wrapper types that classify a cross-thread payload as either body-carrying
(guarded by transferable disposal ownership) or compact (no document-sized
body) SHALL use type shapes in which the compact side cannot represent a
body-carrying value. Consumers of such wrappers MUST NOT need `unreachable!`,
`panic!`, or assert-style arms to reject states the wrapper's own
classification already forbids.

#### Scenario: Compact draft-restore states carry no body variant
- **WHEN** a preloaded draft restore is classified as compact (stale-file
  skip, oversized skip, or lazy aggregate-budget skip) for transfer to GTK
- **THEN** the compact value's type has no case that could carry a document
  body
- **AND** the GTK-side consumers match exhaustively without an
  `unreachable!` arm for a body-carrying compact value

#### Scenario: Body-carrying restores require disposal ownership
- **WHEN** a preloaded draft restore carries an eager document body across
  the GTK boundary
- **THEN** the body is representable only inside the guarded wrapper's
  body-carrying case, whose payload type is the disposal-owned body
- **AND** constructing a body-carrying value without disposal ownership does
  not compile

#### Scenario: Adding a consumer cannot reintroduce the escape hatch
- **WHEN** a new consumer matches on the guarded restore wrapper
- **THEN** exhaustive matching over the wrapper's cases covers every legal
  state without a catch-all or panic arm

### Requirement: Service results do not carry dead document-sized passenger fields
When a service result's document-sized field is by construction always empty
on one side of an ownership boundary, the boundary SHALL split the result
into a metadata value and a separately owned content value instead of
shipping the full result with an empty passenger. Consumers MUST receive the
metadata type directly and MUST NOT re-assert emptiness at destructuring.

#### Scenario: Guarded load results carry metadata plus owned content
- **WHEN** a completed editor load crosses to GTK under disposal ownership
- **THEN** the GTK-side result holds the load metadata value and the
  disposal-owned content as separate fields
- **AND** no consumer destructures an always-empty content string or guards
  it with a debug assertion

#### Scenario: Service callers choose metadata or full content explicitly
- **WHEN** a caller invokes the editor load service
- **THEN** the returned type makes clear at the signature level whether the
  caller receives metadata only or metadata plus content
- **AND** result-equivalence tests compare metadata and content through the
  split shape without reconstructing a combined struct

### Requirement: Scoped byte-admission charges release by ownership
Bounded loading loops that reserve construction bytes against an admission
budget SHALL release each reservation through a scope-owned guard (or
equivalent ownership-based mechanism) rather than through manual release
calls on every exit path. Any early exit — item filtered out, `continue`,
early `return`, or error propagation — MUST release exactly the bytes that
were charged, exactly once.

#### Scenario: Filtered sidecar releases its construction charge
- **WHEN** a palette note-source loop charges construction bytes for a
  parsed sidecar and then filters the item out before admitting an entry
- **THEN** the charge is released when the item's scope ends
- **AND** no manual release call is required on that exit path

#### Scenario: New early exit cannot leak a charge
- **WHEN** a maintainer adds a new `continue` or early `return` to a bounded
  sidecar loop after the construction charge is taken
- **THEN** the scope-owned guard still releases the charge
- **AND** the admission budget's construction accounting returns to its
  pre-item level

#### Scenario: Admitted entries settle their charge exactly once
- **WHEN** an item is admitted and its retained bytes transfer to the
  admission's retained accounting
- **THEN** the construction charge is settled exactly once through the
  guard's consume path
- **AND** double release is not representable through the guard's API
