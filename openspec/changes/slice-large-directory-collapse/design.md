## Context

The workspace sidebar shows each workspace folder as a `GtkTreeListModel`
(passthrough off, autoexpand off) over `gio::ListStore` child stores,
rendered by a `GtkListView` hosted in `gtk_lush_widgets::ViewportSliceBin`
(workflow `WFR-WORKSPACE-TREE`; `crates/lushtext-core/src/ui/sidebar/`).
`bound-lists-by-budget-not-count` (BL) made directories load every entry in
batches and made the section hold each expanded directory's child store
(`retained_child_stores`). After a collapse, BL retires that listing in
`G_PRIORITY_LOW` slices (`retire_child_store`, `drain_retiring_listings`). It
recorded the collapse dispatch as a toolkit-floor exception (A91 + A1) and
queued this change: "an owned collapse path that empties a large directory's
store in slices before GTK's collapse, removing A91's share of the exception,
and the attribution of the rare 14–18 ms re-expansion outlier at 131,072
entries" (`docs/next/long-line-documents.md`, BL follow-ups; archived BL
design, "Stage 4 review", decision 1).

Stage 1 measured before designing. The queued premise is wrong. A91's share
is about a tenth of the collapse. The cost is GTK's list item manager
replacing the row widgets bound around its anchor (A1), and it grows with
the rows that follow the directory, not with the rows it removes. This design
keeps the queued mechanism, an owned collapse that empties the store before
GTK collapses the row, and changes what it slices.

GTK facts used below were read in the GTK 4.24.1 sources, saved in
`.claude/handoffs/slice-large-directory-collapse/gtk-src-4.24.1/`:
`gtktreeexpander.c`, `gtktreelistmodel.c`, `gtklistitemmanager.c` and
`a11y/gtkatspiaction.c`.

## Goals / Non-Goals

**Goals**

- Every dispatch of a sidebar directory collapse that LushText starts (D5)
  fits its budget at every size, with or without rows after the directory.
  That runs from the input until the row is collapsed and its listing
  retired. The input dispatch and every collapse step fit `B_INTERACT_US`,
  and every frame in between fits `B_SETTLE_US`. A toolkit-floor exception is
  allowed only as D2 defines it.
- The user sees the collapse in the first frame after the input, within
  `INPUT_TO_FRAME_US`.
- The collapse's final state is `main`'s. That covers rows, expansion intent,
  watch targets, retired stores, the accessible state, and the scroll
  position when the directory row is on screen.
- Small-directory expansion meets the existing requirement that every
  dispatch applying a sidebar directory's rows fits `B_SETTLE_US`, if quiet
  measurement confirms it is violated.
- BL's queued follow-ups are resolved with evidence: the A91 attribution, the
  re-expansion outlier and the `is_expandable()` probe scan.

**Non-Goals**

- Removing the `is_expandable()` probe scan. `stop-expander-bind-time-child-scans`
  owns it; see D12.
- Lighter sidebar rows, and reuse of removed row widgets by GTK. These are
  follow-ups; see D4.
- Collapses LushText does not start: a refresh splice that removes an expanded
  directory, a section rebuild, a model reset, or an AT-SPI `DoAction` on the
  expander's class actions (D5). They keep GTK's direct collapse and are
  measured as residuals.

## Measured baseline

**Protocol (stage 1, non-verdict).**

- Toolkit: GTK 4.24.1 / libadwaita 1.10.0, toolbox `hypatia-f45`, `main`
  `72c039ef`.
- Build: optimized widget build (`CARGO_PROFILE_DEV_OPT_LEVEL=2
  CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false`), CPUs 8–23.
- Session: headless Mutter, presented 800 px window, harness renderer cairo,
  one workspace folder `root` holding `nested/` (N files) and optionally 400
  files `zz-*` after it.
- Runs: through `scripts/measure.sh` (labels `slc-*` in
  `.claude/handoffs/slice-large-directory-collapse/measure.log`).
- **Load: 1-minute load 7–13 in every run** (five parallel changes), so
  nothing here is a verdict. The code is the scratch patch
  `scratch-measure.patch` in the same directory. The figures agree with BL's
  quiet `m88` figures where they overlap: BL had 15.1–15.3 ms at 10,000
  entries with nothing after the directory; here the call is 15.6–17.7 ms
  from 256 entries up.

**Collapse call** (`TreeListRow::set_expanded(false)`, µs; 3 or 6 reps each).

| rows after | N = 16 | 64 | 256 | 1,024 | 4,096 | 16,385 | 131,072 |
|---|---|---|---|---|---|---|---|
| 0 | 1,353–1,555 | 4,467–4,697 | 15,604–16,665 (one 34,942 first-run outlier) | 16,964–23,493 | 15,712–20,271 | 16,693–23,243 | 24,590–32,554 |
| 400 | 3,912–4,047 | 12,418–13,091 | 36,653–43,605 | 43,149–50,012 | 41,511–46,215 | 42,271–64,131 | 49,506–105,481 |

**The dispatch after it** (worst, the next 600 ms excluding the probe
publication below): 400 rows after → 4,005–4,122 / 12,205–12,986 /
25,572–38,472 µs (one 58,473) at N = 16 / 64 / 256, and 39,711–62,374 µs from
1,024 up. With nothing after it: under 1.3 ms up to 1,024 and 2.3 ms at 4,096.
At 16,385 the worst, 5.6–8.3 ms, is the probe publication of D12.

**List-view children** (`GtkListView` child widgets): 21 / 69 / 207 before the
collapse at N = 16 / 64 / ≥ 256. After it: 5 with nothing after the
directory, 207 with 400 rows after it. The band is full at about 207 rows
(A1), whatever the viewport slice bin allocates.

**Attribution** (`perf record -e cpu-clock:u -F 4000 --call-graph=fp
--user-callchains`, cases 400:256 and 0:16,385; aggregate in
`collapse-attribution.txt`).

