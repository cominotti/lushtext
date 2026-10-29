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
61, `search_panel` 58 and `command_palette` 23. `constructed` installs the three breakpoints
(properties, workspace, Open button; `geometry/execution.rs`) on the window **before** it is
mapped, and the Open-button breakpoint has a setter on a header-bar widget
(`open_button_stack`) as well as on `workspace_split_view.collapsed` (ledger A18).

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
Task 1.3 re-runs under the reference precondition (D9) before implementation.

Attribution inside GTK is by spans and A/B experiments, not samples (D9).

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

**Where the milliseconds go** (sources: GTK 4.22.5 `gtkwindow.c`, `gtkcssnode.c`,
`gtkrevealer.c`, Libadwaita 1.9.3 `adw-breakpoint-bin.c`, and the measurements above):

- **Template (9.5–10 ms first window, 7 ms warm).** Builder instantiation of about 80 objects,
  plus one-time class initialisation (about 1.5 ms). The leaves are small (C4: sidebar 0.4,
  properties 0.9, palette 0.8, preview 0.5, search 0.9, popover 0.2). Most of the cost is the
  skeleton of nested Adwaita containers and the header bar. **Instantiation does not need a
  realized or presented window**: the `split` experiment ran the whole template plus wiring in a
  dispatch of its own at 14.8 ms under load 16.
- **`present` (13.5–16 ms).** `gtk_window_show` calls `gtk_css_node_validate` on the window's CSS
  node, then `gtk_widget_realize`, then `gtk_window_present_toplevel`, then `gtk_widget_map`.
  Realize creates the `GdkSurface` and the renderer, connects `compute-size`, starts layout, sets
  the icon, and calls `gtk_application_handle_window_realize` and `gtk_native_realize`.
  - About 10 ms is a one-time per-process cost of the first realize. It does not depend on tree
    size: the 3-widget shell pays it, and a second window does not. It is not GL and not the icon.
    What it is remains unattributed (D9).
  - CSS validation adds about 2–4 ms and is proportional to the tree. **It skips invisible
    nodes**: `gtk_css_node_validate_internal` does not descend into a child whose CSS node is not
    visible (`gtkcssnode.c`), and a widget's visibility is its node's visibility. Invisible
    widgets are not mapped either.
  - Our own vfunc work is negligible: `size_allocate` is 0.04 ms beyond its parent, and
    breakpoint installation is 0.05 ms.
