## Context

The C6 verdict runs (`.claude/handoffs/c6-verdict/vverdict`, `rverdict`) showed this on both
`main` and the C6 branch: in `type-during-settle` on the dense cases, the window drew no frame
for 1.0–1.8 s while a key arrived every 50 ms. The stretch started either about 230 ms after
activation (once the branch had settled) or about 1,050 ms after it (`main`). It always ended
2.03–2.05 s after activation. The frame counter was contiguous, and no application dispatch of
1 ms or more fell inside the stretch. The baseline/small cases did not show it.

## Evidence

All diagnostic runs used the `main` measurement build (b6330aa3) or this branch's, the lane on
`dense/500/wrap-off`, the plain variant, `type-during-settle`, under `scripts/measure.sh`. Load
averages are in the handoff's `measure.log`.

1. **Wayland trace** (`WAYLAND_DEBUG=client`, fk-diag1). After the second key, the client made
   no commit for 1.8 s, while 34 `wl_keyboard.key` events arrived. The frame callback of its
   last commit had already been answered, so GDK was not frozen waiting for the compositor (the
   wayland surface freezes updates after each paint until `wl_surface.frame` is done). The
   window simply had nothing to draw. Commits resumed at 2.17 s after launch. Their damage
   covered only (1272, 102, 5×110) and (368, 759, 178×5), the editor's overlay scrollbar
   indicators. After the focus click, no commit damaged the text area.
2. **GtkScrolledWindow** (4.22.5, `gtkscrolledwindow.c`):
   - `indicator_value_changed` fades the indicators in over `INDICATOR_FADE_OUT_DURATION`
     (1000 ms).
   - `maybe_hide_indicator` runs every `INDICATOR_FADE_OUT_TIME` (500 ms). It starts the
     fade-out once `INDICATOR_FADE_OUT_DELAY` (2000 ms) has passed since the last value change.

   The load changes the adjustments about 30 ms after activation. That places:
   - the fade-in's tick frames from about 30 ms to about 1,050 ms on `main`;
   - the first fade-out frame at the first poll 2 s later, about 2.03 s.

   That accounts for both ends of the "stretch" and for its fixed end.
