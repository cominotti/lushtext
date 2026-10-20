## Context

**The stream.** This is C4b of the long-line responsiveness programme
(`docs/next/long-line-documents.md`, "C4"; `docs/next/long-line-evolution.md` §2 item 0). The
maintainer invariant is fixed:

- every settle dispatch fits B_settle (16 ms);
- every interaction dispatch fits B_interact (8 ms);
- input-to-presented-frame takes at most 2 reference frames.

The constants live in `model/responsiveness_budget.rs` and are never relaxed. C4 removed every
GSettings write from construction and left 22–26 ms of `present` plus template instantiation
(idle run `rr-20261015T052216Z`). C4's D4, staging hidden surfaces, measured only 3–4 ms and was
not implemented (tasks 6.2 and 6.4–6.6 are annotated in the C4 archive).

**What C4 left.** Today, `open` and `activate` run inside `g_application_run`, before the first
main-loop iteration. Each one does `LushtextWindow::new` (template, `constructed`, 18 wiring
stages), then `present`, then opening the files. One `window.blp` template builds the whole tree:

- the header bar, with the Open popover and menus;
- the tab bar;
- `GtkLushClipBin`;
- three `AdwOverlaySplitView`s;
- two `AdwMultiLayoutView`s;
- an `AdwBottomSheet`;
- the tab view;
- the empty `AdwStatusPage`;
- the sidebar;
- the properties panel;
- the search panel, palette and preview revealers;
- the status bar.

Window code has 32 template children, read through `imp()`: `tab_view` about 134 times, `sidebar`
61, `search_panel` 58 and `command_palette` 23.

### Measured breakdown (2026-10-20)

**Method.** The measurements used a temporary, uncommitted instrumentation patch on the
`responsiveness-probes` release build:

- spans around GTK's `realize` and `map` handlers;
- the frame clock's `update`, `layout`, `paint` and `after-paint` phases for the first four frames;
- the window's `measure` and `size_allocate` vfuncs;
- a post-order first-measure walk attributed to subtrees;
- environment-selected experiments.

The patch is reproduced as task 1.2 and made permanent there. All runs used `make
responsiveness-smoke`'s `baseline/small` construction cases, `plain` variant, 5–7 repetitions,
private `mutter --headless`, cairo, CPUs 8–23, and `RESPONSIVENESS_PHASES=background`.

**Load.** Two other agents were building, so load average was 4–7 for the runs below. The runs at
16–18 are marked. `btrfs-stall-report` said "Storage was NOT a bottleneck" for every run (IO
pressure ≤ 1.1 %). These are **not** idle reference figures: C4's idle run was about 10 % faster.
Task 1.3 re-runs on an idle machine before implementation.

`perf` and `sysprof` are not installed in the toolbox, so attribution inside GTK is by spans and
A/B experiments, not samples. That is open question 2.

**Baseline, all four profiles** (median of 5; "late" is the watchdog lateness of each dispatch in
time order):

| Profile / backend | template | present | construction | first-frame layout (measure_h / allocate) | paint | dispatches ≥ 4 ms, in order |
|---|---|---|---|---|---|---|
| fresh / keyfile | 9.5 | 14.3 | 25.4 | 13.8 (8.0 / 1.3) | 3.3 | 26–29, 15–20, 12–13, 6, 4; flush 8.5–9.6 |
| steady / keyfile | 10.0 | 16.2 | 28.1 | 17.0 (10.0 / 1.5) | 3.4 | 28–36, 16–22, 14–19, 6–8 |
| fresh / dconf | 10.1 | 15.1 | 28.3 | 18.0 (12.0 / 1.5) | 3.6 | 27–32, 16–22, 13–17, 6–9 |
| steady / dconf | 10.4 | 15.0 | 27.8 | 16.5 (10.6 / 1.6) | 3.6 | 29–47, 18–20, 14–17, 7–8 |

Every other construction stage, from `size_restore` to `startup_data_flow`, takes under 0.35 ms
(18 stages, about 2 ms in total). Three dispatches exceed B_settle on every profile: the
construction, the first frame, and the second frame. The second frame is a 4–6 ms re-layout plus
2–5 ms paint after the editor page appears.

**Experiments** (steady/keyfile, median of 7):