- 1,150 samples fell inside the collapse call:
  - 89 % in `gtk_list_item_manager_model_items_changed_cb`;
  - 62 % of all samples in `gtk_list_item_change_finish`, destroying unclaimed
    row widgets: factory teardown, then the overlay, expander, box, buttons,
    style contexts and AT contexts disposing;
  - 27 % in `gtk_list_item_manager_ensure_items`, creating replacement widgets
    through `gtk_list_view_create_list_widget` and factory setup;
  - 10 % in `gtk_tree_list_model_collapse_node` → `gtk_rb_tree_unref`
    (A91's share).
- Frame dispatches outside the call: 82 % in `gtk_css_node_validate`, style
  computation for the newly created rows' nodes.
- GTK 4.24.1 reuses a released row widget only for the same item:
  `gtk_list_item_change_get` looks it up in `deleted_items` by item, and
  `gtk_list_item_change_finish` destroys the rest. Removed rows' widgets are
  never recycled for the rows entering the band.

**After a collapse at N = 131,072** (nothing after the directory, 6 runs). BL's
listing retirement turns take 4.0–5.4 ms each. Then, **449–631 ms after the
collapse, one 37,397–50,310 µs dispatch** runs, and the section's
`scan_pressure.terminal_publications` advances by one in exactly that
dispatch, every run. At 16,385 the same dispatch takes 5.6–8.2 ms, about
0.045 s after the collapse. At N = 256 it is within the first dispatch. See
D12.

**Re-expansion right after a collapse**, worst dispatches over 8 ms:
- N = 256, nothing after: one dispatch of 21,084–32,964 µs (dispatch 1 of 2);
- N = 1,024: one of 17,404–19,321 µs;
- 4,096: two of about 9.5 ms;
- 16,385: one of 10.3–26.8 ms;
- 131,072: none.

BL's verdict test measured only 10,000 entries and up, with nothing after the
directory.

**Prototype A, "A91 only"**: remove all but 512 children from the tail in
16,384-row slices, one per frame, then collapse.
- Each slice takes 4.9–6.6 ms with nothing after the directory and 5.3–11.9 ms
  with 400 rows after.
- The final call: 17.8–21.1 ms at 131,072 (bare: 24.6–31.3) and 13.7–20.2 ms
  from 256 to 16,385 (unchanged).
- With 400 rows after: 38.5–54.1 ms.

So removing A91's share does not bring the collapse within budget.

**Prototype B, "realized-row units"** (`slc-head-1`, load 13): remove units of
K bound children per frame, then collapse.

| case | order | K | worst step | worst frame | final call | first visible | collapsed |
|---|---|---|---|---|---|---|---|
| 400:256 | head | 16 | 7,977 | 10,010 | 3,613 | 13.9 ms | 220 ms |
| 400:256 | head | 32 | 7,431 | 6,273 | 6,448 | 17.2 ms | 130 ms |
| 400:256 | head | 64 | 32,220 | 16,865 | 21,279 | 33.2 ms | 161 ms |
| 400:256 | "tail first" (no tail: repeats head) | 16 | 14,593 | 11,665 | 3,830 | 13.4 ms | 264 ms |
| 400:256 | "tail first" (no tail: repeats head) | 32 | 14,687 | 13,117 | 18,054 | 16.8 ms | 186 ms |
| 0:131,072 | unbound tail first | 16 | 6,495 | 4,173 | 1,856 | 133 ms | 307 ms |
| 0:131,072 | unbound tail first | 32 | 7,090 | 5,869 | 3,259 | 135 ms | 224 ms |
| 400:16,385 | unbound tail first | 16 | 6,183 | 6,832 | 4,626 | 28.8 ms | 244 ms |
| 400:16,385 | unbound tail first | 32 | 10,067 | 9,787 | 9,376 | 37.8 ms | 174 ms |

The 400:256 "tail first" rows run the same removals as the head rows (no
child lies past the 256 kept), so they show the spread between identical runs
at load 13: about 2× on steps and frames, and 6.4 against 18.1 ms on the
final call at K = 32. "Collapsed" is wall time with a 9 ms pump after each
step. In frames: at K = 32, 7 unit steps plus the final call (8 frames: 67 ms
at 120 Hz, 133 ms at 60 Hz); at K = 16, 15 plus 1.

In every case the directory row's rendered top stayed at 44.0 px, the slice
bin's `correction_count()` did not change, and `outer_request_pending()` was
false at the end. Head-only on a huge directory keeps creating widgets for
children it removes next (4,096 units at 131,072), so it was run only up to
4,096 entries.

## Decisions

### D1. Refine the queued change: slice the realized-row churn, not the store

The queued description sliced the store to remove A91's share. Prototype A
shows that leaves every collapse from 256 entries at 14–20 ms, or about 40 ms
with rows after the directory, so the description is rejected. The owned
collapse path it called for is kept. What it slices is the replacement of
bound row widgets per frame: Prototype B, at K = 16–32 and load 13, kept
steps at 6.2–14.7 ms and frames at 4.2–13.1 ms, against about 40 + 38 ms bare.
Whether a unit of 8, 16 or 32 meets `B_INTERACT_US` is decided by the quiet
verdict (D3). Scope follows the cost, not
the entry count. Any directory whose subtree holds more than
`DIRECT_COLLAPSE_ROWS` flattened rows takes the sliced path. That is the
common case: 64 entries with rows after them already cost 12 + 12 ms on
`main`.

*Alternatives rejected.* A91-only slicing (Prototype A, above). Accepting
the exception (BL's decision 1): the exception was attributed to the wrong
cost, and `main` freezes for two dispatches of about 40 ms on a 256-entry
directory, which is ordinary. Lighter rows only, and upstream widget reuse:
see D4.

### D2. Budgets and pass bar

- A collapse is an interaction. The click or key press that starts it is
  read against `B_INTERACT_US`, and so is every collapse step, because each
  runs while the user is waiting on the sidebar.
- Each frame drawn during a collapse, which styles the rows a step brought
  into the band, is read against `B_SETTLE_US`. A frame-paced step runs
  inside the frame clock's dispatch (D4), so that one dispatch carries the
  step and the styling of one unit. The step's own time is read against
  `B_INTERACT_US`. The whole dispatch is read against `B_SETTLE_US`.
- Scope: the routes LushText owns (D5). The residual routes are measured and
  recorded, not judged.
- Exception: if the smallest measured unit (8) still cannot keep a step within
  `B_INTERACT_US` because of the toolkit's per-widget cost, that dispatch is
  recorded as a measured toolkit-floor exception citing A112. This is the
  existing clause of "Workspace list dispatches fit the settle budget". The
  unit then stays at 8. Nothing over budget is left unexplained.
- The first frame after the input shows the collapse: the arrow and the
  accessible state, D6.
- Time until collapsed is printed and recorded, not enforced. Its expected
  ceiling is
  `ceil(bound rows / SIDEBAR_REALIZED_ROWS_PER_FRAME) + ceil(unbound rows / (COLLAPSE_UNBOUND_CHUNK_ROWS × chunks per turn))`
  frames. This is the trade the change makes against `main`'s faster but
  frozen collapse, and the proposal states it as user-visible.
- Excluded from the pass bar, as a named residual: the probe scan's
  publication (D12).

**Measurement protocol (verification-runs.md).**
- Four rounds interleaved `main` → branch → `main` → branch, through
  `scripts/measure.sh`; a round counts only at a recorded 1-minute load
  below 4. Build and session as in "Measured baseline".
- Cases:
  - collapse with 0 and 400 rows after the directory, at N = 64, 256, 4,096,
    16,385 and 131,072;
  - the folder header's collapse of two expanded 4,096-entry roots at once
    (D7);
  - re-expansion in the middle of a sliced 131,072-entry collapse (the refill
    of D7);
  - expansion with 0 and 400 rows after the directory, at N = 64, 256 and
    1,024, measured both cold (first expansion) and right after a collapse
    (D10). Stage 1 measured only the second.
- Recorded per case: the input dispatch, every dispatch until collapsed plus
  700 ms, with frames split into layout and paint phases, and
  time-to-collapsed. Frame verdicts are taken under the GL renderer in steady
  state, and the cairo figures are recorded beside them, as
  `lighten-notes-browser-rows` established for cairo's paint cost (A111).
- Residual routes, measured once on each side and recorded, not judged:
  - a refresh splice that removes an expanded 256-entry and an expanded
    131,072-entry directory (deleted on disk);
  - a section rebuild with such a directory expanded;
  - a collapse while the sidebar is hidden (D4 pacing);
  - an AT-SPI `DoAction` of the expander's `listitem.collapse` on a
    256-entry directory (D5).
- Pass: on the branch, the input dispatch and every step fit
  `B_INTERACT_US`, and every dispatch fits `B_SETTLE_US`, in every round, for
  the owned routes, except a recorded exception (above).
- Long campaigns run detached.

### D3. Constants

All are powers of two (`.agents/rules/rust.md`), chosen by the verdict
campaign (task 1) and then fixed.

- `DIRECT_COLLAPSE_ROWS`: cap, rounded down. Candidate 32. A subtree of at
  most this many flattened rows collapses directly. At 64 entries `main`
  measures 4.5 ms with nothing after the directory and 12.4–13.1 ms with rows
  after, so 32 replacements cost about 6 ms plus their frame.
- `SIDEBAR_REALIZED_ROWS_PER_FRAME`: cap, rounded down. Candidates 8, 16
  and 32. Prototype B at load 13: 16 → steps up to 14.6 ms and frames up to
  11.7 ms; 32 → 14.7 / 13.1 ms; 64 fails. The 8 ms pass bar is not yet
  supported at 32. On `main` one replacement (destroy plus create) costs about
  190 µs in the call (400:256: about 39 ms for about 207). The next frame then
  styles each new row in about 185 µs (about 38 ms), so 32 rows are about
  6 + 6 ms and 16 about 3 + 3 ms. It is the most bound rows one
  step removes, and so the most widgets one frame must style. It is also
  `DIRECT_COLLAPSE_ROWS`'s upper bound, so the final collapse is never larger
  than a step. It is named for the sidebar, not for the collapse, because
  D10's expansion cap uses it too.
  - *Why not an existing constant?* `model::list_budget` already has two:
    - `LIST_RETIREMENT_UNIT_ROWS` (16) sizes an unmapped retirement's
      teardown only (about 70 µs a row);
    - `LIST_GROWTH_STEP_ROWS` (32) sizes the rows a lazy list realizes per
      frame (about 0.16 ms of layout a notes or palette row).

    A sidebar row costs about 190 µs to replace plus about 185 µs to style
    (stage 1, under load). That is a different measured basis on a different
    list. Reusing either value would tie two lists' costs together that
    change independently. The verdict may pick the same number; the
    constant's doc then says so.
- `COLLAPSE_FRAME_STARVATION_MS`: timeout, rounded up. Candidate 128. See D4.
- `COLLAPSE_UNBOUND_CHUNK_ROWS`: cap, rounded down. Candidate 8,192.
  Prototype A measured 4.9–6.6 ms per 16,384 unbound rows under load, where
  the cost is LushText's mirror bookkeeping plus GTK's per-node rb-tree
  removal, so half fits the turn slice. A step's unbound chunks run under
  `model::list_budget::turn_continues` (`LIST_TURN_SLICE_US`), so a step
  costs at most the slice plus one chunk.

### D4. Order of removal

For each step, the policy (D9) picks one removal:

1. **Deepest expanded descendant first.** An ancestor's collapse removes
   every descendant row with it, in GTK's one call. So descendants under the
   collapsing directory are drained before their parent, and each drained
   descendant is then collapsed directly.
2. **Within a store:**
   - first every row no widget is bound to, in chunks, one range at a time:
     the rows after the band, then the rows between the directory row and
     the band. The latter exist only when the user scrolled into the
     children. Per A110 and A112, this destroys and creates no widget. The
     ranges come from `model::list_budget::for_each_unheld_range` (D9),
     which already yields them last first, so earlier starts stay valid;
   - then the bound rows, from the band's first child, in units. Each unit
     brings at most a unit of new rows into the band. Those are rows after
     the subtree, which stay.
3. **Pacing.** A step is one turn. It runs unbound chunks while
   `turn_continues` holds, and at most one bound unit:
   - While the list view is mapped, steps run from its frame clock, through
     a tick callback, as `ui/list_growth.rs` paces a staged publish. The
     callback runs in the frame's update phase, before its layout. The
     widgets a unit creates are therefore styled in that same frame, and two
     units never share a frame. Unbound chunks create no widget (A112), so a
     step that takes only chunks needs no frame of its own. They still run
     on ticks, which keeps one source.
   - **One unit per frame clock, not per section.** The sections of a window
     share its frame clock. A window-wide token admits at most one bound unit
     per frame across every section's collapse (D7).
   - While the list is not mapped (the sidebar hidden, the section body
     collapsed, the window unmapped), no frame is drawn. Steps then run from
     a `G_PRIORITY_LOW` timeout with the same bounds. A110 does **not** show
     that an unmapped list creates no widget for the rows entering its band.
     A110 measured removal from the end of what is left, with nothing after
     it. It also recorded that the wrong order realizes 512 rows on an
     unmapped list. So the unmapped path keeps the same unit bound, and
     A112's probe records the unmapped head removal with rows after it.
   - **A mapped list whose frame clock is starved** (a minimized or occluded
     window keeps its widgets mapped, and its frame clock may not tick): no
     axiom says otherwise, so the step source does not rely on ticks alone.
     If no tick arrives within `COLLAPSE_FRAME_STARVATION_MS` of the last
     step, the next step runs from the `G_PRIORITY_LOW` source. The constant
     is a timeout, rounded up; candidate 128, about 15 reference frames at
     120 Hz and 8 at 60 Hz. The next tick returns pacing to the frame clock,
     but takes no bound unit itself: a starved step's unit and that tick's
     would otherwise share one layout, breaking one unit per frame. The tick
     after it does.
   - The source switches on `map` and `unmap`, and on starvation. A collapse
     therefore always finishes, and `workspace-refresh-complete` cannot stay
     blocked behind a list nobody draws.

*Why this order.* Removing the band first, from the head, on a directory
larger than the band keeps creating widgets for children the next step
removes (Prototype B at 131,072 would need 4,096 units). Removing everything
unbound first, on both sides of the band, leaves only the band, whose
replacements are siblings that stay. That is also `ui/list_retirement.rs`'s
first turn. Removing the band while unbound subtree rows precede it lets GTK
realize those earlier rows instead (A110's last clause, on unmapped lists).
The rows before the band therefore go before any unit, not after. A112 pins
the mapped case. *Fallback*, if A112 shows that a mapped list moves its band
or realizes rows when rows before the band are removed: those rows go last,
as in this design's first draft, and the probe records why.

*Rejected.*
- **Lighter rows alone.** Fewer widgets and CSS nodes per row would cut each
  replacement, but about 200 replacements plus their styling cannot fit
  8 ms. At about 190 µs per replacement in the call and about 185 µs per new
  row in the next frame's styling (400:256, both under load), a reduction of
  about 10× would be needed. Recorded as a follow-up, which this change's constants
  would then let grow.
- **GTK reusing removed widgets for entering rows.** The real fix in
  `gtk_list_item_change_finish`. It is not in LushText's control, so it is an
  upstream-report candidate (task 8.4), never filed without approval.
- **Detaching the model or hiding the list during the collapse.** Either one
  tears every row down at once (A1).

### D5. One collapse entry point and one expand entry point, owning every LushText route

`LushtextWorkspaceSection::collapse_directory(row, origin)` is the only
LushText caller of `TreeListRow::set_expanded(false)` on a directory row.
`LushtextWorkspaceSection::expand_directory(row, origin)` is the only caller
of `set_expanded(true)`. During a collapse the row is still GTK-expanded, so a
bare `set_expanded(true)` would be a no-op and `is_expanded()` would answer
true; the expand entry point instead cancels the collapse and refills the
store (D7), and otherwise expands as today. A `make check-workflow-boundaries`
rule enforces both. Routes:

| route | today | after |
|---|---|---|
| expander click | The `GestureClick` is **not on the expander**. `update_for_list_row` creates it on the expander's `expander` icon child (a `GtkBuiltinIcon`, `gtktreeexpander.c:193-210`), and destroys and re-creates the icon whenever expandability changes. Its `released` handler (`:150-165`) activates `listitem.toggle-expand` and claims the sequence. The expander's only own controller is the drop-motion controller (`:753`). LushText's bind-time loop over `expander.observe_controllers()` (`row_factory.rs:338-357`) therefore finds no `GestureClick` on 4.24.1. It is inert, and its comment describes an older toolkit | LushText adds one capture-phase `GestureClick` to the expander at setup. It lets the press through, as GTK's own gesture does, so the row's press-side behaviour (focus on click, selection, double-press activation) is `main`'s. On a release inside the current `expander` icon's bounds it claims the sequence in the capture phase, before the icon's bubble gesture sees the release, so GTK's `released` handler never runs, whatever icon GTK has created since. That is the moment and the area GTK claims, by construction. Then expanding activates `listitem.scroll-to` and calls the expand entry point, and collapsing calls the collapse entry point. A release elsewhere is left alone. A113 pins where the gesture lives, that the release claim denies it, and that the row's press-side behaviour is unchanged. The inert loop is removed, or re-aimed if A113 shows a toolkit where it acts |
| expander keys | `listitem.expand`: `plus`, `KP_Add`, `asterisk`, `KP_Multiply`. `listitem.collapse`: `minus`, `KP_Subtract`, `slash`, `KP_Divide`. By text direction: `Shift+Right`, `Shift+KP_Right`, `Ctrl+Shift+Right` and `Ctrl+Shift+KP_Right` expand in LTR and collapse in RTL; the `Left` and `KP_Left` four do the opposite. `listitem.toggle-expand`: `Ctrl+space`, `Ctrl+KP_Space` (`:654-690`). Class bindings fire only for key events that reach the expander, i.e. with focus on it or inside it. `GtkTreeExpander` is focusable (`:746`), but LushText leaves `GtkListItem:focusable` at its default `TRUE`, which the expander's own documentation (`:49-50`) says puts keyboard focus on the row instead. A row is the expander's ancestor, so a key with focus on the row never reaches it. These keys may therefore be unreachable today except with focus on a focusable descendant such as the Focus Folder button. That is open question 4, settled by A113 | the capture-phase key controller handles exactly the keys A113 shows GTK honours for the focus it sees, no more, so it adds no keyboard behaviour: collapse keys call the collapse entry point; expand keys on a collapsing row call the expand entry point; everything else propagates. If A113 shows none is reachable, no controller is installed and the row is recorded as "no key route" |
| expand callers | the reveal's `advance_expansion` (`reveal_execution.rs:344`), the new-item flow (`file_execution.rs:119`), the restore (`scan_execution.rs:1664`), folder expand / toggle / load-time auto-expand (`folder_execution.rs:399, 409, 636`) | each calls the expand entry point, so none acts on a draining store |
| row activation | `activate_file_at` toggles | collapse calls the entry point |
| folder header collapse / toggle | `folder_execution.rs` `set_expanded(false)` per root | the entry point per root |
| reveal failure terminal | `reveal_execution.rs` collapses what it expanded | the entry point; the reveal's terminal does not wait for it |
| reorder drag hover | `reorder_execution.rs` defensive idle collapse of an inert, empty model | the entry point (always direct: an inert model has no rows) |

Class actions on `GtkTreeExpander` cannot be overridden by an inserted action
group, and the type is final, which is why the gesture and the keys are taken
over rather than the actions. A113 pins this. Not owned, and measured as
residuals:
- a refresh splice removing an expanded directory, a section rebuild or a
  model reset;
- `listitem.collapse` or `listitem.toggle-expand` activated by anything
  else;
- **AT-SPI `DoAction`.** `GtkTreeExpander` has no dedicated action vtable,
  but `gtk_atspi_get_action_vtable` falls through to the generic
  `widget_action_vtable` for any `GtkWidget` (`gtkatspiaction.c:1063`). That
  vtable lists the widget's own enabled, parameterless muxer actions. The
  expander installs three class actions, so in 4.24.1 an assistive
  technology can call `listitem.expand`, `listitem.collapse` and
  `listitem.toggle-expand` on it. These run GTK's direct `set_expanded`.
  They are not owned, because disabling a class action
  (`gtk_widget_action_set_enabled`) would also disable its key bindings, and
  a class action cannot be shadowed. Such a collapse behaves as on `main`.
  A113 records the action table on both toolkits, and D2 measures the route
  once as a residual. Assistive-technology and keyboard users reach the
  owned path through row activation (`Enter`), which toggles a directory
  (`activate_file_at`). Task 8.3 states both routes in
  `docs/accessibility.md`;
- **GTK's drag-hover expansion** (the expander's drop-motion controller,
  `:715-731`, expands a collapsed row after 500 ms). It never fires on a
  collapsing row, because the row is still GTK-expanded. So a drag hovering a
  row that looks collapsed does not open it until the collapse ends. Recorded
  as a residual; `reorder_execution.rs`'s defensive collapse already treats
  it.

### D6. Showing the intent in the input dispatch

The input dispatch does no row work. It:
- adds `lushtext-collapsing` to the row's `GtkTreeExpander`, whose CSS shows
  the collapsed arrow over `expander:checked`, both text directions (A114);
- sets the row overlay's accessible `expanded` state to false;
- sets the `GtkTreeExpander`'s own accessible `expanded` state (role
  button) to false. GTK sets that state only in `update_for_list_row`, at
  bind (`gtktreeexpander.c:220-228`). Its `notify::expanded` handler updates
  the role-none icon instead (`:278-301`). Any other row notify calls
  `update_for_list_row` again (`:306-309`). So nothing of GTK's overwrites
  the state mid-collapse except a rebind or such a notify, and the bind path
  below re-applies it. Both the overlay and the
  expander are restored when a collapse is cancelled by re-expansion (D7);
- marks the section's collapse state, and arms the first step.

The row stays GTK-expanded until the final step. Every LushText reader of
expansion state must read the collapse state as well. That includes the
row factory's bind-time projection (`row_accessibility.rs:86-91`, the
overlay's `expanded` state and the row description) and its
`notify::expanded` hook (`:133`). A rebind of the collapsing row, for example
when it scrolls out of and back into the band, would otherwise report it
expanded again. The bind path also re-applies the expander's accessible state
and the `lushtext-collapsing` class from the collapse state. The authoritative
`expanded_paths` drops the subtree at once, as a collapse does today. The
evidence surface reports `collapsing`.

*Fallback*, if A114 is refuted on either toolkit: the first step removes a
bound unit in the frame after the input, so the first visible change is
still in the first frame (Prototype B: 13.9–17.2 ms at 400:256 under load),
and only the arrow lags.

### D7. State while a collapse runs

- **Loads, scans, refresh and watch.** At the start, every store in the
  subtree loses its scan runtime and load turns (`cancel_child_store_runtime`).
  A queued refresh target under the directory is dropped. Watch targets
  follow `expanded_paths`, so the subtree's watches retire with today's
  generation guard. No batch can append to a store the collapse is draining.
- **Selection and focus.** If the `SingleSelection` item or the list's focus
  row is inside the subtree, both move to the directory row first, before
  any removal. Otherwise each removal of the selected row would re-select a
  later child and file peek would follow it. The entry point does the same on
  the direct path, so both paths behave alike. This is a deliberate change
  from `main`, stated in the proposal.
- **Re-expansion during a collapse**, by any route (D5: click, expand keys,
  reveal, new-item flow, restore, folder toggle). It cancels the collapse. The
  row never left GTK's expanded state, so the store, partly drained, is
  refilled through `populate_child_store`, the existing refresh path: a fresh
  scan reconciled into the same store, in BL's batches. The arrow class and
  the accessible state are restored at once, and the expansion intent is
  re-added. A caller that needs the children (the reveal, the new-item flow)
  waits on that refill's terminal exactly as it waits on a first expansion's
  scan.
- **A repeated collapse request** for a collapsing row is a no-op. A collapse
  of an ancestor of a collapsing directory takes the descendant's collapse
  over as its first part.
- **Several collapses at once.** `collapse_folders` and `toggle_folders`
  collapse every root of a section in one input. A user can also collapse a
  second directory while one is still collapsing, in the same section or in
  another one of the window. Each requested row shows its intent at once
  (D6). The collapsing rows are kept in request order in the section's
  collapse queue. The window-wide token of D4 admits one bound unit per
  frame, taken from the first collapse that needs one. Unbound chunks of
  later collapses may run in the same step while `turn_continues` holds.
  Small subtrees (at most `DIRECT_COLLAPSE_ROWS` rows each) collapse
  directly in the input dispatch until the rows removed directly reach
  `SIDEBAR_REALIZED_ROWS_PER_FRAME` in total. The rest join the queue. So
  one input never replaces more than one unit of widgets, however many roots
  the header collapses. The evidence (D11) reports the queue.
- **A refresh splice, section rebuild, model reset or dispose** that removes
  the collapsing row cancels the collapse: GTK removes what is left directly
  (a residual), and the drained stores go to BL's retirement as today.
- **Readiness.** `refresh_blocks_readiness`, and with it
  `workspace-refresh-complete`, is true while a collapse runs. The workspace
  path reveal treats a collapsing ancestor as collapsed: it expands it through
  the expand entry point, which cancels the collapse and refills the store,
  and then waits for that refill's scan as for any ancestor.

### D8. Scroll position

When the directory row is on screen, it does not move. Rows after it move up
as the children go. Prototype B: rendered top constant at 44.0 px, zero slice
bin corrections. When it is not on screen (folder header collapse, reveal
failure, or the user scrolled into the children), D4's unbound ranges
remove rows above the viewport, before any unit. The list keeps its anchor and the slice bin forwards the
list's value change to the outer scroller (`classify_child_scroll`), which is
the path BL's and later changes' rendered-placement tests protect. Tasks pin
both with rendered-placement tests:
- the directory row on screen does not move;
- an off-screen collapse ends with the same outer value and row placement as
  GTK's direct collapse from the same state;
- the slice bin's `correction_count()` and `outer_request_pending()` settle.

### D9. Pure step policy

`ui/sidebar/policy.rs` gains `CollapseFacts` and `CollapseStep`:
- `CollapseFacts`: the subtree's expanded stores in depth order, each with
  its length and its rows' flattened span, the directory's flattened position,
  and the band, the sorted positions GTK holds rows for;
- `CollapseStep`: `RemoveUnbound { store, start, count }`,
  `RemoveBound { store, start, count }`, `CollapseDescendant(store)` or
  `CollapseRow`.

`next_collapse_step(&CollapseFacts) -> CollapseStep` is pure, integer-only
and float-free (`#![forbid(clippy::float_arithmetic)]`).

- **Shared, not forked.** Its unbound ranges come from
  `model::list_budget::for_each_unheld_range`, and its turn continuation from
  `turn_continues`. That is the proved pure half `ui/list_retirement.rs`
  already uses (`core-list-budget`). The workflow convention counts owning
  workflows, and a second copy of the unheld-range rule could drift.
- **Allocation-free core.** The step core takes borrowed slices of facts the
  GTK caller has already sorted and deduplicated. It allocates nothing, for
  the reason `for_each_unheld_range`'s doc gives: CBMC cannot afford the
  standard library's sort or a growing `Vec` over symbolic input. A thin
  wrapper does the sort and dedup, covered by the property tests.

The band comes from the list view's children (at most about 207 widgets,
each → expander → `list_row()` → `position()`), read once per step. That is
O(band) and allocates no tree rows. Positions GTK holds outside the band (the
selection, the focus row) count as bound.

