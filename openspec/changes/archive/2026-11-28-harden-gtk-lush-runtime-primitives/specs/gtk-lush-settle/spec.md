# Delta Specification: gtk-lush-settle

## MODIFIED Requirements

### Requirement: Pure generation logic is testable without GTK scheduling
`gtk-lush-settle` SHALL separate deterministic generation, staleness, and pending-state logic from GLib timer installation so pure unit tests run without a live loop. Property tests MUST live behind an explicit `property-tests` Cargo feature so they run under the property lane rather than default non-widget or mutation runs.

#### Scenario: Stale-token logic runs as pure tests
- **WHEN** `cargo test -p gtk-lush-settle` runs without presenting a GTK window
- **THEN** tests prove that newer generations make older tokens stale
- **AND** stale tokens cannot pass the current-generation check

#### Scenario: Pending-state transitions are property-tested
- **WHEN** `cargo test -p gtk-lush-settle --features property-tests` generates sequences of open, extend, finish, clear, and stale-finish operations
- **THEN** the settle state machine never reports settled before the current repair is allowed to finish

### Requirement: Debounce primitive coalesces latest-generation work
`gtk-lush-settle` SHALL provide a `Debounce` primitive that advances generation, weakly captures its target, and removes any superseded GLib `SourceId` on `schedule`, `invalidate`, and `advance`. It MUST expose `is_armed(&self) -> bool` reporting whether a callback is currently pending.

#### Scenario: Burst coalesces to latest callback
- **WHEN** debounce work is scheduled repeatedly within the quiet window
- **THEN** earlier GLib sources are removed immediately and earlier tokens become stale
- **AND** only the latest scheduled callback runs after the final quiet window elapses

#### Scenario: Dead target cancels silently
- **WHEN** the weak GTK target is destroyed before a scheduled debounce fires
- **THEN** the callback does not run and `is_armed()` returns `false` after dispatch or `invalidate`
- **AND** no panic, stale widget access, or warning is emitted

#### Scenario: Immediate empty state can invalidate pending work
- **WHEN** a search-like surface clears its input and calls `advance()` or `invalidate()`
- **THEN** the superseded GLib `SourceId` is removed immediately, `is_armed()` becomes `false`, and older callbacks cannot run

### Requirement: Settle-burst primitive exposes readiness-safe pending state
`gtk-lush-settle` SHALL provide `SettleBurst` so opening, extending, or clearing a burst eagerly removes all armed primary and follow-up GLib `SourceId`s. Dropping the weak target before dispatch MUST clear `pending()` if the handle is still current, preventing permanently stuck readiness blockers.

#### Scenario: Extension restarts settle window
- **WHEN** a settle burst is scheduled and then extended before its quiet window elapses
- **THEN** the previous GLib source is removed immediately and the first settle callback is stale
- **AND** the repair callback runs only after a full quiet window passes after the latest extension

#### Scenario: Pending remains true through repair
- **WHEN** a current settle callback starts its repair work
- **THEN** `pending()` remains true until that repair has completed
- **AND** pending clears in the same ordered dispatch that completes the repair

#### Scenario: Stale settle handle cannot clear current pending
- **WHEN** an older settle handle tries to finish after a newer burst has opened
- **THEN** the older handle does not clear pending state for the newer burst

#### Scenario: Dropped target clears pending state
- **WHEN** the weak target of the current `SettleBurst` drops before the timer fires
- **THEN** `pending()` clears when that handle's source fires rather than remaining stuck `true` forever
- **AND** `clear()` removes every armed primary and follow-up `SourceId` immediately

### Requirement: Superseding one-shot primitive replaces delayed cleanup work
`gtk-lush-settle` SHALL provide `SupersedingTimer` for delayed one-shot cleanup or reveal actions where re-arming or invalidating removes the superseded GLib `SourceId` immediately. It MUST expose `is_armed(&self) -> bool` without requiring caller `SourceId` bookkeeping.

#### Scenario: Re-armed timer supersedes previous arm
- **WHEN** a delayed cleanup timer is armed and then re-armed before the first deadline
- **THEN** the first GLib source is removed immediately and its token is stale
- **AND** only the latest arm can perform cleanup

#### Scenario: Invalidation cancels side effects
- **WHEN** a superseding timer is invalidated before its scheduled callback fires
- **THEN** its `SourceId` is removed immediately, `is_armed()` returns `false`, and no callback runs
- **AND** the caller does not need to remove a GLib source manually

#### Scenario: Dropped target cancels cleanup
- **WHEN** a widget owning a delayed cleanup action is destroyed before the timer fires
- **THEN** the cleanup callback does not run
- **AND** no stale widget access occurs

### Requirement: Public API documentation and tests prove scheduling behavior
Every public item in `gtk-lush-settle` SHALL keep `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]`, and doc explanations distinguishing settle primitives from polling, yielding, and async freshness. Unit tests MUST pump GLib's `MainContext` to verify real timer firing, re-arm/clear cancellation, and dead-target cleanup.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public settle helper, token, handle, or state type is added without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Doctest demonstrates weak-target cancellation
- **WHEN** the debounce or timer docs are tested
- **THEN** at least one runnable example demonstrates that dropping the target before the callback fires prevents side effects

#### Scenario: README teaches the choosing rule
- **WHEN** the README is rendered
- **THEN** it tells consumers when to choose debounce, settle burst, superseding timer, or an explicit non-settle pattern
- **AND** it states the anti-framework constraints and pre-publication status

#### Scenario: Main-loop unit tests fire real callbacks
- **WHEN** `cargo test -p gtk-lush-settle` pumps `glib::MainContext`
- **THEN** tests observe `Debounce::schedule`, `SupersedingTimer::arm`, `SettleBurst::schedule`, and `SettleHandle::schedule_follow_up` firing, replacing sources, and clearing `pending()` on target drop
