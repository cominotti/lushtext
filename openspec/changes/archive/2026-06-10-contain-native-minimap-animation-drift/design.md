## Context

The current minimap/sidebar work proved the final settled endpoint: after the
workspace sidebar finishes showing or hiding, the native `GtkSourceMap` viewport
highlight returns to the correct rendered row.
The remaining defect is earlier in the frame timeline. While the sidebar is
animating, the editor column receives intermediate width allocations, and the
native `GtkSourceMap` can paint its viewport highlight from private slider
geometry that is one frame behind the visible editor/source-map allocation.

The investigation produced two important constraints. First,
`LushtextEditorPage` is a `GtkBox` subclass with a class-installed layout
manager, so a page-level `WidgetImpl::size_allocate` override is not a reliable
signal for passive viewport reflow; the text view's scroll-adjustment page sizes
are the live allocation signal. Second, the native `GtkSourceMap` slider uses
document-height estimates that are lazily validated while wrapped text reflows,
so mid-animation repairs read transient geometry.

The user-facing requirement is stricter than "eventually correct": preserve the
exact existing native minimap effect and stop the visible animation-frame drift.
That means the robust path is not another app-owned clone, recolor, or
replacement of the highlight. The robust path is to make the visual proof catch
the bad frame reliably and to coordinate editor/minimap reflow so the user only
sees either the previously rendered native pixels during the burst or the live
native source map after one settled repair.

## Goals / Non-Goals

**Goals:**

- Preserve the exact native `GtkSourceMap` viewport highlight effect, including
  styling, interaction, marker layering, and final settled behavior.
- Prove animation-frame behavior from screenshots, not from app-computed
  geometry alone.
- Fail reliably when the minimap highlight moves during workspace-sidebar
  show/hide at the reproduced intermediate size class.
- Correlate each captured frame with timestamped Automation1 geometry samples
  and reject stale or guessed frame/sample pairings.
- Fix the product by preventing visible stale intermediate-width source-map
  frames through adjustment-driven reflow detection, a native-pixel freeze
  during the width burst, and one settle-once repair from rested geometry.
- Keep final-settle visual geometry proof as a separate required lane.
- Preserve bounded, content-safe artifacts that future agents can inspect
  quickly.

**Non-Goals:**

- Do not replace the native source-map viewport highlight with an app-owned
  overlay or cloned drawing.
- Do not fade, restyle, recolor, or draw a substitute highlight as the product
  fix. A temporary snapshot of the already-rendered native map is allowed only
  during a detected width-reflow burst and must be removed after the settled
  repair.
- Do not require whole-window video goldens or exact equality of intentionally
  moving editor/sidebar pixels.
- Do not make every UI change run the expensive animation-frame lane; require it
  for minimap/source-map/editor-width/sidebar-animation sensitive changes.
- Do not expose document contents in Automation1 snapshots or visual summaries.

## Decisions

### Decision: Rendered pixels are the animation oracle

The animation proof must treat screenshot-derived pixel anchors as the pass/fail
authority for native rendered effects. Automation1 geometry can explain what
the app believed was allocated, but it cannot excuse a rendered native highlight
row that moved.

Rationale: the failure was visible in screenshots while app-level geometry could
look stable or eventually correct. The invariant is about what the user sees.

Alternatives considered:

- Use only Automation1 rectangles. Rejected because native `GtkSourceMap`
  slider allocation is private and can lag app-owned geometry.
- Use final before/after screenshots only. Rejected because the defect is gone
  by final settle.

### Decision: Stream frames must be timestamp-correlated

The visual runner should record stream frame timestamps, action trigger time,
geometry sample timestamps, phase, and sample/frame skew. A frame only counts as
intermediate proof when it maps to an intermediate sidebar/editor phase within a
bounded skew window. Missing intermediate PNG frames, stale pairings, or
proportional frame-to-sample guessing fail the case.

Rationale: the earlier framework could pass by seeing only the quiet endpoint.
Reliable animation proof needs to show that at least one captured PNG actually
covered the moving phase.

Alternatives considered:

- Map frames to geometry samples by index ratio. Rejected because capture and
  Automation1 sampling are not guaranteed to run at the same cadence.
- Accept any captured frame burst. Rejected because a burst that misses the
  intermediate phase does not prove the invariant.

### Decision: Freeze native pixels during width-reflow bursts, then repair once

