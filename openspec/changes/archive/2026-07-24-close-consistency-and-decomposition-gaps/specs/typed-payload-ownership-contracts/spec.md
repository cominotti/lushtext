## ADDED Requirements

### Requirement: Buffer replacement requests pair body and cancellation kinds by construction
Buffer-replacement request types SHALL make the pairing of body kind and
cancellation-callback kind correct by construction: a guarded (disposal-owned)
cancellation callback MUST NOT be pairable with a plain body, and a plain
cancellation callback MUST NOT be pairable with a guarded body. Consumers of
the request MUST NOT need `unreachable!`, `panic!`, or assert-style arms to
reject a mismatched pairing, and default/`mem::take` placeholder values MUST
remain representable without weakening the pairing guarantee.

#### Scenario: Mismatched pairing does not compile
- **WHEN** a caller constructs a buffer-replacement request with a guarded
  cancellation callback and a plain body, or the reverse
- **THEN** the construction is rejected at compile time by the request's
  type shape
- **AND** no runtime panic arm exists for the mismatch in the replacement
  session

#### Scenario: Session teardown matches exhaustively
- **WHEN** a replacement session is cancelled or superseded and its body and
  cancellation callback are consumed
- **THEN** the session matches only legal pairings exhaustively
- **AND** existing guarded-disposal routing, plain-body handling, and
  `mem::take` placeholder behavior are unchanged

## MODIFIED Requirements

### Requirement: Scoped byte-admission charges release by ownership
Bounded loading loops and traversal ledgers that reserve construction or
scratch bytes against an admission budget SHALL release each reservation
through a scope-owned guard (or equivalent ownership-based mechanism) rather
than through manual release calls on every exit path. This applies to every
palette admission ledger, including the note-source construction budget and
the file-index build ledger's scratch and installed accounting. Any early
exit — item filtered out, `continue`, early `return`, budget rejection, or
error propagation — MUST release exactly the bytes that were charged,
exactly once.

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

#### Scenario: File-index scratch charges release by scope
- **WHEN** file-index traversal charges scratch or installed bytes for a
  directory batch, raw path, or canonical identity and then rejects,
  truncates, supersedes, or errors out of that work
- **THEN** the reservation is released through the same scope-owned guard
  mechanism used by note-source loops
- **AND** no exit path in the traversal depends on a hand-placed release
  call to keep the ledger's accounting exact

#### Scenario: Residual manual releases are retired
- **WHEN** the palette note-source path settles canonical-folder or
  live-identity byte charges
- **THEN** those releases flow through scope ownership rather than paired
  manual calls
- **AND** documented direct settlement points (such as a parse reservation
  consumed into admission) state their invariant at the settlement site
