## ADDED Requirements

### Requirement: A background document is read without waiting for installs
After a file load's worker read, its transient load permit SHALL keep only the
decoded body it still holds outside the buffer, plus the fixed per-load
overhead, and SHALL stop counting against the concurrent-read limit, so that an
installation waiting behind the visible tab never keeps the next document from
being read. The protected-residency rule SHALL still count an installing body
as a payload, and the permit SHALL be released exactly once.

#### Scenario: Ten large documents are all read at once
- **WHEN** ten 8 MiB documents are opened while one of them is shown
- **THEN** every document is read and its installation begun without waiting
  for another installation to finish, within the byte budget

### Requirement: A hidden tab's editor view lays out nothing
An editor's text view SHALL show an empty placeholder buffer from the editor's
construction until its tab is first shown, and again once it is seen laying
out while its tab is hidden (its adjustment's `upper` changing while the view is
unmapped), so that no background tab validates its document (ledger A82, A83).
Showing the tab SHALL give the view its document buffer back before the tab's
first frame. The document buffer SHALL be the one every workflow reads and
writes, whatever the view shows: loading, drafts, saves, session persistence,
search, bookmarks and memory accounting SHALL behave as for a shown tab, and a
detach or attach SHALL change no byte, modified flag, undo state or caret.

#### Scenario: Background documents are not laid out
- **WHEN** several large documents are opened or restored and only one tab is
  shown
- **THEN** the hidden tabs' views show their placeholders and validate nothing
- **AND** a `G_PRIORITY_DEFAULT_IDLE` source is not held back by them, even
  after a view-wide restyle of every editor

#### Scenario: A hidden tab seen laying out is detached
- **WHEN** a tab that was shown is hidden while its view still has lines to lay
  out, or is invalidated while hidden
- **THEN** its view is given the placeholder before its next validation batch

#### Scenario: A detached tab keeps its position
- **WHEN** a detached tab is shown again
- **THEN** its view shows the line it showed when hidden (or the line a
  restore, search or bookmark navigation asked for while it was hidden), and
  keeps it there while it lays the document out, until the user gives input
- **AND** the session saved meanwhile records that line

#### Scenario: Switching to a large hidden tab stays within budget
- **WHEN** the user switches to a large tab whose view laid nothing out while
  hidden
- **THEN** every dispatch of the switch and of the re-layout that follows stays
  within `B_SETTLE_US`

## MODIFIED Requirements

### Requirement: Bounded-stage hand-offs are not held behind text validation
A hand-off between bounded stages of a LushText workflow SHALL run at
`gtk_lush_tasks::completion_priority()`, so that a text view's validation
backlog (ledger A80) cannot hold it back. A hand-off is a worker completion,
the file-load and save admission drains, or the release of a file-load or save
payload permit. Paced work whose slices must follow validation, such as the
post-open install slices at `PACED_SLICE_PRIORITY` (ledger A70), SHALL stay
below the validation idle while the editor's view lays the document out or the
tab is its window's selected tab; a paced install or whole-buffer replacement
into any other tab, whose view shows its placeholder, SHALL queue its slices
one priority step lower, `G_PRIORITY_DEFAULT_IDLE + 1`, within the same slice
budgets: nothing lays it out, and the visible document installs and lays out
first. Every
hand-off dispatch SHALL stay within `B_SETTLE_US`.

#### Scenario: A save during a validation backlog is admitted and written promptly
- **WHEN** the user presses Ctrl+S on a small document while other open
  documents are still being validated
- **THEN** the save's admission drain, its snapshot and its worker completion
  run without waiting for the backlog
- **AND** the saved bytes are on disk long before the backlog ends

#### Scenario: Install pacing still follows validation
- **WHEN** a paced installation into a shown tab queues its next slice
- **THEN** the slice waits at `PACED_SLICE_PRIORITY`, below the validation idle,
  as before
- **AND** a paced installation into a background tab queues its next slice at
  `G_PRIORITY_DEFAULT_IDLE + 1`, and switches to `PACED_SLICE_PRIORITY` from the next
  slice once its tab is selected or shown

#### Scenario: A composition stage dispatches alone
- **WHEN** a worker completion makes default-idle work ready before a window
  composition stage runs, such as the session load queuing the first restore
  turn before stage P
- **THEN** the stage runs at a priority one step above `G_PRIORITY_DEFAULT_IDLE`,
  in an iteration of its own, and the default-idle work runs in the next one
- **AND** the language catalog's one-off build runs at
  `G_PRIORITY_DEFAULT_IDLE - 2`, a priority no other source uses, so it never
  shares a stage's or a turn's dispatch
