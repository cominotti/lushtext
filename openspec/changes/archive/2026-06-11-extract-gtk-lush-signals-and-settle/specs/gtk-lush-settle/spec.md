## ADDED Requirements

### Requirement: Leaf GLib main-loop scheduling crate
`gtk-lush-settle` SHALL provide small GLib-main-loop scheduling helpers for
stock gtk-rs applications while remaining an independently adoptable GTK Lush
leaf crate. The crate MUST NOT depend on LushText crates, MUST NOT depend on
another GTK Lush family crate at runtime, MUST NOT create or own an executor,
runtime, component hierarchy, message loop, or widget lifecycle, and MUST NOT
replace ordinary GLib/GTK scheduling APIs. It SHALL react to GTK and GLib main
loop scheduling; GTK remains authoritative for rendering, allocation, and
event delivery.

#### Scenario: Standalone application adopts only settle
- **WHEN** `cargo test -p gtk-lush-settle --examples` builds the crate's
  standalone example
- **THEN** the example uses stock gtk-rs and `gtk-lush-settle`
- **AND** no other GTK Lush crate or LushText crate is required

#### Scenario: No custom runtime is introduced
- **WHEN** a consumer schedules debounce, settle-burst, or superseding-timer
  work through the crate
- **THEN** callbacks run through the GLib main context or GTK main loop
- **AND** the crate does not start a separate executor, worker runtime, or app
  control-flow loop

#### Scenario: Runtime family dependency is rejected
- **WHEN** `gtk-lush-settle` declares another `gtk-lush-*` crate as a
  non-dev dependency
- **THEN** the family policy check fails until the dependency is removed

### Requirement: Pure generation logic is testable without GTK scheduling
The crate SHALL separate deterministic generation, staleness, and pending-state
logic from GLib timer installation. Pure decision logic MUST be unit-tested and
property-tested without a live GTK main loop. Scheduling adapters MAY use GLib
timers or idle callbacks, but stale-token decisions MUST be independently
testable.

#### Scenario: Stale-token logic runs as pure tests
- **WHEN** `cargo test -p gtk-lush-settle` runs without presenting a GTK
  window
- **THEN** tests prove that newer generations make older tokens stale
- **AND** stale tokens cannot pass the current-generation check

#### Scenario: Pending-state transitions are property-tested
- **WHEN** property tests generate sequences of open, extend, finish, clear,
  and stale-finish operations
- **THEN** the settle state machine never reports settled before the current
  repair is allowed to finish

### Requirement: Debounce primitive coalesces latest-generation work
The crate SHALL provide a `Debounce`-class primitive for trailing work where
only the newest scheduled request may run. Each schedule MUST advance a
generation, install a GLib-main-loop callback for the requested quiet window,
capture the target weakly, and invoke the callback only if the target is still
alive and the captured generation is still current. The primitive MUST expose
a way to advance or invalidate the generation for workflows that need
immediate actions, empty-state rebuilds, or async freshness checks tied to the
same debounce family.

#### Scenario: Burst coalesces to latest callback
- **WHEN** debounce work is scheduled repeatedly within the quiet window
- **THEN** earlier scheduled callbacks become stale
- **AND** only the latest scheduled callback can perform side effects after
  the final quiet window elapses

#### Scenario: Dead target cancels silently
- **WHEN** the weak GTK target is destroyed before a scheduled debounce fires
- **THEN** the callback does not run
- **AND** no panic, stale widget access, or warning is emitted

#### Scenario: Immediate empty state can invalidate pending work
- **WHEN** a search-like surface clears its input and performs an immediate
  empty-state rebuild
- **THEN** the debounce generation can be advanced or invalidated so older
  pending non-empty callbacks cannot repopulate stale results

### Requirement: Settle-burst primitive exposes readiness-safe pending state
The crate SHALL provide a `SettleBurst`-class primitive for layout, preview, or
repair storms that must be repaired once after a quiet window. Opening or
extending a burst MUST make previous settle callbacks stale and keep
`pending()` true until the current settle repair completes. The settle callback
and pending-state clear MUST be ordered so observers cannot see a
settled-but-unrepaired state. If same-generation follow-up work is exposed, it
MUST remain tied to the current settle handle and MUST NOT become a general
task scheduler.

