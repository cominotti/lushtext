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
realizing before present only moves the cost: the first-frame dispatch stays at 17–22 ms.

A measured minimal shell has a header bar, an empty body and the status bar. It constructs in
1.4 ms and presents in 10–12 ms, which is almost all the one-time realize. Its first frame takes
7–9 ms. The rest of the window can then be composed in later dispatches. This is the only measured
arrangement in which every dispatch can fit B_settle.

## What Changes

- **A minimal first-frame shell.** `LushtextWindow`'s template shrinks to the chrome whose geometry
  the first frame needs:
  - the header bar, with its buttons and menu models;
  - the tab bar;
  - one geometry-faithful body placeholder;
  - the status bar.

  The `AdwTabView` is created with the shell but stays unparented until the body is composed. The
  workspace rail placeholder has the exact restored visible width. Window size and the maximized
  state are restored before the surface is realized, so the surface opens at its final size.
- **Staged composition in bounded dispatches.** Each step runs in its own dispatch and fits B_settle:
  - (W) warm text and icons;
  - (R) build and realize the shell;
  - (P) present it;
  - (B1) the body skeleton: split views, layout views, bottom sheet, breakpoints, editor box with
    the tab view, and the empty state;
  - (B2) the workspace sidebar;
  - (B3) the document-properties panel;
  - (H) the hidden surfaces: search panel, command palette, Markdown preview, Open popover. These
    are built once composition completes, or on first use.

  `window.blp` splits into a shell template plus lazily instantiated sub-templates.
  `open`/`activate` only schedule the stages, so no window work runs before the main loop.
- **Forced stages.** An action, accelerator, CLI open, session-restore completion or automation
  call that reaches an unbuilt surface first completes the stages it needs, in order. Each stage is
  bounded by B_settle. Accelerators are never dropped.
- **No visible layout jump.** The shell's placeholder geometry comes from the same pure restore
  plan and shell reconciliation (`ui/window/geometry/policy.rs`) that the composed window applies.
  A Kani harness proves the two agree for every width, preset and intent. A same-session
  visual-geometry proof shows that the header bar, tab bar, status bar and rail edge are
  pixel-identical between the shell frame and the composed frame.
- **Evidence, readiness, automation.**
  - `window_construction.stages_pending/stages_total` become real.
  - The snapshot gains the shell's first-frame and composition-complete spans and the largest
    composition dispatch.
  - A new readiness blocker, `window-composition`, joins `app-startup` and every predicate that
    depends on a staged surface.
- **Measurement made permanent.** The probe gains frame-phase spans (layout and paint of the first
  frames) and per-stage dispatch spans. The construction verdict becomes per dispatch through
  composition. `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` fails the lane on any composition dispatch
  above B_settle.
- **GTK axiom ledger growth (A42–A44):**
  - A42: the first toplevel realize has a one-time per-process cost.
  - A43: `gtk_window_show` validates CSS for the whole tree before realize, and the size
    computation and first measure run in the first frame's layout phase.
  - A44: a subtree parented into a mapped window is styled and measured in the next frame, not at
    insertion.
- **Upstream investigation.** The ~10 ms first realize belongs to GTK/GDK. It fits B_settle only
  when it runs alone with a minimal shell, with about 4–6 ms of margin on the reference lane. If
  the implementation measures it above B_settle on the idle reference run, the change records
  that against the axiom, files the profile upstream, and does not claim the budget.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: the construction budget extends to every dispatch until window
  composition completes. Composition is staged, and early access forces stages.
- `adaptive-editor-geometry`: the first-frame shell's geometry is a pure plan that matches the
  composed shell, and it is Kani-checked.
- `visual-geometry-invariants`: a same-session proof that staged composition causes no layout jump.
- `dbus-automation-spine`: real stage counts, composition spans, and the `window-composition`
  readiness blocker.
- `gtk-axiom-ledger`: toplevel presentation and late-composition behaviours are ledger axioms.
- `performance-regression-coverage`: the construction gates cover every composition dispatch and
  the forced-stage paths.

## Impact

- **Code:**
  - `resources/ui/window.blp` is split, and new sub-templates are added (`window-body.blp` and the
    surface hosts), together with the generated `.ui` files.
  - `ui/window/{imp,mod,construction}.rs`, and a new `ui/window/composition/` role home: facade,
    `policy.rs`, `execution.rs`, `evidence.rs`. Every `imp()` access to a staged template child
    goes through an ensure accessor.
  - `ui/window/geometry/{policy,execution}.rs` (shell plan), `app.rs` (`open`/`activate`
    scheduling), `ui/automation.rs`, and `ui/responsiveness_probe.rs` (frame-phase spans).
- **GTK Lush:** probes and samples for A42–A44 in `gtk-lush-axioms`.
- **Tests:**
  - a new widget module `window_composition` in the `surfaces` shard;
  - a Kani harness in `core-geometry-policies`, or a new shard if it exceeds the pull-request
    budget;
  - a new visual-geometry scenario;
  - the responsiveness lane's `baseline/small` cases.
- **Docs:**
  - `AGENTS.md`, `.agents/rules/ui.md` and `widget-wiring.md` (template-child access through
    ensure accessors);
  - `docs/workflow-readability-matrix.md` (new row `WFR-WINDOW-COMPOSITION`);
  - `docs/automation.md`, `docs/automation-reference.md`, `docs/accessibility.md` and
    `docs/accessibility-matrix.md`;
  - the GTK axiom ledger and `docs/next/formal-verification.md`;
  - `docs/next/long-line-documents.md` and `docs/next/long-line-evolution.md`.
- **User-visible:** the first pixels appear earlier than today (an estimated 25–30 ms after the
  window work starts, against 45–50 ms today). The full window is composed about 20–40 ms later.
  Chrome geometry never moves. Content areas fill in place.
- **No change** to persisted data, settings keys, packaging, or permissions.
