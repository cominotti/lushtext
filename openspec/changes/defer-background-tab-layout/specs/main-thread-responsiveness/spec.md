## ADDED Requirements

### Requirement: A background document is read without waiting for installs
After a file load's worker read, its transient load permit SHALL keep the
decoded body it still holds outside the buffer, the live editor's four-byte
residency for every character not yet installed, and the fixed per-load
overhead (never more than it was granted), and SHALL stop counting against the
concurrent-read limit, so that an installation waiting behind the visible tab
never keeps the next document from being read while the byte budget still
covers everything the overlapping loads own. An oversize load SHALL keep
running alone until it is released. The protected-residency rule SHALL still
count an installing body as a payload, and the permit SHALL be released exactly
once. A background install slice that has waited behind visible work for
`BACKGROUND_SLICE_AGE` (256 ms) SHALL run once at the visible tab's paced
priority, so no background install waits indefinitely.

#### Scenario: Overlapping installs stay within the byte budget
- **WHEN** several loads have finished reading and are installing
- **THEN** their retained charges, each covering its body and the residency of
  its characters not yet installed, fit the shared budget together
- **AND** a load larger than the budget installs with no other load admitted

#### Scenario: A background install keeps moving behind visible work
- **WHEN** a background slice waits while `G_PRIORITY_DEFAULT_IDLE` work stays
  ready
- **THEN** it runs within `BACKGROUND_SLICE_AGE` of being queued, and removing
  its source still cancels it

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
search, bookmarks, printing and memory accounting SHALL behave as for a shown
tab, and a detach or attach SHALL change no byte, modified flag, undo state,
caret or primary selection.

No view SHALL be given the placeholder while an assistive technology listens
to the application's events (GTK sends each swap's whole old and new text to
it, about 2 ms per MiB in one dispatch), nor while its document has more than
`VIEW_DETACH_MAX_LINES` (2^18) lines (the swap walks every line); when a
listener appears, a detached view whose document has at most
`AT_ATTACH_MAX_CHARS` (2^20) characters SHALL be attached at once. A detached
view SHALL keep Protected View's primary-selection choice for its attach, and
the kept position inside a wrapped paragraph SHALL be the display line at the
top of the view.

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

#### Scenario: No view is detached while an assistive technology listens
- **WHEN** an assistive technology listens and a hidden tab's view lays out
- **THEN** the view stays attached and lays out in GTK's own bounded batches
- **AND** a small detached view attaches as soon as the listener appears, and
  views are detached again once no listener remains

#### Scenario: A document with very many lines is never detached
- **WHEN** a hidden tab's document has more than `VIEW_DETACH_MAX_LINES` lines
- **THEN** its view stays attached

#### Scenario: A hidden tab's selection stays on PRIMARY
- **WHEN** a tab whose selection owns the primary selection is hidden and its
  view detached
- **THEN** the primary selection still offers it, and still does after the
  attach

#### Scenario: Protected View entered while detached
- **WHEN** a detached tab's document enters Protected View
- **THEN** no toolkit critical is logged and, once shown, the document offers
  nothing on the primary selection

#### Scenario: A never-shown tab prints and opens bookmarks
- **WHEN** a tab that was never shown (preview-only mode) is printed or a
  bookmark in it is opened
- **THEN** the print covers its document, and the bookmark places the caret
  and the kept line without a toolkit critical

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