- **First-frame layout (13–19 ms).**
  - `compute-size` runs `gtk_window_compute_default_size`, the window's first
    `measure(HORIZONTAL)`: 8–14 ms. That includes about 6 ms of one-time first text layout and
    icon-theme loading (the header bar's bucket is 11.3 ms first and 3.9 ms warm).
    `gtk_widget_measure` of an invisible widget returns 0 without measuring it.
  - Allocation takes 1.3–1.6 ms.
  - About 4 ms is style work in the layout phase.
  - Hidden surfaces add about 3 ms (search panel, palette, preview). The reason is specific:
    `search_panel_revealer` (`slide_up`) and `palette_revealer` (`slide_down`) are vertical slide
    revealers, and `gtk_revealer_measure` scales only the slide orientation by the reveal
    progress, so the window's horizontal measure measures their **visible** children at full
    scale even while concealed. It returns 0 at once when the child is invisible
    (`gtkrevealer.c`, `get_child_size_scale`).
- **Paint: 3–4 ms** (cairo, 1200 × 800).

**Honest conclusion.** GTK's floor for the first window of a process has two parts:

- the first-realize cost of about 10 ms, at load 4–7;
- the one-time text and icon initialisation of about 6–9 ms, which can be moved to a dispatch of
  its own (the `prewarm` experiment).

Neither alone exceeds B_settle. Both together, or either plus our template, CSS and measure, do.
So the budget is achievable only if every dispatch carries one of these costs and almost nothing
else. The margin of the realize dispatch is about 4–6 ms. Under load 16 it measured 13–16 ms, at
the edge. If the reference run (task 1.3) measures the first realize above B_settle by itself, the
budget is not met by any LushText design. The change then records that against axiom A46 and
escalates upstream (D8) instead of claiming the budget.

What the breakdown does **not** support is the proposal's first conclusion, that the tree has to
be *built* late. Every cost that has to leave the realize and first-frame dispatches (CSS
validation, first measure, map) is a cost of **visible, parented** widgets. Instantiation is a
separate, already-measured cost that fits a dispatch of its own. That is the basis of D2.

## Adversarial review (2026-10-20)

The first draft of this change was reviewed against the code and the toolkit sources before
implementation. Findings, each confirmed (C) or refuted (R), and what changed:

1. **(C) GApplication exits if `open`/`activate` return without a window.** `g_application_run`
   iterates the main loop only `while (use_count || inactivity_timeout_id)` (GLib 2.88
   `gapplication.c`), and a GtkApplication's use count comes from its windows. Scheduling the
   window from an idle would end the process before the idle runs. → D1: the handlers `hold()`
   the application and the stage that adds the window releases it.
2. **(C) A second `open`/`activate` before the window exists would create a second window**,
   because `app.active_window()` is `None` until construction. → D1: one pending window request,
   into which later requests merge.
3. **(C) Breakpoints installed after map take a different Adwaita path.**
   `adw_breakpoint_bin_size_allocate` applies a breakpoint **before** allocating the child only
   on the first allocation after `map`; every later switch snapshots the old content into
   `old_node`, hides the child, and swaps on the next tick (`breakpoint_changed_tick_cb`). The
   draft installed breakpoints in the body stage, after the shell was mapped and allocated, so a
   restore at a matching width would have drawn one frame of a frozen snapshot and changed the
   chrome a frame later. → D2 installs every breakpoint before the window is realized, as today;
   ledger A50 pins both paths.
4. **(C) The draft's "no layout jump" claim was false at narrow widths.** The Open-button
   breakpoint's setter changes the header bar (`open_button_stack`). Installed in the body stage,
   it would have switched the header's Open button between the shell frame and the composed
   frame, inside the region `first-frame-shell-chrome` protects. Setting the compact form early
   cannot fix it: a setter restores its **add-time** value on unapply (A16). → Fixed by finding 3's
   change.
5. **(C) The draft's Kani shell-plan harness was close to a tautology.** `plan_first_frame_shell`
   was to be computed from `plan_split_view_restore` and the reconciliation rules, and then proved
   equal to the first `plan_shell_reconciliation` of the same facts. The risk of a jump is not in
   the plans but in GTK: the placeholder's `width-request` is in px, while the split views'
   sidebar bounds are in sp (A14, A17, the text scale), and an `AdwOverlaySplitView` raises its
   sidebar to the sidebar's own minimum (A17), which the plan cannot see. → D3 removes the
   placeholder and the harness: the real split views draw the rail from the first frame, and a
   widget test proves the staged content's minimum widths never widen it.
6. **(C) An unparented `AdwTabView` breaks file loading.** `editor_page/load/admission.rs`
   derives a load's `Active` priority and the protected live-editor residency from
   `editor.root()`, which is `None` for a page in an unparented view; `gtk_widget_grab_focus`
   returns `FALSE` without a root (`gtkwidget.c`). A CLI open during composition would have
   loaded at `Normal` priority and outside the memory policy's protection. → D2 keeps the tab view
   parented from the first stage.
7. **(C) Routing every staged template child through a forcing accessor would undo the staging.**
   `size_allocate` reads `search_panel` and `command_palette` on every allocation,
   `initial_projection` reads `content_stack`, and `refresh_status_bar`, the notification render
   and the sidebar projections run during composition. Every one of those reads would have forced
   the hidden stages into the first frame. → D2: every object exists from the first stage, so a
   read never forces anything; only **revealing** a staged surface does (D4).
8. **(C) CLI opens are unbounded in one dispatch today** (`app.rs` opens every file in the `open`
   handler). With many files that is far above B_settle once composition is budgeted. → D5:
   activation opens go through the session-restore admission's bounded page turns.
9. **(C) The readiness blocker's scope contradicted itself.** The draft cleared
   `window-composition` before the hidden-surface stage but also attached it to the search and
   palette predicates, which depend on that stage. And folding it into `app-startup` would have
   blocked `window-actions-exported`, which shares that blocker, so automation could not exercise
   the forced paths it was meant to test. → D6.
10. **(C) The hidden-surface stage is mostly unnecessary.** Finding 7's ~3 ms comes from concealed
    vertical revealers measuring visible children. Keeping a concealed revealer's child invisible
    removes it from CSS validation and measurement at no staging cost. → D2.
11. **(R) "The settings writer flushes into the composition."** The writer's flush is armed
    `PERSISTENCE_WINDOW` (500 ms) after the first presented frame, then runs from a
    `G_PRIORITY_LOW` idle (`ui/settings_writes.rs`); composition takes tens of milliseconds. It can
    only land inside composition when composition is delayed past 500 ms, and then it is one
    dispatch the verdict already counts. Unchanged; D7 records it.
12. **(R) "The breakpoint-loop Kani model's precondition breaks."** With finding 3's change the
    breakpoints are installed before map exactly as today, so the model
    (`geometry/kani_proofs.rs`, shard `core-shell-geometry`) and its A14–A18 envelope are
    unchanged. The draft's own design would have broken it (finding 3).
13. **(C, partly) Cheaper ways to absorb the one-time text and icon cost.** Verified against the
    sources: GTK already loads the icon-theme index on a worker thread when
    `IconTheme::for_display` is first called (`gtk_icon_theme_load_in_thread`), which
    `theme::register_app_icons` does in `startup`; `GTK_ICON_LOOKUP_PRELOAD` decodes an icon's
    texture on a worker (`gtkicontheme.c`), so the shell's icons can be preloaded off the main
    thread. Pango's default font map is **per-thread** (Pango ≥ 1.32.6, `pangocairo-fontmap.c`) and
    GTK widgets use the main thread's (`gtkwidget.c`), so a worker-thread Pango layout cannot warm
    the main font map's caches; it can only warm process-wide fontconfig state. How much of the
    3.9 ms that is, is unknown. → D1 keeps stage W as the mechanism that is known to work and adds
    the two off-thread variants as A/B experiments (task 1.5); neither is required for the budget.
14. **(C) GtkStack offers no lazy pages.** Pages are widgets built with the stack. Invisible pages
    are skipped by measurement, which is what D2 uses.
15. **(C) Endpoint pixel comparison cannot prove "no jump".** Comparing the shell frame and the
    composed frame says nothing about the frames between them, and capturing the shell frame at
    all needs a test-only hold, because it lasts about one frame. → D7 adds a per-frame allocation
    trace of the protected chrome over the real, unheld composition.
16. **(C) Task-list gaps:** initial focus parity; close and teardown during composition; the
    workflow-readability rows that change besides the new one; the axioms crate's governance docs;
    the Kani shard filter (a harness under `geometry::policy::kani_proofs` lands in
    `core-geometry-policies` by substring whatever the plan says). Folded into the tasks.

## Goals / Non-Goals

**Goals:**

- Every dispatch from `open`/`activate` until window composition completes fits B_settle on the
  reference lane (D9), for all four `baseline/small` profiles. The first-frame and later frame
  dispatches are included.
- The first presented frame shows the final chrome geometry: header bar, tab bar, status bar,
  window size, and the rail and pane edges of the restored layout. Nothing in the chrome moves in
  any later frame of composition.
- Every action, accelerator, CLI open, session restore, automation call and AT-SPI path behaves
  exactly as it does today, whatever stage composition has reached.
- Time to the first pixels is no later than today.

**Non-Goals:**

- `gtk_init`/`adw_init` (`startup_toolkit`, 39–45 ms). It stays reported and not budgeted, as in C4.
- GPU renderer cost (D9).
- The post-open editor-page dispatches. They belong to C6 (`bound-post-open-settle`), but the
  second-frame re-layout above is measured and reported here.
- Changing settings, persisted formats, or the C4 settings writer.

## Decisions

### D1. Compose the window in a fixed sequence of bounded dispatches

`open` and `activate` record the request (files, hint) in one per-application
`PendingWindowRequest`, call `application.hold()`, and schedule stage W. They do no window work
before the main loop. A later `open` or `activate` that arrives before the window exists merges
its files into the pending request instead of scheduling a second window. Stage C, which adds the
window to the application, releases the hold; from then on the window holds the application as
today. If the process is asked to quit before C, the request is dropped and the hold released.

| Stage | Work | Estimate (load 4–7) |
|---|---|---|
| W, warm | the first Pango layout of the UI font, and icon lookups for the shell's icon names; one-time init moved out of the frame | ≈ 8–9 ms (Pango 3.9, icons 4.8) |
| C, construct | `LushtextWindow::new` with the **whole** template (D2), every wiring stage, size and maximized restore, the split-view restore plan, and the three breakpoints; staged content invisible | ≈ 9.5–10 + 2 ms (`split`: 14.8 at load 16) |
| R, realize | `gtk_widget_realize` | ≈ 10 ms |
| P, present | `present`: CSS of the visible tree, `map` | ≈ 1–2 ms, to measure |
| F1 | the first frame: chrome and skeleton | 5–9 ms measured for the shell; to measure with the skeleton |
| B1, content | the content stack's visible page (empty page or tab view); activation opens admitted (D5) | to measure; its frame ≤ B_settle |
| B2, workspace | the workspace sidebar's content, when the restored layout shows it | about 1 ms, plus its frame |
| B3, properties | the document-properties panel's content, when the restored layout shows it | about 1–3 ms, plus its frame |

Hidden surfaces (search panel, command palette, Markdown preview, Open popover) are not a stage:
they are constructed in C and stay invisible until first revealed (D2).

Stages W to P run from successive `G_PRIORITY_DEFAULT_IDLE` sources. From B1 on, each stage
schedules its successor only after the frame that shows it has painted (`after-paint`), so no
frame carries two stages' first-measure work. A sidebar or properties surface the restored layout
does not show is not revealed at all until its toggle (D4).

If the reference run puts a stage above budget, it is split, never merged: C into the template
and the wiring; B1 into the empty page and the tab view.

**Alternatives rejected:**

- Present split into the next dispatch, or realize before present, with the tree fully visible:
  measured; the first-frame dispatch stays at 17–22 ms.
- Staging only the hidden surfaces (C4 D4): 3–4 ms.
- A constant-size request mode: no effect.
- Prewarming in `startup`: the cost stays in the pre-loop stall.

### D2. One template; stage visibility, not construction

`window.blp` keeps one template and every template child. The staged content is marked
`visible: false` in the template, and each stage only makes its host's content visible:

- `content_stack`'s pages (the empty `AdwStatusPage` and the tab view's page) until B1;
- the `LushtextSidebar` inside the workspace split view's sidebar slot until B2;
- the `LushtextPropertiesPanel` inside its layout slot until B3;
- the children of the three revealers (search panel, palette, Focus Mode affordance) and the
  Markdown preview whenever they are concealed, which is the rule from now on, not only during
  composition: a concealed revealer's child is made visible right before `reveal-child` is set and
  hidden again on `notify::child-revealed` = false.