Proved by Kani in the new shard `core-sidebar-collapse`, over bounded store
counts, lengths and bands. The shard is scheduled. Its budget is recorded
locally as `PENDING DISPATCH (owed by slice-large-directory-collapse)`, as
build.md allows, because GitHub CI is checked only at the end of the
programme (maintainer, 2026-11-14). `make check-kani-shards` fails the
archiving commit of a change that still owes a figure. So before this change
is archived, either two dispatched runs are recorded, or the debt is moved to
an active programme change and recorded in the programme record.
`ORACLES` in `scripts/kani-shards.py` gains `ui/sidebar/policy.rs`, scoped to
the collapse functions (`functions=`), with its harnesses. The properties:
- **C1** every step removes at least one row or collapses one;
- **C2** a `RemoveBound` step covers at most
  `SIDEBAR_REALIZED_ROWS_PER_FRAME` band positions, and a `RemoveUnbound`
  step covers none and at most `COLLAPSE_UNBOUND_CHUNK_ROWS` rows;
- **C3** a descendant is drained and collapsed before its ancestor;
- **C4** `CollapseRow` is chosen only when the directory's remaining subtree
  is at most `DIRECT_COLLAPSE_ROWS` rows;
- **C5** the sequence terminates within the bound D2 states.

Unit tests cover the reference cases, and a property test drives the policy
against a model of GTK's band (A112) to the end.

