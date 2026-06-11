# settle-timer-normalization Specification

## Purpose
Define how LushText normalizes app-local settle, debounce, and superseding
timer helpers before any reusable GTK Lush settle API is extracted.

## Requirements
### Requirement: Timer-like call sites are audited before conversion
The implementation SHALL audit timer-like LushText call sites before
normalizing helpers. The audit MUST include GLib one-shot timers, recurring
timers, idle deferrals, timeout futures, generation counters, and `SourceId`
cancellation sites under `crates/lushtext-core/src/ui` and any directly related
service helpers. Each candidate MUST be classified as debounce, delayed
settle/repair, superseding one-shot, chunked yield, heartbeat/polling, stale
async freshness, pure model/domain generation, or intentionally out of scope.

#### Scenario: Debounce candidate is identified
- **WHEN** a GTK main-loop timer coalesces repeated user or model changes by
  capturing a generation and no-oping stale firings
- **THEN** the audit classifies it as debounce or superseding one-shot
- **AND** records the expected helper treatment

#### Scenario: Polling candidate is not forced into settle
- **WHEN** a recurring timer owns a lifecycle poll, heartbeat, autosave tick, or
  readiness loop
- **THEN** the audit classifies it as heartbeat/polling
- **AND** the implementation leaves it outside the private settle helper unless
  a later explicit design changes that lifecycle contract

### Requirement: Private helper prototypes future settle primitives
LushText SHALL introduce a private in-tree helper shape for Phase 0 timer
normalization. The helper MUST prototype the future `gtk-lush-settle`
contracts for debounce, delayed settle bursts, and superseding one-shot timers
without exposing a public GTK Lush crate API, adding a family crate dependency,
or replacing GTK's main-loop ownership.

#### Scenario: Helper stays application-internal
- **WHEN** the helper is added
- **THEN** it is private to the LushText application or `lushtext-core`
- **AND** no new public `gtk-lush-settle` crate API is created or documented as
  available to external consumers

#### Scenario: Pure decision logic is testable
- **WHEN** the helper implements generation advancement, staleness decisions, or
  pending-state transitions
- **THEN** that deterministic logic is covered by unit or property tests without
  requiring a live GTK main loop

### Requirement: Converted one-shot timers preserve latest-generation semantics
Every converted debounce or superseding one-shot timer SHALL preserve the
existing latest-generation-wins behavior. Re-arming or scheduling newer work
MUST make older timer firings no-op without relying on stale `SourceId`
bookkeeping, and dropping the weak GTK target before the timer fires MUST
prevent callback side effects.

#### Scenario: Stale callback no-ops
- **WHEN** a converted debounce is scheduled, superseded, and the earlier timer
  later fires
- **THEN** the earlier callback performs no workflow side effects
- **AND** only the latest scheduled callback can apply the user-visible result

#### Scenario: Dropped target cancels silently
- **WHEN** a converted timer's GTK target is destroyed before the timer fires
- **THEN** the callback does not run
- **AND** no panic, stale widget access, or warning is emitted

### Requirement: Converted settle bursts preserve pending readiness
Every converted delayed settle or repair burst SHALL expose or preserve the
pending state needed by existing readiness predicates. The pending state MUST
remain true from the first scheduled burst work until the final repair callback
has completed, and MUST clear only after the same user-visible repair that
cleared the previous hand-rolled pending flag.

#### Scenario: Visual readiness waits for repair completion
- **WHEN** a converted minimap, preview, or layout settle path schedules
  post-allocation repair work
- **THEN** readiness predicates that previously waited for that work continue to
  report a bounded blocker until the repair has completed

#### Scenario: Settled state is not observable before repair
- **WHEN** a settle burst's quiet window elapses
- **THEN** the settle action and pending-state clear occur in one ordered
  dispatch
- **AND** observers cannot see a settled-but-unrepaired state

### Requirement: Non-settle timer families remain explicit exceptions
The implementation SHALL NOT convert timer-like code whose lifecycle belongs to
recurring pollers, chunked UI yielding, stale async freshness guards, pure
domain generations, or external debouncer backends unless the audit explicitly
documents why the helper's contract matches that site and tests prove behavior
is unchanged.

#### Scenario: Stale async generation stays out of scope
- **WHEN** a generation counter guards completion of background I/O, preview
  loading, replace preview, note excerpt loading, encoding analysis, or other
  async worker results
- **THEN** the implementation leaves it out of the settle helper
- **AND** records it as future `gtk-lush-tasks` or freshness-token source
  material when relevant

#### Scenario: Chunked yield is handled carefully
- **WHEN** a one-millisecond timer or idle callback exists to yield between
  buffer slices, tree batches, model reconciliation, or scrolling restoration
- **THEN** it is not converted as debounce
- **AND** any optional conversion requires focused tests for the model,
  selection, scroll, or large-buffer behavior it protects

### Requirement: Normalization preserves UI state extremes and proof gates
Converted timer workflows SHALL preserve behavior across the state extremes of
their surfaces: no required context, representative populated data, many or
awkward items, and constrained geometry. Commands MUST remain reachable, empty
states MUST remain readable, item regions MUST scroll or clip in the same
region as before, persistent headers and actions MUST remain visible, and
converted timing MUST NOT introduce unintended scrollbars, fake rows, focus
loss, stale status pulses, duplicate saves, or readiness drift.

#### Scenario: Search and command surfaces preserve debounce behavior
- **WHEN** a converted search, command palette, notes browser, or glob entry is
  exercised with empty input, representative input, and rapid input changes
- **THEN** results update with the same immediate-empty and trailing-debounce
  behavior as before
- **AND** stale results from superseded input do not appear

#### Scenario: Persistence debounce remains latest-state-wins
- **WHEN** session, workspace, notes, bookmark, draft, or local-history
  scheduling is converted
- **THEN** rapid mutations cannot let an older scheduled save overwrite newer
  in-memory state
- **AND** dirty/inflight follow-up behavior remains equivalent to the previous
  workflow

#### Scenario: Visual surfaces keep rendered geometry stable
- **WHEN** a converted timer affects minimap, preview, adaptive layout, focus
  affordances, or status pulse rendering
- **THEN** widget tests cover the affected state transitions
- **AND** visual-geometry proof is run when rendered pixels or
  geometry-sensitive surfaces can change

### Requirement: Rules and roadmap update after proof
Project rules, GTK Lush planning docs, and related guidance SHALL be updated
only after the private helper pattern is proven in code. The guidance MUST
describe the normalized private helper and the exception classes, and MUST NOT
claim that `gtk-lush-settle` has a public functional API before the later
extraction change provides one.

#### Scenario: Guidance follows implementation
- **WHEN** timer normalization has converted the safe candidates and recorded
  exceptions
- **THEN** `.agents/rules/widget-wiring.md`, `docs/next/gtk-lush.md`, and any
  affected local guidance describe the proven helper pattern
- **AND** the guidance keeps public GTK Lush extraction deferred to
  `extract-gtk-lush-signals-and-settle`