The skeleton — `GtkLushClipBin`, the three split views, the two layout views, the bottom sheet,
the editor box with the tab view, and the slot hosts that carry the side-rail background — is
visible from C, so the rail and pane edges are drawn by the real split views from the first frame.

Because every object exists from C:

- every `imp()` access stays valid, and no access site changes;
- the tab view is parented, so `editor.root()` and `grab_focus` behave as today (finding 6);
- the breakpoints and their setter targets exist before map, so the first allocation applies them
  before allocating the child (A50) and the breakpoint-loop model's precondition holds literally;
- invisible content costs nothing in R, P or F1: CSS validation, measurement and mapping skip it
  (A47, A49).

A new type is needed only if the reference run requires splitting C (below).

**Gating experiment (task 1.5).** Before implementation, an A/B on the instrumentation patch
measures this layout: the full template in C with the staged content invisible, R, P, F1. D2
holds if, on the reference lane, C ≤ 12 ms, P is within 2 ms of the `shell` experiment's present
minus its realize, and F1 is within 2 ms of the `shell` experiment's first frame. Otherwise the
fallback is a split instantiation: C1 builds the window template (header bar, tab bar, skeleton,
status bar, breakpoints) and C2 builds the leaves (`LushtextSidebar`, `LushtextPropertiesPanel`,
the empty page, the hidden surfaces) as separate objects and parents them, still invisible, before
R. The fallback keeps every property of D2 above except that the leaves become non-template
children reached through window-owned `OnceCell`s filled in C2, before any other code can run.