### D10. Expansion, measurement first

The re-expansion figures (21–33 ms at 256 entries, 17–19 ms at 1,024, both
non-verdict) violate the existing "Workspace list dispatches fit the settle
budget" requirement. Under `.agents/rules/preexisting-blockers.md` that is in
scope. Task 1 measures expansion at N = 64, 256 and 1,024, with 0 and 400
rows after the directory, on quiet `main`. It measures both a cold
expansion and a re-expansion right after a collapse, because stage 1
measured only the latter, where BL's listing retirement and the probe
scan's publication (D12) may share the window. It attributes any dispatch
over budget with a profile before a fix is chosen. Applying all of a small
directory's rows in one turn, so that about 200 row widgets are created at
once, is the hypothesis, not a finding:
- If every dispatch fits `B_SETTLE_US`, the figures are recorded and nothing
  changes.
- If one does not and the profile puts it in row-widget creation and
  styling, the load's batch application is capped so that at most
  `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows enter the band per frame. That is
  BL's `FIRST_BATCH_ROWS` path, applied to every batch while the band is not
  full. The frame pacing follows `ui/list_growth.rs` (one step per tick)
  unless the batch path's existing timer already gives one frame per batch.
  The expansion verdict test then enforces it.
- If the profile puts it elsewhere (the retirement turns, the probe scan's
  publication, or the toolkit), the fix follows the attribution. A dispatch
  that still cannot fit because of a toolkit cost is a measured
  toolkit-floor exception citing an axiom with a probe and a sample, per the
  existing requirement. The probe scan's publication stays D12's residual.

The decision is recorded either way.

### D11. Evidence

`WorkspaceSectionEvidence` gains `collapses`, the section's collapse queue
(D7) in order, empty when none runs. Each record holds the directory path,
origin, steps taken, and bound and unbound rows left. `last_collapse_end`
holds the reason the most recent collapse ended: completed, cancelled by
re-expansion, taken over by an ancestor, or cancelled by removal. It also
gains `collapse_steps_total`, `collapse_direct_total` and
`collapse_starved_steps_total` (steps the starvation fallback of D4 ran).
Reading them makes the toolkit do nothing, as the surface requires (no
`row.children()`, no `item()`). The automation snapshot projects
`collapse_pending` beside `refresh_blocks_readiness`. That is a snapshot
field, so `docs/automation.md` and `docs/automation-reference.md` are updated.

### D12. The probe scan, the re-expansion outlier and BL's other follow-up

- **Attributed.** The 37–50 ms dispatch about 0.5 s after a 131,072-entry
  collapse is the publication of the scan that GTK's post-collapse
  `is_expandable()` probe starts through LushText's create function. Two
  facts point to it:
  - `gtk_tree_list_model_clear_node_children` clears the row's model, and the
    bound expander re-queries expandability on the `children` notify
    (`stop-expander-bind-time-child-scans` proposal, "Why", verified there
    against 4.22.5);
  - the terminal-publication counter advanced in exactly that dispatch, 6 of
    6 runs.

  BL's rare 14–18 ms re-expansion outlier is that publication landing inside
  the re-expansion window. BL's verdict test re-expands right after
  `wait_for_refresh_idle`, before a 131,072-entry scan (about 0.5 s) can
  finish. That explains why it "only follows a collapse of the same
  directory". The attribution rests on one counter so far. Task 1.4 confirms
  it: the counter-annotated re-expansion timeline, plus the store the
  publishing scan belongs to, which must be a store no row shows, for the
  collapsed directory's path.
- **Not folded in.** "Not scanning the discarded `is_expandable()` probe
  store" is `stop-expander-bind-time-child-scans`, a full design of its own
  (deferred materialization, a new axiom, counters). A partial fix here would
  duplicate that design and conflict in `scan_execution.rs`. The publication
  is this change's **named residual, owner
  `stop-expander-bind-time-child-scans`**. It is excluded from D2's pass bar,
  printed by the verdict test, and recorded in the programme record as the
  reason to queue that change right after this one.
- **BL's other follow-ups** (a process-wide retained-byte ledger, `Arc<Path>`
  caches) are unrelated and stay queued.

### D13. GTK axioms

**Relied on:**
- A1: the realized band of about 205–207 rows;
- A10: mapped is not drawn;
- A22: the synchronous `set_expanded`, and the `is_expandable()` probe;
- A81: equal-priority co-dispatch;
- A88/A89: tail splices under a tree list;
- A91, corrected: node teardown grows with the children;
- A110: unmapped-list retirement.

**New**, provisional ids (final = next free at apply time):

- **A112**: on a mapped, presented `GtkListView` over a `GtkTreeListModel`,
  a removal destroys, inside its `items-changed`, the row widgets bound to
  removed items. It creates fresh widgets (factory setup and bind) for the
  rows that enter the band, and never reuses a removed widget for another
  item. Their style computation is left to the layout phase of the frame:
  the same frame when the removal runs in a tick callback, the next one when
  it runs outside a frame. Removing rows no widget is bound to, after the
  band or between an ancestor row and the band, destroys and creates none,
  and does not move the band.
  - Probe: counted factory setup and teardown, and timed calls at 16 / 64 /
    256 removed rows, with 0 and 400 rows after. Also the unbound ranges
    after and before the band (the list scrolled into the children), and the
    same head removal with 400 rows after on an **unmapped** list. That last
    case settles what A110 did not measure (D4).
  - Sample: an interactive list with live counters, plus `--check`.
- **A113**: `GtkTreeExpander`'s disclosure routes and their ownership, on
  both toolkits:
  - its click `GestureClick` lives on the `expander` icon child, not on the
    expander, and is re-created with the icon. It activates
    `listitem.toggle-expand` on release inside the icon. A capture-phase
    `GestureClick` on the expander that claims the sequence on a release
    inside the icon denies it, so the click toggles nothing, also across an
    icon re-creation. The row's press-side behaviour (focus on click,
    selection, double-press activation) is the same with and without it. The
    probe also records that `observe_controllers()` on the expander lists no
    `GestureClick`, the fact behind `row_factory.rs`'s inert loop;
  - its key bindings (the full list in D5) fire only for key events that
    reach it (focus on it or inside it). The probe records which are
    reachable in a row built like LushText's (`GtkListItem:focusable` `TRUE`,
    a focusable button inside the expander's child), from the row's focus
    and from the button's. A capture-phase key controller on an ancestor
    that returns `Stop` prevents them;
  - its class actions resolve before an inserted `listitem` action group;
  - its AT-SPI action table, read through the generic widget vtable
    (`gtkatspiaction.c:1063`): the names it lists (expected
    `listitem.expand`, `listitem.collapse`, `listitem.toggle-expand`), and
    that `DoAction` on `listitem.collapse` calls GTK's direct
    `set_expanded(false)`;
  - which accessible `expanded` state GTK maintains on the expander and which
    on its icon, at bind and on `notify::expanded` (D6).
- **A114**: a CSS rule on a class of `GtkTreeExpander` overrides the arrow
  icon its `expander:checked` node shows, in both directions, without
  changing the row's expanded state or the node's `:checked` state, on
  Adwaita 1.10 and on the GNOME 50 floor.

All three run on GNOME 51 and the GNOME 50 floor. A112's sample includes the
timed figures and is marked slow if it exceeds the slow threshold.

### D14. Workflow placement (`WFR-WORKSPACE-TREE`)

- **New** `ui/sidebar/workspace_section/collapse_execution.rs`, coordination
  role `execution`, stage-order-qualified for the collapse stage order. Its
  module doc narrates input → steps → final collapse → retirement, and names
  each resumption point (the frame-paced step source) and each inversion (a
  re-expansion or removal cancelling a collapse).
- Pure half: `ui/sidebar/policy.rs` (D9). Evidence: `ui/sidebar/evidence.rs`
  (D11).
- `row_factory.rs` stays a called presentation surface: it installs the
  gesture, the key controller and the class, and decides nothing.
- **Visual proof keys.** `collapse_execution.rs` removes rendered rows, so it
  joins `WORKSPACE_TREE_ROW_ANCHOR_EXACT` in
  `scripts/check-visual-proof-policy.py` and its mirror in
  `crates/cargo-gtk-proof/src/policy.rs`. Both self-tests assert it.
  `row_factory.rs`, `folder_execution.rs`, `imp.rs` and `reveal_execution.rs`
  are already keys, so this change requires the
  `workspace-tree-rendered-row-anchors` invariant either way.
- The matrix row is re-derived: files, current size, seam counts and policy
  consumers, row-scoped and excluding `#[cfg(test)]`.