| Experiment | What it isolates | Result |
|---|---|---|
| `split`: `present` in the next idle | construction vs present | construction 14.8 (template 12.6), present dispatch 17.8, first frame 32–38 at load 16 |
| `realize`: explicit `realize` in its own idle, then `present` | realize alone | realize dispatch 9.6–12.0, present dispatch 14–15, first frame 16.8–21 |
| `GDK_DISABLE=gl,vulkan` + `realize` | GL initialisation | realize 10.1 (unchanged): **not GL** |
| `noicon` (the window icon set to a missing name) | `gtk_window_realize_icon` | realize 12.1 vs 9.8, shell present 10.9 vs 11.8: **≤ 1 ms** |
| `second`: a second window 1.5 s later | one-time vs per-instance | new 8.5 (template 7.0), **present 4.4** (1.2–2.0 in an earlier run), construction 12.8, first-frame dispatch 10–11 |
| `measure`: a post-order measure walk before present (load 16) | first measure by subtree | header bar 11.3 (first text and icons), properties 3.4, empty page 2.9, search panel 1.8, status bar 1.9, palette 1.1, sidebar 1.0, tab bar 0.4, preview 0.2; in the second window: header bar 3.9, total 12.1 vs 22.7 |
| `prewarm`: class refs, a Pango label and icon lookups in startup (load 12) | one-time init | classes 1.5, Pango 3.9, icons 4.8; template −1.5, first-frame layout −3…−4, present unchanged |
| `constant`: the window reports `CONSTANT_SIZE` | GTK's min-size binary search | no change: one `measure_h` call, the search does not run |
| `shell`: header bar, empty body, status bar | the minimal first frame | new **1.4**, present **10.9–11.8**, first dispatch 12.5–16.1, first frame 7–9 (layout 5.0, paint 3.4) |
| `shell`, then the full window 1 s later | the cost of composing after the shell | template 9.4–10.8, present 4.6–5.0, dispatch 14–21, its frames 10–15 |
| `RESPONSIVENESS_RENDERER=gl` (llvmpipe) | software GL | present 47, paint 25–36: the headless lane cannot judge GPU rendering |

**Where the milliseconds go** (sources: GTK 4.22.5 `gtkwindow.c` and the measurements above):

- **Template (9.5–10 ms first window, 7 ms warm).** Builder instantiation of about 80 objects,
  plus one-time class initialisation (about 1.5 ms). The leaves are small (C4: sidebar 0.4,
  properties 0.9, palette 0.8, preview 0.5, search 0.9, popover 0.2). Most of the cost is the
  skeleton of nested Adwaita containers and the header bar.
- **`present` (13.5–16 ms).** `gtk_window_show` calls `gtk_css_node_validate` on the whole tree,
  then `gtk_widget_realize`, then `gtk_window_present_toplevel`, then `gtk_widget_map`. Realize
  creates the `GdkSurface` and the renderer, connects `compute-size`, starts layout, sets the
  icon, and calls `gtk_application_handle_window_realize` and `gtk_native_realize`.
  - About 10 ms is a one-time per-process cost of the first realize. It does not depend on tree
    size: the 3-widget shell pays it, and a second window does not. It is not GL and not the icon.
    What it is remains to be profiled (open question 2).
  - CSS validation of the full tree adds about 2–4 ms. That is ours, and it is proportional to
    the tree.
  - Our own vfunc work is negligible: `size_allocate` is 0.04 ms beyond its parent, and
    breakpoint installation is 0.05 ms.
