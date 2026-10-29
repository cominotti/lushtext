## Context

**The stream.** This is C4c of the long-line responsiveness programme
(`docs/next/long-line-documents.md`). The maintainer invariant is fixed and the constants live in
`model/responsiveness_budget.rs`:

- every settle dispatch fits B_settle (16 ms);
- every interaction dispatch fits B_interact (8 ms);
- input-to-presented-frame takes at most 2 reference frames (8,333 µs each).

C4b (`first-frame-window-shell`, archived 2026-10-29) composes the window in stages:

- W warms the one-time text and icon state;
- C constructs the whole window, with the staged content invisible;
- R realizes the window;
- P presents it;
- F1 is the first frame;
- B1, B2 and B3 reveal the content stack (and admit the activation files), the workspace
  sidebar, and the properties panel. Each one waits for the previous frame to paint.

Its idle verdict met B_settle in every dispatch. The maintainer accepted the reported fill-in,
51–61 ms (median 58.9 ms), on 2026-10-29, on the condition that it is reduced right away. The
target is ≤ 40 ms (5 reference frames) or lower, with:

- no dispatch over B_settle;
- no chrome movement;
- a first frame no later than today.

### Method

The measurements use a temporary A/B patch on the release `responsiveness-probes` build. A copy
is kept in `.claude/handoffs/c4c/c4c-ab-instrumentation.patch`, with the analysis scripts and every
run's artifacts; it is reverted from the tree. The patch adds:

- a span marker at every composition `after-paint`, labelled mapped or unmapped;
- a marker on the construction workflow's first `after-paint`;
- spans over the activation turn, editor-page construction, its wiring, the first language guess,
  and the tab append;
- frame-phase spans for the first eight frames, not four;
- environment-selected experiments: `LUSHTEXT_C4C_RP` (`split`, `merged`, `high`),
  `LUSHTEXT_C4C_REVEAL` (`staged`, `merged`, `content-in-p`, `all-in-p`) and `LUSHTEXT_C4C_WARM`
  (`editor-page`, `language`).

The lane ran with `make responsiveness-smoke`'s `baseline/small` cases (4 profiles: fresh or
steady, keyfile or dconf), the `plain` variant, `RESPONSIVENESS_PHASES=background`, tier `full`,
private `mutter --headless`, `1280x800@120`, cairo, CPUs 8–23. Each run held the shared
measurement flock. The runs were **exploratory**: they kept to the 1-minute load average of 1.7–6
and ran beside another agent's Kani proofs, so none meets the reference precondition of C4b's D9.
The 7-repetition pair (`x7-base`, `x7-recommended`) ran back to back at load 1.7–2.1.
`x7-base` reproduces C4b's idle verdict within 1–2 % (fill-in 57.9 vs 58.9 ms, honest first frame
46.7 vs 46.1, complete 82.1 vs 82.7), so the deltas below are credible. The verdict itself comes
from the reference run (task 6.2).

### Measured breakdown of today's fill-in

All values are medians in ms from the window request, `x7-base`, 28 runs. The timeline of one
steady/dconf run is in the handoff (`timeline.py`).

| Interval | ms | What happens |
|---|---|---|
| W, C, R | 6.2, 10.0, 5.3 | unchanged by this change |
| **realize frame** | 1.2–1.4 | Realize queues a layout on the window's frame clock. Its paint idle (`GDK_PRIORITY_REDRAW`, 120) runs before stage P's `G_PRIORITY_DEFAULT_IDLE` (200) source, on an **unmapped** window. Frame counter 0 has no presentation time. The construction workflow's first `after-paint` fires here (lane "first frame", 24.5), and so does the settings writer's first-frame gate. Seen in 28 of 28 runs and in all 28 of C4b's verdict runs. |
| P | 0.5 | `present`: CSS of the visible tree, map |
| **pacing wait** | ≈ 15 | Frame 0 has no presentation timings, so `gdk_frame_clock_paint_idle` falls back to `FRAME_INTERVAL` (16667 µs) and sets `min_next_frame_time = smooth_cycle_start + period`. F1 starts 16.7–17.1 ms after frame 0's frame time (for example 23.93 → 41.05). |
| F1 | 6.6 (layout 4.5, paint 2.1) | the shell; painted at **46.7**, presented at 49.9 |
| B1 + activation turn | 0.03 + 12.0 | B1 reveals the content stack and queues the CLI file's turn: editor page 7.0, its wiring 4.0 (of which the first `guess_language` is 3.8), tab append 1.0 |
| F2 | 7.0 (5.2 / 1.9) | the editor page and content stack, painted at 66.1 |
| B2, wait, F3 | 0.03 + ≈ 6 + 4.7 | the sidebar (empty in this profile) |
| B3, wait, F4 | 0.01 + ≈ 3.6 + 1.9 | the properties panel (hidden in this profile); complete at **82.1** |