**Alternatives rejected:**

- *The draft's minimal shell template plus a lazily instantiated `LushtextWindowBody`.* It
  installs breakpoints after map (finding 3), changes the header at narrow widths (finding 4),
  leaves the tab view unparented (finding 6), and needs a forcing accessor at about 300 access
  sites, several of which run every frame (finding 7). Its reason for building the skeleton late,
  "about 5–6 ms extra in the realize dispatch", assumed construction and realize share a
  dispatch; here they do not.
- *Leaves as unparented objects until their stage.* Same root and focus problems as the tab view.

### D3. Final geometry from the first frame, proved on the rendered widgets

There is no placeholder, so there is no placeholder plan. The rail edge, the pane edge and the
bottom-sheet slot are the composed split views from the first frame, restored by the existing
`plan_split_view_restore` and reconciled by the existing `plan_shell_reconciliation`, both
applied in C before the window is realized, as today.

The one way a staged reveal could still move an edge is A17: an `AdwOverlaySplitView` raises its
sidebar to the sidebar child's own minimum. A slot host whose content is invisible has a minimum
of zero; when B2 or B3 reveals the content, its minimum must not exceed the width already
allocated. This is proved where it can fail, on the rendered widgets: a widget test reveals each
staged surface at every workspace width preset, at text scales 1, 1.25, 1.5 and 2, at the wide,
medium and compact widths, and asserts that the rail and pane allocations do not change. A
LushText-side fix (a `width-request` floor on the slot host from the same restore plan, converted
from sp to px in the adapter) is applied only if the test finds a case.

Kani is not extended. The breakpoint-loop model is unchanged (finding 12), and the composition
policy (D4) is a finite stage order whose every reachable state an exhaustive unit test
enumerates; a Kani harness would add CI time to a shard (`core-geometry-policies`, 13.6 of 15
pull-request minutes) without adding coverage.

### D4. Forced reveals: `ensure_composed(stage)`

`ui/window/composition/` owns the `CompositionStage` sequence. It is a migrated role home: facade
`mod.rs`, pure `policy.rs` (stage order, dependencies, which stage a surface needs, which stages a
forced reveal completes), `execution.rs` (idle scheduling, frame-gated succession, forced
completion), and `evidence.rs`.

Reads and projections never force a stage: they act on objects that exist. A stage is forced only
by an operation that needs a staged surface **visible, focusable or allocated**:

- `win.toggle-sidebar` / `win.set-sidebar-visible` (B2);
- `win.toggle-properties` / `win.set-properties-visible` / F9 (B3);
- anything that focuses or scrolls the editor, the empty page or the tab view, including the
  activation's focus (B1);
- automation actions that reach those surfaces.

