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

- [ ] 1.1 Add A132. Its fixture is a presented 40-tab `AdwTabBar`; probe it in
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
- [ ] 1.2 Add A133 the same way. It must cover:
  - `notify::is-overflowing` across the threshold, recording whether each
    notify ran inside allocation;
  - unpinned overflow holding with a 3-button end widget attached;
  - the bar's allocated height being equal with and without the end widget;
  - selecting the last page bringing it inside the page.

  Verify with `make gtk-axiom-sample AXIOM=A133 CHECK=1`.
- [ ] 1.3 Run both samples with `--check` inside `org.gnome.Sdk//50` and `//51`
  (`scripts/run-gtk-axioms.sh runtimes --only A132,A133`), and record the values
  in the two ledger rows and `crates/gtk-lush/axioms/CHANGELOG.md`. Verify with
  `make check-gtk-axioms` and
  `scripts/heavy.sh --envelope 16G -- make gtk-axioms`.
- [ ] 1.4 **Gate.** If A132, or a gating item of A133 (overflow notify, held overflow
  with the end widget, unchanged height), fails on either toolkit, record the
  failure in the ledger rows, commit and push the probe work as it stands, and stop
  here. Report the probe results to the maintainer and do not start group 2: the scroll buttons
  are the user's literal request, so dropping them is the maintainer's decision
  (design D3). Otherwise commit `test(gtk-lush-axioms): pin AdwTabBar scroll and
  overflow behaviour (A132, A133)` and push.

## 2. Scroll buttons (spec: "Scroll controls appear…", "Scroll buttons move…", the automation requirement's scroll half)

- [ ] 2.1 Create the role home `ui/window/tab_overflow/`, with a facade `mod.rs`
  that narrates the stages and inversions, and `policy.rs` carrying the
  whole-pixel `forbid` pair. Write failing-first unit tests in `policy.rs`
  that fail to compile until 2.2:
  - `unpinned_overflows`;
  - `controls_visible`: show, hide, the hysteresis band, and the frozen hold;
  - `can_scroll_back` and `can_scroll_forward` in LTR and RTL reading order;
  - `next_scroll_target`: clamping at both ends and a zero-width page;
  - `all_tabs_button_visible`, `group_attached`,
    `scroll_controls_available` and `focus_after_disable`.
- [ ] 2.2 Implement the D4 policy functions. Add the whole-pixel table row in
  `workflow-convention.md` and the entry in `WHOLE_PIXEL_POLICY_MODULES`
  (`scripts/check-workflow-boundaries.py`). Verify with
  `make check-workflow-boundaries` and the unit tests through `heavy.sh`.
- [ ] 2.3 Write failing-first widget tests in a new
  `crates/lushtext/tests/widget/tab_overflow_navigation.rs`, assigned to a shard
  in `scripts/widget-shards.py`. Animations complete at once through
  `tab_overflow/test_policy.rs`. Each must fail on main (no controls, no actions):
  - no buttons while tabs fit;
  - the buttons appear on overflow and the strip's allocated height is unchanged;
  - closing tabs until they fit beside the buttons hides them, and a width in the
    band keeps the state;
  - pinned-only overflow shows no buttons;
  - Forward scrolls without changing `selected-page`, text or focus;
  - edge enablement on both actions;
  - focus hands off when a focused button disables;
  - hold-repeat stops at the edge, adds no step on release, and its source is gone
    when the button hides mid-hold;
  - during a hold, with a long `test_policy` repeat delay keeping it pending,
    each of these cancels the repeat and leaves the strip's own scroll in
    effect: selecting a tab, a page attach, `page-reordered`,
    `is-transferring-page`, and the bar's scroll controller and press gesture
    emitted directly;
  - the scroll buttons have `focus-on-click` false, and their hold gesture runs in
    the capture phase (properties; the real press is proved in 2.8's
    accessibility smoke and task 4.3);
  - wheel-like movement, simulated by writing the adjustment, updates enablement
    only;
  - the group detaches with the popover closed first, when a tab closes down to
    one or Focus Mode starts while the popover is open;
  - pinned tabs stay put;
  - Focus Mode and the no-tab state hide the group (`group_attached` false);
  - a restored 40-tab session publishes the controls once at the terminal;
  - the RTL mapping;
  - the lookup returns `None` on a bar-shaped fixture without an unpinned
    scroller;
  - the `tab-context-actions` constrained-geometry scenario still holds at the
    640 px minimum with the controls shown.
