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

- Every dispatch of a sidebar directory collapse fits its budget at every
  size, with or without rows after the directory. That runs from the input
  until the row is collapsed and its listing retired. The input dispatch and
  every collapse step fit `B_INTERACT_US`, and every frame in between fits
  `B_SETTLE_US`.
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
  directory, a section rebuild, or a model reset. They keep GTK's direct
  collapse and are measured as residuals.

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
| 0:131,072 | unbound tail first | 16 | 6,495 | 4,173 | 1,856 | 133 ms | 307 ms |
| 0:131,072 | unbound tail first | 32 | 7,090 | 5,869 | 3,259 | 135 ms | 224 ms |
| 400:16,385 | unbound tail first | 16 | 6,183 | 6,832 | 4,626 | 28.8 ms | 244 ms |
| 400:16,385 | unbound tail first | 32 | 10,067 | 9,787 | 9,376 | 37.8 ms | 174 ms |

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
bound row widgets per frame: Prototype B, at K = 16–32, keeps every step at
6–8 ms and every frame at 4–10 ms under load 13. Scope follows the cost, not
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
  into the band, is read against `B_SETTLE_US`.
- The first frame after the input shows the collapse: the arrow and the
  accessible state, D6.
- Time until collapsed is printed and recorded, not enforced. Its expected
  ceiling is
  `ceil(bound rows / COLLAPSE_REALIZED_ROWS_PER_FRAME) + ceil(unbound rows / (COLLAPSE_UNBOUND_CHUNK_ROWS × chunks per turn))`
  frames. This is the trade the change makes against `main`'s faster but
  frozen collapse, and the proposal states it as user-visible.
- Excluded from the pass bar, as a named residual: the probe scan's
  publication (D12).

**Measurement protocol (verification-runs.md).**
- Four rounds interleaved `main` → branch → `main` → branch, through
  `scripts/measure.sh`; a round counts only at a recorded 1-minute load
  below 4. Build and session as in "Measured baseline".
- Cases: collapse with 0 and 400 rows after the directory, at N = 64, 256,
  4,096, 16,385 and 131,072; expansion with 0 and 400 rows after it, at
  N = 64, 256 and 1,024 (D10).
- Recorded per case: the input dispatch, every dispatch until collapsed plus
  700 ms, with frames split into layout and paint phases, and
  time-to-collapsed. Frame verdicts are taken under the GL renderer in steady
  state, and the cairo figures are recorded beside them, as
  `lighten-notes-browser-rows` established for cairo's paint cost (A111).
- Pass: on the branch, the input dispatch and every step fit
  `B_INTERACT_US`, and every dispatch fits `B_SETTLE_US`, in every round.
- Long campaigns run detached.

### D3. Constants

All are powers of two (`.agents/rules/rust.md`), chosen by the verdict
campaign (task 1) and then fixed.

- `DIRECT_COLLAPSE_ROWS`: cap, rounded down. Candidate 32. A subtree of at
  most this many flattened rows collapses directly. At 64 entries `main`
  measures 4.5 ms with nothing after the directory and 12.4–13.1 ms with rows
  after, so 32 replacements cost about 6 ms plus their frame.
- `COLLAPSE_REALIZED_ROWS_PER_FRAME`: cap, rounded down. Candidate 16 or 32
  (Prototype B: 16 → steps ≤ 8.0 ms and frames ≤ 10.0 ms; 32 → 7.4 / 6.3 ms
  at 400:256, both under load 13; 64 fails). It is the most bound rows one
  step removes, and so the most widgets one frame must style. It is also
  `DIRECT_COLLAPSE_ROWS`'s upper bound, so the final collapse is never larger
  than a step.
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
   - first the rows no widget is bound to after the band, in chunks (A112:
     no widget destroyed or created, the scrollbar only shrinks);
   - then the bound rows, from the band's first child, in units, each
     bringing at most a unit of new rows into the band;
   - then the unbound rows between the directory row and the band, which
     exist only when the user scrolled into the children, in chunks.
