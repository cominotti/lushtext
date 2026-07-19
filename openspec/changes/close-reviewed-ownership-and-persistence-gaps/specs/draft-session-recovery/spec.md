## ADDED Requirements

### Requirement: Draft restore bodies preserve transferable disposal ownership
Startup eager preloads and serialized lazy draft reads SHALL represent every admitted recovery body with individually transferable plain-disposal ownership before that body can be extracted or published to GTK. The guard MUST enter the bounded buffer replacement request, MUST be returned on every terminal outcome, and MUST transfer into an eligible restored local-history baseline without cloning or unguarded wrapping. Ineligible, stale, cancelled, unused, or teardown bodies MUST retire on the admitted disposal worker while the original draft evidence remains governed by the existing recovery ticket and manifest rules.

#### Scenario: Eager preload body is extracted
- **WHEN** startup restore removes one eager recovery body from the preload collection for an editor
- **THEN** the body's disposal reservation moves with it rather than remaining attached only to the collection
- **AND** dropping the remaining preload map cannot leave the extracted body unguarded

#### Scenario: Lazy restore waits for progress capacity
- **WHEN** a queued lazy draft ticket is current but the recovery disposal lane cannot reserve the maximum automatic body bound
- **THEN** the workflow retains only the compact serialized ticket and waits for capacity
- **AND** it does not read or retain an unguarded recovery body while waiting

#### Scenario: Guarded replacement is superseded
- **WHEN** edit, load, manifest, path, or lifetime generation invalidates a guarded draft replacement
- **THEN** replacement returns or retires the same guard without publishing restored state
- **AND** the draft body and manifest remain available for a later eligible recovery attempt

#### Scenario: Restored body becomes local-history baseline
- **WHEN** incoming-size policy permits an accepted restored body to seed local history
- **THEN** the returned replacement guard becomes the baseline owner without a second full-body clone
- **AND** baseline replacement later uses bounded off-GTK disposal

#### Scenario: Restored body is ineligible for local history
- **WHEN** incoming-size policy forbids an automatic baseline for the accepted body
- **THEN** the implementation does not wrap that document-sized text with a small unreserved owner
- **AND** the guard retires off GTK after restore finalization no longer needs the source
