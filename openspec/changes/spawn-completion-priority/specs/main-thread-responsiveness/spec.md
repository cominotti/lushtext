## ADDED Requirements

### Requirement: Bounded-stage hand-offs are not held behind text validation
A hand-off between bounded stages of a LushText workflow SHALL run at
`gtk_lush_tasks::completion_priority()`, so that a text view's validation
backlog (ledger A80) cannot hold it back. A hand-off is a worker completion,
the file-load and save admission drains, or the release of a file-load or save
payload permit. Paced work whose slices must follow validation, such as the
post-open install slices at `PACED_SLICE_PRIORITY` (ledger A70), SHALL stay
below the validation idle. Every hand-off dispatch SHALL stay within
`B_SETTLE_US`.

#### Scenario: A save during a validation backlog is admitted and written promptly
- **WHEN** the user presses Ctrl+S on a small document while other open
  documents are still being validated
- **THEN** the save's admission drain, its snapshot and its worker completion
  run without waiting for the backlog
- **AND** the saved bytes are on disk long before the backlog ends

#### Scenario: Install pacing still follows validation
- **WHEN** a paced installation queues its next slice
- **THEN** the slice waits at `PACED_SLICE_PRIORITY`, below the validation idle,
  as before

#### Scenario: A composition stage dispatches alone
- **WHEN** a worker completion makes default-idle work ready before a window
  composition stage runs, such as the session load queuing the first restore
  turn before stage P
- **THEN** the stage runs at a priority one step above `G_PRIORITY_DEFAULT_IDLE`,
  in an iteration of its own, and the default-idle work runs in the next one
- **AND** the language catalog's one-off build runs at
  `G_PRIORITY_DEFAULT_IDLE - 2`, a priority no other source uses, so it never
  shares a stage's or a turn's dispatch
