# gtk-lush-tasks Specification

## Purpose
Define the reusable GTK Lush background task helpers used by stock gtk-rs
applications and by LushText after the runtime-geometry extraction.

## Requirements
### Requirement: Leaf background task crate
`gtk-lush-tasks` SHALL provide background task helpers for stock gtk-rs
applications while remaining an independently adoptable GTK Lush leaf crate.
The crate MUST NOT depend on LushText crates, MUST NOT depend on another GTK
Lush family crate at runtime, MUST NOT create a custom executor or application
runtime, and MUST NOT replace ordinary GLib or GTK main-loop ownership.

#### Scenario: Standalone application adopts only tasks
- **WHEN** `cargo test -p gtk-lush-tasks --examples` builds the crate's
  standalone example
- **THEN** the example uses stock gtk-rs plus `gtk-lush-tasks`
- **AND** no other GTK Lush crate or LushText crate is required

#### Scenario: Runtime family dependency is rejected
- **WHEN** `gtk-lush-tasks` declares another `gtk-lush-*` crate as a non-dev
  dependency
- **THEN** the family policy check fails until the dependency is removed

#### Scenario: No custom runtime is introduced
- **WHEN** a consumer schedules blocking work through the crate
- **THEN** work runs through ordinary Rust worker threads or documented GLib
  main-loop callbacks
- **AND** the crate does not install a message loop, component update loop, or
  application-wide executor

### Requirement: Worker concurrency is bounded and panic-safe
The crate SHALL bound concurrent worker execution with backpressure equivalent
to LushText's existing `spawn_blocking_then` contract. Acquiring a worker slot
MUST be atomic, exhausted capacity MUST defer pending work without blocking the
GTK thread, and every acquired slot MUST be released even if the worker panics.

#### Scenario: Limit prevents unbounded workers
- **WHEN** more blocking tasks are scheduled than the configured worker limit
- **THEN** at most the configured number of workers runs concurrently
- **AND** excess work is started from the GLib main loop when capacity returns
  rather than spinning or blocking user interaction

#### Scenario: Panic releases worker slot
- **WHEN** worker code panics after acquiring a slot
- **THEN** the slot is released exactly once
- **AND** later tasks can acquire capacity without manual repair

#### Scenario: Limit is documented and testable
- **WHEN** the crate's tests run
- **THEN** they prove the default limit, slot acquisition, saturated
  backpressure, FIFO draining, and release behavior without linking LushText

### Requirement: Completion returns to the GTK main thread
The crate SHALL deliver task completion to the GTK main thread through GLib
main-loop dispatch. Non-`Send` GTK-thread state passed to the completion MUST
be protected by a thread-guard or equivalent type boundary so worker code
cannot access it off the originating thread.

#### Scenario: Worker receives only Send work input
- **WHEN** a task is scheduled with worker code
- **THEN** the worker closure can access only data satisfying the worker-thread
  contract
- **AND** GTK widgets or other non-`Send` state are not available to worker
  code

#### Scenario: Completion callback can use guarded state
- **WHEN** worker work finishes successfully
- **THEN** the completion callback runs on the GTK main thread
- **AND** the guarded GTK-thread state is unwrapped only inside that completion
  dispatch

#### Scenario: Widget-test wait semantics remain stable
- **WHEN** LushText widget tests wait for asynchronous task completion through
  their existing main-loop drain helpers
- **THEN** migrated task completions are observable through the same GLib
  delivery contract as before

### Requirement: Freshness helpers make stale completion explicit
The crate SHALL provide typed completion or freshness helpers that let consumers
state, check, and apply current-generation or current-identity worker results
without hiding application-specific invariants. A stale result MUST NOT be
applicable through a helper that claims freshness, and callers MUST be able to
retain explicit domain checks when their freshness rule belongs to app state.

#### Scenario: Current token permits application
- **WHEN** a worker result returns with a completion token that still matches
  the current consumer generation or identity