### D15. Records to correct

- The ledger's A91 row: its dependent-design text now says its share is the
  node teardown, about 10 % of a collapse. It points to A112 for the row
  widgets and to this change for the sliced collapse.
- `docs/next/long-line-documents.md`, BL record: the exception is
  re-attributed, the follow-ups resolved (D12), and a status line for this
  change is added.
- The archived BL design's stage 4 decisions 1 and 2 get an appended note.
  Archived text is not rewritten.

### D16. Landing order (the commit hook and the visual proof)

The pre-commit hook runs `make check-policy`. That includes
`cargo gtk-proof policy`, which fails while any visual-sensitive file is
changed in the working tree without a same-session visual-geometry proof of
exactly those bytes. Untracked files count. Every path under
`crates/lushtext-core/src/ui/` (including `ui/sidebar/AGENTS.md`),
`crates/lushtext/tests/widget/` and `resources/style/` is visual-sensitive,
and the hook's all-targets Clippy rejects tests that do not compile.
Therefore:
- The verdict test (task 1.1) and the failing-first tests (task 3) are not
  committed on their own. The verdict test is kept as a patch in the handoff
  directory, applied identically to the `main` worktree and to the branch.
  The failing-first run is recorded in `impl/failing-first.log`.
- The commits before the code touch no visual-sensitive path, and are made
  while the tree holds none uncommitted:
  - design tables (task 1.6);
  - the axioms crate, the ledger, and the axioms' README and CHANGELOG (task
    2.5).
