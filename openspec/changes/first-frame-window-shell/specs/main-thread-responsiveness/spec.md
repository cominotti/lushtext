## MODIFIED Requirements

### Requirement: Window construction and the dispatches before the first frame fit the settle budget
The system SHALL keep each of the following within the settle-dispatch budget B_settle recorded by
the responsiveness policy constants (working value 16 ms), each one separately:
- LushText-owned application startup work;
- the work an `open` or `activate` handler performs before the main loop runs;
- every GTK main-loop dispatch from that handler until the window's composition is complete,
  including each window's first presented frame and every frame dispatch that shows a newly
  composed surface.

The budget SHALL hold on the reference lane (private `mutter --headless`, `GSK_RENDERER=cairo`, at
most 16 cores, release build, idle machine) for a fresh profile and a steady-state profile, under
both the keyfile and the dconf GSettings backends.

The `open` and `activate` handlers SHALL perform no window construction before the main loop runs:
they SHALL record the request and schedule the first composition stage. The window SHALL be
composed in a fixed sequence of stages, each built in its own dispatch within B_settle:
1. a minimal first-frame shell (header bar, tab bar, a geometry-faithful body placeholder, and the
   status bar), realized and then presented;
2. after its first frame, the body skeleton and the visible secondary surfaces;
3. the hidden surfaces.

A stage's successor SHALL NOT start before the frame that shows that stage has painted.

When an action, accelerator, activation file open, session-restore completion, or automation call
needs a surface whose stage has not run, the system SHALL first complete the stages that surface
needs, in order and synchronously, within B_settle. The operation SHALL then behave exactly as it
does on a fully composed window. Accelerators and actions SHALL NOT be dropped during composition.

Toolkit-owned startup (`gtk_init`) SHALL be measured and reported separately, and is not counted
against this budget. The one-time cost of the first toplevel's realize SHALL be counted against
the budget of the dispatch that incurs it. If that cost alone exceeds B_settle on the reference
lane, the measured lane SHALL report the dispatch as over budget, and the programme record SHALL
record it against the ledger axiom that pins it.

#### Scenario: Steady-state launch composes the window within budget
- **WHEN** the release build opens any file on a profile whose settings were already migrated
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle, the first presented frame and each composition frame included

#### Scenario: First launch composes the window within budget
- **WHEN** the release build opens a file on a fresh profile under the keyfile GSettings backend
- **THEN** every dispatch from the `open` handler until composition is complete is at most B_settle
- **AND** no GSettings backend write occurs before the first presented frame

#### Scenario: The open handler defers all window work
- **WHEN** the application handles `open` or `activate` with no existing window
- **THEN** the handler returns to the main loop without instantiating a window template, realizing a surface, or presenting a window

#### Scenario: A staged surface is needed early
- **WHEN** a user presses Ctrl+Shift+F, or an automation client activates `win.toggle-sidebar`, before the stage owning that surface has run
- **THEN** the stages that surface needs complete synchronously within B_settle before the action proceeds
- **AND** the action's visible effect, state, and automation snapshot equal those on a fully composed window

#### Scenario: Files opened during composition
- **WHEN** the activation names files, or session restore completes, before the body stage has run
- **THEN** each file opens as a tab through the ordinary open path once the body stage parents the tab view, with the same duplicate-path, draft-claim, and focus behaviour as on a composed window

#### Scenario: The first realize exceeds the budget on its own
- **WHEN** the reference lane measures the realize dispatch of the minimal shell above B_settle
- **THEN** the lane reports that dispatch as over budget
- **AND** the programme record states that the budget is not met and names the toolkit axiom and the upstream report