- [ ] 2.4 Implement `tab_overflow/watch.rs` (observers, `SettleBurst`
  reconcile, projection deferral hooked into `refresh_tab_model_projections`),
  `tab_overflow/scroll_execution.rs` (lookup, animation, cancellation by the
  strip's own input, capture-phase hold gesture, repeat `SourceId`),
  `tab_overflow/controls.rs`, and the two scroll actions in
  `ui/window/actions.rs`. Accessible names go through `ui::accessibility`.
  Verify that the 2.3 tests pass.
- [ ] 2.5 Add `tab_overflow/evidence.rs` with the scroll fields (D4) and its three
  driven proofs:
  - side-effect freedom across a step, a hold and a guard cancellation;
  - disposal honesty;
  - no materialization.

  Verify with the widget tests.
- [ ] 2.6 Automation (the scroll half of the `dbus-automation-spine` delta):
  - the snapshot fields `window.tab_strip.{overflowing,scroll_back_enabled,scroll_forward_enabled,tab_count}`;
  - the readiness blocker `tab-overflow` (animation, repeat, reconcile);
  - the visual-geometry surface `tab-strip-overflow-controls` with pixel
    anchors;
  - catalog rows for both scroll actions.

  Document all of it in `docs/automation.md` and `docs/automation-reference.md`.
  Teach `scripts/lushtext-automation.py` the new blocker if it lists blockers.
  Verify with `make check-automation-docs` and, if the client changed,
  `make automation-client-self-test`.
- [ ] 2.7 Update the docs this group owes:
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
- [ ] 2.8 Add the visual-geometry case `tab-strip-overflow`. It crosses the overflow
  point, keeps the strip height constant, protects the editor region, and anchors
  pixels on the rendered Back and Forward buttons. Add an accessibility-smoke check
  for both names and `docs/end-user-coverage.md` entries. Run `make fmt`,
  `make check` and `cargo check -p lushtext --bins --locked`, then
  `make accessibility-smoke`, `make visual-smoke` and
  `make visual-geometry-smoke`. Commit right after (`feat(ui): scroll buttons
  for an overflowing tab strip`) and push.

## 3. All Tabs button and popover (spec: the All Tabs, search, choose, follow and accessibility requirements, and the automation requirement's popover half)

- [ ] 3.1 Write failing-first widget tests in `tab_overflow_navigation.rs`. Each
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
  - one announcement per settled query.
- [ ] 3.2 Write `resources/ui/all-tabs-popover.blp` and run
  `make blueprint-generate`. Implement `tab_overflow/all_tabs_popover/{mod,imp,item}.rs`
  (D6): `LushtextRowText` rows, `RowAccessibility`, `RowGrowth`,
  `RowRetirement` and `FuzzyQuery`. Verify with `make check-blueprint`.
- [ ] 3.3 Implement `tab_overflow/all_tabs.rs`:
  - the snapshot (no document text);
  - the `Debounce` re-snapshot;
  - focus saved in `set_create_popup_func` and restored;
  - `select_tab_from_all_tabs` through `ui/window/editor_focus/`;
  - the `GtkMenuButton` in the end group (D5);
  - `win.toggle-all-tabs` with `Ctrl+Shift+O`, and the two target-state
    actions.

  The copy and the accessible name live in `policy.rs`, with unit tests. Verify
  that the 3.1 tests pass.
- [ ] 3.4 Extend `TabOverflowEvidence` with the popover fields, and its proofs to
  popup, filter, close, growth and retirement. Verify with the widget tests.
- [ ] 3.5 Automation (the popover half of the `dbus-automation-spine` delta):
  - `surfaces.all_tabs_popover_visible`;
  - `active_transient_surface` = `all-tabs-popover`;
  - the readiness blocker covering growth and retirement;
  - the visual-geometry surface `all-tabs-popover`;
  - catalog rows for the three popover actions.

  Document them. Verify with `make check-automation-docs` and, if the client
  changed, `make automation-client-self-test`.
- [ ] 3.6 Add `all_tabs_filter_1024` to `crates/lushtext-core/benches/benchmarks.rs`.
  Run it once through `heavy.sh` and record the figure in `design.md` D6.
- [ ] 3.7 Update the docs this group owes:
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
- [ ] 3.8 Extend the accessibility smoke (button name, popover, rows) and the
  visual-geometry case (popover open at the minimum size, driven by
  `win.set-all-tabs-visible` and `win.set-all-tabs-query`). Run the gate order
  (`testing.md`), then the three smokes, and commit at once (`feat(ui): All Tabs
  list for the tab strip`) and push.

## 4. Integration

- [ ] 4.1 Run `scripts/heavy.sh --envelope 16G -- make test`. Verify there is no
  `FLAKY`, `OOM-KILL:` or `SESSION-HYGIENE:` line, and that
  `make check-widget-shards` passes.
- [ ] 4.2 Run the rustdoc lint gate (`testing.md`, "Rustdoc lint gate") for the
  new module docs. Verify it exits 0.
- [ ] 4.3 Do a live check with an isolated-XDG launch and 40 tabs restored:
  - cross the overflow point by resizing;
  - hold Forward, and select a tab during the hold;
  - open the popover by click and by `Ctrl+Shift+O`;
  - toggle Focus Mode.

  Verify stderr has no `Gtk-CRITICAL`, `Gtk-WARNING`, `GLib-GObject-WARNING` or
  `Trying to …` lines.
- [ ] 4.4 Run `openspec validate tab-strip-overflow-navigation --strict` and verify
  it is valid. If anything changed since 3.8, commit and push.

## Workflow follow-up

- Review the change and archive it after merge (`openspec-archive-change`).