So the lane's fill-in of 57.9 ms is:

- 22.2 ms from the unseen frame to the first painted frame (P, then ≈ 15 ms of pacing, then F1);
- 36.1 ms of honest fill-in. Of that, about 10 ms is waiting for frame ticks between the reveal
  stages, about 12 ms is the activation turn, and the rest is frame work.

The reveal dispatches themselves cost 0.03 ms each.

**C4b's first frame is not earlier than before C4b.** The pre-C4b build (a0adc5e2, the verdict's
baseline run) presented its first frame at 42.3 ms median after the end of startup. C4b paints its
first real frame at 46.1 ms (idle verdict) and presents it about 3 ms later. Its "first frame
16–18 ms" (from construction start) measured the unseen frame. C4b's "first pixels no later than
today" goal was therefore not met. The lane could not see that until `959018d2` restored the frame
records.

### Candidates measured

Experiments ran with 3 repetitions × 4 profiles. Rows marked × 7 used 7 repetitions. Values are
medians in ms from the request. "Fill" is honest: from the first painted frame to the later of
composition complete and the first painted frame after the activation turn.

| Run | Experiment | First painted frame | Complete | Fill | Largest dispatch | Note |
|---|---|---|---|---|---|---|
| v0 / **x7-base** | today | 47.9 / **46.7** | 86.0 / **82.1** | 38.1 / **36.1** | 10.8 / 10.1 | unseen frame in 12 / 28 of 12 / 28 runs |
| v1 | present in the realize dispatch | 31.6 | 66.4 | 34.2 | 10.5 | no unseen frame; R+P dispatch 7.1 |
| v2 | P as a `HIGH_IDLE` source | 31.3 | 68.5 | 37.4 | 10.5 | no unseen frame; R and P stay separate |
| v3 / **x7-recommended** | v1 + one reveal dispatch after F1 | 32.4 / **31.4** | 55.3 / **52.9** | 22.5 / **21.6** | 10.6 / 9.9 | F2 8.8 (7.9–9.5) |
| v4 | v1 + content revealed in P, B2 and B3 after F1 | 34.6 | 55.8 | 21.2 | 10.5 | F1 8.7: first frame +3 ms for no gain |
| v5 | v1 + every reveal in P | 38.0 | 57.1 | 19.0 | 10.9 | F1 12.5 (12.1–13.2): first frame +6.6 ms, near budget |
| v7 | v3 + a throwaway editor page in W | 38.4 | 54.8 | 16.7 | 12.4 | W 14.2: one-time editor cost ≈ 5 ms, moved before F1 |
| v9 | v3 + the first `guess_language` in W | 35.6 | 53.2 | 17.5 | 8.8 | W 10.1, activation turn 12.4 → 7.9 |

Findings:

1. **Pacing, not work.** v1 removes about 15 ms from everything after R. It does so by keeping the
   unseen frame from running, not by doing less.
2. **Frame-gated reveals cost a frame cycle each.** v3 reveals all three in one dispatch and
   saves about 13.5 ms. The merged reveal frame is 8.8 ms, against 7.0, 4.7 and 1.9 for the three
   frames it replaces.
3. **Moving content into the first frame does not pay.** v4 and v5 make the first frame later and
   heavier, and the composed window is no earlier, because the activation turn still follows F1.