- Every visual-sensitive edit lands in **one commit**, after task 7.4's
  visual-geometry and accessibility smokes ran on that exact tree. That
  covers the policy, `collapse_execution.rs`, the routes, the CSS, the tests,
  `ui/sidebar/AGENTS.md` and, if D10 applies, the expansion cap. The
  scripts that must agree with it go in the same commit:
  - `scripts/kani-shards.py`;
  - the workflow-boundaries rule;
  - the visual-proof keys.
- The accessibility smoke's fingerprint covers `docs/accessibility.md`,
  `docs/accessibility-matrix.md` and the automation docs. Task 8.3 edits
  them before task 7.4's smoke runs, so the proof matches what lands.
- Documentation outside those paths may follow in a later commit.

## Risks / Trade-offs

- **The collapse finishes later than `main`'s frozen one** (D2): about 130 ms
  at 256 entries with rows after the directory, and about 220 ms at 131,072,
  against about 80 ms frozen. Mitigation: the intent shows in the first frame
  (D6), input is handled throughout, and the constants are measured, so the
  per-frame unit grows if a later lighter-rows change lowers the per-row cost.
- **Owning the expander's click and keys.** If a future GTK adds a disclosure
  route, LushText would collapse directly through it, no worse than today.
  A113's probe fails on a toolkit that changes the routes, which is how a
  toolkit bump finds out (`make gtk-axioms` first).