`ensure_composed(stage)` completes the missing stages up to `stage`, in order, synchronously in
the current dispatch. A reveal is a visibility flip, so the dispatch itself stays small; its
measure and paint cost lands in the next frame. Task 4.5 measures the costliest forced frame
(B1 + B2 + B3 at once). If it exceeds B_settle, a forced reveal completes only its own stage and
the stages it depends on, and the rest stay scheduled.

Hidden surfaces are revealed by their own actions (Ctrl+Shift+F, Ctrl+Shift+P, `begin-search`,
the preview actions, the Open popover button) exactly as today, with their child made visible
first (D2); the first reveal's measure cost is a property of that interaction and is reported by
the lane. Actions and accelerators exist from C, so none is dropped.

A policy check, an extension of `make check-workflow-boundaries`, fails on a `set_visible(true)`
of a staged content widget, or a `grab_focus` on one, outside `composition/` without a preceding
`ensure_composed`.

**Alternative rejected:** disabling actions until composition completes, which would drop
accelerators typed immediately.

### D5. Startup data, session restore and CLI opens

- `begin_startup_data_flow`, the draft and session load, starts in C as today, so its I/O overlaps
  composition. Its GTK completion appends pages to the parented, still invisible tab view in its
  existing bounded turns (four pages per turn); nothing about it waits for a stage.
- Activation opens (`open` files) are admitted after P through the session-restore admission's
  bounded page turns, not in one dispatch (finding 8). The duplicate-path, draft-claim and focus
  rules are unchanged, because the same `open_document_from_activation` runs for each file.
- The activation's focus and the empty-state focus are applied in B1, by the same code that sets
  them today after `present`, so the initial focus is the same as on today's composed window.

### D6. Evidence, readiness, automation, accessibility

- `WindowConstructionEvidence` gains:
  - real `stages_pending` and `stages_total`;
  - `shell_first_frame_us`;
  - `composition_complete_us`;
  - `largest_composition_dispatch_us` (`null` without the watchdog);
  - `forced_stages` (a count).

  The automation snapshot projects all of them; integers only.
- A new readiness blocker, `window-composition`, is active from C until the last scheduled stage
  has painted. It is derived from the active window's composition evidence, never from a global
  counter, so it cannot outlive its window. It joins `app-startup`, `idle`,
  `session-restore-complete`, `visual-geometry-settled`, and `accessibility-settled`.
  `window-actions-exported` does **not** wait for it: actions exist from C, and automation must be
  able to issue forced paths during composition.
- Each staged surface applies its accessible metadata in C, as the template does today. Invisible
  widgets are absent from the accessibility tree, so its AT-SPI anchors appear when their stage
  reveals them. `make accessibility-smoke` waits for `accessibility-settled`, which now includes
  the blocker. `docs/accessibility-matrix.md` records that anchors exist after composition.

### D7. Measurement and gates

- **Permanent instrumentation.** The probe gains frame-phase spans (`frame_layout`, `frame_paint`,
  for the first frames of each window) and one span per composition stage and dispatch.
- **Construction verdict.** The lane's verdict becomes per dispatch until `composition_complete`:
  the largest watchdog lateness inside the composition window must be ≤ B_settle.
  `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` fails the run on an over-budget dispatch. A settings
  flush that lands inside the window (only when composition is delayed past the writer's 500 ms
  persistence window) is one of those dispatches and is counted.
- **Content fill-in (maintainer decision 2026-10-20).** Sidebar rows, the empty page and tabs may
  fill in over up to **40 ms after the shell's first frame — 5 reference frames at 120 Hz** —
  provided no chrome moves and every dispatch fits B_settle. The lane reports
  `composition_complete_us − shell_first_frame_us` for every run. A median above 40 ms on the
  reference lane is reported to the maintainer as outside the accepted fill-in; it is not a new
  budget constant and does not relax B_settle.
- **Deterministic widget gate** (`window_composition`, `surfaces` shard). It proves:
  - that `open` and `activate` return without a window and hold the application, that the
    process keeps running, and that a second request before C merges;
  - stage order and frame-gated succession;
  - every forced-reveal path (toggle-sidebar, F9, focus, a CLI open, session completion, an
    automation action) issued before each stage gives the same state and snapshot as a composed
    window;
  - that reads and projections during composition force nothing;
  - the readiness blocker and its scope;
  - close, and window teardown, during each stage: no stage runs afterwards, the hold is
    released, and the session save keeps unrestored descriptors;
  - initial focus equal to today's;
  - the staged reveals' rail and pane allocations (D3);
  - **a per-frame allocation trace**: from the first `after-paint` to `composition_complete`, the
    header bar, tab bar and status bar bounds, the Open button's visible child, and the rail and
    pane edges are recorded at every `after-paint` and must be constant.