4. **The remaining fill-in is the activation turn plus one frame.** With v3, every dispatch after
   F1 is busy: reveal 0.1, activation turn 11.7, F2 8.8. Warming the turn's one-time costs in W
   (v7, v9) only moves them before the first frame, and the composed window lands at the same
   time (54.8 and 53.2 vs 55.3). v7 also takes W to 14.2 ms under load, too close to B_settle.
5. **Not on the fill-in path at all.** These delay the first frame and the composed window
   equally, and each fits B_settle:
   - W 6.2;
   - C 10.0 (template 8.8);
   - R 5.3 (against A46's 0.2 ms for a minimal window, so most of it is ours and still
     unattributed).

   Cutting them is first-frame work, not fill-in work (Non-Goals).
6. **Pango and icon warming off the main thread** (C4b finding 13) would only shorten W, which is
   before the first frame. Pango's default font map is per thread (`pangocairo-fontmap.c`), so a
   worker cannot warm the main thread's map. It was not re-measured.
7. **A frame after completion.** About 4 ms of paint with 0.3 ms of layout follows composition in
   every variant (F3 in `x7-recommended`). It is not composition, it moves no chrome, and it fits
   B_settle. It belongs to C6 (`bound-post-open-settle`).

## Goals / Non-Goals

**Goals:**

- Content fill-in at most 40 ms (5 reference frames) in every reference run, with a target
  median of 25 ms (3 reference frames). Measured from the first frame painted on the mapped
  window, to the later of composition complete and the first painted frame that shows the
  activation's opened documents.
- The first painted frame earlier than C4b's (46.1 ms median from the request on the idle
  verdict); the expected value is about 31 ms.
- Every dispatch from the request to composition complete within B_settle, in all four profiles
  and in a new populated profile.
- No chrome movement in any frame. Every C4b adversarial finding stays fixed (Risks).
- The first frame measured where it is seen, everywhere it is reported.

**Non-Goals:**

- Shortening W, C or R (first-frame work, measured in finding 5). It is recorded as a follow-up in
  the programme record.
- Cutting the activation turn's one-time editor and language costs (finding 4). They are recorded
  as a candidate with their measured sizes; the target is met without them.
- The post-composition paint frame (finding 7, C6).
- GPU rendering (C4b D9) and `gtk_init`.

## Decisions

### D1. Realize and present in one dispatch

Stage R realizes the window and presents it in the same `G_PRIORITY_DEFAULT_IDLE` dispatch, so no
main-loop iteration separates them and the realize frame cannot run on the unmapped window. The
policy keeps `Realize` and `Present` as two stages and two spans, so evidence and tests still see
both. Execution runs them back to back. `composition_window_realized` and
`composition_window_mapped` stay observers, so a window its caller presents (widget tests)
composes the same way.

- **Measured:** the R+P dispatch is 7.1 ms median (6.6–9.6) under load. The first painted frame
  is 15.3 ms earlier. The realize frame (1.2–1.4 ms of layout on an unmapped window) is gone.
- **Why it fits now:** C4b split R from P when present took 13.5–16 ms. With the staged content
  invisible, P is 0.4–0.9 ms on the idle lane (1.8 ms in the merged dispatch, which now also does
  the window's first CSS validation without the realize frame's help).

**Alternatives:**

- *P as a `G_PRIORITY_HIGH_IDLE` source (v2).* It gives the same first frame and keeps R and P as
  separate dispatches. It is rejected as the default because its correctness rests on an
  idle-priority race with the frame clock (100 < 120 today), which a GDK change could reorder.
  It is kept as the fallback if the merged dispatch ever measures above B_settle (task 6.2).
- *Realizing nothing explicitly and letting `present` realize.* This is the same dispatch,
  without R's own span. Rejected: A46's attribution needs the R span.

### D2. One reveal dispatch after the first frame

After the first frame paints, one dispatch reveals every scheduled reveal stage that has not run:
content, then workspace, then properties, in stage order, each with its own span. It queues the
activation turn exactly as B1 does today. Composition completes on the next painted frame.
`CompositionStage` and `StagedSurface` keep their three reveal values, so the following stay as
they are:

- forced reveals (`forced_chain`);
- the evidence labels;
- the readiness blocker;
- the test-only hold.