#### Scenario: Extension restarts settle window
- **WHEN** a settle burst is scheduled and then extended before its quiet
  window elapses
- **THEN** the first settle callback is stale
- **AND** the repair callback runs only after a full quiet window passes after
  the latest extension

#### Scenario: Pending remains true through repair
- **WHEN** a current settle callback starts its repair work
- **THEN** `pending()` remains true until that repair has completed
- **AND** pending clears in the same ordered dispatch that completes the
  repair

#### Scenario: Stale settle handle cannot clear current pending
- **WHEN** an older settle handle tries to finish after a newer burst has
  opened
- **THEN** the older handle does not clear pending state for the newer burst

### Requirement: Superseding one-shot primitive replaces delayed cleanup work
The crate SHALL provide a `SupersedingTimer`-class primitive for delayed UI
cleanup or reveal actions where re-arming replaces the previous deadline. Each
arm MUST advance a generation, capture the target weakly, and run only the
latest current-generation callback. Invalidating the timer MUST make already
scheduled callbacks no-op without requiring callers to keep `SourceId`
bookkeeping.

#### Scenario: Re-armed timer supersedes previous arm
- **WHEN** a delayed cleanup timer is armed and then re-armed before the first
  deadline
- **THEN** the first callback is stale
- **AND** only the latest arm can perform cleanup

#### Scenario: Invalidation cancels side effects
- **WHEN** a superseding timer is invalidated before its scheduled callback
  fires
- **THEN** the callback performs no side effects
- **AND** the caller does not need to remove a GLib source manually

#### Scenario: Dropped target cancels cleanup
- **WHEN** a widget owning a delayed cleanup action is destroyed before the
  timer fires
- **THEN** the cleanup callback does not run
- **AND** no stale widget access occurs

### Requirement: Public API documentation and tests prove scheduling behavior
Every public item in `gtk-lush-settle` SHALL be documented under the family
engineering bar. Observable behavior MUST have runnable doctests or ordinary
tests, and pure decision logic MUST be covered by unit and property tests.
The crate MUST keep `#![forbid(unsafe_code)]` and `#![deny(missing_docs)]`.
Docs MUST describe the difference between debounce, settle burst,
superseding one-shot, recurring polling, chunked yielding, and async freshness
tokens so consumers do not force unrelated timing contracts into the crate.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public settle helper, token, handle, or state type is added
  without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Doctest demonstrates weak-target cancellation
- **WHEN** the debounce or timer docs are tested
- **THEN** at least one runnable example demonstrates that dropping the target
  before the callback fires prevents side effects

#### Scenario: README teaches the choosing rule
- **WHEN** the README is rendered
- **THEN** it tells consumers when to choose debounce, settle burst,
  superseding timer, or an explicit non-settle pattern
- **AND** it states the anti-framework constraints and pre-publication status

### Requirement: LushText private settle helper migrates to the crate
LushText SHALL replace fitting uses of the private
`crate::ui::settle::{Debounce, SettleBurst, SupersedingTimer}` helper with
`gtk-lush-settle` primitives. The migration MUST preserve the existing delay
windows, latest-generation semantics, weak-target cancellation behavior,
readiness-visible pending state, and user-visible workflow behavior. After the
last fitting call site migrates, the private helper module MUST be deleted or
reduced only to documented compatibility glue that has a removal task in the
same change.

#### Scenario: Private helper has no remaining fitting consumers
- **WHEN** the LushText migration completes
- **THEN** no UI module imports the old private `crate::ui::settle`
  primitives for a fitting debounce, settle-burst, or superseding timer site
- **AND** replaced private helper code is removed rather than kept as a
  duplicate implementation

#### Scenario: Delay windows remain unchanged
- **WHEN** a migrated debounce or timer site is inspected
- **THEN** the quiet window or delayed cleanup duration matches the previous
  workflow unless a spec explicitly changes it

### Requirement: Search and picker debounces preserve state extremes
Migrated search and picker debounces SHALL preserve their existing state
extremes and result freshness contracts.