- **Slice-bin interaction when rows above the viewport go** (D8). This class
  has regressed before. Mitigation: the rendered-placement tests and the
  bin's own evidence, and no change to the bin.
- **Routes left to GTK.** An AT-SPI `DoAction` on the expander, a drag
  hover, and the removal routes keep `main`'s direct collapse (D5). They are
  measured once and recorded. They are no worse than today, and row
  activation gives assistive technologies the owned path.
- **A starved frame clock** (D4). The fallback source keeps a collapse
  finishing on a minimized or occluded window. It cannot be driven headless
  by minimizing, so a test policy forces the starvation path, and the
  evidence counts its steps.
- **A row that stays GTK-expanded while it looks collapsed.** Any reader that
  takes `row.is_expanded()` as intent would be wrong during a collapse.
  Mitigation: D6 routes intent through `expanded_paths` and the collapse
  state, and an audit task lists every `is_expanded()` reader.

## Migration

None. No persisted format, GSettings key or action changes. One automation
snapshot field is added (D11).

## Open questions (answered by tasks, not by the reviewer)

1. The values of the four constants (task 1, D3).
2. Whether small-directory expansion is over budget on quiet `main`, and so
   whether D10's cap is applied (task 1).
3. Whether A114 holds on both toolkits, or D6's fallback is used (task 2).
4. Whether the expander's key bindings are reachable from LushText's rows
   today (A113, task 2). The capture controller mirrors exactly what is
   reachable, so the answer changes its key list, not the keyboard
   behaviour.
5. Whether A112 confirms that removing the unbound rows before the band
   leaves a mapped list's band in place, or D4's fallback order is used
   (task 2).