3. **One step per frame.** A step runs from a frame-clock-paced source (the
   `after-paint` of the previous frame). The widgets it creates are therefore
   styled in the next frame, and two steps never share one.

*Why this order.* Removing the band first, from the head, on a directory
larger than the band keeps creating widgets for children the next step
removes (Prototype B at 131,072 would need 4,096 units). Removing everything
unbound first leaves only the band, whose replacements are siblings that
stay. Removing the band from the end while unbound rows precede it makes GTK
realize earlier rows instead (A110's last clause, on unmapped lists; A112
pins the mapped case).

*Rejected.*
- **Lighter rows alone.** Fewer widgets and CSS nodes per row would cut each
  replacement, but about 200 replacements plus their styling cannot fit
  8 ms. At about 110 µs per replacement in the call and about 190 µs per row
  in the next frame's styling (400:256, both under load), a 7× reduction
  would be needed. Recorded as a follow-up, which this change's constants
  would then let grow.
- **GTK reusing removed widgets for entering rows.** The real fix in
  `gtk_list_item_change_finish`. It is not in LushText's control, so it is an
  upstream-report candidate (task 8.4), never filed without approval.
- **Detaching the model or hiding the list during the collapse.** Either one
  tears every row down at once (A1).

### D5. One collapse entry point, owning every LushText route

`LushtextWorkspaceSection::collapse_directory(row, origin)` is the only
LushText caller of `TreeListRow::set_expanded(false)` on a directory row. A
`make check-workflow-boundaries` rule enforces that. Routes:

| route | today | after |
|---|---|---|
| expander click | `GtkTreeExpander`'s bubble `GestureClick` activates `listitem.toggle-expand` (`gtktreeexpander.c:150-165`); LushText already sets that gesture's phase to `None` on file rows | the expander's own gesture is `None` on every row. A LushText gesture on the expander replicates GTK's: claim on release inside the widget; expanding activates `listitem.scroll-to` and calls `set_expanded(true)`; collapsing calls the entry point |
| expander keys | `minus`, `KP_Subtract`, `slash`, `KP_Divide` → `listitem.collapse`; `Shift(+Ctrl)+Left/Right` by text direction; `Ctrl+space` / `Ctrl+KP_Space` → `listitem.toggle-expand` (`:654-690`), firing only with focus inside the expander | a capture-phase key controller on the list view handles exactly those collapse keys for a focused expanded directory row, calls the entry point and stops propagation. Expand keys stay GTK's |
| row activation | `activate_file_at` toggles | collapse calls the entry point |
| folder header collapse / toggle | `folder_execution.rs` `set_expanded(false)` per root | the entry point per root |
| reveal failure terminal | `reveal_execution.rs` collapses what it expanded | the entry point; the reveal's terminal does not wait for it |
| reorder drag hover | `reorder_execution.rs` defensive idle collapse of an inert, empty model | the entry point (always direct: an inert model has no rows) |

Class actions on `GtkTreeExpander` cannot be overridden by an inserted action
group, and the type is final, which is why the gesture and the keys are taken
over rather than the actions. A113 pins this. Not owned, and measured as
residuals: a refresh splice removing an expanded directory, a section rebuild
or model reset, and `listitem.collapse` activated by anything else.
`gtkatspiaction.c` gives `GtkTreeExpander` no AT-SPI action in 4.24.1, so
there is no assistive-technology route.

### D6. Showing the intent in the input dispatch

The input dispatch does no row work. It:
- adds `lushtext-collapsing` to the row's `GtkTreeExpander`, whose CSS shows
  the collapsed arrow over `expander:checked`, both text directions (A114);
- sets the row overlay's accessible `expanded` state to false;
- marks the section's collapse state, and arms the first step.

The row stays GTK-expanded until the final step. Every LushText reader of
expansion state must read the collapse state as well. The authoritative
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
  later child and file peek would follow it. This is a deliberate change
  from `main`, stated in the proposal.
- **Re-expansion during a collapse.** It cancels the collapse. The row never
  left GTK's expanded state, so the store, partly drained, is refilled through
  `populate_child_store`, the existing refresh path: a fresh scan reconciled
  into the same store, in BL's batches. The arrow class and the accessible
  state are restored at once, and the expansion intent is re-added.