Search-like and picker-like LushText surfaces migrated to `gtk-lush-settle`
SHALL preserve their current empty-input, representative-input, rapid-input,
and stale-result behavior. This includes command palette query debounce,
command-palette file-index update flushes, content-search query and glob
debounce, notes-browser search debounce, bookmark-dialog search debounce, and
other fitting entry-driven trailing work.

#### Scenario: Empty input clears immediately
- **WHEN** a migrated search or picker entry changes from non-empty to empty
- **THEN** the surface updates to its empty-state behavior immediately where
  it did before
- **AND** older pending non-empty callbacks cannot restore stale rows

#### Scenario: Rapid input shows latest results only
- **WHEN** a user types several distinct queries faster than the debounce
  window
- **THEN** stale scheduled callbacks and stale async results from earlier
  queries do not replace the latest visible result set

#### Scenario: Dense results keep scrolling contract
- **WHEN** a migrated picker or search surface displays many results, long
  labels, or awkward paths
- **THEN** item regions scroll or clip in the same region as before
- **AND** persistent headers, actions, close controls, and mode controls
  remain reachable

### Requirement: Persistence debounces preserve latest-state-wins behavior
Persistence-related LushText debounces migrated to `gtk-lush-settle` SHALL
preserve latest-state-wins behavior and existing domain generation ordering.
The settle primitive may coalesce the timer window, but ordered save
generations, dirty/inflight handling, and stale background write rejection MUST
remain in the owning workflow where they are part of the persistence contract.

#### Scenario: Session save cannot overwrite newer state
- **WHEN** rapid tab mutations schedule multiple session saves after the
  migration
- **THEN** an older scheduled or in-flight save cannot overwrite a newer
  accepted session snapshot

#### Scenario: Workspace persistence keeps dirty/inflight semantics
- **WHEN** workspace mutations occur while a previous persistence write is
  pending or in flight
- **THEN** the latest state is saved after the debounce window
- **AND** older snapshots cannot replace newer in-memory workspace state

#### Scenario: Sidecar and draft scheduling remain durable
- **WHEN** migrated note, bookmark, draft, or sidecar persistence scheduling
  coalesces bursts of edits
- **THEN** the previous save safety and retry/follow-up semantics remain
  equivalent

### Requirement: Visual and readiness settle paths preserve blockers and pixels
Migrated visual and readiness settle paths SHALL preserve their blockers,
repair ordering, and rendered geometry contracts.

Visual-sensitive or readiness-sensitive LushText settle paths migrated to
`gtk-lush-settle` SHALL preserve existing readiness blockers, pending-state
queries, repair ordering, rendered geometry, and animation behavior. This
includes minimap refresh/reflow/reveal timing, preview layout settle, and any
other migrated layout repair path. Visual-geometry proof MUST run when rendered
pixels or geometry-sensitive surfaces can change.

#### Scenario: Minimap pending blocker remains equivalent
- **WHEN** minimap refresh or reflow work is migrated to `gtk-lush-settle`
- **THEN** readiness predicates that previously waited for minimap work still
  report a blocker until the migrated repair is complete
- **AND** they clear only after the same user-visible repair point

#### Scenario: Visual geometry proof covers minimap drift
- **WHEN** minimap or preview rendered geometry changes during the migration
- **THEN** widget tests and visual-geometry scenarios covering pixel anchors,
  sidebar animation, and animation-frame sampling pass before archive

#### Scenario: No settled-before-repaired state is observable
- **WHEN** automation or test waits observe a migrated settle path
- **THEN** they cannot observe `pending() == false` before the associated
  layout or render repair has completed

### Requirement: Non-settle timing families remain explicit exceptions
The implementation SHALL NOT convert recurring pollers, heartbeats, chunked UI
yield loops, idle allocation repairs, async worker freshness tokens, pure
domain/model generations, external backend debouncers, or lifecycle maintenance
delays unless the implementation audit proves the site matches a public
`gtk-lush-settle` primitive and tests prove behavior unchanged. Remaining
explicit sites MUST be audited and classified.