- **First-frame layout (13–19 ms).**
  - `compute-size` runs `gtk_window_compute_default_size`, the window's first
    `measure(HORIZONTAL)`: 8–14 ms. That includes about 6 ms of one-time first text layout and
    icon-theme loading (the header bar's bucket is 11.3 ms first and 3.9 ms warm).
  - Allocation takes 1.3–1.6 ms.
  - About 4 ms is style work in the layout phase.
  - Hidden surfaces, which revealers measure even while concealed, add about 3 ms (search panel,
    palette, preview).
- **Paint: 3–4 ms** (cairo, 1200 × 800).

**Honest conclusion.** GTK's floor for the first window of a process has two parts:

- the first-realize cost of about 10 ms, at load 4–7;
- the one-time text and icon initialisation of about 6–9 ms, which can be moved to a dispatch of
  its own (the `prewarm` experiment).

Neither alone exceeds B_settle. Both together, or either plus our template, CSS and measure, do.
So the budget is achievable only if every dispatch carries one of these costs and almost nothing
else. The margin of the realize dispatch is about 4–6 ms. Under load 16 it measured 13–16 ms, at
the edge. If the idle reference run (task 1.3) measures the first realize above B_settle by
itself, the budget is not met by any LushText design. The change then records that against axiom
A42 and escalates upstream (D8) instead of claiming the budget.

## Goals / Non-Goals

**Goals:**

- Every dispatch from `open`/`activate` until window composition completes fits B_settle on the
  idle reference lane, for all four `baseline/small` profiles. The first-frame and later frame
  dispatches are included.
- The first presented frame shows the final chrome geometry: header bar, tab bar, status bar,
  window size, and the workspace rail at its restored width. Nothing in the chrome moves when the
  rest is composed.
- Every action, accelerator, CLI open, session restore, automation call and AT-SPI path behaves
  exactly as it does today, whatever stage composition has reached.
- Time to the first pixels is no later than today.

**Non-Goals:**

- `gtk_init`/`adw_init` (`startup_toolkit`, 39–45 ms). It stays reported and not budgeted, as in C4.
- GPU renderer cost. The headless lane runs llvmpipe for GL, so that cost is a
  presentation-capable-lane question (`long-line-evolution.md` §2 item 5).
- The post-open editor-page dispatches. They belong to C6 (`bound-post-open-settle`), but the
  second-frame re-layout above is measured and reported here.
- Changing settings, persisted formats, or the C4 settings writer.

## Decisions

### D1. Compose the window in a fixed sequence of bounded dispatches

`open` and `activate` record the request (files, hint) and schedule stage W. They do no window work
before the main loop.

| Stage | Work | Estimate (load 4–7) |
|---|---|---|
| W, warm | the first Pango layout of the UI font, and icon lookups for the shell's icon names; one-time init moved out of the frame | ≈ 8–9 ms (Pango 3.9, icons 4.8) |
| R, shell | the shell template (D2), size and maximized restore, the shell plan (D3), then `gtk_widget_realize` | ≈ 1.5 + 10 ms |
| P, present | `present`: CSS of the small tree, `map`, then queue the activation's file opens (D5) | ≈ 1–2 ms |
| F1 | the shell's first frame | 5–9 ms measured |
| B1, body | the body sub-template: split views, layout views, bottom sheet, `GtkLushClipBin`, the editor box with the existing `AdwTabView` parented, the empty-state page; breakpoints installed; restore plan applied | to measure; target ≤ 6 ms, plus its frame ≤ 10 ms |
| B2, sidebar | `LushtextSidebar` in the workspace slot | about 1 ms, plus its frame |
| B3, properties | `LushtextPropertiesPanel` in its layout slot, only when the restored intent shows it; otherwise in H | about 1–3 ms, plus its frame |
| H, hidden | search panel, command palette, Markdown preview, Open popover: one per idle dispatch after B3, each before first use | ≤ 2 ms each |

Each stage runs from a `G_PRIORITY_DEFAULT_IDLE` source and schedules its successor only after the
frame that shows it has painted. A stage never shares a frame with the next stage's
first-measure work. If B1 measures above budget (task 3.6), it is split again: skeleton and editor
box, then the empty page.

**Alternatives rejected:**

- Present split into the next dispatch, or realize before present: measured; the first-frame
  dispatch stays at 17–22 ms.
- Staging only the hidden surfaces (C4 D4): 3–4 ms.
- A constant-size request mode: no effect.
- Prewarming in `startup`: the cost stays in the pre-loop stall.

### D2. One shell template, lazily instantiated sub-templates

`window.blp` keeps `LushtextWindow` with:

- the header bar (the full chrome; the Open popover is attached in H);
- the tab bar;
- a `GtkLushClipBin` holding one `AdwBin` body host;
- the status bar.

The body moves to a new composite widget, `LushtextWindowBody` (`window-body.blp`), which owns the
split views, layout views, bottom sheet, editor box, empty page and the slot hosts. B2, B3 and H
fill those hosts (`AdwBin`s) with the existing custom widgets.

The `AdwTabView` is constructed in R as an unparented object owned by the window, so the tab bar
binds to it and every `imp().tab_view` access stays valid. B1 parents it into the editor box.
`open_document` may append pages before B1; a page is realized when B1 parents the view (task 2.4
verifies the load pipeline under this).

**Alternatives rejected:**

- Building the skeleton in R: about 5–6 ms extra in the realize dispatch, past the margin.
- One sub-template per leaf with the skeleton kept in the shell: same objection.

### D3. The shell placeholder geometry is the composed geometry, as pure policy

`geometry::policy` gains `plan_first_frame_shell(facts) -> FirstFrameShellPlan`, whole-pixel and
float-free. Its facts are the restore plan's facts and the window's restored size. It returns:

- the workspace rail's visible width, or none (collapsed, compact layout, Focus Mode never
  restored, or not requested);
