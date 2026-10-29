## MODIFIED Requirements

### Requirement: Window construction and the dispatches before the first frame fit the settle budget
The system SHALL keep each of the following within the settle-dispatch budget B_settle recorded by
the responsiveness policy constants (working value 16 ms), each one separately:
- LushText-owned application startup work;
- the work an `open` or `activate` handler performs before the main loop runs;
- every GTK main-loop dispatch from that handler until the window's composition is complete,
  including each window's first presented frame and every frame dispatch that shows newly revealed
  content.

The budget SHALL hold on the reference lane (private `mutter --headless`, `GSK_RENDERER=cairo`, at
most 16 cores, release build) for a fresh profile and a steady-state profile, under both the
keyfile and the dconf GSettings backends, in runs that meet the reference precondition: the
1-minute load average below 4 for 30 s before the run, no process at 80 % CPU or more, and no other
agent's build, test, or proof process running. A run that loses the precondition SHALL be
discarded.

The `open` and `activate` handlers SHALL perform no window construction before the main loop runs:
they SHALL hold the application, record the request, and schedule the first composition stage. A
request that arrives before the window exists SHALL merge into the pending one rather than create
a second window. The hold SHALL be released once the window belongs to the application, or when
the application quits first.

The window SHALL be composed in a fixed sequence of stages, each in its own dispatch within
B_settle:
1. warming the one-time text and icon state;
2. constructing the whole window, with the content the first frame does not need invisible, and
   its breakpoints installed;
3. realizing it;
4. presenting it;
5. after its first frame, revealing the content page, then the workspace sidebar and the
   document-properties panel when the restored layout shows them.

From the content-page stage on, a stage's successor SHALL NOT start before the frame that shows
that stage has painted. A concealed revealer's child SHALL be invisible while it is concealed.

When an action, accelerator, activation file open, session-restore completion, or automation call
needs a staged surface visible, focused, or allocated, the system SHALL first complete the stages
that surface needs, in order and synchronously; the forced dispatch and the frame that follows it
SHALL each fit B_settle. Reading or projecting onto a staged surface SHALL NOT force a stage. The
operation SHALL then behave exactly as it does on a fully composed window. Accelerators and actions
SHALL NOT be dropped during composition. Activation file opens SHALL be admitted in bounded turns.

The content may fill in over several frames after the first one; the lane SHALL report the time
from the first frame to composition complete, and the maintainer's accepted fill-in is up to
40 ms (5 reference frames at 120 Hz). That figure is not a budget constant and does not relax
B_settle.

Toolkit-owned startup (`gtk_init`) SHALL be measured and reported separately, and is not counted
against this budget. The one-time cost of the first toplevel's realize SHALL be counted against
the budget of the dispatch that incurs it. If that cost alone exceeds B_settle on the reference
lane, the measured lane SHALL report the dispatch as over budget, and the programme record SHALL
record it against the ledger axiom that pins it.

Software rendering on the headless lane overstates paint and present relative to a GPU, so a pass
there is conservative for those phases; the gate SHALL stay the headless lane and SHALL NOT run
against the live desktop.

#### Scenario: Steady-state launch constructs the window within budget
- **WHEN** the release build opens any file on a profile whose settings were already migrated
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle, the first presented frame and each composition frame included

#### Scenario: First launch constructs the window within budget
- **WHEN** the release build opens a file on a fresh profile under the keyfile GSettings backend
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle
- **AND** no GSettings backend write occurs before the first presented frame

#### Scenario: The open handler defers all window work and keeps the application alive
- **WHEN** the application handles `open` or `activate` with no existing window
- **THEN** the handler returns to the main loop without instantiating a window template, realizing a surface, or presenting a window
- **AND** the application keeps running until the window is constructed

#### Scenario: A second request before the window exists
- **WHEN** a second `open` arrives before the first request's window has been constructed
- **THEN** one window is constructed, and it opens the files of both requests

#### Scenario: A deferred surface is needed early
- **WHEN** a user presses F9, or an automation client activates `win.toggle-sidebar`, before the stage owning that surface has run
- **THEN** the stages that surface needs complete before the action proceeds, and the dispatch and the frame after it each fit B_settle
- **AND** the action's visible effect, state, and automation snapshot equal those on a fully composed window

#### Scenario: Projections during composition force nothing
- **WHEN** the window allocates, refreshes its status bar, or renders notifications before the sidebar or content stages have run
- **THEN** no stage is completed early

#### Scenario: Files opened during composition
- **WHEN** the activation names files, or session restore completes, before the content stage has run
- **THEN** each file opens as a tab through the ordinary open path, in bounded turns, with the same duplicate-path, draft-claim, load-priority, and focus behaviour as on a composed window

#### Scenario: The first realize exceeds the budget on its own
- **WHEN** the reference lane measures the realize dispatch above B_settle
- **THEN** the lane reports that dispatch as over budget
- **AND** the programme record states that the budget is not met and names the toolkit axiom and the upstream report