- **A repeated collapse request** for a collapsing row is a no-op. A collapse
  of an ancestor of a collapsing directory takes the descendant's collapse
  over as its first part.
- **A refresh splice, section rebuild, model reset or dispose** that removes
  the collapsing row cancels the collapse: GTK removes what is left directly
  (a residual), and the drained stores go to BL's retirement as today.
- **Readiness.** `refresh_blocks_readiness`, and with it
  `workspace-refresh-complete`, is true while a collapse runs. The workspace
  path reveal treats a collapsing ancestor as collapsed: it expands it, which
  cancels the collapse.

### D8. Scroll position

When the directory row is on screen, it does not move. Rows after it move up
as the children go. Prototype B: rendered top constant at 44.0 px, zero slice
bin corrections. When it is not on screen (folder header collapse, reveal
failure, or the user scrolled into the children), step 3 of D4 removes rows
above the viewport. The list keeps its anchor and the slice bin forwards the
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

The band comes from the list view's children (at most about 207 widgets,
each → expander → `list_row()` → `position()`), read once per step. That is
O(band) and allocates no tree rows. Positions GTK holds outside the band (the
selection, the focus row) count as bound.

Proved by Kani in the new shard `core-sidebar-collapse` (scheduled; its
budget recorded from two CI runs, per build.md), over bounded store counts,
lengths and bands:
- **C1** every step removes at least one row or collapses one;
- **C2** a `RemoveBound` step covers at most
  `COLLAPSE_REALIZED_ROWS_PER_FRAME` band positions, and a `RemoveUnbound`
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
rows after the directory, on quiet `main`:
- If every dispatch fits `B_SETTLE_US`, the figures are recorded and nothing
  changes.
- If one does not, the load's batch application is capped so that at most
  `COLLAPSE_REALIZED_ROWS_PER_FRAME` rows enter the band per frame. That is
  BL's `FIRST_BATCH_ROWS` path, applied to every batch while the band is not
  full, which the expansion verdict test then enforces.

The decision is recorded either way.

### D11. Evidence

`WorkspaceSectionEvidence` gains `collapse`, an optional record holding the
directory path, origin, steps taken, bound and unbound rows left, and the
reason the collapse ended (completed, cancelled by re-expansion, cancelled by
removal). It also gains `collapse_steps_total` and `collapse_direct_total`.
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
  directory". Task 1.4 confirms it with the counter-annotated re-expansion
  timeline.
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
  removed items, creates fresh widgets (factory setup and bind) for the rows
  that enter the band, never reuses a removed widget for another item, and
  leaves their style computation to the next frame. Removing rows no widget
  is bound to, after or before the band, destroys and creates none.
  - Probe: counted factory setup and teardown, and timed calls at 16 / 64 /
    256 removed rows, with 0 and 400 rows after.
  - Sample: an interactive list with live counters, plus `--check`.
- **A113**: `GtkTreeExpander`'s disclosure routes in 4.24.1 and their
  ownership:
  - its bubble `GestureClick` activates `listitem.toggle-expand` on release
    inside the widget, and with its phase `None` a click toggles nothing;
  - its collapse key bindings fire only for focus inside the expander, and a
    capture-phase key controller on an ancestor that returns `Stop` prevents
    them;
  - its class actions resolve before an inserted `listitem` action group;
  - it has no AT-SPI action.
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
- **A row that stays GTK-expanded while it looks collapsed.** Any reader that
  takes `row.is_expanded()` as intent would be wrong during a collapse.
  Mitigation: D6 routes intent through `expanded_paths` and the collapse
  state, and an audit task lists every `is_expanded()` reader.

## Migration

None. No persisted format, GSettings key or action changes. One automation
snapshot field is added (D11).

## Open questions (answered by tasks, not by the reviewer)

1. The values of the three constants (task 1, D3).
2. Whether small-directory expansion is over budget on quiet `main`, and so
   whether D10's cap is applied (task 1).
3. Whether A114 holds on both toolkits, or D6's fallback is used (task 2).
