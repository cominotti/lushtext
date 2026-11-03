## MODIFIED Requirements

### Requirement: Completion returns to the GTK main thread
The crate SHALL deliver task completion to the GTK main thread through GLib
main-loop dispatch. Non-`Send` GTK-thread state passed to the completion MUST
be protected by a thread-guard or equivalent type boundary so worker code
cannot access it off the originating thread. Completions SHALL be delivered by
one GLib source at the priority `completion_priority()` returns,
`GDK_PRIORITY_REDRAW + 4`: above GtkTextView's incremental validation idle
(`GTK_TEXT_VIEW_PRIORITY_VALIDATE`, ledger A80) and below the frame clock's
paint, GTK's resize and input. That source SHALL deliver at most one completion
per dispatch, in the order the workers finished. After eight consecutive
completions, or 16 ms of continuous occupancy, whichever comes first, the source
SHALL yield: it detaches until the main loop has dispatched
`G_PRIORITY_DEFAULT_IDLE` once, or for at most 16 ms when a source between the
two priorities keeps the loop from reaching it, and then resumes at
`completion_priority()`.

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

#### Scenario: A completion is not held behind text validation
- **WHEN** a worker finishes while a text view, on screen or on a hidden page,
  still has lines to validate
- **THEN** its completion runs before the view's next incremental validation
  batch
- **AND** the crate's unit test and LushText's adoption widget test fail
  against a delivery at `G_PRIORITY_DEFAULT_IDLE`

#### Scenario: One completion per dispatch
- **WHEN** several completed results are waiting
- **THEN** each main-loop dispatch of the completion source runs exactly one of
  them, oldest first
- **AND** the main loop's priority check runs between two completions, so a
  ready frame or event dispatches first

#### Scenario: Completions never delay frames or input
- **WHEN** completions are waiting while a frame or an input event is ready
- **THEN** the frame or the event dispatches first, because the completion
  priority is below `GDK_PRIORITY_REDRAW` and `G_PRIORITY_DEFAULT`

#### Scenario: A completion chain yields to lower priorities
- **WHEN** completions keep arriving faster than they are delivered, from a
  FIFO deeper than the worker cap or from completions that spawn their
  successors
- **THEN** at most eight completions run with no dispatch of an always-ready
  `G_PRIORITY_DEFAULT_IDLE` source in between
- **AND** beside an always-ready source at the validation priority, that source
  runs during the chain and the chain still completes
- **AND** the crate's unit tests fail against a source that does not yield

#### Scenario: Measurement builds can observe each completion
- **WHEN** the `completion-observer` feature is enabled and an observer is
  installed with `set_completion_observer`
- **THEN** the observer is called on the GTK thread after each completion with
  its start and end in `glib::monotonic_time()` microseconds
- **AND** a build without the feature compiles no observer