- **Visual geometry.** A new scenario, `first-frame-shell`, captures the first frame and the
  composed frame in one session. Capturing the first frame needs a test-only hold after F1
  (`composition/test_policy.rs`), so the screenshot pair proves the pixels, and the per-frame
  trace above proves the unheld path. The header bar, tab bar, status bar and the rail and pane
  edges are protected with zero difference; the content regions are allowed to change. Invariant
  id: `first-frame-shell-chrome`.

### D8. GTK axioms A46–A50 and the upstream investigation

A42–A45 are taken by C1 (`document-scale-policy`, in flight) and A38–A41 are on `main`, so this
change uses A46–A50. Each gets a `gtk-lush-axioms` probe and a sample with interactive and
`--check` modes.

- **A46.** The first `gtk_widget_realize` of a toplevel in a process costs a one-time amount that
  later toplevels do not pay. The probe compares the realize spans of two windows. The ledger
  records the measured magnitude without a hard threshold, and the sample prints both.
- **A47.** `gtk_window_show` validates CSS only for visible nodes, realizes, and presents, and the
  size computation (`compute-size`, the first measure) runs in the first frame's layout phase, not
  in `present`. The probe counts `measure` calls during `present` (zero) and in the first frame
  (one or more), and shows that an invisible subtree is neither validated nor measured.
- **A48.** A subtree made visible, or parented, in a mapped window is styled and measured in the
  next frame's layout phase, not at the call. The probe flips a subtree in an idle and counts its
  measures before and after the next `layout`.
- **A49.** A concealed vertical-slide `GtkRevealer` measures a visible child horizontally at full
  scale and returns 0 for an invisible child. The probe measures both.
- **A50.** On the first allocation after `map`, `AdwBreakpointBin` applies a matching breakpoint
  before allocating its child; a breakpoint that starts matching on a later allocation, including
  one added after map, snapshots the old content, hides the child and swaps on the next tick. The
  probe installs a matching breakpoint before and after map and records what the first two frames
  draw.

**Upstream.** The ~10 ms first realize is attributed by spans and A/B experiments first (D9). If
the maintainer later approves a profile (task 1.6), it is filed as a GTK issue with the breakdown
when the cost is in GTK or GDK. A one-time cost that could run before the first toplevel exists
(display-level initialisation) would let stage W absorb it. That would be an upstream change, not
a LushText workaround.

### D9. Reference lane, attribution and rendering (maintainer decisions 2026-10-20)

- **Reference precondition, not a time.** Every verdict run (the failing-first baseline, the
  gating experiment, the final verdict) starts only when the machine is quiet, using C0's gate:
  the 1-minute load average stays below 4 for 30 s with no process at ≥ 80 % CPU, and **no other
  agent's build, test or proof process is running** (`cargo`, `rustc`, `nextest`, `kani`/`cbmc`,
  another worktree's `mutter --headless` harness). The run records the load average, the process
  check and `btrfs-stall-report` for its window. A run that loses the precondition part-way is
  discarded and re-run. There is no scheduled window.
- **Attribution with the tools that exist.** Nothing is installed in the maintainer's toolbox.
  Attribution uses the permanent spans (task 1.2) and A/B experiments. A sample-based profile
  (`sysprof`) is an optional, maintainer-gated task (1.6) that the change does not depend on.
- **Rendering.** The gate stays the headless lane: private `mutter --headless` with cairo (or
  llvmpipe when GL is selected), never the live desktop. Software rendering overstates paint and
  present relative to a real GPU, so a dispatch that passes headless is a conservative pass for
  those phases. The CPU-side phases (template, CSS, measure, allocation) and the realize floor,
  which A46 shows does not depend on GL, are not overstated, so they are judged as measured.

## Risks / Trade-offs

- **The realize floor is near the budget.** About 10 ms at load 4–7, and 13–16 ms at load 16. → It
  has its own dispatch (R) with nothing else in it. The reference run decides (task 1.3). If that
  run is over, the change says so and escalates (D8) rather than claiming the budget.
- **C carries the whole template.** About 11.5–12 ms idle, 14.8 ms at load 16. → The gating
  experiment (task 1.5) decides; the fallback splits it into C1 and C2.
- **Invisible-by-default content changes what code sees.** A widget that is invisible has no
  allocation and is not in the accessibility tree. → Reads keep working (objects exist); the
  policy check and the widget gate catch a reveal or focus that skips `ensure_composed`.
- **A concealed revealer's child is now invisible.** Code that measured or focused it while
  concealed would change behaviour. → The reveal path makes it visible before `reveal-child`, and
  the widget tests for each revealer's open and close paths run unchanged.
- **Content pops in.** The sidebar rows, empty page and tabs appear over up to 5 frames (about
  20–40 ms) after the first frame. → Accepted by the maintainer (D7), provided no chrome moves,
  which D3 and D7 prove.
- **Close or teardown during composition.** → Stage sources hold weak references and are removed
  when the window leaves the application (`window-removed`, ledger A21), not in `dispose`.
  `close_request` never forces a stage, and the existing close merge keeps unrestored session
  descriptors.
