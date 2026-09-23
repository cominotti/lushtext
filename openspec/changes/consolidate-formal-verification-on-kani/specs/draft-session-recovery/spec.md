## ADDED Requirements

### Requirement: Draft journal decisions live in a pure, verified state machine
The system SHALL express the draft journal's ordering and ownership decisions
as a GTK-free, I/O-free state machine that `draft_service` and the GTK drafts
coordination both drive. Those decisions include registration before a body
write, body ownership, body-then-entry deletion, stale-body preservation,
reconciliation authority, and cleanup eligibility. Kani harnesses SHALL check,
over bounded action sequences that include crashes and restarts:

- **acceptance durability (S1):** an accepted generation stays recoverable
  until discard, clean save, or a stale file;
- **cleanup safety (S2):** a body is deleted only when it is unreferenced,
  its identity has been revalidated, the inventory is trusted, and no write is
  in flight;
- **delete ordering (S3):** delete intent never coexists with a present body
  and a missing manifest entry;
- **trust (S4):** a trusted manifest implies that the last reconciliation was
  complete;
- **bounded liveness (L1):** without I/O faults, a dirty open editor becomes
  clean within a stated number of steps.

Every body-ownership decision MUST go through one ownership check, and a draft
body MUST NOT be written unless the service can prove that a manifest entry
for it exists.

#### Scenario: Invariants hold under crashes
- **WHEN** the Kani harness explores action sequences within its stated bounds, including crashes at every step
- **THEN** S1–S4 hold in every reachable state

#### Scenario: Production drives the verified machine
- **WHEN** the service or the GTK coordination decides whether to write, preserve, delete, or trust
- **THEN** that decision comes from the verified state machine rather than from duplicated inline logic

### Requirement: Preserved set-aside drafts have a recovery surface
The system SHALL list the drafts preserved in the set-aside location on the
`Preferences > Data` page. Each row SHALL show the original path when known
and the preservation time, and SHALL offer Open (as a new untitled tab) and
Delete (with confirmation) actions. The group MUST NOT be shown when the
set-aside location is empty. The rows SHALL follow the grouped-row
readability and accessibility rules.

#### Scenario: Open a set-aside draft
- **WHEN** the user activates Open on a set-aside row
- **THEN** its content opens in a new untitled tab and the set-aside copy remains until deleted

#### Scenario: Delete a set-aside draft
- **WHEN** the user confirms Delete on a set-aside row
- **THEN** the set-aside body is removed durably and the row disappears

#### Scenario: Nothing set aside
- **WHEN** the set-aside location is empty
- **THEN** the Data page shows no set-aside group
