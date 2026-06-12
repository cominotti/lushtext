## ADDED Requirements

### Requirement: Reusable task boundary preserves responsiveness
Fitting LushText background workflows SHALL use `gtk-lush-tasks` for bounded
worker execution and GLib-main-loop completion delivery after the Phase 3
migration. GTK-owned snapshots, large-buffer chunking, durable-write ordering,
and domain-specific freshness checks MUST remain in the owning workflow when
they express app behavior rather than reusable task dispatch.

#### Scenario: Worker dispatch uses gtk-lush-tasks
- **WHEN** a LushText workflow schedules blocking filesystem work, canonical
  probes, expensive pure analysis, or persistence through a fitting
  `spawn_blocking_then`-style path
- **THEN** the workflow uses `gtk-lush-tasks` for worker scheduling and
  main-thread completion delivery
- **AND** it does not keep a duplicate app-local worker dispatcher for fitting
  call sites

#### Scenario: GTK snapshots still happen before worker scheduling
- **WHEN** a migrated workflow needs editor text, widget visibility, active
  file paths, selected rows, or other GTK-owned state
- **THEN** it captures that state on the GTK thread before scheduling worker
  code
- **AND** the worker receives owned non-GTK data

#### Scenario: App-owned freshness checks remain visible
- **WHEN** a migrated completion depends on current tab identity, document
  path, search generation, encoding request, undo generation, persistence
  ordering, or another workflow-specific state
- **THEN** that check remains visible in the owning LushText module or is
  represented through an explicit typed helper from `gtk-lush-tasks`
- **AND** older worker results cannot overwrite newer visible state

#### Scenario: Data safety is not weakened by extraction
- **WHEN** migrated persistence or save-adjacent workflows complete after
  newer state exists
- **THEN** durable-write, retry, dirty-state, and latest-state-wins semantics
  remain equivalent to the pre-migration behavior
- **AND** data-safety review findings are fixed before archive
