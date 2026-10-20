## ADDED Requirements

### Requirement: Window construction and the dispatches before the first frame fit the settle budget
The system SHALL keep LushText-owned application startup work, each main
window's construction (from `LushtextWindow::new` until control returns to the
main loop), and every GTK main-loop dispatch between construction and the
window's first presented frame within the settle-dispatch budget B_settle
recorded by the responsiveness policy constants (working value 16 ms). The
budget SHALL hold on the reference lane (private `mutter --headless`,
`GSK_RENDERER=cairo`, at most 16 cores, release build) for a fresh profile
and a steady-state profile under both the keyfile and the dconf GSettings
backends. Construction work that the first frame does not need MAY be deferred
to later settle dispatches, each within B_settle, provided an action that
needs a deferred surface first completes its construction synchronously
within the same budget. Toolkit-owned startup (`gtk_init`) SHALL be measured
and reported separately and is not counted against this budget.

#### Scenario: Steady-state launch constructs the window within budget
- **WHEN** the release build opens any file on a profile whose settings were already migrated
- **THEN** the recorded window-construction span and every dispatch before the first presented frame are at most B_settle

#### Scenario: First launch constructs the window within budget
- **WHEN** the release build opens a file on a fresh profile under the keyfile GSettings backend
- **THEN** the window-construction span and every dispatch before the first presented frame are at most B_settle
- **AND** no GSettings backend write occurs before the first presented frame

#### Scenario: A deferred surface is needed early
- **WHEN** construction is staged and an action reaches a surface whose stage has not yet run
- **THEN** that stage completes synchronously within B_settle before the action proceeds
- **AND** the action behaves exactly as it does when the stage ran earlier

### Requirement: GSettings backend writes stay off the construction and interaction paths
The system SHALL NOT execute a GSettings backend write synchronously on the
GTK thread during window construction or before the first presented frame.
Imperative LushText settings writes SHALL go through one application-wide
coalescing writer that keeps the new value in memory immediately, drops a
value equal to the last persisted value, and flushes at most once per
persistence window, no earlier than the first presented frame. Until a value
is flushed, every LushText read of that key SHALL return it, and every
LushText subscriber to that key's changes SHALL be notified when it is
enqueued, so effects delivered through another settings instance stay
immediate. The writer's
flush SHALL run on a path that keeps every GTK dispatch within its budget, as
established by the GSettings backend axiom probes. The last window's `close_request` and
application shutdown SHALL flush pending values synchronously, after any
in-flight flush, so persisted window state is exact after close; closing a
window while others remain SHALL NOT flush synchronously.
Persisted keys, values, defaults, and the split-view migration result SHALL
remain unchanged.

#### Scenario: Resizing does not write per notification
- **WHEN** the user resizes the window and GTK emits many `default-width` and `default-height` notifications
- **THEN** no notification handler performs a backend write on the GTK thread
- **AND** at most one flush per persistence window persists the latest size

#### Scenario: Shell toggles do not write synchronously
- **WHEN** the user toggles the workspace sidebar, document properties, Markdown preview, search panel, or colour scheme
- **THEN** the visible effect is immediate
- **AND** the setting is persisted by the coalescing writer, not by a synchronous backend write in the toggle's dispatch

#### Scenario: Close persists the exact state
- **WHEN** the last window closes with pending writer values
- **THEN** the persisted window size, maximized state, and surface visibility equal the values at close

#### Scenario: Closing one of several windows does not stall the others
- **WHEN** a window closes while another LushText window remains open
- **THEN** the close performs no synchronous backend write
- **AND** the closed window's final values are flushed by the writer on its ordinary path

#### Scenario: Effects through other settings instances stay immediate
- **WHEN** the user changes invisible-character display or the workspace sidebar preset from Preferences
- **THEN** open editor pages and windows reflect the new value in the same interaction, before any flush

#### Scenario: Equal values are not rewritten
- **WHEN** a restore or toggle computes a value equal to the last persisted value
- **THEN** the writer performs no backend write for that key
