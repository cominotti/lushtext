## ADDED Requirements

### Requirement: Generation-counter debounce primitive
`gtk-lush-settle` SHALL provide a debounce primitive that coalesces bursts of
events into one trailing callback on the GLib main context: each schedule
bumps a generation, the callback fires only when its captured generation is
still current, and callbacks capture their target through weak references so a
destroyed target cancels silently. The pure generation/decision logic SHALL be
separated from GLib scheduling so it is unit- and property-testable without a
main loop.

#### Scenario: Burst coalesces to one callback
- **WHEN** schedule is called five times within the debounce window
- **THEN** the callback runs exactly once, after the window measured from the
  last schedule

#### Scenario: Superseded generation no-ops
- **WHEN** a scheduled callback fires after a newer schedule has bumped the
  generation
- **THEN** the stale callback returns without side effects

#### Scenario: Dead target cancels
- **WHEN** the weak target is dropped before the debounce fires
- **THEN** the callback does not run and no panic or warning occurs

### Requirement: Settle-burst primitive with queryable pending state
The crate SHALL provide a settle-burst primitive modeling "a storm of events
that must be repaired once after it ends": opening or extending a burst
restarts the settle window, exactly one on-settle action runs when the window
elapses without extension, and a `pending()` query reports whether a burst is
open so consumers can integrate it with readiness or idle predicates. The
on-settle action and the pending transition MUST occur in the same main-loop
dispatch so observers cannot see a settled-but-unrepaired state.

#### Scenario: Extension restarts the window
- **WHEN** a burst is extended before its settle window elapses
- **THEN** the on-settle action runs only after a full window passes without
  further extension

#### Scenario: Pending state brackets the burst
- **WHEN** a burst opens and later settles
- **THEN** `pending()` is true from open until the on-settle action completes
  and false immediately after, within one dispatch

### Requirement: Superseding timer primitive
The crate SHALL provide a superseding one-shot timer for auto-dismiss flows:
re-arming replaces the previous deadline without `SourceId` bookkeeping, and a
fired timer whose arm-generation is stale performs no action.

#### Scenario: Re-armed dismissal supersedes
- **WHEN** an auto-dismiss timer is re-armed before firing
- **THEN** only the latest deadline dismisses, and the earlier timer firing is
  a no-op

### Requirement: LushText migration with semantics preserved
LushText SHALL replace its hand-rolled generation-counter sites — including
the minimap refresh debounce and reflow settle, status-bar message
auto-dismiss, sidebar tree loading, draft scheduling, search scheduling, and
focus indexing — with the crate's primitives, preserving each site's existing
windows and observable semantics. Readiness integration MUST be preserved:
predicates that today read pending flags (for example the minimap work
pending blocker) read the primitive's `pending()` state with identical
blocker behavior, verified by the automation contract checks.

#### Scenario: Minimap settle behavior unchanged
- **WHEN** the minimap reflow settle migrates to the settle-burst primitive
- **THEN** the sidebar-toggle widget tests, the automation idle/readiness
  tests, and the visual-geometry minimap scenarios pass unchanged

#### Scenario: Hand-rolled counters are deleted
- **WHEN** the migration completes
- **THEN** no `ui/` module outside the crate re-implements generation-counter
  debounce, and the corresponding rules section points at the crate
