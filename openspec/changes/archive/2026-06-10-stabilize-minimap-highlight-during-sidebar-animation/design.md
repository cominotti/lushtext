## Context

The completed `fix-native-minimap-rendered-drift` change proved and corrected
the endpoint: after the workspace sidebar finishes showing or hiding, the native
`GtkSourceMap` viewport highlight and first minimap content row settle at the
right rendered pixel rows. The newly reported symptom is different. During the
sidebar animation itself, the native highlight can briefly paint at an
intermediate wrong position before quieting in the correct final position.

The current implementation detects editor width changes in
`LushtextEditorPage::size_allocate()`, then schedules minimap reflow repair work
through `glib::idle_add_local_once`. That is appropriate for final settling, but
an `AdwOverlaySplitView` sidebar animation can allocate a sequence of
intermediate editor widths, and GTK can snapshot a frame before the idle repair
has synchronized the source map. The current visual geometry runner also waits
for final sidebar geometry before capture, so it is structurally unable to prove
or disprove a transient animation-frame jump.

## Goals / Non-Goals

**Goals:**

- Preserve the exact native `GtkSourceMap` viewport highlight effect.
- Prove whether the highlight jumps during sidebar show/hide animation frames.
- Fix the animation path so the rendered native highlight remains stable through
  intermediate editor widths, not only at the final endpoint.
- Keep expensive marker refreshes debounced while moving lightweight native
  source-map geometry synchronization early enough for the frame that will paint.
- Produce bounded artifacts that show frame index, elapsed time, sidebar/editor
  geometry, source-map diagnostics, detected pixel rows, and failure reasons.
- Keep the existing final-settle visual geometry proof intact.

**Non-Goals:**

- Do not replace, hide, restyle, or clone the native `GtkSourceMap` highlight.
- Do not require whole-window video goldens or exact equality of changing editor
  body pixels during animation.
- Do not make every normal visual geometry case capture animation bursts.
- Do not expose document text or private persistence identifiers in frame
  artifacts.
- Do not remove sidebar animation as a shortcut unless a separate design accepts
  that UX change.

## Decisions

### Decision: Add a dedicated animation-frame visual lane

The visual runner should gain a scenario mode that starts frame sampling, triggers
the workspace-sidebar action, and records screenshots plus bounded Automation1
snapshots for a short window that covers the sidebar animation. It should detect
declared pixel anchors in each frame and compare them against either the initial
stable row, a bounded expected row band, or declared relative relationships.

Rationale: the current before/after proof waits until the defect is gone. A
burst trace gives agents the evidence they need without relying on human timing.

Alternatives considered:

- Increase sleeps before final capture. Rejected because the symptom happens
  before final capture.
- Use a continuous video file only. Rejected because frame-by-frame PNG plus JSON
  artifacts are easier to inspect, diff, and summarize.

### Decision: Correlate rendered rows with app geometry per frame

Each sampled frame should preserve enough bounded state to explain the source of
movement: editor width, sidebar x/width, minimap shell/source-map rectangles,
native minimap diagnostics, source-view adjustment, source-map adjustment,
document-height ratio, top-margin compensation, and detected pixel rows.

Rationale: if rendered pixels move, we need to know whether app diagnostics moved
with them, lagged behind them, or stayed stable while the private native slider
painted stale geometry.

Alternatives considered:

- Store only screenshots. Rejected because that would prove the jump but not
  guide a narrow fix.
- Store full widget trees or document text. Rejected because the automation
  boundary should remain bounded and content-safe.

### Decision: Separate lightweight per-frame sync from debounced marker work

The likely product fix should distinguish two classes of minimap work:

1. lightweight native source-map synchronization required before the next frame
   paints, such as wrap mode, dynamic top margin, source-map adjustment clamps,
   source-map resize/draw invalidation, and possibly a frame-scoped rebind if
   measured necessary;
2. expensive semantic marker recomputation, which can stay debounced and settle
   after the animation as long as markers do not visibly contradict the native
   highlight contract.

Rationale: doing all minimap work synchronously on every animation allocation
risks UI jank, but delaying the native slider prerequisites until idle lets GTK
paint stale frames.