A hold before a reveal stage reveals the stages before it and waits, as today.

The policy change is local. `next_scheduled_reveal` becomes `scheduled_reveals_now` (a `StageSet`
of the pending reveal stages before any hold), and `FramePaintedDecision::Queue` carries that set.
The exhaustive unit tests over every reachable plan are rewritten to the new decision.

- **Measured:** the merged frame (F2) is 8.8 ms median (7.9–9.5). Composition completes 13.5 ms
  earlier than v1. The honest fill-in is 21.6 ms (20.2–22.7).
- **Gate:** the lane's profiles have an empty sidebar and a hidden properties pane, so they do not
  exercise the merged frame's worst content. C4b's 2026-10-20 first-measure table (load 16) gives
  the sidebar 1.0 ms and the properties panel 3.4 ms, which suggests 13–14 ms under load. The new
  populated profile (D4) measures it. If its frame exceeds B_settle on the reference run, the
  reveal splits into two dispatches, content and workspace, then properties. That costs one frame
  cycle (≈ 8 ms) and is the fallback in task 3.3.
- **C4b task 4.5** (the costliest forced frame, B1 + B2 + B3 at once) is subsumed: that frame is
  now the ordinary path, measured by every lane run.

**Alternatives:**

- *Content in P (v4), or everything in P (v5).* Measured: the first frame is later by 3.2 or
  6.6 ms, and the composed window is no earlier, because the activation turn still follows the
  first frame. v5's first frame takes 12.5 ms, which leaves too little margin.
- *Keep frame-gated stages but schedule the next stage before the paint.* It still needs a frame
  per stage. The cost is the frame cycle, not the gate.

### D3. Measure the first frame where it is seen

- The construction workflow's first-frame watcher (`ui/window/construction.rs`) records the first
  `after-paint` on which the window is **mapped**. The following inherit it:
  - the `first_frame` span;
  - `WindowConstructionEvidence.first_frame_us`;
  - `largest_dispatch_before_first_frame_us`;
  - `settings_writes::first_frame_presented()`.

  With D1 no unmapped frame occurs, but the watcher must not depend on that.
- The composition workflow already stamps its first frame only after P (its `shell_first_frame_us`
  is honest). It gains `activation_shown_us`: the first painted frame after the activation queue
  first drains, `None` when the request named no files. Its frame handler stays connected until
  both are known. Neither stamp changes when composition completes or the readiness blocker.