- the body background classes.

It is computed with `plan_split_view_restore` and the integer reconciliation that
`plan_shell_reconciliation` uses, so no rule is copied. The R stage gives the rail placeholder
`width-request` = that width and the side-rail CSS class. B1 replaces the placeholder with the
split view, whose first reconciliation yields the same width.

A Kani harness in `geometry/policy/kani_proofs.rs` proves, for every `i32` width, preset and
intent, that `plan_first_frame_shell` agrees with the first `plan_shell_reconciliation` of the
composed shell: the same rail visibility and visible width, and no properties surface in the shell.

The breakpoint loop model (`geometry/kani_proofs.rs`, shard `core-shell-geometry`) is unchanged,
because breakpoints and split views are installed together in B1. The model's precondition, that
breakpoints exist from the first allocation of the split views, still holds.

The properties pane is not drawn in the shell. On a wide window with document properties
restored open, the editor area shows a right-side placeholder of the planned width. That case is
covered by the same harness, so the right edge does not move either.

### D4. Forced stages: `ensure_composed(stage)`

`ui/window/composition/` owns the `CompositionStage` sequence. It is a migrated role home: facade
`mod.rs`, pure `policy.rs` (stage order, dependencies, which stage a surface needs),
`execution.rs` (idle scheduling, frame-gated succession, forced completion), and `evidence.rs`.

Every staged template child is reached through an accessor that calls
`ensure_composed(stage_of(surface))`. That call runs the missing stages synchronously, in order, in
the current dispatch. It is bounded: the costliest forced chain (B1 to H for the search panel) is
measured by task 4.5 and must fit B_settle. If it does not, a forced call completes only B1 and
the target surface, and later stages stay scheduled.

The actions, accelerators and menu items for staged surfaces exist from R:

- `win.toggle-sidebar`;
- `win.toggle-properties`;
- `toggle-search-panel` (Ctrl+Shift+F);
- the command palette (Ctrl+Shift+P);
- `begin-search`;
- the preview actions;
- the Open popover button;
- the notes actions.

They are never dropped.

A policy check, an extension of `make check-workflow-boundaries`, fails on a direct
`imp().<staged child>` access outside `composition/`.

**Alternative rejected:** disabling actions until composition completes, which would drop
accelerators typed immediately.

### D5. Startup data, session restore and CLI opens wait on composition, not on stages

`begin_startup_data_flow`, the draft and session load, still starts in R, so its I/O overlaps
composition. Its GTK completion, and every activation file open queued in P, runs through
`composition::when_composed(stage, f)`: `f` runs immediately if the stage is built, otherwise
after it. Tab pages are appended to the unparented tab view, so a file opened during composition
appears when B1 parents it. The duplicate-path, draft-claim and focus rules are unchanged, because
the same `open_document_from_activation` runs.

### D6. Evidence, readiness, automation, accessibility

- `WindowConstructionEvidence` gains:
  - real `stages_pending` and `stages_total`;
  - `shell_first_frame_us`;
  - `composition_complete_us`;
  - `largest_composition_dispatch_us` (`null` without the watchdog);
  - `forced_stages` (a count).

  The automation snapshot projects all of them; integers only.
- A new readiness blocker, `window-composition`, is active while any stage before H is pending. It
  joins `app-startup` and every predicate whose surface is staged (`session-restore-complete`,
  `visual-geometry-settled`, `accessibility-settled`, the search and palette predicates).
- Each staged surface applies its accessible metadata when it is built, exactly as the template
  does today. `make accessibility-smoke` already waits for `accessibility-settled`, which now
  includes the blocker. `docs/accessibility-matrix.md` records that anchors exist after
  composition.

### D7. Measurement and gates

- **Permanent instrumentation.** The probe gains frame-phase spans (`frame_layout`, `frame_paint`,
  for the first frames of each window) and one span per composition stage and dispatch.
- **Construction verdict.** The lane's verdict becomes per dispatch until `composition_complete`:
  the largest watchdog lateness inside the composition window must be ≤ B_settle.
  `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` fails the run on an over-budget dispatch.
- **Deterministic widget gate** (`window_composition`, `surfaces` shard). It proves:
  - stage order and frame-gated succession;
  - every forced-stage path: accelerator, action, CLI open and session completion before each
    stage;
  - that results are identical to a fully composed window;
  - the readiness blocker;
  - no template-child access before its stage;
  - equal shell and composed rail geometry at allocation level.
