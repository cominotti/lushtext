# Tasks

Scope lines:

- **Kani: none (no proven module touched).** `scripts/kani-shards.py oracle`
  names no harness for the new `ui/window/tab_overflow/policy.rs`.
  `model/list_budget.rs` is consumed unchanged through `ui::list_growth`.
- **Timing: none (no timed path touched).** The `all_tabs_filter_1024` bench case
  is regression evidence run through `scripts/heavy.sh`, not a verdict.
- **Tests** are failing-first, listed before the code in each group, each naming
  where it must fail.
- **Proof lanes:** UI-prefix files need the three smokes current at each commit
  (`testing.md`). So only groups 2 and 3 touch them, and each ends with its
  smokes run just before its commit.

Every heavy command and every commit goes through `scripts/heavy.sh` (commits:
`scripts/heavy.sh --mem none -- git commit -F …`). Every commit is signed and
pushed to the change branch.

## 1. Gate: toolkit axioms A132 and A133 (design D3, D10)

Mirror the A122 commit (`3a086b0e`): a ledger row, `CHANGELOG.md`, an
`examples/aNNN_…` sample, `catalogue.rs`, `fixtures.rs`, `probes/aNNN.rs` and
`probes/mod.rs`, following `build.md`, "GTK Axioms and Toolkit Updates".

- [x] 1.1 Add A132. Its fixture is a presented 40-tab `AdwTabBar`; probe it in
  `probes/a132.rs`, with the sample `examples/a132_…`. Its control checks the
  template shape. It must check:
  - an external hadjustment write moves a tab's bounds by the delta and holds
    across two frames with the box idle;
  - a write pauses a playing box animation;
  - `page-increment` = 0.9 × `page-size`;
  - the RTL lower end is the last tab.

  It also records, as an observation that is not a pass condition, whether a
  write in the frame where a selection-driven box animation finishes is reverted
  by that frame's allocation (the accepted race, design D3).
  Verify with `make gtk-axiom-sample AXIOM=A132 CHECK=1`.
- [x] 1.2 Add A133 the same way. It must cover:
  - `notify::is-overflowing` across the threshold, recording whether each
    notify ran inside allocation;
  - unpinned overflow holding with a 3-button end widget attached;
  - the bar's allocated height being equal with and without the end widget;
  - selecting the last page bringing it inside the page.

  Verify with `make gtk-axiom-sample AXIOM=A133 CHECK=1`.
- [x] 1.3 Run both samples with `--check` inside `org.gnome.Sdk//50` and `//51`
  (`scripts/run-gtk-axioms.sh runtimes --only A132,A133`), and record the values
  in the two ledger rows and `crates/gtk-lush/axioms/CHANGELOG.md`. Verify with
  `make check-gtk-axioms` and
  `scripts/heavy.sh --envelope 16G -- make gtk-axioms`.
- [x] 1.4 **Gate.** If A132, or a gating item of A133 (overflow notify, held overflow
  with the end widget, unchanged height), fails on either toolkit, record the
  failure in the ledger rows, commit and push the probe work as it stands, and stop
  here. Report the probe results to the maintainer and do not start group 2: the scroll buttons
  are the user's literal request, so dropping them is the maintainer's decision
  (design D3). Otherwise commit `test(gtk-lush-axioms): pin AdwTabBar scroll and
  overflow behaviour (A132, A133)` and push.

## 2. Scroll buttons (spec: "Scroll controls appear…", "Scroll buttons move…", the automation requirement's scroll half)

- [x] 2.1 Create the role home `ui/window/tab_overflow/`, with a facade `mod.rs`
  that narrates the stages and inversions, and `policy.rs` carrying the
  whole-pixel `forbid` pair. Write failing-first unit tests in `policy.rs`
  that fail to compile until 2.2:
  - `unpinned_overflows`;
  - `controls_visible`: show, hide, the hysteresis band, and the frozen hold;
  - `can_scroll_back` and `can_scroll_forward` in LTR and RTL reading order;
  - `next_scroll_target`: clamping at both ends and a zero-width page;
  - `all_tabs_button_visible`, `group_attached`,
    `scroll_controls_available` and `focus_after_disable`.