- The lane:
  - reads the first presentation time from the probe's `frame` records (`959018d2`);
  - reports the fill-in as `max(composition_complete, activation_shown) − first_frame`;
  - fails a run under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` when the fill-in exceeds
    `5 × REFERENCE_FRAME_US`;
  - reports the median against `3 × REFERENCE_FRAME_US`.

  These are read from the driver's existing `REFERENCE_FRAME_US`; no new budget constant is
  added.
- `docs/next/long-line-documents.md` corrects C4b's first-frame column. The C4b verdict table
  stays as recorded, with a correction note beside it.

**Alternative:** keep the definitions and only report the honest values beside them. Rejected:
the settings writer's first-frame gate and the "no backend write before the first presented frame"
requirement would keep resting on an unseen frame.

### D4. A populated `baseline/small` profile

The lane gains a fifth construction profile, `steady-populated` (keyfile backend). Its seed adds:

- a `workspaces.json` with one workspace folder holding a few hundred entries, generated
  synthetically in the run's temp scope;
- `workspace-sidebar-visible` and `properties-sidebar-visible` set to `true`.

The verdict counts it like the other four. It is the first lane case where the sidebar has rows
and the properties pane is visible at composition.

### D5. GTK axiom A51

A51 is the next free id: A42–A45 belong to C1 and A46–A50 to C4b. Its statement: realizing a
toplevel schedules a frame on its frame clock whose paint idle (`GDK_PRIORITY_REDRAW`) runs ahead
of `G_PRIORITY_DEFAULT_IDLE` sources. When the surface is not mapped, that frame runs its layout,
emits `after-paint`, and never receives presentation timings, and the frame clock does not start
the next frame until one `FRAME_INTERVAL` (16,667 µs) after it.

- The probe realizes a window in one idle and presents it in a later one, and records each
  `after-paint`'s mapped state, frame counter, frame time and presentation.
- It then repeats with realize and present in one dispatch and asserts that no unmapped
  `after-paint` occurs.
- A sample has interactive and `--check` modes.

D1 cites A51. A toolkit update that stops running the realize frame would make D1 unnecessary but
not wrong.

### D6. Kani, policy and evidence

Kani is not extended, for the same reason as C4b's D3. The composition policy is a finite stage
order, and an exhaustive unit test enumerates every reachable plan. The geometry models and their
envelopes are unchanged.

`CompositionEvidence` gains `activation_shown_us`. The surface's three driven proofs are
re-run over the new field. `WFR-WINDOW-COMPOSITION`'s measured cells are re-derived.

## Risks / Trade-offs

Every C4b adversarial finding stays fixed. Each was checked against this design:

- **Application hold (finding 1):** unchanged. Stage C releases it, and nothing moved before C.
- **A merged early `open` (finding 2):** unchanged. A request before C merges. A request between C
  and RP finds `composition_presented() == false` and only admits files, as today. RP is one
  dispatch, so there is no window in which P has run without R.
- **Breakpoints before map (findings 3 and 4):** unchanged. They are installed in C, before RP,
  so A50's first-allocation path applies and the breakpoint-loop model's precondition holds.
- **Forced reveals (finding 7):** reads never force a stage. A forced reveal before the reveal
  dispatch completes its chain, and the reveal dispatch then reveals the rest. The forced-path
  tests run unchanged against every stage.
- **CLI opens through bounded turns, and the session restore after them (finding 8, D5 of
  C4b):** unchanged. The reveal dispatch queues the activation turn as B1 did, and the restore
  still waits for queued activation opens.
- **The readiness blocker (finding 9):** same derivation; it clears earlier.
- **No layout jump (findings 5 and 15):** the following stay gates:
  - the per-frame chrome trace;
  - the reveal-at-every-preset test;
  - `first-frame-shell-chrome`.

  The visual lane's hold before `Content` still captures the first frame.

**[Risk] The merged reveal frame on a populated window exceeds B_settle.** → D4 measures it. The
fallback is two reveal dispatches (task 3.3), which costs one frame cycle and still meets the
40 ms bound by the measured arithmetic: 21.6 + ≈ 8.

**[Risk] The merged RP dispatch grows past B_settle on a slower profile.** It measured 7.1
(6.6–9.6) under load. → The fallback is D1's `HIGH_IDLE` alternative, measured to give the same
first frame.

**[Risk] The fill-in figure looks better partly because the definition changes.** → The proposal
and the record state both definitions side by side. The composed window arrives 29 ms earlier from
the request, and that figure does not depend on any definition.

**[Risk] The reported first-frame number rises** (from about 17 ms to about 24 ms from
construction start). → The record explains that the old figure was an unpresented frame. The goal
is stated against the honest C4b value, 46.1 ms from the request.

**[Risk] Activation-shown keeps the frame handler connected longer.** With many files it runs
until the last turn. → It is disconnected at teardown (A21) as today, and it does nothing but read
the clock.

**[Trade-off] The activation turn (≈ 12 ms under load, ≈ 10 ms idle) stays the largest
composition dispatch.** It fits B_settle and is now the largest part of the fill-in. Cutting it
(its one-time editor and language state) is recorded as a candidate, with v7's and v9's
measurements.

## Migration Plan

No persisted state changes. Rollback is a revert. `test_policy.rs`'s
`set_composition_collapsed_for_test` stays for bisection.

## Open Questions

- **Should the fill-in bound become a hard lane failure?** This design makes 5 reference frames a
  failure under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` only (never in CI), with the 3-frame
  median as a target. A stricter "every run ≤ 3 frames" is possible on the measured numbers
  (20.2–22.7 under load), but it leaves about 2.5 ms of margin. That is the maintainer's choice.
- **Does the maintainer want the C4b record's first-frame figures corrected in place?** This
  design adds a correction note beside them and leaves the archived verdict text unchanged.
