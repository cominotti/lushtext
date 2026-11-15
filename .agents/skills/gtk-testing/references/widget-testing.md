# Widget Testing Patterns for GTK4 and Rust

## Table of Contents

- [Current Harness](#current-harness)
- [Headless Runs](#headless-runs)
- [Waiting for Async UI State](#waiting-for-async-ui-state)
- [Visibility and Realization Traps](#visibility-and-realization-traps)
- [`AdwDialog` Geometry Assertions](#adwdialog-geometry-assertions)
- [Animations and Frame Clocks](#animations-and-frame-clocks)
- [List Models and Row Reuse](#listview-treelistmodel-and-row-reuse)
- [Assertion Rules](#assertion-rules)
- [When Widget Tests Are the Wrong Tool](#when-widget-tests-are-the-wrong-tool)

Use this reference for the current LushText widget harness and the failure modes that make GTK widget tests confusing.

## Current Harness

Widget tests live under:

- `crates/lushtext/tests/widget.rs`
- `crates/lushtext/tests/widget/*.rs`
- `crates/lushtext/tests/widget/common.rs`

Important repo facts:

- `crates/lushtext/tests/widget.rs` is a custom single-threaded harness, not the default libtest runner.
- Each widget test runs in its own child process via `LUSHTEXT_WIDGET_CHILD`.
- The harness supports `--list --format terse`, which matters for nextest-style discovery.
- Non-list harness executions self-supervise into a private `mutter --headless`
  session before GTK initializes, so plain Cargo and Makefile runs cannot place
  widget windows on the developer's live desktop.
- `crates/lushtext/tests/widget/common.rs` already sets up GTK, GResources, in-memory GSettings, and unique application IDs.
- App data is isolated **before any test code runs**, not by `ensure_gtk_init()`:
  the first statement of `main` in `widget.rs` points `LUSHTEXT_DATA_DIR` and
  `XDG_{DATA,CONFIG,CACHE,STATE}_HOME` at a root inside the process's
  `TMPDIR` (the per-attempt directory in a child), using
  `lushtext_core::services::test_isolation::isolated_environment`.
  `ensure_gtk_init()` only asserts it. A test may therefore save sessions,
  drafts, or any app data before initializing GTK (rule:
  `.agents/rules/testing.md`, "Test Isolation From Real App Data"). In test
  builds `json_store::data_dir()` also panics if it resolves into `$HOME`'s
  XDG trees or the Flatpak home.
  `common::isolated_data_dir()` still gives one test a fresh directory of its own.

Prefer the existing helpers:

```rust
use crate::common::{ensure_gtk_init, test_application, test_window};
```

## Headless Runs

Prefer `mutter --headless` for GTK4 and Libadwaita:

Use `scripts/run-widget-tests.sh --headless` or `make test-widget-headless` when possible so CI and local repro share the same wrapper, retry policy, and monitor size. `make test-widget`/`test-widget-headless` run the CI shards side by side (`scripts/widget-shards.py parallel`), each shard through that same runner in its own memory scope and session; `WIDGET_PARALLEL=0` keeps one serial session. Plain `cargo test --test widget` is still Cargo-visible by default, but it must re-launch itself into headless Mutter before running tests.

```bash
export GDK_BACKEND=wayland
export LUSHTEXT_WIDGET_HEADLESS_RUNNER=1
scripts/with-temp-scope.sh --session-runtime widget -- \
  dbus-run-session -- \
  mutter --headless --wayland --no-x11 --virtual-monitor 2560x1600 -- \
    cargo test --test widget
```

`--session-runtime` owns the private `XDG_RUNTIME_DIR` and removes it only
after the session's services have exited; a bare `mktemp -d` runtime dir leaks.

Never run widget tests against a live display server. The runner intentionally rejects native/live-display modes.

The shared runner and widget harness default `GSK_RENDERER` to `cairo`. This
keeps CI on GTK's CPU fallback renderer so a headless Fedora container does not
emit Mesa/EGL GPU-probe warnings just because no render device is available.
Override `GSK_RENDERER` explicitly only when the test run is meant to chase a
renderer-specific GTK bug.

`gtk_lush_proof_harness::recommended_pre_gtk_environment()` sets five variables,
and the principle behind all five is the same: **the private compositor
advertises desktop services it cannot provide, so do not start the client half
against it.** `NO_AT_BRIDGE=1` (no accessibility bus), `GDK_DEBUG=no-portals` and
`GTK_USE_PORTAL=0` (no portal service), `GSK_RENDERER=cairo` (no GPU), and
`GTK_IM_MODULE=gtk-im-context-simple` (no input method).

**The input-method one is the least obvious and it is a crash, not noise.**
Headless Mutter advertises `zwp_text_input_manager_v3`, so GTK selects its
Wayland IM backend and enables the text-input protocol for every focused
editable. Destroying a focused `GtkEntry` — an inline rename confirming, a search
bar closing — races the compositor's reply, and when the event lands after the
context is gone the result is `gtk_widget_get_display: assertion 'GTK_IS_WIDGET
(widget)' failed` followed by a **SIGSEGV** inside `wl_proxy_get_version`,
which presents as a `FLAKY:` line (history: `docs/next/persistent-format-hardening.md`
S7B-6).

Two lessons worth carrying:

- **`entry.has_focus()` is `false` for a focused `GtkEntry`.** `GtkEntry` is
  composite; focus lands on its internal `GtkText`. A focus guard written that
  way compiles, reads correctly, and never fires. Ask the root's focus widget
  instead: `window.focus()`, then `focused == entry || focused.is_ancestor(entry)`.
- **Measure a toolkit-race "fix" before believing it.** Here the dangling
  state is in GTK's private per-display IM global, which application code
  cannot reach, so application-side reorderings do not fix it.

## Waiting for Async UI State

Many widget changes land through idle callbacks, timeouts, or `spawn_blocking_then`. Use the **shared** wait helpers from `crates/lushtext/tests/widget/common.rs` — do not paste a private copy into your module:

```rust
use crate::common::{flush_events, wait_until};

// `wait_until` polls, then drains ALL ready main-loop sources. The drain is
// required: `spawn_blocking_then` delivers completions from one source at
// `gtk_lush_tasks::completion_priority()`, which runs only once nothing
// higher-priority is pending.
wait_until(Duration::from_secs(5), || window.imp().sidebar.section_count() == 1);
```

Why shared, not local: a private copy is missed when the shared helper is fixed. The canonical version lives once in `common.rs`; everything imports it. Do **not** rewrite it as a single blocking `MainContext::iteration(true)` with a timeout source — the timeout (higher priority) starves the completion source and every `spawn_blocking_then`-backed wait then times out. The flake to fix is the *budget*, not the poll mechanism.

**Budget the wait to what it waits on.** Window realization and `spawn_blocking_then`/file-I/O completion are async and scheduling-dependent — give them a generous budget (≥5–10s). A 2s budget for async work flakes under load. The predicate returns the moment the work lands, so a larger ceiling never slows the fast path and only matters when a loaded machine delays the thread. Keep short budgets for synchronous UI-state flips.

If a widget test fails then passes (or the harness prints `FLAKY:`), that is a blocker, not noise: read the real panic, classify the wait, fix the cause, and rerun to confirm. See the skill's Flake Discipline section.

**Wait for the admission the action needs, not only for its precondition.**
Some app operations share one process-wide admission and **reschedule** rather
than queue when it is taken: a local-history periodic capture that finds the
automatic-capture permit held (by the baseline capture the test's own edit
started) tries again a whole interval later, five minutes. A test that calls
such an operation right after the edit times out whenever a loaded runner has
not finished the earlier capture (`protected_view_surfaces`, CI run
43627976239). Wait for the admission to be free first
(`local_history_evidence().automatic_capture_inflight`), and prove the fix with
a test that slows the competing work deterministically
(`set_local_history_baseline_delay_for_test`) rather than with load.

**A scroll made while a reflow pauses the rest state is still a scroll.** The
`editor_page` minimap flake (`y = -6`) was a settle repair undoing a
`scroll_to_mark` made during a passive width burst. To test inside such a
burst, act *from* the event that opens it, never after waiting for its effect:
connect the horizontal adjustment's `changed` (after the editor's own
observer), and on the first page-size change queue a `G_PRIORITY_HIGH` idle
that records `minimap_evidence().reflow_settle_pending` and then acts
(`editor_page::act_during_width_reflow`). Waiting for the width change and then
acting let CI's main-loop drain run the 150 ms settle first: the top-anchor
guard failed on both PR runs and the scroll tests passed vacuously
(`restore-main-ci` R6). Also wait until no burst is pending before opening
one, and open it with the minimap *shown* (show it, or change the scroller's
end margin): while the minimap is hidden every minimap refresh cancels a
pending settle. Read anything the settle will change inside the action, not
after the helper returns. Test a later reflow with a single `set_value`,
because a `scroll_to_mark` keeps refining its value after the settle and that
later `value-changed` can hide a missing rest-state record.

Use predicates tied to visible behavior:

- tab count changed
- selected page changed
- a label or title updated
- a `Cell` flag in the widget `imp()` flipped

**A state that exists only between two dispatches is invisible to `wait_until`.**
`wait_until` drains every ready source before it looks, so a state recorded by
one source and cleared by the next lower-priority one inside the same drain is
never observed. A `ViewportSliceBin` forwarded request is the in-tree case: the
frame clock's layout (priority 120) records it and a default-priority idle (200)
applies it. Use `common::observed_between_dispatches`, which runs the main
loop one `iteration(false)` at a time and samples between dispatches (GLib
dispatches one priority level per iteration), to assert such a state was ever
true; then `wait_until` for it to clear.

The same holds for **any in-flight state of a workflow that reschedules itself
on a short timeout**, such as a buffer replacement's `in_progress` or
`mutation_started` (1 ms slice turns). When a turn takes longer than its
timeout, as on a loaded host, the next turn is already due when it returns, so
one `wait_until` drain chains every turn to the terminal and the in-flight state
is never sampled: a test that passes on an idle machine and fails under load
(reproduced 2 of 6 runs with CPUs saturated; `harden-flatpak-sdk-update-ci`).
Assert in-flight states with `observed_between_dispatches`; keep `wait_until`
for terminal states, which persist.

**A fixed iteration window starves a lower-priority source under load.** A
test that iterates its own context for a fixed time and expects a
`G_PRIORITY_DEFAULT` source to run between two dispatches of a
higher-priority one is load-sensitive: once every iteration takes longer than
the high-priority source's period, that source is ready at every prepare and
GLib dispatches only it, so the lower-priority source can run in the window's
last iteration, after which nothing records it. The in-tree case was
`ui::responsiveness_probe::watchdog::tests::a_blocking_dispatch_is_recorded_between_its_bounds`
(a 2 ms `G_PRIORITY_HIGH` watchdog and a 40 ms stalling idle in a 70 ms
window; "lateness 756 µs below the lower bound" at a load of 8-12,
`reduce-build-disk-writes` design D11). Iterate until the event that proves
the outcome has happened (there: one watchdog dispatch after the stall), with
a generous deadline, instead of for a fixed time.

**`window.destroy()` does not dispose a window a test still holds** (ledger
A21). A teardown test that needs the widget tree torn down must drop every
strong handle to the window (and to children it keeps, such as a section)
after `destroy()`; keep only the widget whose post-teardown evidence you read.
The sidebar's path reveal ends from `unroot`, which then fires.

For local-history browser tests, remember that the "Before edits" baseline is
captured on the first modified transition, not when the file load completes. If
the test relies on browser row ordering, modify the buffer first and then wait
until the stored metadata/text proves the baseline and target snapshot are in the
expected rows.

## Visibility and Realization Traps

The most common bad assertion in GTK widget tests is using `is_visible()` when you really mean the widget's own `visible` property.

In an unpresented window:

- `widget.is_visible()` walks the parent chain and often returns `false`
- `widget.property::<bool>("visible")` reads the widget's own flag

So prefer:

```rust
assert!(widget.property::<bool>("visible"));
```

**Hiding the focus widget moves focus one painted frame later.** When a
focused widget (or its ancestor) is hidden, GTK keeps focus on it and only at
the next `after-paint` moves focus to the first visible ancestor that accepts
it (`gtkwindow.c` `maybe_unset_focus_and_default`; libadwaita containers
often hand that to an internal `AdwGizmo`). `set_focus(None)` after the hide
does not cancel the move; focusing a visible widget does, and so does
clearing focus *before* the hide. A focus assertion right after a hide
therefore passes only while no frame has painted, and flakes under load:
call `wait_for_painted_frame(&window)` (`tests/widget/common.rs`) before
asserting where focus went. (The preview alert copy releases focus before it
hides for this reason.)

If the question becomes "what does GTK consider visible, mapped, realized, or accessible here?", stop and read:

- [../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md](../../gtk4-libadwaita-internals/references/lifecycle-and-ownership.md)
- [../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md](../../gtk4-libadwaita-internals/references/geometry-measurement-and-allocation.md)

## `AdwDialog` Geometry Assertions

`AdwDialog` can be tricky to test geometrically because the dialog widget itself
is an overlay participant inside the parent window, while the user-visible card
or sheet is usually the dialog's child content.

For LushText widget tests:

- use `dialog.content_width()` / `dialog.content_height()` to assert the
  configured target size
- use the dialog child widget's `width()` / `height()` to assert the rendered
  floating surface the user actually sees
- do **not** assume `dialog.width()` / `dialog.height()` matches the visible
  card size

This matters for regressions where the dialog looks unchanged on screen even
though the `content-width` property was updated.

## Edit/Render Dialog Geometry Regressions

Dialogs with `GtkStack` Edit/Render modes need tests for the first activation, not only repeated toggles. The first click can perform extra work such as rendering Markdown, replacing an `AdwStatusPage` placeholder, or mounting a scrolled text surface; that is exactly when small dialog-size regressions appear.

For this class of test:

- compare the content widget's `measure()` natural sizes before and after the first Render activation
- use `dialog.content_width()` / `content_height()` or the dialog child's measured size instead of `dialog.width()` / `height()`
- assert inner text-surface margins before and after mode switches so Edit and Render do not drift
- assert that the Render page has already mounted its final scrolled text surface before first activation when the implementation supports that contract
- cover existing non-empty content and initially-empty dialogs where the user types text before first Render; those paths fail in different ways

If allocation is unavailable or unstable in the harness, measuring the dialog child is still useful. The important invariant is that the first Edit -> Render switch does not change the natural size contract that the parent dialog follows.

## Fixed Row Chrome Regressions

When a bug is really "the hover pill or active highlight of this button no longer fits inside its fixed row", prefer a property-level widget regression over screenshot-only verification.

Good assertions for this class of bug:

- the row still keeps its expected fixed height
- the button uses `valign == gtk4::Align::Center`
- the button carries the expected `margin_top` and `margin_bottom`

This is especially important for flat icon buttons living beside a dropdown or label in a fixed top row. If the bug is the button chrome itself, do not lock in a wrong fix by asserting extra margin on the list or scroller below.

## Animations and Frame Clocks

The custom harness does not guarantee that spinning the GLib main loop advances `AdwTimedAnimation` the way a live desktop session does.

Rules:

- Do **not** sleep for `150ms` or `250ms` and assume the animation finished.
- Prefer assertions on stable pre/post invariants.
- If the code already exposes a narrow test-only knob for animation completion, use it.
- If the behavior only reproduces with real frame-clock progression, run under `mutter --headless` or switch to `gtk-agentic-debugging`.
- To assert something does **not** happen inside a frame's paint (snapshot), bracket the paint with a one-pixel `GtkDrawingArea` packed *before* the widget under test and redrawn at every frame clock `before-paint` (its draw function runs inside the snapshot), and the clock's `after-paint`. Bracketing with `connect_after("layout")` misses frames that paint without a layout phase. To make a change happen inside a frame (as input does), make it from a tick callback (update phase). See `editor_page::PaintPhaseAdjustmentChanges` and ledger A37.

### Timing frames under the harness renderer

- **Under `GSK_RENDERER=cairo` (the harness default) an `AdwDialog`'s
  open and close animation frames cost 12–30 ms of paint** whatever the
  dialog holds (ledger A111): cairo composites the scaled, faded dialog in
  software. A whole-frame worst value in a dialog test is that paint, not the
  widgets under test.
- **The first GL dialog of a process includes the GL renderer's pipeline
  warm-up** (a first frame of about 86 ms, a close of 81 ms), recurring in no
  later dialog; a test that opens one dialog per process measures it.
- **Judge a frame by its layout phase**, with
  `common::phase_split_dispatch_until` (layout = dispatch start to the end of
  the frame clock's `layout`; paint = the rest), or measure GL in steady
  state (the second and later dialogs of a process). Print whole-frame worst
  values; never assert them under cairo.

### Parity with a stock widget is proved in situ

- **A parity test between a custom widget and the stock widget it replaces
  must run where the widget is used, not only in a test window.** CSS sizes
  fonts, and with them values such as `.caption`'s absolute `line-height`,
  from the widget's ancestors. `LushtextRowText` matched `GtkLabel` for every
  class and script in `row_text.rs`, yet notes-browser rows came out 1 px
  shorter, because its unparented CSS probe and the test window shared the
  default font while the browser's rows did not (found by comparing visual
  smoke screenshots of `main` and the branch). The in-situ test puts a
  `GtkLabel` with the same classes and text beside each realized row text and
  compares measure, baseline and allocation
  (`notes_browser::test_notes_row_text_measures_like_a_label_in_the_browser`).
  Add one for each new consumer surface.

## ListView, TreeListModel, and Row Reuse

When testing list or tree widgets:

- assert on model or selection state, not on assumptions about fresh row instances
- remember rows are recycled
- keep factory lifecycle questions separate from app-level expectations
- for "rows are missing" bugs, assert **rendered** rows, not model rows: walk the
  list view's realized children (visible, non-zero height) and read their bound
  labels after scrolling the real outer scroller to the position under test. The
  workspace-tree defect that motivated this had every entry in the
  `GtkTreeListModel` and only ~205 realized rows; a model-count assertion passed.
  `crates/lushtext/tests/widget/workspace_tree_virtualization.rs` has the helpers
  (`rendered_rows`, `scroll_outer_to_bottom`, `inside_outer_viewport`).
- GTK keeps more realized rows than the viewport shows (roughly one to two extra
  pages around the visible range, and up to its 200-widget anchor window after a
  fast drag). Bound realized counts against the viewport, not against exact row
  counts.
- reproduce against the real window and sidebar when the bug involves an outer
  scroller: a standalone section window has no outer scroller, so the slice bin
  simply hands the list the window height and the bug cannot appear.
- after `sidebar.load_workspaces()` the sidebar rebuilds its sections; a section
  handle taken from the first `sections` entry can point at a detached widget
  whose geometry reads as 0 px or a stale 48 px. Wait for `is_mapped()` on the
  sections (see `mapped_sections` in the module above) before measuring.
- keyboard-focus traversal in `GtkListView` calls the same `scroll_to(FOCUS |
  SELECT)` primitive as the Down-arrow binding; drive it directly rather than
  synthesizing key events under the headless harness.
- opening a popover from a test while the window has no focus widget trips a
  `Gtk-CRITICAL` inside `gtk_popover_activate_default` (`gtk_widget_is_ancestor`
  on a null focus). Give the list focus first, as the real keyboard flow does.

If the confusing part is `connect_setup`, `connect_bind`, `connect_unbind`, row reuse, or `GtkTreeListModel` behavior, read:

- [../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md](../../gtk4-libadwaita-internals/references/containers-lists-and-factories.md)

### Timing a list's first rows (`bound-lists-by-budget-not-count`)

- **Measure dispatches on the optimized widget build** (`CARGO_PROFILE_DEV_OPT_LEVEL=2
  CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false`), presented, with
  `common::worst_dispatch_until`; gate a timing assertion on
  `common::timing_verdicts_enforced()` so the ordinary debug suite prints
  without failing.
- **Destroy each repetition's window before the next** (`window.destroy()`,
  then drain). A measurement test that leaves its windows alive measures every
  earlier repetition's editors too: their validation idles and relayouts run in
  the later repetitions' dispatches and inflate them several times over.
- **A line the view has not laid out yet has zero height.** `line_yrange` and
  `line_at_y` read the text layout without validating it, so lines inserted a
  moment ago report height 0 until the view's validation idle or the next
  frame lays them out. An assertion about where such a line is drawn either
  waits for its height to become positive or treats a zero-height line as part
  of the row above it, as the minimap's row groups do (ledger A99).
- **The first rows into an empty `GtkListView` create its row widgets**, up to
  its realized cap (about 205, A1), in the dispatch that adds them and the
  frame after: each is a factory setup, a bind and a first layout. A
  benchmark of a large list's steady state misses this one-time cost, and a
  test that only counts model rows cannot see it; time the dispatch that
  first makes the list non-empty. An unrealized list does **not** walk its
  items per `items-changed` (A87), so a slow first turn is the widget
  creation, not the model.
- **A `GtkTreeListModel` row can own more than one child store**: the
  expander's `is_expandable()` probe runs the create function and discards
  the store (A22), so a cache keyed by directory path must follow the live
  store of an expanded row, or the probe's rows leak into it.
- **Resident memory**: reset the high-water mark (`common::reset_vm_hwm`) after
  the fixture is written and read `VmHWM` growth against the charge the
  budget made; each widget test runs in its own child process, so the reading
  is the test's.

## Assertion Rules

Assertion rules for widget and visual-geometry tests, which the
[skill](../SKILL.md#assertion-rules-that-save-time) points to. State extremes and
`is_visible()` versus the `visible` property are in `widget-wiring.md`,
"Testing", and above.

- For transient overlays, test the user's dismissal contract rather than only
  the revealer property: focus moved away but Escape still closes, outside
  pointer presses close and restore focus, inside pointer presses keep the
  surface open, and one Escape closes only the topmost surface when stacked.
- For accessible visibility, assert the `Hidden` state's **value** with `common::accessible_hidden` (`!accessible_hidden(w)` for "not hidden"). Never assert "not hidden" as `!gtk4::test_accessible_has_state(w, Hidden)`: `has_state` reports whether the state is *present*, and GTK writes `Hidden` itself (`true` at init and on unmap, `false` on map), so a widget that maps after the app cleared the state fails it while visible. When it maps is compositor timing (a revealer child maps on its transition's first frame, which a Wayland window awaiting a fullscreen configure does not draw), which is how `window::test_focus_mode_affordance_stays_visible_while_leave_button_has_focus` flaked. Likewise, focus a control only once it is mapped, as a user can: focus given to an unmapped widget may be dropped by the next configure.
- For `GtkListView` keyboard shortcuts, do not assume the focused widget is the list itself. In real sessions focus usually sits on a realized row descendant, so synthetic key delivery should target the focused widget and, if needed, walk up its parent chain until it reaches the ancestor that owns the `EventControllerKey`. Emitting the key directly on the list view can produce false-green tests for shortcuts that do nothing in the live app.
- For adaptive surfaces that animate, especially `AdwBottomSheet` used by the document-properties pane, do not close the animated surface as a cleanup step in headless widget tests unless the test is specifically proving the close animation. Assert the open/compact state, then explicitly destroy the presented test window and flush once. Closing the sheet right before process teardown can make Mutter report `gdk-frame-clock: layout continuously requested` and `Trying to snapshot LushtextWindow ... without a current allocation`, even when the production UI state is correct.
- For split-view animation regressions, add widget tests around the contracts the harness can prove: allocation sync should not rewrite persisted GSettings fractions, and cached breakpoint thresholds should only change when the effective integer threshold changes. Do not try to prove frame-rate smoothness in the widget harness; confirm that part in the real app or locally installed Flatpak after the contract tests pass.
- For same-session visual invariants, widget tests should assert the GTK
  allocation and scroll-anchor contract, while `make visual-geometry-smoke`
  should protect unaffected chrome with exact pixel comparisons and describe
  every allowed-changing region. Do not count two unrelated screenshots from
  separate launches as proof that an unaffected element had zero pixel variance.
- For before/after visual-geometry cases with setup such as search queries,
  scrolling, fixture selection, or focus state, drive that precondition and
  wait for its narrow readiness predicate before capturing the baseline image.
  A baseline taken before the intended state exists can make the comparison
  pass or fail for the wrong reason.
- For rendered effects such as highlights, edge lines, minimap markers, or
  overlay chrome, widget tests should assert the app-owned allocation/projection
  and `make visual-geometry-smoke` should assert named screenshot-derived
  `pixel_anchors` for the actual pixels. If the proof policy requires a named
  pixel invariant, the root summary must include it in
  `pixel_verified_invariant_ids`, and the relevant case rows must include
  screenshot-derived pixel row evidence plus final sidebar/editor/minimap
  geometry and final-frame rendered-anchor stability; `verified_invariant_ids`
  alone is not enough. If the rendered effect can drift during an animation,
  require stream-frame evidence with at least one mapped intermediate frame;
  a final settled frame is not proof for that class.
- For user-reported visual oddities that only reproduce at a live desktop size,
  run `scripts/lushtext-automation.py visual-geometry-capture ...` while the
  window is in the reproduced state, provide explicit overrides for unknown
  theme, word-wrap, fixture, direction, or viewport fields, and replay the
  generated scenario with `scripts/visual-geometry-smoke.py --scenario-dir ...`.
  Do not substitute nearby 720p, 1080p, 1440p, or generic maximized-like passes
  for the captured threshold class.
- For "the rows/rendering must not move" contracts inside a scroller, assert
  **where a row is drawn**, not what an adjustment holds. A `GtkScrollable`
  renders against its own adjustment value while the host positions it by a
  transform; adjustment-value tests pass while the two disagree by a few pixels.
  Use `common::RowStillnessProbe` /
  `assert_rows_still_across_selection`: `compute_bounds` relative to a fixed
  sibling above the host (the section header), resolved **by label each
  sample** (list rows are recycled), over **mapped rows only** (rows around
  the selection are realized but unmapped), across a *sequence* of forced
  layouts (`queue_allocate` + `flush_after_delay(16ms)`; `flush_after_delay`
  alone does not pump the frame clock), with the outer value unchanged as a
  precondition. Drive selection with `set_selected` and focus with
  `grab_focus` on already-visible rows; `select_and_scroll_to`/`scroll_to`
  always issue a genuine request and belong in the positive control. Events
  that change the model assert post-settle stability only, not across the
  event.

## When Widget Tests Are the Wrong Tool

Switch tools when:

- the bug depends on portal behavior, compositor state, or desktop focus handoff
- you need live stderr or journal evidence
- the failure only reproduces in a running app session

That is a `gtk-agentic-debugging` problem, not a better assertion problem.
