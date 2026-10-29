## Why

LushText must stay instantly responsive, and B_settle (16 ms per settle dispatch) is never relaxed.
C4 (`bound-window-construction-dispatch`) removed every GSettings write from window construction.
What remains is still one main-loop stall of 22–26 ms (idle run `rr-20261015T052216Z`), and the
first-frame dispatch after it takes another 15–22 ms. Construction is never within budget.

This change's own measurement (2026-10-20, release `responsiveness-probes` build, `baseline/small`
cases, load average 4–7, design §"Measured breakdown") gives these numbers:

- **Constructing dispatch: 25–28 ms.** Template instantiation takes 9.5–10 ms, and `present` takes
  13.5–16 ms. About 10 ms of `present` is a **one-time, per-process cost of the first toplevel's
  realize**: a second window presents in 2–4 ms. That cost is not GL initialisation
  (`GDK_DISABLE=gl,vulkan` leaves it unchanged) and not the window icon (≤ 1 ms).
- **First-frame dispatch: 15–22 ms.** Its layout phase takes 13–19 ms. The window's first
  horizontal measure is 8–14 ms of that, which includes about 6 ms of one-time first-text and
  first-icon initialisation. Allocation takes 1.5 ms and paint 3–4 ms.

Staging hidden surfaces (C4's D4) saves only 3–4 ms. Splitting `present` from construction or
realizing before present only moves the cost while the whole tree is visible: the first-frame
dispatch stays at 17–22 ms.

A measured minimal shell (header bar, empty body, status bar) presents in 10–12 ms, which is almost
all the one-time realize, and its first frame takes 7–9 ms. The costs that must leave those two
dispatches — CSS validation, first measure and mapping — are costs of **visible** widgets, and GTK
skips invisible ones. Instantiating the whole template is a separate cost that already fits a
dispatch of its own (14.8 ms under load 16). So every dispatch can fit B_settle if the window is
built whole in one dispatch, realized and presented with its content invisible, and then revealed
in later dispatches.

## What Changes

- **Build whole, reveal in stages.** `LushtextWindow` keeps one template and every template child.
  The content that the first frame does not need is invisible in the template: the content
  stack's pages, the workspace sidebar, the document-properties panel, and the children of
  concealed revealers. The skeleton (split views, layout views, bottom sheet, editor box with the
  tab view) and the chrome are visible, and the three breakpoints are installed before the window
  is realized, as today. The rail and pane edges are therefore the real split views from the
  first frame.
- **Staged composition in bounded dispatches.** Each step runs in its own dispatch and fits B_settle:
  - (W) warm text and icons;
  - (C) construct the window: the whole template, wiring, size and maximized restore, the
    split-view restore plan, and the breakpoints;
  - (R) realize;
  - (P) present;
  - (B1) reveal the content page (the empty page or the tab view) and admit the activation's
    file opens in bounded turns;
  - (B2) reveal the workspace sidebar, when the restored layout shows it;
  - (B3) reveal the document-properties panel, when the restored layout shows it.

  From B1 on, a stage starts only after the frame showing its predecessor has painted. Hidden
  surfaces are not a stage: a concealed revealer's child is invisible from now on and is made
  visible when the revealer opens.
  `open`/`activate` hold the application, record the request, and schedule W, so no window work
  runs before the main loop. A second request before the window exists merges into the first.
- **Forced reveals.** An action, accelerator, CLI open, session-restore completion or automation
  call that needs a staged surface visible or focused first completes the stages it needs, in
  order. Reads and projections never force a stage, because every object exists from C.
  Accelerators are never dropped.
- **No visible layout jump.** A widget test proves that revealing staged content never widens the
  rail or the pane, at every preset and at text scales up to 2. A per-frame allocation trace
  proves that the header bar, tab bar, status bar and the rail and pane edges are constant in
  every frame of composition. A same-session visual-geometry proof shows the chrome
  pixel-identical between the first frame and the composed frame (invariant
  `first-frame-shell-chrome`).
- **Evidence, readiness, automation.**
  - `window_construction.stages_pending/stages_total` become real.
  - The snapshot gains the first-frame and composition-complete spans and the largest composition
    dispatch.
  - A new readiness blocker, `window-composition`, joins `app-startup`, `idle` and every predicate
    that depends on a staged surface. `window-actions-exported` does not wait for it.
- **Measurement made permanent.** The probe gains frame-phase spans (layout and paint of the first
  frames) and per-stage dispatch spans. The construction verdict becomes per dispatch through
  composition. `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` fails the lane on any composition dispatch
  above B_settle. The lane reports the content fill-in time, which the maintainer accepted up to
  40 ms (5 reference frames) after the first frame.
- **GTK axiom ledger growth (A46–A50; A42–A45 belong to C1):**
  - A46: the first toplevel realize has a one-time per-process cost.
  - A47: `gtk_window_show` validates CSS only for visible nodes, and the size computation and first
    measure run in the first frame's layout phase.
  - A48: a subtree made visible or parented in a mapped window is styled and measured in the next
    frame, not at the call.
  - A49: a concealed vertical-slide revealer measures a visible child horizontally, and an
    invisible child not at all.
  - A50: a breakpoint matching on the first allocation after map applies before the child is
    allocated; one that starts matching later goes through a one-frame snapshot swap.
- **Upstream investigation.** The ~10 ms first realize belongs to GTK/GDK. It fits B_settle only
  when it runs alone, with about 4–6 ms of margin on the reference lane. If the implementation
  measures it above B_settle on the reference run, the change records that against A46, files the
  breakdown upstream, and does not claim the budget.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: the construction budget extends to every dispatch until window
  composition completes. Composition is staged by visibility, and early access forces stages.
- `adaptive-editor-geometry`: the secondary-surface geometry is final from the first frame, and
  revealing staged content never moves it.
- `visual-geometry-invariants`: a same-session proof, plus a per-frame trace, that staged
  composition causes no layout jump.
- `dbus-automation-spine`: real stage counts, composition spans, and the `window-composition`
  readiness blocker.
- `gtk-axiom-ledger`: toplevel presentation, visibility and late-composition behaviours are
  ledger axioms.
- `performance-regression-coverage`: the construction gates cover every composition dispatch and
  the forced-reveal paths.

## Impact

- **Code:**
  - `resources/ui/window.blp` (visibility defaults of the staged content) and its generated `.ui`.
  - `ui/window/{imp,mod,construction}.rs`, and a new `ui/window/composition/` role home: facade,
    `policy.rs`, `execution.rs`, `evidence.rs`, `test_policy.rs`.
  - `app.rs` (`open`/`activate` hold the application and schedule), the revealer open and close
    paths (search panel, palette, Focus Mode affordance, Markdown preview), `ui/automation.rs`,
    `model/automation.rs`, and `ui/responsiveness_probe.rs` (frame-phase spans).
  - No change to `ui/window/geometry/` policy or its Kani models.
- **GTK Lush:** probes and samples for A46–A50 in `gtk-lush-axioms`, with its README and CHANGELOG.
- **Tests:**
  - a new widget module `window_composition` in the `surfaces` shard;
  - a new visual-geometry scenario;
  - the responsiveness lane's `baseline/small` cases.
  - No new Kani harness (design D3).
- **Docs:**
  - `AGENTS.md`, `.agents/rules/ui.md`, `widget-wiring.md` and `build.md`;
  - `docs/workflow-readability-matrix.md`: a new row `WFR-WINDOW-COMPOSITION`, and re-derived
    cells for `WFR-SHELL-GEOMETRY`, `WFR-SETTINGS-WRITES`, `WFR-SESSION-RESTORE`,
    `WFR-STARTUP-PREFLIGHT` and `WFR-AUTOMATION-SPINE`;
  - `docs/automation.md`, `docs/automation-reference.md`, `docs/accessibility.md`,
    `docs/accessibility-matrix.md` and `docs/end-user-coverage.md`;
  - the GTK axiom ledger and `docs/next/formal-verification.md`;
  - `docs/next/long-line-documents.md` and `docs/next/long-line-evolution.md`.
- **User-visible:** the first pixels appear no later than today (an estimated 40–45 ms after the
  window work starts, against 45–50 ms today). The content fills in over up to 40 ms after that.
  Chrome geometry never moves.
- **No change** to persisted data, settings keys, packaging, or permissions.
