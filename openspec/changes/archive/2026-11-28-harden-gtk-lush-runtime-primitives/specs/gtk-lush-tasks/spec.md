# Delta Specification: gtk-lush-tasks

## MODIFIED Requirements

### Requirement: Worker concurrency is bounded and panic-safe
`gtk-lush-tasks` SHALL bound concurrent workers with atomic slot acquisition and backpressure on the GTK thread. A worker panic MUST NOT abort the process, MUST release its acquired slot once, and MUST clean up main-thread completion state on the GTK main loop; failed `thread::spawn` attempts MUST also release the acquired slot.

#### Scenario: Limit prevents unbounded workers
- **WHEN** more blocking tasks are scheduled than the configured worker limit
- **THEN** at most the configured number of workers runs concurrently
- **AND** excess work is started from the GLib main loop when capacity returns rather than spinning or blocking user interaction

#### Scenario: Panic releases worker slot
- **WHEN** worker code panics inside `spawn_blocking_then` after acquiring a slot
- **THEN** the process does not abort from dropping main-thread state on the worker thread
- **AND** the slot and main-thread state are released on the GTK main loop so later tasks run normally

#### Scenario: Limit is documented and testable
- **WHEN** `cargo test -p gtk-lush-tasks` runs
- **THEN** unit tests verify real `> 8` task saturation, automatic FIFO draining, and real worker-closure panics without linking LushText

#### Scenario: Failed thread spawn does not leak a slot
- **WHEN** `try_acquire_slot()` succeeds and worker thread creation fails or panics before entering the worker closure
- **THEN** the `SlotGuard` acquired before `spawn` drops and restores the available worker count

### Requirement: Completion returns to the GTK main thread
`gtk-lush-tasks` SHALL keep non-`Send` completion state on the GTK thread without dropping `ThreadGuard` on a worker thread. Pending-task wakeups MUST schedule at `completion_priority()`, and `spawn_blocking_then` MUST fail loudly on off-main-thread calls. Consecutive completions SHALL age after eight deliveries or 16 ms across sustained or self-chaining runs without splitting the delivery source priority contract (`completion_priority()`, ledger A80).

#### Scenario: Worker receives only Send work input
- **WHEN** a task is scheduled with worker code
- **THEN** the worker closure can access only data satisfying the worker-thread contract
- **AND** GTK widgets or other non-`Send` state are not available to worker code

#### Scenario: Completion callback can use guarded state
- **WHEN** worker work finishes successfully
- **THEN** the completion callback runs on the GTK main thread
- **AND** main-thread state is retrieved only inside that completion dispatch without cross-thread drop aborts

#### Scenario: Widget-test wait semantics remain stable
- **WHEN** LushText widget tests wait for asynchronous task completion through their existing main-loop drain helpers
- **THEN** migrated task completions are observable through the same GLib delivery contract as before

#### Scenario: A completion is not held behind text validation
- **WHEN** a worker finishes while a text view, on screen or on a hidden page, still has lines to validate
- **THEN** its completion runs before the view's next incremental validation batch
- **AND** the crate's unit test and LushText's adoption widget test fail against a delivery at `G_PRIORITY_DEFAULT_IDLE`

#### Scenario: One completion per dispatch
- **WHEN** several completed results are waiting
- **THEN** each main-loop dispatch of the completion source runs exactly one of them, oldest first
- **AND** the main loop's priority check runs between two completions, so a ready frame or event dispatches first

#### Scenario: Completions never delay frames or input
- **WHEN** completions are waiting while a frame or an input event is ready
- **THEN** the frame or the event dispatches first, because the completion priority is below `GDK_PRIORITY_REDRAW` and `G_PRIORITY_DEFAULT`

#### Scenario: A completion chain yields to lower priorities
- **WHEN** completions keep arriving faster than they are delivered, from a FIFO deeper than the worker cap or from self-chaining completions
- **THEN** at most eight completions run before yielding to `G_PRIORITY_DEFAULT_IDLE` or validation priorities
- **AND** the crate's unit tests fail against a source that does not yield

#### Scenario: Measurement builds can observe each completion
- **WHEN** the `completion-observer` feature is enabled and an observer is installed with `set_completion_observer`
- **THEN** the observer is called on the GTK thread after each completion with its start and end in `glib::monotonic_time()` microseconds
- **AND** queued-task `thread::spawn` wakeups run in a separate `idle_add_full(completion_priority(), ...)` dispatch rather than inside the observer span

#### Scenario: Off-main-thread scheduling fails explicitly
- **WHEN** `spawn_blocking_then` is invoked from a non-main thread
- **THEN** delivery detects the non-main-thread origin and panics explicitly rather than silently stranding work in a worker's thread-local queue