#### Scenario: Recurring heartbeat remains explicit
- **WHEN** a recurring progress heartbeat, notification sweep, readiness loop,
  watcher poller, periodic autosave, or local-history capture timer is
  encountered
- **THEN** the implementation leaves it outside `gtk-lush-settle` unless a
  focused requirement and tests prove conversion is correct

#### Scenario: Async freshness remains future task material
- **WHEN** a generation counter guards background I/O, worker results, replace
  preview, encoding probes, notes preview loads, file peeks, or undo backup
  persistence
- **THEN** the implementation keeps that freshness token in the owning workflow
  or records it as future `gtk-lush-tasks` source material

#### Scenario: Chunked yield is not converted as debounce
- **WHEN** a timeout or idle callback yields between buffer slices, tree
  batches, model population, scroll restoration, or inline rename focus
- **THEN** it is not converted to debounce
- **AND** the audit records why its lifecycle is not a settle helper

### Requirement: Retained timer-like sites are audited
The implementation SHALL produce a retained-site audit for timer-like,
generation-counter, idle, recurring, and async freshness patterns that remain
explicit after the migration. Each retained site MUST name its file, lifecycle
owner, classification, and reason it is not part of the first public
`gtk-lush-settle` API.

#### Scenario: Remaining generation counter has classification
- **WHEN** the final audit finds a generation counter or token in LushText UI
  code
- **THEN** it is classified as settle-owned and migrated, or explicitly
  retained as async freshness, persistence ordering, pure domain generation,
  chunked yield, heartbeat, poller, or other documented exception

#### Scenario: Rule rewrite matches audit
- **WHEN** `.agents/rules/widget-wiring.md` is updated
- **THEN** its settle guidance matches the retained-site audit
- **AND** it does not tell future agents to convert audited non-settle classes
  into `gtk-lush-settle`

### Requirement: Rule and roadmap guidance follows proven migration
After LushText migration and proof, project guidance SHALL point at
`gtk-lush-settle` as the required mechanism for fitting debounce,
settle-burst, and superseding one-shot UI timing. `.agents/rules/widget-wiring.md`,
`.agents/rules/rust.md` when relevant, the crate README, CHANGELOG, and
`docs/next/gtk-lush.md` MUST be updated in the same change to describe the
proven API, exception classes, and Phase 2 completion status without claiming
Phase 5 publication readiness.

#### Scenario: Rules stop teaching private helper use
- **WHEN** the migration completes
- **THEN** project rules no longer instruct agents to use
  `crate::ui::settle` for fitting new work
- **AND** they direct fitting new debounce, settle-burst, and superseding
  one-shot work to `gtk-lush-settle`

#### Scenario: Vision document advances the phase
- **WHEN** this change completes
- **THEN** `docs/next/gtk-lush.md` records that Phase 2 extracted the settle
  API
- **AND** later runtime-geometry, proof-toolchain, publishing, and upstreaming
  phases remain in the roadmap

### Requirement: Settle migration preserves LushText proof gates
The settle migration SHALL preserve LushText's full existing proof surface.
The phase MUST pass the family crate tests, doctests, standalone examples,
policy checks, pure/property tests, focused workflow tests, the full
non-widget gate set, the widget suite, automation readiness checks when
readiness fields are touched, and visual-geometry proof for rendered
geometry-sensitive migrations. GTK/GLib warning gates MUST remain clean.

#### Scenario: Widget tests preserve workflow behavior
- **WHEN** migrated debounce, settle-burst, or superseding-timer workflows are
  exercised by the widget suite
- **THEN** search, picker, notes, workspace, status, focus-mode, preview, and
  minimap behavior remains equivalent to the pre-migration behavior

#### Scenario: Automation readiness remains contract-compatible
- **WHEN** a migrated settle path is represented in automation readiness
  blockers or snapshots
- **THEN** `make check-automation-docs` and the automation client self-test
  pass if those contracts changed or were touched
- **AND** public readiness semantics do not drift silently

#### Scenario: Warning gate stays clean
- **WHEN** migrated timers fire after widgets are destroyed, hidden, or
  superseded
- **THEN** callbacks no-op safely
- **AND** no new unexpected GTK, GLib, GObject, or source-removal warnings are
  emitted