- **Visual geometry.** A new scenario, `first-frame-shell`, captures the shell frame and the
  composed frame in one session. The header bar, tab bar, status bar and the rail's right edge
  are protected with zero difference. The editor and rail content regions are allowed to change.
  Invariant id: `first-frame-shell-chrome`.

### D8. GTK axioms A42–A44 and the upstream investigation

All three get `gtk-lush-axioms` probes and samples with interactive and `--check` modes.

- **A42.** The first `gtk_widget_realize` of a toplevel in a process costs a one-time amount that
  later toplevels do not pay. The probe compares the realize spans of two windows. The ledger
  records the measured magnitude without a hard threshold, and the sample prints both.
- **A43.** `gtk_window_show` validates CSS for the whole tree, realizes, and presents. The size
  computation (`compute-size`, the first measure) runs in the first frame's layout phase, not in
  `present`. The probe counts `measure` calls during `present` (zero) and in the first frame (one
  or more).
- **A44.** A subtree parented into a mapped window is styled and measured in the next frame's
  layout phase, not at insertion. The probe parents a subtree in an idle and counts its measures
  before and after the next `layout`.

**Upstream.** Profile the first realize with sysprof (open question 2) and file a GTK issue with
the breakdown when the cost is in GTK or GDK. A one-time cost that could run before the first
toplevel exists (display-level initialisation) would let stage W absorb it. That would be an
upstream change, not a LushText workaround.

## Risks / Trade-offs

- **The realize floor is near the budget.** About 10 ms at load 4–7, and 13–16 ms at load 16. → It
  shares its dispatch only with the 1.4 ms shell. The idle reference run decides (task 1.3). If
  that run is over, the change says so and escalates (D8) rather than claiming the budget.
- **Template-child access before its stage panics or misbehaves.** → Ensure accessors, the policy
  check, and widget tests for each forced path.
- **Content pops in.** The sidebar rows, empty page and tabs appear over 2–5 frames
  (about 20–40 ms) after the first frame. → The chrome geometry is fixed by D3 and proved by D7,
  so only content fills in, which already happens today for workspaces and session tabs.
  Recorded for the maintainer.
- **Adwaita may react to late children.** `AdwOverlaySplitView` could animate a sidebar inserted
  after map, and a layout view could emit `notify` and re-run reconciliation. → B1 inserts the
  split views with `show-sidebar` already set from the plan, inside one dispatch, before their
  first allocation. A44's probe and widget tests pin it. Any animation is suppressed the way
  restore does today.
- **The breakpoint and geometry Kani models.** → Unchanged, because breakpoints and split views
  arrive together. The new harness adds the shell-agreement proof. If it pushes
  `core-geometry-policies` past its 15-minute pull-request margin (13.6 today), it goes to a new
  `core-shell-plan` shard.
- **Load sensitivity of the measurement.** → The PR gate is deterministic (stage order, forced
  paths, geometry). Timing verdicts come only from the idle reference run, with stall-report
  evidence.
- **Sessions with many tabs.** → Session restore keeps its four-page turns. It only starts
  applying after B1.
- **Time to a fully composed window grows by about 20–40 ms, while the first pixels come about
  20 ms earlier.** → The trade the budget requires. Recorded.

## Migration Plan

No persisted state changes. The template split is mechanical and lands with `make
blueprint-generate` and `make check-blueprint`. Rollback is a revert. The stages can be collapsed
into one dispatch for bisecting through a test-only `test_policy.rs` override, never in shipped
builds.

## Open Questions

1. **Content pop-in.** Is a 20–40 ms progressive fill of sidebar rows, empty page and tabs
   acceptable when no chrome moves? This is a maintainer decision. The alternative, a frozen
   placeholder snapshot, is not possible on a first launch.
2. **Profiling the first realize.** `perf` and `sysprof` are not installed in the `devbox`
   toolbox. May task 1.4 install `sysprof` there (`dnf install sysprof-cli`) to attribute the
   one-time ~10 ms before filing upstream?
3. **The idle reference run.** It must happen on an idle machine; today two other agents were
   building (load 4–18). The maintainer needs to schedule a quiet window for task 1.3 and the
   final verdict.
4. **GPU presentation.** The lane's llvmpipe GL renderer measured `present` 47 ms and paint
   25–36 ms. Production uses a GPU. Should C4b's verdict wait for the presentation-capable lane
   (`long-line-evolution.md` §2 item 5)?
5. **Keyfile flush.** C4's dconf question is unchanged. This run measured the keyfile flush at
   8.5–10.4 ms, within budget, but C4 saw up to 17.9 ms.
