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
  `present()`;
- at a session restore's terminal, in `finish_session_restore`, after the selection settles.

It focuses the selected editor's text view in that dispatch when the view can take focus, so a
key already queued lands in the document, and falls back to the existing retrying
`focus_selected_editor_after_action` otherwise. A restore can finish before the content stage
(B1) has revealed the content. The handoff then waits for that reveal
(`after_content_revealed`) instead of forcing it: reads and projections must never force a stage
(`first-frame-window-shell` D5). The first full test run caught exactly that forced reveal in
`test_reads_and_projections_during_composition_force_nothing`.

**D2. The handoff never takes focus from the user.** The pure
`opened_document_takes_focus(focus, dialog_visible, transient_surface_open)` allows it only when
the focus is unset or on the chrome (header bar, tab strip, status bar), where GTK's initial pick
lands, and no AdwDialog or dismissible transient surface (palette, editor search, search panel,
header menus) is up. A focus the user put in the sidebar, the properties panel or another editor
is kept. When an activation arrives in a window whose editor already has focus, AdwTabView moves
focus to the newly selected page itself. The decision is cross-cutting, with two owning workflows
(`WFR-WINDOW-COMPOSITION`, `WFR-SESSION-RESTORE`), so it lives in `editor_focus.rs`, which already
carries no role for the same reason.

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
the probe's last `motion` record. It corrects the screen point by the difference, at most
`POINTER_AIM_ATTEMPTS` (4) moves. It clicks and waits up to 2 s (`CLICK_DELIVERY_TIMEOUT_S`) for
the button-press record, and fails the phase unless that record names an editor. Every click the
driver makes is aimed at the text, so all of them use it: the standard `click` phase,
`type-middle`, the keystroke group, recovery relaunch, draft seeding and preflight.

**D5. Typing must reach a document.** Every injected printable key without Control is flagged
`inserts`. The phase row's `delivery` counts:
- `typed`: the flagged keys;
- `to_editor`, `to_read_only_editor`, `elsewhere`: where the probe saw them arrive;
- `inserted`: the `editor_user_insert` marks in the phase.

A verified phase with any key `elsewhere`, or with fewer insertions than keys into an editable
editor, is misdirected. `report` lists it under `misdirected_typing` and the run fails, like
`frameless_cases`.

**D6. Seed the layout flag.** `fresh_state` writes `split-view-layout-migrated=true`, so every
case renders the sidebar and properties visibility it records in `settings.json`.

## Risks / Trade-offs

- Lane figures recorded before this change were taken with the sidebar shown, which left a
  920 px editor. From now on the editor is 1280 px, which paints more text per frame. Old and new
  figures are not directly comparable. An interleaved comparison runs both sides with the same
  driver, so it is unaffected.
- A future layout where the text view is covered at 1/3 of its width (an overlay, a minimap on
  the left) would make the aimed click fail loudly rather than measure silently. That is the
  intended failure.
- The focus handoff takes focus from the status bar's toggle even if the user deliberately
  tabbed to it within the first few dozen milliseconds after opening a document. That is
  negligible, and the same race exists in every editor that focuses its document on open.

## Verification

- Widget tests: `window_composition::test_an_activation_open_gives_the_opened_editor_keyboard_focus`
  (collapsed and staged) and `test_a_restored_session_gives_the_selected_editor_keyboard_focus`,
  both failing before the fix with focus `status:GtkToggleButton`.
- Driver self-tests: the `inserts` flag, target pass-through, the delivery counts, the
  misdirected verdict (sidebar, swallowed key, Protected View, older probe), `run_failed`, and
  the editor aim.
- An interleaved lane run, main/branch/main/branch, on `baseline/small` and `dense/{120,500,710}`
  with this driver on both sides, with load recorded. The handoff's `first-keys-verdict` holds
  the runs and `docs/next/long-line-documents.md` the result.