- [x] 2.2 Implement the D4 policy functions. Add the whole-pixel table row in
  `workflow-convention.md` and the entry in `WHOLE_PIXEL_POLICY_MODULES`
  (`scripts/check-workflow-boundaries.py`). Verify with
  `make check-workflow-boundaries` and the unit tests through `heavy.sh`.
- [x] 2.3 Write failing-first widget tests in a new
  `crates/lushtext/tests/widget/tab_overflow_navigation.rs`, assigned to a shard
  in `scripts/widget-shards.py`. Animations complete at once through
  `tab_overflow/test_policy.rs`. A hold is started and ended by emitting its
  gesture's `pressed` and `released` by name, and the bar's observers are found
  through `observe_controllers()` and emitted the same way (design, Risks).
  Where a test needs code that does not exist yet, it fails by not compiling.
  Until group 3 the end group holds only the two scroll buttons, so the All Tabs
  focus hand-off and every popover case belong in 3.1. Each must fail on main (no controls, no actions):
  - no buttons while tabs fit;
  - the buttons appear on overflow and the strip's allocated height is unchanged;
  - closing tabs until they fit beside the buttons hides them, and a width in the
    band keeps the state;
  - pinned-only overflow shows no buttons;
  - Forward scrolls without changing `selected-page`, text or focus;
  - edge enablement on both actions;
  - focus hands off between the two scroll buttons, and to the editor, when a
    focused button disables;
  - hold-repeat stops at the edge, an emitted `released` adds no step, and its
    source is gone when the button hides mid-hold (the real pointer release is
    proved in 2.8's accessibility smoke and task 4.3);
  - during a hold, with a long `test_policy` repeat delay keeping it pending,
    each of these cancels it, asserted as `repeat_active` and
    `animation_active` false, `guard_cancellations` incremented and the
    adjustment value unchanged by the cancellation (pause, never reset):
    selecting a tab, a page attach, `page-reordered`, `is-transferring-page`,
    and the bar's scroll controller and press gesture emitted directly;
  - a press resolved inside the end group, and any observer emission while no
    step is active, cancel nothing and count nothing;
  - a keyboard step is refused while `is-transferring-page` is true;
  - the scroll buttons have `focus-on-click` false, and their hold gesture runs in
    the capture phase (properties; the real press is proved in 2.8's
    accessibility smoke and task 4.3);
  - wheel-like movement, simulated by writing the adjustment, updates enablement
    only;
  - pinned tabs stay put;
  - Focus Mode and the no-tab state hide the group (`group_attached` false);
  - a restored 40-tab session publishes the controls once at the terminal;
  - the RTL mapping;
  - the lookup returns `None` on a bar-shaped fixture without an unpinned
    scroller;
  - the `tab-context-actions` constrained-geometry scenario still holds at the
    640 px minimum with the controls shown.
- [x] 2.4 Implement `tab_overflow/watch.rs` (observers, `SettleBurst`
  reconcile, projection deferral hooked into `refresh_tab_model_projections`),
  `tab_overflow/scroll_execution.rs` (lookup, animation, cancellation by the
  strip's own input, capture-phase hold gesture, repeat `SourceId`),
  `tab_overflow/controls.rs`, and the two scroll actions in
  `ui/window/actions.rs`. Accessible names go through `ui::accessibility`.
  Verify that the 2.3 tests pass.
- [x] 2.5 Add `tab_overflow/evidence.rs` with the scroll fields (D4) and its three
  driven proofs:
  - side-effect freedom across a step, a hold and a guard cancellation;
  - disposal honesty;
  - no materialization.

  Verify with the widget tests.
- [x] 2.6 Automation (the scroll half of the `dbus-automation-spine` delta):
  - the snapshot fields `window.tab_strip.{overflowing,scroll_back_enabled,scroll_forward_enabled,tab_count}`;
  - the readiness blocker `tab-overflow` (animation, repeat, reconcile),
    included in `visual-geometry-settled`, `accessibility-settled` and
    `idle`, with a derived `tab-overflow` workflow event;
  - the visual-geometry surface `tab-strip-overflow-controls` with pixel
    anchors, absent with reason `controls-hidden`;
  - catalog rows for both scroll actions.

  Document all of it in `docs/automation.md` and `docs/automation-reference.md`.
  Teach `scripts/lushtext-automation.py` the new blocker if it lists blockers.
  Verify with `make check-automation-docs` and, if the client changed,
  `make automation-client-self-test`.
- [x] 2.7 Update the docs this group owes:
  - `docs/thresholds.md` rows with `Kind:` lines for `TAB_SCROLL_ANIMATION_MS`,
    `TAB_SCROLL_REPEAT_DELAY_MS` and `TAB_SCROLL_REPEAT_INTERVAL_MS`
    (`out: time`), plus any accessible-name bound (presentation cap);
  - a new `WFR-TAB-OVERFLOW-NAVIGATION` matrix row, declaring every
    `tab_overflow/` file by backticked path, recording `test_policy.rs` in its
    seam cell, and deriving its measured cells;
  - `ui/window/AGENTS.md`, the `AGENTS.md` module layout, and the `ui.md` widget
    hierarchy;
  - a new `A11Y-TAB-STRIP-OVERFLOW` row in `docs/accessibility-matrix.md`, and
    `docs/accessibility.md`.

  Verify with `make check-thresholds`, `make check-workflow-boundaries` and
  `make check-agent-docs`.
- [x] 2.8 Add the visual-geometry case `tab-strip-overflow`. It crosses the overflow
  point, keeps the strip height constant, protects the editor region, and anchors
  pixels on the rendered Back and Forward buttons. Add an accessibility-smoke check
  for both names and `docs/end-user-coverage.md` entries. Run `make fmt`,
  `make check` and `cargo check -p lushtext --bins --locked`, then
  `make accessibility-smoke`, `make visual-smoke` and
  `make visual-geometry-smoke`. Commit right after (`feat(ui): scroll buttons
  for an overflowing tab strip`) and push.

## 3. All Tabs button and popover (spec: the All Tabs, search, choose, follow and accessibility requirements, and the automation requirement's popover half)

- [x] 3.1 Write failing-first widget tests in `tab_overflow_navigation.rs`. Each
  must fail on main (no button, no action):
  - the button is hidden with one tab, leaving no empty space, and shown with the
    count from two tabs;
  - the count follows open and close, and updates once after a restore;
  - the count's width at 1, 2 and 3 digits and with large text, and the strip
    keeping a usable width at the 640 px minimum;
  - the popover lists pinned, modified and untitled rows in strip order, with
    the current row highlighted;
  - an in-situ `LushtextRowText` against `GtkLabel` measure inside the popover
    rows;
  - filtering by name and by folder (`palette` → that `mod.rs` first), the top
    match highlighted, the no-match state, and a cleared query restoring strip
    order with the current tab highlighted;
  - Enter selects an out-of-view tab, which becomes visible, with the editor
    focused;
  - Escape and a second `Ctrl+Shift+O` keep the selection and restore the focus
    saved before opening, by shortcut and by click;
  - a tab closed while open removes its row, and a stale activation only closes;
  - 256 tabs at the minimum size, with only the list scrolling;
  - `Ctrl+Shift+O` in a focused editor with one tab changes no text and opens
    nothing;
  - one announcement per settled query;
  - the group detaches with the popover closed first, when a tab closes down to
    one or Focus Mode starts while the popover is open;
  - focus hands off to All Tabs when both scroll buttons are disabled.
- [x] 3.2 Write `resources/ui/all-tabs-popover.blp` and run
  `make blueprint-generate`. Implement `tab_overflow/all_tabs_popover/{mod,imp,item}.rs`
  (D6): `LushtextRowText` rows, `RowAccessibility`, `RowGrowth`,
  `RowRetirement` and `FuzzyQuery`. Verify with `make check-blueprint`.
- [x] 3.3 Implement `tab_overflow/all_tabs.rs`:
  - the snapshot (no document text);
  - the `Debounce` re-snapshot;
  - focus saved in `set_create_popup_func` and restored;
  - `select_tab_from_all_tabs` through `ui/window/editor_focus/`;
  - the `GtkMenuButton` in the end group (D5);
  - `win.toggle-all-tabs` with `Ctrl+Shift+O`, and the two target-state
    actions.

  The copy and the accessible name live in `policy.rs`, with unit tests. Verify
  that the 3.1 tests pass.
- [x] 3.4 Extend `TabOverflowEvidence` with the popover fields, and its proofs to
  popup, filter, close, growth and retirement. Verify with the widget tests.
- [x] 3.5 Automation (the popover half of the `dbus-automation-spine` delta):
  - `surfaces.all_tabs_popover_visible`;
  - `active_transient_surface` = `all-tabs-popover`, and the generic
    `active-transient` visual surface (`push_active_transient_surface`);
  - the readiness blocker covering growth and retirement;
  - the visual-geometry surface `all-tabs-popover`, absent with reason
    `popover-closed`;
  - catalog rows for the three popover actions.

  Document them. Verify with `make check-automation-docs` and, if the client
  changed, `make automation-client-self-test`.
- [x] 3.6 Add `all_tabs_filter_1024` to `crates/lushtext-core/benches/benchmarks.rs`.
  Run it once through `heavy.sh` and record the figure in `design.md` D6.
- [x] 3.7 Update the docs this group owes:
  - the matrix row, declaring the popover files with re-derived cells;
  - README features and shortcut table, and `resources/ui/shortcuts.blp`
    (`make blueprint-generate`);
  - the `AGENTS.md` module layout, `ui.md`, and `docs/accessibility.md`
    keyboard paths;
  - the accessibility-matrix row, and the `docs/accessibility-orca-checklist.md`
    popover rows;
  - `docs/end-user-coverage.md`, and `docs/thresholds.md` for any new
    constant.

  Verify with `make check-agent-docs`, `make check-thresholds` and
  `make check-blueprint`.
- [x] 3.8 Extend the accessibility smoke (button name, popover, rows) and the
  visual-geometry case (popover open at the minimum size, driven by
  `win.set-all-tabs-visible` and `win.set-all-tabs-query`). Run the gate order
  (`testing.md`), then the three smokes, and commit at once (`feat(ui): All Tabs
  list for the tab strip`) and push.

## 4. Integration

- [x] 4.1 Run `scripts/heavy.sh --envelope 16G -- make test`. Verify there is no
  `FLAKY`, `OOM-KILL:` or `SESSION-HYGIENE:` line, and that
  `make check-widget-shards` passes.
- [x] 4.2 Run the rustdoc lint gate (`testing.md`, "Rustdoc lint gate") for the
  new module docs. Verify it exits 0.
- [x] 4.3 Do a live check with an isolated-XDG launch and 40 tabs restored:
  - cross the overflow point by resizing;
  - hold Forward, and select a tab during the hold;
  - open the popover by click and by `Ctrl+Shift+O`;
  - toggle Focus Mode.

  Verify stderr has no `Gtk-CRITICAL`, `Gtk-WARNING`, `GLib-GObject-WARNING` or
  `Trying to …` lines.

  Done headless, never on the live desktop: `live_check.py` (in the change's
  handoff directory) launched the debug binary with isolated XDG directories
  under `scripts/headless-session run --profile plain` (1920 × 1080), drove it
  with the automation client and real pointer and key input through Mutter's
  RemoteDesktop, and also covered the keyboard step during a tab drag (open
  item 2 of group 2). It found the release-after-selection step that D12
  records; all eighteen steps pass on the final tree.
- [x] 4.4 Run `openspec validate tab-strip-overflow-navigation --strict` and verify
  it is valid. If anything changed since 3.8, commit and push.

## 5. Implementation round of 2026-11-19 (design D14)

- [x] 5.1 Merge `origin/main` (#118 and its archive) and fix what fails under
  a stage `TMPDIR`: the two policy self-tests (`with-temp-scope.sh`,
  `next-rustc-wrapper.py`) and the session runtime directories of
  `cargo-gtk-proof`'s live runner, the Mutter capture helper and the Python
  smokes. Verify `make check-policy` passes under the stage `TMPDIR` and
  `/tmp`, and a widget run, an axiom sample and the three smokes start
  under it.
- [x] 5.2 Add the unchanged-selection re-announce to A133 (probe, sample
  control, ledger, CHANGELOG). Verify the sample `--check` holds on the host
  and inside `org.gnome.Sdk//50` and `//51`.
- [x] 5.3 Add `policy::tab_in_view`, `selected_tab_in_view` and its evidence
  and snapshot field, with unit and widget assertions. Verify the
  `tab_overflow` unit tests and the `tab_overflow_navigation` widget module
  pass.
- [x] 5.4 Extend the live check with the middle and last restore shapes and
  the return from Focus Mode, and key its drag step on `tab-strip`. Verify all
  steps pass.
- [x] 5.5 Add the restore requirement to the change's spec delta and list
  every exposed surface in the automation delta; update
  `docs/automation*.md`, `docs/end-user-coverage.md`, the workflow matrix row
  and `ui/window/AGENTS.md`. Verify `make check-automation-docs` and
  `openspec validate tab-strip-overflow-navigation --strict`.
- [x] 5.6 Reproduce the run-mutants stub failure fixed by `6ee67e14`, or
  record why it cannot be. Verify the fixed stub does not reproduce it.
- [x] 5.7 Run the final gates once on the final tree, each recorded through
  `scripts/gate-trees.sh record`: `make check`, `make test` (no `FLAKY`,
  `OOM-KILL:` or `SESSION-HYGIENE:` line), the rustdoc gate, and the three
  smokes; commit and push.

## 6. Review apply (programme template, 2026-11-19)

- [x] 6.1 Publish the overflow actions once `setup_actions` has registered
  them. Verify `test_the_overflow_actions_start_disabled_before_the_first_dispatch`
  fails first and then passes.
- [x] 6.2 Ignore a wheel over the trailing group in the strip observer. Verify
  `test_a_wheel_over_the_end_group_cancels_nothing_and_keeps_the_reveal`
  fails first and then passes.
- [x] 6.3 Cancel a pending focus restore on reopen and disposal, and hold
  readiness while it waits. Verify
  `test_a_reopen_before_the_focus_restore_keeps_the_focus_in_the_popover`
  fails first and then passes.
- [x] 6.4 Match the absolute folder in the All Tabs filter. Verify the policy
  unit test and the bench build.
- [x] 6.5 Move the reveal's per-frame step into `policy.rs`, end it without
  announcing on a strip that fits, and bound its hold on readiness. Verify
  `test_a_reveal_on_a_strip_that_fits_announces_nothing` fails first and then
  passes, and the policy unit tests pass.
- [x] 6.6 Plan the controls in one pure function; the smaller cleanups of D15;
  `docs/thresholds.md`, the automation docs, `ui/window/AGENTS.md` and the
  workflow matrix row. Verify the `tab_overflow` unit tests and the
  `tab_overflow_navigation` widget module pass.

## Workflow follow-up

- Review the change and archive it after merge (`openspec-archive-change`).
