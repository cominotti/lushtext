## Why

The C6 interleaved verdict (`bound-post-open-settle`, 2026-11-02) found that on `main` and on the
C6 branch, every key with an input-to-frame time over 33 ms was the first key after a 1.0–1.8 s
stretch in which the window drew no frame at all. The stretch ended about 2.03 s after
activation, with no application dispatch of 1 ms or more inside it. If a real user saw this, the
first characters typed after opening a document would not appear for up to 2 s. The programme
queued it as the next change, top priority.

Investigation found two separate problems. One is in the lane and one is in the app.

- **The lane typed into nothing.** `type-during-settle` clicks at a fixed screen point to give
  the editor focus. In the case layout that point fell inside the workspace sidebar, 22 px left
  of the text view. The request was screen (426, 400); the probe saw it arrive at window
  (336–338, 400). Every one of the 60 keys went to a widget outside the editor, and none was
  inserted. The window had nothing to draw. The frames on either side of the "stretch" were
  GtkScrolledWindow's overlay-indicator fades, which damage only the 5 px indicator strips:
  - a fade-in after the load changed the adjustments;
  - a fade-out that starts once 2 s have passed since the last value change
    (`INDICATOR_FADE_OUT_DELAY`, polled every 500 ms).
  The fade-out is why the stretch ended about 2.03 s after activation. The keys over 33 ms were
  keys attributed to the first fade-out frame. With the click inside the text, all 60 keys
  insert and draw: input-to-frame median 9.3 ms, maximum 23.7 ms, no stretch.
  The sidebar was visible only because the lane never seeded `split-view-layout-migrated`, so
  the app migrated from the legacy `sidebar-visible` (default true) over the case's
  `workspace-sidebar-visible=false`.
- **A document opened from outside did not take the keyboard.** With no click, after a
  command-line open the keyboard focus was the status bar's sidebar toggle. The same was true
  after a session restore at startup. GTK gives a window its initial focus in `gtk_window_show`
  (move focus forward from nothing). That happens before any activation or restored page exists,
  and nothing moved focus afterwards. A user who opens a file and starts typing sees nothing:
  the keys go to the toggle. This is a real, pre-existing defect, independent of the input path.
  Widget tests reproduce it with staged and with collapsed composition.

## What Changes

- **App:** after an activation's last open turn, and at a session restore's terminal, the
  selected document's editor takes keyboard focus. It does so only when focus is unset or on the
  window chrome (header bar, tab strip, status bar), and no dialog or transient surface is up.
  The pure decision is `opened_document_takes_focus` in `ui/window/editor_focus.rs`, which has
  two owning workflows: window composition and session restore.
- **Probe:** each input record carries where the input was headed:
  - `target`: `editor`, `read-only-editor`, `other` or `none`. For a key this is the focus
    widget; for a button, the widget under the pointer.
  - `x` and `y`: the window coordinates of a button or a pointer motion (a new `motion` kind).

  An `editor_user_insert` mark follows every editor-buffer insertion made inside a user action.
  All of this is measurement-build only.
- **Lane:**
  - Every editor click aims at a point of the snapshot's `source-view` rectangle that no sidebar
    or bottom sheet covers. It corrects the pointer by where the probe saw it arrive, and fails its
    phase unless the probe's record of the press names an editor.
  - Each phase reports `delivery` for its text keys, and a phase whose keys arrived outside an
    editor or inserted nothing fails the run (`misdirected_typing`).
  - The keyfile seeds `split-view-layout-migrated`, so the declared layout is the one rendered.

## Capabilities

### New Capabilities

### Modified Capabilities

- `desktop-open-activation-coverage`: adds that an opened or restored document takes keyboard
  focus.
- `responsiveness-measurement`: adds that the lane proves its inputs reached the document it
  measures.

## Impact

- Code: `ui/window/editor_focus.rs`, `ui/window/composition/execution.rs`,
  `ui/window/session_restore/admission.rs`, `ui/window/transient_dismissal/mod.rs`,
  `ui/responsiveness_probe.rs`, `services/responsiveness_log.rs`,
  `scripts/responsiveness-smoke-driver.py`.
- Tests: two widget tests in `window_composition`, a unit test of the focus decision, an encoding
  test of the new probe fields, and driver self-tests of the delivery gate and the editor aim.
- Measurements: every lane case now renders its declared layout (sidebar hidden). Lane figures
  recorded before this change were taken with the workspace sidebar shown, so the editor was
  920 px wide instead of 1280 px. The older records carry no input target, so they cannot show
  whether the main launch's clicks (`click`, then `pagedown` and `type-start`) reached the
  editor. From this change on, every editor click and every typed key is checked.
- Docs: `docs/next/long-line-documents.md`, `docs/accessibility.md`,
  `docs/accessibility-matrix.md` and `.agents/rules/build.md`.