The implementation observes viewport width and height through the source view's
scroll adjustments, whose page sizes track allocations during each sidebar
animation frame. The first width change starts a reflow burst and captures the
last rendered `GtkSourceMap` pixels, including the native slider outset, into a
hidden `GtkPicture` overlay. While the width is still moving, minimap margin
sync is pinned so transient wrapped-height estimates cannot move the live
slider. After the width stops changing for the debounce window, one repair runs:
restore top/left anchors when the editor rested there, recompute the compensated
source-map top margin from rested document heights, clear stale source-map
scroll, refresh markers, and reveal the live map in the same callback.

Rationale: public invalidation and scroll nudges did not make the private
source-map slider paint correctly on every intermediate frame. Freezing the last
native-rendered pixels preserves the exact effect already on screen while the
widget's transient estimates are unsafe, then returns to the live native widget
only after its settled geometry is repaired.

Alternatives considered:

- Draw our own highlight over the native map. Rejected because the user asked
  for the exact same native effect and no visual replacement.
- Keep trying arbitrary invalidation/rebind nudges without proof. Rejected
  because previous variants changed baselines or still drifted.
- Stage the entire sidebar as non-consuming layout. Rejected as broader than
  needed once the smaller native-pixel freeze plus settle repair proved the
  minimap-specific invariant.

### Decision: Separate animation proof from final-settle readiness

`visual-geometry-settled` remains the readiness predicate for final comparisons.
Animation capture starts from a settled baseline, triggers the sidebar action,
samples while the transition is moving, then separately waits for final settle
and proves the endpoint.

Rationale: a single readiness concept cannot represent both "capture the moving
bug" and "wait until everything is quiet."

Alternatives considered:

- Make readiness wait longer. Rejected because waiting longer hides the defect.
- Remove final-settle proof. Rejected because endpoint regressions remain
  possible and should remain protected.

### Decision: Proof policy must include negative cases

The policy/self-test suite should include failures for final-settle-only proof,
screenshot-sampling without stream mode, no mapped intermediate PNG, stale
timestamp pairings, app geometry moving while minimap pixels drift, and missing
required anchors.

Rationale: the framework itself is now part of the product safety surface. It
must prove it catches the exact ways it previously missed the bug.

Alternatives considered:

- Rely on the real scenario alone. Rejected because a product fix could make the
  scenario pass while the proof framework remains structurally incomplete.

## Risks / Trade-offs

- [Risk] Frame capture can perturb animation timing. -> Mitigation: record
  timing/skew metadata, require intermediate mapped frames, and keep detector
  work bounded.
- [Risk] Layout containment may subtly change the sidebar/editor animation feel.
  -> Mitigation: preserve the native minimap effect as the hard invariant, add
  visual artifacts for sidebar/editor motion, and keep changes scoped to
  minimap-sensitive consuming transitions.
- [Risk] Same-frame synchronization may appear to work on one GTK/GtkSourceView
  version but fail on another. -> Mitigation: keep rendered pixels as the
  oracle and avoid version-specific assumptions unless documented.
- [Risk] Animation proof is slower than final-settle proof. -> Mitigation:
  require it only for minimap/source-map/editor-width/sidebar-animation
  sensitive diffs.
- [Risk] New Automation1 diagnostics could leak too much. -> Mitigation:
  expose only named rectangles, timings, booleans, counters, phase labels, and
  bounded detector outputs.

## Migration Plan

1. Preserve the known failing baseline artifact from the current implementation
   and verify the new proof framework fails it for the right reason.
2. Add timestamp-correlated animation capture and negative self-tests before
   relying on the product fix result.
3. Implement the adjustment-observed native-pixel freeze and settle-once repair
   while preserving the exact native highlight.
4. Run animation-frame proof at the reproduced intermediate size class and
   final-settle proof at existing minimap/sidebar sizes.
5. Update automation docs, proof policy, rules, and skills so future visual
   work knows when animation-frame evidence is mandatory.

Rollback should keep the final-settle minimap fix and revert only the new
layout-containment or synchronization path if it introduces a broader shell
regression. The animation proof tooling should remain because it captures a
real class of visual bugs.

## Open Questions

- Which layout-containment strategy preserves the best sidebar feel while
  keeping native minimap frames stable: reserve-final-width-before-animation,
  non-consuming sidebar motion during the transition, or delayed consuming width
  commit after a frame barrier?
- Does the drift reproduce symmetrically on sidebar show and hide after the
  final framework improvements, or is one direction the only required product
  fix?
- What timestamp skew bound is strict enough to catch stale pairing but loose
  enough for slower CI hosts?
- Should the visual proof lane preserve a short GIF/video preview in addition
  to PNG frames, or are frame crops plus JSON summaries enough?
