## Why

C4b (`first-frame-window-shell`) put every dispatch of window composition within B_settle. Its
idle verdict (2026-10-29) reported the content fill-in at 51–61 ms (median 58.9 ms). The
maintainer accepted that on the condition that it is reduced right away. The target is at most
40 ms (5 reference frames at 120 Hz), or lower, with no dispatch over B_settle, no chrome movement,
and a first frame no later than today.

This change's measurement (design, "Measured breakdown") found that the fill-in is not work. It
is frame pacing, plus a measuring error:

- **The "first frame" C4b reports is a frame nobody sees.** Stage R realizes the window in its
  own dispatch. Realizing schedules a frame on the window's frame clock, and that frame runs
  before the next `G_PRIORITY_DEFAULT_IDLE` source, while the window is still unmapped. It lays
  the window out (1.2–1.4 ms) and is never presented. The lane's `first_frame` span (the construction
  workflow's first `after-paint`) and the settings writer's "first frame" both fire on it, in all
  28 of the verdict's runs.
- **That frame delays the real first frame by a full 60 Hz interval.** It has no presentation
  timings, so GDK's frame clock (`gdkframeclockidle.c`: `FRAME_INTERVAL 16667`,
  `min_next_frame_time = smooth_cycle_start + period`) starts the next frame no earlier than
  16.7 ms after it. Stage P is done 0.5 ms after that unseen frame, and the first painted frame
  still waits about 15 ms.
- **The reveal stages pay one frame cycle each.** B1, B2 and B3 each wait for the previous paint,
  and each dispatch is 0.03 ms. So the content, the workspace sidebar and the properties panel
  appear over three more frames.

Measured honestly (the first frame painted on the mapped window, at 46.1 ms median from the
request on the idle verdict), C4b's fill-in is 36.8 ms. Its first painted frame is also **later**
than before C4b: the pre-C4b build presented its first frame at 42.3 ms median. The C4b record's
"first frame 16–18 ms" is the unseen frame.

## What Changes

- **Realize and present in one dispatch (stage RP).** No frame can run between them, so the
  window's first frame is its first presented frame, and the clock does not pace it. This was
  measured at 7.1 ms median (6.6–9.6) per dispatch. The first painted frame moves from 46.7 to
  31.4 ms (median, from the request).
- **One reveal dispatch after the first frame.** The content stack, the workspace sidebar and the
  document-properties panel are revealed together after the first frame paints, and composition
  completes on the next painted frame. That frame was measured at 8.8 ms median (7.9–9.5). A
  forced reveal still completes only the stages its surface needs.
- **The first frame is measured where it is seen.** Each of these uses the first frame painted
  while the window is mapped:
  - the construction `first_frame` span;
  - `WindowConstructionEvidence.first_frame_us`;
  - the settings writer's first-frame gate;
  - the lane's fill-in.

  The lane also reports the first presentation time from the frame records. It reports the
  fill-in up to the later of composition complete and the first painted frame after the activation
  files opened, so an opened document counts as content.
- **A budgeted fill-in target.** The fill-in is at most 40 ms (5 reference frames) in every
  reference run, with a target median of 25 ms (3 reference frames). These are multiples of the
  existing `REFERENCE_FRAME_US`, not new budget constants. The first presented frame must come
  earlier than C4b's measured one.
- **GTK axiom A51.** Realizing a toplevel schedules a frame that runs before default-idle sources.
  On an unmapped surface that frame is never presented, and the frame clock paces the next frame
  one `FRAME_INTERVAL` (16.667 ms) after it. A `gtk-lush-axioms` probe and sample pin it.
- **A populated lane profile.** A `baseline/small` profile with a restored workspace folder and the
  properties pane shown. It measures the merged reveal frame on its worst realistic content, which
  C4b's lane never did (its sidebar was empty and the pane hidden).

Measured on the A/B instrumentation patch, 4 profiles × 7 repetitions, load 1.7–2.1 with another
agent's Kani proofs running, so **exploratory, not the reference precondition**:

| | today (C4b) | proposed |
|---|---|---|
| fill-in the lane reports | 57.9 ms (51.3–61.0) | 21.6 ms (20.2–22.7) |
| fill-in, honest (from the first painted frame) | 36.1 ms (29.4–38.2) | 21.6 ms (20.2–22.7) |
| first painted frame, from the request | 46.7 ms | 31.4 ms |
| first presentation, from the request | 49.9 ms | 35.4 ms |
| composition complete, from the request | 82.1 ms | 52.9 ms |
| largest dispatch until complete | 10.1 ms | 9.9 ms |

The lane's reported "first frame" figure (from construction start) **rises** from about 17 ms to
about 24 ms, because it stops counting the unseen frame. The first frame anyone sees comes about
15 ms earlier.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: the composition sequence changes. Realize and present share one
  dispatch, and one reveal dispatch follows the first frame. The first frame is the first frame
  presented on the mapped window. The content fill-in is bounded at 5 reference frames, with a
  3-frame median target.
- `performance-regression-coverage`: the composition gate proves that no frame runs on the
  unmapped window and that the reveals run in one dispatch. The measured lane reports the honest
  first frame, the first presentation, and the fill-in up to the opened document, and it gains a
  populated profile.
- `dbus-automation-spine`: the first-frame field means the first frame painted on the mapped
  window, and the snapshot gains the activation-shown span.
- `gtk-axiom-ledger`: A51, the realize frame and the frame clock's pacing after an unpresented
  frame.

## Impact

- **Code:**
  - `ui/window/composition/` (`policy.rs`: the stage plan and the painted-frame decision;
    `execution.rs`: the merged lifecycle dispatch, the single reveal dispatch, the
    activation-shown stamp; `evidence.rs`; the facade narration);
  - `ui/window/construction.rs` (the first-frame watcher waits for a mapped frame);
  - `ui/settings_writes.rs` (only through that watcher);
  - `ui/automation.rs`, `model/automation.rs`, `model/construction_spans.rs`.
- **Lane:** `scripts/responsiveness-smoke-driver.py`: the honest first frame and presentation,
  the fill-in definition, the populated profile, the 40 ms fill-in check under
  `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1`, and the self-tests.
- **GTK Lush:** the A51 probe and sample in `gtk-lush-axioms`, plus its README and CHANGELOG.
- **Tests:** the `window_composition` widget module (shard `surfaces`, no new module, so no shard
  table change); the `window_construction` gate's first-frame assertions. No Kani change.
- **Docs:**
  - `AGENTS.md` (the staged-composition decision);
  - `docs/workflow-readability-matrix.md` (`WFR-WINDOW-COMPOSITION` and the cells of
    `WFR-SETTINGS-WRITES` re-derived);
  - `docs/automation.md` and `docs/automation-reference.md`;
  - the GTK axiom ledger and `docs/next/formal-verification.md`;
  - `docs/next/long-line-documents.md`, which gets a C4c row and section, and corrects C4b's
    first-frame figures;
  - `docs/next/long-line-evolution.md`;
  - `.agents/rules/build.md` (lane metrics).
- **User-visible:** the first pixels appear about 15 ms earlier, and the composed window, with the
  opened document, about 29 ms earlier. Chrome never moves.
- **No change** to persisted data, settings keys, packaging or permissions.