Alternatives considered:

- Run the full existing minimap refresh on every allocation. Rejected because it
  can rescan document markers during a live animation.
- Leave sync in idle and only make readiness wait longer. Rejected because users
  still see the transient.

### Decision: Use GTK frame-clock timing for animation correctness

If measurement confirms the idle callback trails painting, the implementation
should move the native highlight prerequisites into a frame-clock-aware path:
for example a coalesced tick callback or immediate post-allocation sync that
runs before snapshot for the next frame. The design should not assume the exact
mechanism before frame evidence is collected.

Rationale: the bug is about what GTK paints during a frame, so the fix should be
framed in GTK's allocation/snapshot/frame-clock phases rather than a later final
readiness predicate.

Alternatives considered:

- Fixed-duration animation blackout for the minimap. Rejected because hiding the
  native highlight during sidebar motion changes the effect and masks the bug.
- Disable sidebar animation. Rejected as a UX regression and too broad for this
  minimap-specific issue.

### Decision: Keep policy scoped but explicit

Proof-policy checks should require animation-frame evidence only for files that
can affect minimap native rendering, sidebar/editor allocation animation, or the
animation proof tooling. Ordinary non-animation UI changes should not inherit the
slower animation burst lane.

Rationale: the check should be strong where it matters without making every UI
change pay for high-frame-count screenshot capture.

Alternatives considered:

- Make all visual-sensitive work run animation capture. Rejected because it is
  slower and unrelated to many surfaces.
- Keep animation capture manual-only. Rejected because the previous manual-only
  gap is how the transient escaped the final-settle framework.

## Risks / Trade-offs

- [Risk] Frame burst capture is noisy on slow or loaded hosts. -> Mitigation:
  sample by bounded frame count/time, preserve timing metadata, and allow clear
  unsupported-host skips without counting skipped coverage as verified.
- [Risk] Per-frame PNG capture slows animation enough to hide the issue. ->
  Mitigation: support a low-overhead mode that samples Automation1 every frame
  and screenshots at configured cadence, then compare against a higher-fidelity
  targeted capture when needed.
- [Risk] Moving sync earlier could make size allocation too expensive. ->
  Mitigation: keep only lightweight source-map geometry work on the frame path
  and leave marker recomputation debounced.
- [Risk] GtkSourceView private slider behavior varies by version. ->
  Mitigation: keep screenshot pixels authoritative and diagnostics explanatory.
- [Risk] A frame-clock fix could leave stale callbacks during rapid resize. ->
  Mitigation: use generation counters and visibility/mapping checks like the
  existing minimap readiness path.
- [Risk] Captured screenshots may include generated fixture text. -> Mitigation:
  use generated non-private fixtures for committed scenarios and keep summaries
  bounded.

## Migration Plan

1. Add an animation-frame reproducer for the user-reported sidebar show/hide path
   at the known intermediate size class.
2. Extend the visual runner to capture frame bursts and summarize per-frame
   native minimap pixel anchors.
3. Use the frame artifacts to identify whether idle repair, source-map layout
   lag, rounding thresholds, or native slider invalidation is causing the jump.
4. Implement the narrowest product fix that preserves the native effect and keeps
   expensive work off the animation frame path.
5. Run targeted animation-frame proof, final-settle visual geometry proof, and
   focused Rust/widget tests.
6. Update docs, proof policy, and agent guidance so final-settle and
   during-animation evidence remain distinct.

Rollback should preserve the final-settle fix and the native minimap effect. If
animation-frame tooling is too noisy, narrow the detector or sample cadence; do
not weaken the product requirement to final-settle-only.

## Open Questions

- How many frames or milliseconds are needed to reliably catch the reported
  transient without making the lane too slow?
- Does the rendered jump happen on sidebar show, hide, or both?
- Does it happen only at top-of-file, or also at mid-document scroll positions?
- Is the root cause idle repair timing, mixed editor/source-map layout epochs,
  integer margin rounding at intermediate widths, or private native slider
  invalidation?
- Should the product fix use immediate post-allocation sync, a coalesced tick
  callback, or a narrowly scoped animation-active state?
