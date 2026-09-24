## MODIFIED Requirements

### Requirement: Preserved set-aside drafts have a recovery surface
The system SHALL list the drafts preserved in the set-aside location on the
`Preferences > Data` page. Each row SHALL show the original path when known,
the preservation time, and the size. Each row SHALL offer Open (as a new
untitled tab) and Delete (with confirmation) actions. The listing SHALL
choose its rows from the whole set-aside area, within a bounded scan budget, so
the rows shown are the newest bodies. The listing SHALL report the total count
and total size, or a stated lower bound when the scan budget was reached. When
not every body is shown, it SHALL say so. The group SHALL include a summary
row with the count, total size, and whether the soft retention bound is
exceeded. It SHALL offer one bulk action, "Delete All Preserved Drafts…",
which follows the retention requirement below. The group MUST NOT be shown
when the set-aside location is empty. The rows, summary, and bulk action SHALL
follow the grouped-row readability and accessibility rules.

#### Scenario: Open a set-aside draft
- **WHEN** the user activates Open on a set-aside row
- **THEN** its content opens in a new untitled tab and the set-aside copy remains until deleted

#### Scenario: Delete a set-aside draft
- **WHEN** the user confirms Delete on a set-aside row
- **THEN** the set-aside body is removed durably and the row disappears

#### Scenario: Nothing set aside
- **WHEN** the set-aside location is empty
- **THEN** the Data page shows no set-aside group

#### Scenario: More bodies than rows
- **WHEN** the set-aside area holds more bodies than the listing shows
- **THEN** the rows shown are the newest bodies in the area
- **AND** the summary states the total (or its lower bound) and that only some are listed

## ADDED Requirements

### Requirement: Set-aside retention never deletes a draft without a user decision
The system MUST NOT delete a set-aside body automatically, on any bound, age,
count, size, or startup path. A set-aside body SHALL be removed only by the
user's confirmed per-row Delete or the confirmed "Delete All Preserved
Drafts…" bulk deletion. The bulk confirmation SHALL state the exact count and
total size of the bodies it will delete. The bulk-deletion plan SHALL be
computed by a GTK-free, I/O-free retention core, and it SHALL satisfy:

- with no user decision, the plan is empty;
- the plan contains only bodies whose fingerprint (name, size, and file
  identity) the user was shown in the confirmation, so a body that appeared or
  changed after the confirmation dialog opened is kept.

A Kani harness and a property test SHALL check these properties over bounded
inputs. The system SHALL NOT persist any record of which set-aside bodies the
user has opened.

#### Scenario: Crossing the bound deletes nothing
- **WHEN** the set-aside area exceeds the soft retention bound at startup or after a new body is set aside
- **THEN** no set-aside body is deleted

#### Scenario: Delete All states what it deletes
- **WHEN** the user activates "Delete All Preserved Drafts…"
- **THEN** a destructive confirmation states the exact count and total size of the bodies it will delete, with Cancel as the default response
- **WHEN** the user confirms
- **THEN** exactly the bodies shown in that confirmation are removed

#### Scenario: Cancelling Delete All deletes nothing
- **WHEN** the user cancels or closes the "Delete All Preserved Drafts…" confirmation
- **THEN** no set-aside body is deleted

#### Scenario: A body set aside during the confirmation survives
- **WHEN** a new body is set aside, or a listed body changes, after the confirmation dialog was shown
- **THEN** the confirmed bulk deletion does not remove it

### Requirement: Set-aside growth past a soft bound is surfaced for review
The system SHALL evaluate the set-aside area against a soft retention bound (a
body count and a total byte size) off the GTK thread once per process after
startup restore settles, and after a body is newly set aside. When the bound
is exceeded, it SHALL publish a window status warning that names the count and
size and points to the `app.review-preserved-drafts` action. That action
SHALL be in the action catalog and the command palette, and SHALL open
`Preferences > Data` with the Preserved Drafts group in view. The notice SHALL
be rate-limited within the process, without persisting any acknowledgment: at
most one notice after startup restore, a further one after a placement only
when the area has grown materially past the last notified totals, and none for
the rest of the process once the user has run the review action. The notice
and the review surface MUST NOT expose any draft body text.

#### Scenario: Notice after crossing the bound
- **WHEN** startup restore settles and the set-aside area exceeds the soft bound
- **THEN** one status warning is published naming the count and size and the review action

#### Scenario: Review action opens the group
- **WHEN** the user activates `app.review-preserved-drafts`
- **THEN** `Preferences > Data` is presented with the Preserved Drafts group in view

#### Scenario: No repeated nagging within a process
- **WHEN** the area is still over the bound after a placement but has not grown materially since the last notice, or the user has already run the review action in this process
- **THEN** no further notice is published

### Requirement: A set-aside copy counts as kept only when it holds the same bytes
When the system keeps a draft body in the set-aside location, an existing
set-aside file SHALL count as that body already kept only if it is
byte-identical to the body being preserved. Otherwise the body SHALL be kept
under a distinct set-aside name, and neither file SHALL be overwritten. A
preservation that reports a body set aside MUST leave that exact body in the
set-aside location. This decision SHALL be a pure journal-core function, and a
Kani harness SHALL check that every body reported kept is in the set-aside
location and no kept body is replaced.

#### Scenario: A newer body under an unchanged entry stamp is still set aside
- **WHEN** a crash falls between a draft body write and its manifest commit, so the body on disk is newer than its entry's stamp, and a set-aside copy for that stamp already holds the older body
- **AND** an unapplied restore preserves the body again
- **THEN** the newer body is copied to a distinct set-aside name, the older copy is unchanged, and only then is the restore hold released

#### Scenario: Keeping the same body again does not duplicate it
- **WHEN** a body is preserved again and a byte-identical set-aside copy for its id and stamp already exists
- **THEN** no second copy is written