- **A second window.** It composes independently; the one-time costs (W, A46) are paid once per
  process. The blocker is per window, and readiness reads the active window.
- **Load sensitivity of the measurement.** → The PR gate is deterministic (stage order, forced
  paths, geometry). Timing verdicts come only from runs that meet the reference precondition.
- **Sessions with many tabs.** → Session restore keeps its four-page turns.
- **Time to a fully composed window grows by about 20–40 ms, while the first pixels come
  earlier.** → The trade the budget requires, accepted (D7).

## Migration Plan

No persisted state changes. The template keeps its structure; the visibility defaults land with
`make blueprint-generate` and `make check-blueprint`. Rollback is a revert. The stages can be
collapsed into one dispatch for bisecting through a test-only `composition/test_policy.rs`
override, never in shipped builds.

## Open Questions

None blocking. The maintainer settled content fill-in, attribution tooling, the reference
precondition and the rendering statement on 2026-10-20 (D7, D9). C4's dconf question about the
keyfile flush (this run measured 8.5–10.4 ms, within budget, where C4 saw up to 17.9 ms) is
unchanged and stays with C4's record.

## Implementation notes (2026-10-20)

Where the implementation departs from the decisions above, the code is right and
this section is the correction:

- **D2, the content stack.** `content_stack` itself is invisible until B1, not
  its two pages. `GtkStack` ignores `set_visible_child` for an invisible page
  (`gtkstack.c`), so invisible pages would have changed what
  `update_content_stack` does and what every reader of `visible-child-name`
  sees. An invisible stack keeps its visible-child bookkeeping exactly as today
  and is skipped by CSS validation, measurement and mapping all the same (A47).
- **D2, concealed revealers.** A concealed revealer's child becomes visible in
  the revealer's `notify::reveal-child` handler, which GTK emits inside
  `set_reveal_child` before the transition starts, and invisible again on
  `notify::child-revealed` once the child is concealed. One helper wires the
  three window revealers in C, so no open path changed. The Markdown preview
  already followed the rule (`apply_preview_shell_state`).
- **D1/D4, which rails are scheduled.** B2 and B3 are **always** scheduled. A
  rail the restored layout hides gets its content revealed into the hidden rail,
  which draws nothing (the hidden sidebar is allocated off-screen at
  `x = -width`, the hidden sheet is closed). An earlier implementation scheduled
  them only when requested, and the visual-geometry lane found what that costs:
  a never-revealed sidebar has no rect at all, a state no unstaged window has, so
  every `minimap-sidebar-*` case's hidden step failed, and fixing that in the
  proof tool meant loosening its geometry predicates one by one. Revealing
  always makes the composed window identical to an unstaged one, at the cost of
  up to two more fill-in frames when a rail is hidden. The toggle setters still
  force their stage when they run before it.
- **D4, forced chains.** A forced reveal completes its own stage and the
  content stage (`CompositionPlan::forced_chain`); a lifecycle stage (W, C, R,
  P) is never forced. A reveal issued before R or P is a visibility flip whose
  cost lands in P and F1, so forcing realize or present would gain nothing.