- **THEN** the helper can produce a fresh value for the completion path
- **AND** applying that value is visibly separate from handling a stale result

#### Scenario: Stale token cannot masquerade as fresh
- **WHEN** a newer generation or identity has replaced the one captured by a
  worker result
- **THEN** the helper rejects the stale result
- **AND** the stale value cannot be passed to an API that requires a fresh
  completion without an explicit caller decision

#### Scenario: Domain freshness remains visible
- **WHEN** a LushText workflow's freshness depends on tab identity, path
  identity, undo generation, persistence ordering, search state, encoding
  request, or another domain-specific rule
- **THEN** the migration keeps that rule in the owning workflow unless the new
  helper can represent it without reducing the rule's visibility

### Requirement: Public documentation and tests prove task behavior
Every public item in `gtk-lush-tasks` SHALL be documented under the GTK Lush
engineering bar. Observable behavior MUST have runnable doctests, unit tests,
or property tests as appropriate. The crate MUST keep `#![forbid(unsafe_code)]`
and `#![deny(missing_docs)]`.

#### Scenario: Missing public docs fail the crate
- **WHEN** a public task, completion, freshness, guard, or error type is added
  without documentation
- **THEN** the crate fails to build under its lint configuration

#### Scenario: Tests run without LushText
- **WHEN** `cargo test -p gtk-lush-tasks` runs
- **THEN** task scheduling, saturated backpressure, slot release, main-thread
  completion, and freshness decisions are tested without linking LushText

#### Scenario: README teaches the bug classes
- **WHEN** the README is rendered
- **THEN** it explains worker-thread GTK safety, stale-result bugs, backpressure
  behavior, and the anti-framework constraints

### Requirement: LushText background task helper migrates to the crate
LushText SHALL replace fitting uses of `services::async_task::spawn_blocking_then`
with `gtk-lush-tasks`. The migration MUST preserve existing worker limits,
completion ordering, GLib delivery semantics, GTK-thread state guards, and
visible workflow behavior. The old helper MUST be deleted or reduced to
documented compatibility glue with a removal task in the same change.

#### Scenario: Fitting callers migrate
- **WHEN** the implementation audits `spawn_blocking_then` call sites
- **THEN** every fitting caller is migrated to `gtk-lush-tasks`
- **AND** any retained app-local call site is documented with its owner,
  freshness class, and reason

#### Scenario: Persistence ordering remains app-owned
- **WHEN** session, workspace, draft, sidecar, replace-undo, or local-history
  persistence work migrates to the task crate
- **THEN** the reusable task helper performs worker dispatch and completion
  delivery
- **AND** the workflow's durable-write, dirty/inflight, retry, and
  latest-state-wins ordering remains in the owning LushText module

#### Scenario: UI result freshness remains equivalent
- **WHEN** migrated search, preview, encoding, file-load, file-tree, or
  minimap-related task completions return after the UI state has changed
- **THEN** stale results are ignored or classified exactly as before
- **AND** the newer visible state is not overwritten by the older result

### Requirement: Task migration preserves proof gates
The task extraction SHALL preserve LushText's responsiveness and data-safety
contracts. The phase MUST pass family crate tests, doctests, examples, focused
async tests, widget tests covering task-backed workflows, warning scans, and
delegated reviews for responsiveness, data safety, Rust architecture, and code
comments before archive.

#### Scenario: Main thread remains responsive
- **WHEN** task-backed workflows handle large files, many tabs, large search
  result sets, slow filesystems, or rapid UI input
- **THEN** filesystem work and expensive pure analysis run away from GTK signal
  handlers
- **AND** user interaction, repaint, and async completions remain able to
  progress

#### Scenario: Delegated reviews cover task risks
- **WHEN** the task migration is implementation-complete
- **THEN** focused delegated reviews examine stale-result protection,
  persistence safety, main-thread blocking risk, architecture boundaries, and
  comments for the changed Rust code
- **AND** actionable findings are fixed before the phase is marked complete