3. **Probe input targets** (this change's probe, fk-diag2). The focus click was requested at
   screen (426, 400). It arrived at window (336, 400) and (338, 400), on a non-editor widget.
   All 60 key presses went to a non-editor focus, and the buffer took 0 user insertions. The
   snapshot at the click (fk-diag3) shows why: the workspace sidebar was visible at x 0–360, and
   the text view started at x = 360, although the case's keyfile sets
   `workspace-sidebar-visible=false`. The app never reads that key here: without
   `split-view-layout-migrated` it runs the one-shot migration from the legacy `sidebar-visible`
   (default true).
4. **Click inside the text** (fk-diag4-x640). The click arrived at window (640, 400) on the
   editor. All 60 keys went to the editor and inserted 60 characters. Input-to-frame: median
   9.3 ms, maximum 23.7 ms, no key over 33 ms, no gap between frames.
5. **No click at all** (fk-diag4-noclick, fk-diag5). All 60 keys went to
   `GtkToggleButton < LushtextStatusBar`, the status bar's workspace-sidebar toggle, and none was
   inserted.
6. **Widget tests** (failing first). After `app.open(file)` the window's focus was
   `status:GtkToggleButton`, with staged and with collapsed composition alike. After a session
   restore at startup it was the same. In GTK 4.22, `gtk_window_show` calls
   `gtk_window_move_focus (TAB_FORWARD)` when the window has no focus widget, before any page
   exists. No later code moves focus to the document.

The pointer mapping itself is not explained. A plain GTK4 window in the same headless Mutter
receives requested relative moves exactly (426 → 426, 640 → 640). LushText received 640 at 640,
but 426 at 336–338 in the dense cases and at 280–297 in the populated profile. The lane no longer
trusts a requested position: it moves the pointer, reads where the probe saw it arrive, corrects,
and checks where the press arrived (D4).

### Is it real for a user?

- **The 1–2 s stretch with the keys typed into the document:** no. With the keys in the editor
  there is no stretch, and every key draws within 24 ms. The stretch was the lane's own focus
  click missing the editor.
- **Typing straight after opening a document from outside:** yes, and worse than the lane
  suggested. The keys do not appear late, they never appear: they go to the status bar's
  toggle. Space or Enter would toggle the workspace sidebar. The focus is GTK's own pick, made
  inside the app before any input arrives, so the input path does not matter. The in-process
  widget harness reproduces it without RemoteDesktop.

## Goals / Non-Goals

**Goals**
- A document opened by activation (command line, file manager) or restored at startup takes
  keyboard focus, so typing needs no click first.
- The lane proves that each input it measures reached what it was aimed at, and fails a run
  whose typing reached no document.
- The lane renders the layout each case declares.

**Non-goals**
- Explaining Mutter's relative-motion mapping. The lane now checks delivery instead of relying
  on it.
- Re-deriving the C6 decisions with the corrected layout. They are recorded as measured with
  the sidebar shown.

## Decisions

**D1. Focus handoff on open, not a change to GTK's initial pick.** At show time no document page
exists, so no initial pick can name one. The handoff runs:
- after an activation's last open turn, in composition's `run_activation_turn`, before
  `present()`, when a turn of the batch added or selected a page in this window (not when another
  window owned the path and was presented instead, nor for an open held by the startup gate);
- at a session restore's terminal, in `finish_session_restore`, after the selection settles,
  when the restore created a page.

It focuses the selected editor's text view in that dispatch when the view can take focus, so a
key already queued lands in the document; otherwise it retries briefly (six 30 ms attempts),
deciding again on every attempt, so input in between wins. A restore can finish before the content stage
(B1) has revealed the content. The handoff then waits for that reveal
(`after_content_revealed`) instead of forcing it: reads and projections must never force a stage
(`first-frame-window-shell` D5). The first full test run caught exactly that forced reveal in
`test_reads_and_projections_during_composition_force_nothing`.

**D2. The handoff never takes focus from the user.** The pure
`opened_document_takes_focus(OpenedDocumentFocusFacts)` (`ui/window/editor_focus/policy.rs`)
allows it only when all of these hold:
- the focus is unset or on the window's own chrome (header bar, tab strip, status bar), where
  GTK's initial pick lands. Only a widget in the window's own surface counts as chrome
  (`focus.native() == window`): focus in a popover or menu the chrome owns (the Open popover,
  `Ctrl+K`; the tab context and overflow menus; the main menu) is the user's;
- no AdwDialog or dismissible transient surface (palette, editor search, search panel) is up;
- the window has seen no key or button press since it was mapped. A capture-phase
  `EventControllerKey` and `GestureClick` on the window (claiming nothing) set the latch; `map`
  clears it. Any deliberate focus is kept, however it looks, including an Orca user tabbing
  through the chrome while a session restores.

A focus the user put in the sidebar, the properties panel or another editor is kept. When an activation arrives in a window whose editor already has focus, AdwTabView moves
focus to the newly selected page itself. The decision is cross-cutting, with two owning workflows
(`WFR-WINDOW-COMPOSITION`, `WFR-SESSION-RESTORE`), so it stays in the role-less, cross-cutting
`editor_focus/`; its pure half is `editor_focus/policy.rs`, named so the `ui/**/policy.rs`
mutation scope reaches it.

**D3. Probe fields, not a new probe stream.** Input records gain an optional `target` and, for
buttons and pointer motions (a new `motion` input kind), `x`/`y`. A key's target is the window's focus widget; a button's is
`gtk_widget_pick` at the event position. A widget inside an editor page's text view is `editor`,
or `read-only-editor` when the view is not editable (Protected View). An `editor_user_insert`
mark follows each buffer insertion inside a user action; load and replacement installs never
open one. The mark does not arm the settle sentinel. An older binary's records carry none of
this, and the lane treats them as unverified, never as failed, so `main` stays measurable as a
baseline.

**D4. Aim from the app's geometry, correct by arrival, then verify.** `editor_click` reads the
snapshot's `visual_geometry` `source-view` rectangle and picks a point inside it, at 1/3 of its
width unless asked otherwise. The point must not lie under a surface that can cover the text
view: an overlaid workspace sidebar, or the document-properties bottom sheet, which the populated
profile opens over the lower half of the editor. The lane then moves the pointer there and reads
the probe's last `motion` record. Right after a launch the snapshot may not yet show a free point
(the populated profile's layout settling), so the click waits up to `EDITOR_POINT_WAIT_S` (3 s)
for one and, on a probe build, fails as `no-editor-point` rather than click a blind point. It corrects the screen point by the difference, at most
`POINTER_AIM_ATTEMPTS` (4) moves. It clicks and waits up to 2 s (`CLICK_DELIVERY_TIMEOUT_S`) for
the button-press record, and fails the phase unless that record names an editor. Every click the
driver makes is aimed at the text, so all of them use it: the standard `click` phase,
`type-middle`, the keystroke group, recovery relaunch, draft seeding and preflight.

**D5. Typing must reach a document.** Every injected printable key without Control is flagged
`inserts`. The phase row's `delivery` counts:
- `typed`: the flagged keys;
- `to_editor`, `to_read_only_editor`, `elsewhere`: where the probe saw them arrive;
- `inserted`: the `editor_user_insert` marks in the phase.

- `undelivered`: flagged keys with no input record at all.

A phase with any undelivered key (whatever the probe's vintage: a phase whose keys never reached
the application must not pass because there is no target to judge), or a verified phase with any
key `elsewhere` or with fewer insertions than keys into an editable editor, is misdirected.
`report` lists it under `misdirected_typing` and the run fails, like `frameless_cases`.

**D5a. Typed text is never "no visible change".** A text key into an editable editor always
changes what is drawn. `input_to_frame` therefore never applies the `no-visible-change` rule to
it: its latency runs to the eventual frame and the row is flagged `withheld_frame`, as is such a
key whose first recorded frame starts after the phase. A phase with a withheld frame is listed
under `withheld_typing_frames` and fails the run. A key after which the log holds no frame at all
is left `no-frame` and unjudged: the probe writes a frame only once it or a later one is final,
and headless Mutter completes a lone last frame only at the next commit, so the last key of a
quiet phase has no record without having been withheld. Keys into a protected, read-only editor
are flagged `protected` and reported apart (`protected_input_to_frame`), outside the editable
input-to-frame figures and verdicts.

**D5b. A recovery that cannot focus the editor ends the case.** After a relaunch the driver
retries its proven editor click; if none lands, the case stops with a driver failure instead of
running its paging and typing phases on a window whose keys go elsewhere.

**D6. Seed the layout.** `fresh_state` writes `split-view-layout-migrated=true` with the case's
pane visibility, so every case renders the sidebar and properties visibility it records in
`settings.json`. The fresh construction profiles that show no pane instead seed the legacy
`sidebar-visible=false` unmigrated, so their launch still runs the one-shot split-view migration
(and writes its result once, after the first frame) and renders the same layout.

## Risks / Trade-offs

- Lane figures recorded before this change were taken with the sidebar shown, which left a
  920 px editor. From now on the editor is 1280 px, which paints more text per frame. Old and new
  figures are not directly comparable. An interleaved comparison runs both sides with the same
  driver, so it is unaffected.
- A future layout where the text view is covered at 1/3 of its width (an overlay, a minimap on
  the left) would make the aimed click fail loudly rather than measure silently. That is the
  intended failure.
- The user-input latch also keeps the focus after an incidental key or click in the window
  before the document opened (a click to raise it, say). The document then stays where GTK's
  focus left it, the pre-change behaviour, which is the safe side: the handoff is a convenience
  and must never take the keyboard from a deliberate choice.

## Verification

- Widget tests: `window_composition::test_an_activation_open_gives_the_opened_editor_keyboard_focus`
  (collapsed and staged) and `test_a_restored_session_gives_the_selected_editor_keyboard_focus`,
  both failing before the fix with focus `status:GtkToggleButton`. During a composition held
  before its last reveal, a restore keeps the user's focus in the sidebar, an open dialog, the
  command palette, the Open popover, and on the chrome after a key press
  (`test_a_held_restore_keeps_focus_*`); the Open popover and key cases failed first without the
  native check and the latch. `test_a_held_restore_hands_an_untouched_window_to_the_document` is
  the control. Unit tests in `editor_focus/policy.rs` cover every input combination.
  `test_the_focus_handoff_precedes_a_protected_documents_alert_announcement` proves, for a
  command-line open and a session restore of a protected document, that the editor takes focus
  before the one-time "View-Only: Very Long Lines" alert is announced, so a screen reader's
  focus-change interruption cannot cut that announcement off.
- Driver self-tests: the `inserts` flag, target pass-through, the delivery counts, the
  misdirected verdict (sidebar, swallowed key, Protected View, older probe, no input records at
  all, a lost key), withheld frames (a late frame, a frame after the phase, no frame in the log),
  protected keys, `run_failed`, the fresh profiles' migration seed, and the editor aim.
- An interleaved lane run, main/branch/main/branch, on `baseline/small` and `dense/{120,500,710}`
  with this driver on both sides, with load recorded. The handoff's `first-keys-verdict` holds
  the runs and `docs/next/long-line-documents.md` the result.