- **D5, activation opens.** The bounded turns are the composition workflow's
  own activation queue (`execution`), which admits at most
  `SESSION_RESTORE_PAGES_PER_TURN` (the session restore's page turn, four)
  files per idle dispatch through the unchanged `open_document_from_activation`,
  rather than session-restore descriptors: the session runtime carries
  `SessionRestore` open intent, which skips recent-history recording and the
  activation's failed-placeholder bypass, both of which activation needs. The
  startup gate's held opens drain through the same turns.
- **D5, initial focus.** No code sets the activation or empty-state focus after
  `present` today: `gtk_window_show` moves focus to the first focusable widget
  when the window has none (`gtkwindow.c`), which is a header-bar control
  whether or not the content is visible. The widget gate compares the initial
  focus of a staged and a collapsed composition.
- **D7, the hold.** The test-only hold is a hold *before a stage*
  (`set_composition_hold_before_for_test`), so a widget test can issue a forced
  path before any one reveal stage; holding before `Content` is the hold after
  F1. The visual-geometry lane launches the real binary, so its hold is a
  separate smoke-only Cargo feature, `composition-hold`
  (`LUSHTEXT_COMPOSITION_HOLD_MS`, built into `target/composition-hold/`, like
  `crash-kill-points`), which releases itself after the given time, so the lane
  needs no test-only action.
- **D6, snapshot surfaces.** The rail and pane edges are the split views' hosts,
  exported as the visual surfaces `workspace-rail` (the workspace split view's
  sidebar host) and `content-area` (the content box, whose right edge is the
  pane edge): the sidebar and the panel have no allocation until revealed, and
  the properties layout slot is unmapped while its panel is invisible, so the
  pane edge is read from the content beside it.
- **D2, opaque cover and the tab strip (found by the visual lane, 2026-10-28).**
  The window background is transparent (the background-opacity feature), so
  with the content stack invisible the first frame showed what is behind the
  window in the content area. `content_box` carries `shell-chrome-opaque` until
  B1 removes it. And the tab strip, shown only while the window has a tab,
  appeared a frame after B1 admitted a CLI file; it now also counts queued
  activation opens, so it is drawn from the first frame. Both are widget-gated
  (`window_composition`). The `first-frame-shell` scenario opens no file, so it
  protects the header bar and the status bar, not a tab strip that exists in
  neither frame.
- **A46, magnitude.** In an application-less minimal `AdwWindow` the probe
  measured the one-time first-realize cost at about 0.2 ms (fresh process:
  266 vs 90 µs), not the ~10 ms the breakdown above attributes to the first
  realize of LushText's window. Most of that cost is therefore per-application
  or per-template work (`gtk_application_handle_window_realize`, the menus, the
  template's surfaces), not a toolkit floor; the "realize floor" escalation
  premise of task 1.4 has to be judged against the verdict run's stage R span,
  not against A46.

## Budget verdict (idle, 2026-10-29)

**Met.** Every dispatch from `open` until `composition_complete` is within
B_settle in all four profiles and all 7 repetitions. **Open:** the content
fill-in is above the 40 ms D7 accepted, and that is the maintainer's decision.

- **Runs.** Run directory `.claude/handoffs/c4b-verdict-20261029-0923/`
  (local). Baseline `rr-20261029T122411Z-toolbx-1782972`, `main` a0adc5e2
  (before this change), 2026-10-29 09:24:10–09:26:41 −03:00. Branch
  `rr-20261029T122740Z-toolbx-1787430`, b73b9663 (this change),
  09:27:39–09:30:09. Both used `make responsiveness-smoke`, tier `full`,
  `baseline/small*`, 4 profiles × 7 repetitions, the `plain` variant, and the
  release `responsiveness-probes` build. Both runs passed with 0 integrity
  violations.
- **Reference precondition (D9).** The gate lines from `runs.log`:
  - `quiet: load 1.65 2.21 2.57 at 2026-10-29T09:24:10-03:00; no build/test/proof process; no process >= 80% CPU` (before);
  - `quiet: load 0.85 1.62 2.26 at 2026-10-29T09:27:39-03:00; …` (between);
  - `quiet: load 0.69 1.11 1.91 at 2026-10-29T09:31:07-03:00; …` (after).
- **btrfs.** Storage was not a bottleneck in either window: IO pressure 0.7 %
  and 0.9 %, the longest commit 159 ms and 61 ms, and no stall episode.

All values are ms, per profile, min–max (median).

| Profile / backend | largest dispatch before first frame, before → after | largest composition dispatch | first frame, before → after | construction, before → after | fill-in |
|---|---|---|---|---|---|
| fresh / keyfile | 23.1–24.6 → 8.0–8.6 (8.5) | 9.3–9.9 (9.5) | 38.5–41.1 → 16.2–18.1 | 22.8–24.3 → 9.9–10.1 | 51.2–59.9 (58.6) |
| fresh / dconf | 22.9–24.7 → 7.9–8.6 (8.5) | 9.4–10.6 (9.7) | 37.4–39.6 → 16.5–18.2 | 22.5–24.3 → 9.9–10.0 | 51.0–60.5 (59.4) |
| steady / keyfile | 23.2–25.9 → 8.0–9.2 (8.5) | 9.3–10.7 (9.8) | 37.9–41.4 → 16.4–18.2 | 22.9–25.5 → 9.9–10.6 | 52.1–59.9 (57.6) |
| steady / dconf | 22.5–23.6 → 7.9–8.7 (8.5) | 9.6–10.3 (10.0) | 37.6–39.0 → 16.5–18.2 | 22.1–23.3 → 9.9–10.1 | 52.0–60.6 (59.0) |

Stages over the 28 runs:

- W 5.9–6.6;
- C 9.9–10.6;
- R 4.8–6.6. R is the "realize floor" of the risks section. It is well under
  B_settle, so the task 1.4 escalation does not apply;
- P 0.4–0.9;
- B1, B2 and B3 at most 0.07 each;
- the first frame's layout plus paint 3.2–4.0.

The post-composition `background` phase is outside this verdict and unchanged:
a median of 9.7–10.2 per profile and a maximum of 11.4. Paint and present are
conservative relative to a GPU, because the lane renders with cairo (D9).

**Maintainer decision (2026-10-29): accepted.** The fill-in is 51–61 ms
(median 58.9 over 28 runs), above the 40 ms accepted in D7. No chrome moves
during it, and every dispatch in it is within B_settle; the maintainer
accepted the measured fill-in, which replaces D7's 40 ms for this design. The full record and a lane
finding are in `docs/next/long-line-documents.md` ("C4b", "Budget verdict").
The finding: C4b builds wrote no probe `frame` records; fixed by `959018d2` (PR #66).
