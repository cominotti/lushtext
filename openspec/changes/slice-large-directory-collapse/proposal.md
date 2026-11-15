## Why

`bound-lists-by-budget-not-count` (BL) recorded the collapse of a sidebar
directory as a toolkit-floor exception: 15.1–15.3 / 16.0–16.8 / 23.3–23.7 ms at
10,000 / 16,385 / 131,072 entries. It attributed the cost to GTK's rb-tree
teardown (ledger A91) plus "about 10 ms whatever the size" of row-widget
teardown (A1). It queued this change to empty a large directory's child store
in slices before GTK's collapse, removing A91's share. That queued
description does not hold up against measurement.

Stage 1 measured `main` (`72c039ef`, GTK 4.24.1 / libadwaita 1.10.0, toolbox
`hypatia-f45`, optimized widget build, presented 800 px window, cairo
renderer). Every figure below is **non-verdict**: the 1-minute load was 7–13
because five other changes were running. Raw data, the profile and the scratch
code are in `.claude/handoffs/slice-large-directory-collapse/`. The magnitudes
are far outside load noise and agree with BL's quiet figures where they
overlap. The design's "Measured baseline" holds the full tables.

1. **The collapse costs the row widgets GTK replaces, not the rows it
   removes.**
   - The list view keeps about 207 row widgets around its anchor (A1). The
     collapse call destroys every one bound to a child and creates
     replacements for the rows that move into the band.
   - With nothing after the directory, the call takes 1.4–1.6 ms at 16
     entries, 4.5–4.7 ms at 64, 15.6–16.0 ms at 256 and 15.8–17.7 ms at 4,096.
     The cost is flat once the band is full.
   - With 400 rows after the directory, the call takes 3.9–4.0 / 12.4–13.1 /
     36.7–43.6 ms at 16 / 64 / 256 entries, and 41–64 ms up to 16,385. The
     next dispatch, the frame that styles the new rows, takes 4.0 / 12.2–13.0 /
     25.6–38.5 ms.
   - A `perf` profile puts 89 % of the call in the list item manager's
     `items-changed`: 62 % destroying unclaimed row widgets and 27 % creating
     their replacements. GTK's rb-tree (A91) is 10 %. The frame after is 82 %
     CSS validation of the new rows' nodes.
2. **Removing A91's share alone buys almost nothing.** A prototype that
   emptied all but 512 children in 16,384-row slices before the collapse cut
   the 131,072-entry call from 24.6–31.3 ms to 17.8–21.1 ms. Every
   256-to-16,385 case stayed at 14–20 ms, or about 40 ms with rows after the
   directory.
3. **What does work is bounding the realized-row churn.** A prototype first
   removed the children GTK has no widget for, in 16,384-row chunks. It then
   removed the bound children in units of 16 or 32 per dispatch, and collapsed
   the nearly empty row last. At load 13, steps took 6.2–14.7 ms, frames
   4.2–13.1 ms and the final call 1.9–18.1 ms, with a 2× spread between
   identical runs. The bare collapse took about 40 + 38 ms (400 rows after,
   256 or 16,385 entries; 131,072 entries with nothing after). Units of 64
   failed (a 32 ms step). The directory row's rendered position never moved,
   and the viewport slice bin made no correction. Whether 16, 32 or 8 rows per
   frame meets `B_INTERACT_US` is for the quiet verdict to decide.
4. **The rest is the probe scan.** About 0.45–0.63 s after collapsing a
   131,072-entry directory, a 37–50 ms dispatch publishes a directory scan. In
   6 of 6 runs the scan terminal counter advanced in exactly that dispatch.
   The collapse clears the row's model, the bound expander asks
   `is_expandable()` again, and LushText's create function starts a full scan
   of the directory the user just closed. That is the hazard
   `stop-expander-bind-time-child-scans` (proposed 2026-10-11, not applied)
   exists to remove. It also explains BL's rare 14–18 ms re-expansion outlier:
   it appears only when the scan's publication lands inside the re-expansion.

So the change is worth doing, **refined**. Its scope is no longer "huge
directories". It is every directory whose collapse replaces more row widgets
than one dispatch can afford: about 64 entries when rows follow it, and 256
otherwise. The existing requirement that every dispatch applying a sidebar
directory's rows fits `B_SETTLE_US` is also violated by small-directory
re-expansion: 21–33 ms at 256 entries (non-verdict). That brings the
expansion side in under the pre-existing-blockers rule, measurement first.

## What Changes

**Revised after stage 4 (2026-11-15; design "Design revision after stage
4").** The quiet verdict failed at a 32-row unit, and the attribution shows
why: a unit's frame grows with its rows (0.45–0.6 ms a row replaced and
styled, no dominant fixed cost), and a paced collapse finishes one unit per
frame, so a smaller unit fits the frame budget only by finishing two to four
times later than `main`. The revision therefore adds, beside what follows:

- **A completion bar:** a collapse draws its collapsed rows no later than
  `main`'s plus two frames, or within 100 ms where `main` is faster.
- **Lighter rows:** the reorder handle, the Focus Folder button and the drop
  surfaces are created on demand instead of in every row, cutting what every
  collapse, expansion and scroll pays per row.
- **Destroy-only units:** a step that brings no row into the band takes a
  larger unit.
- **Unbound rows removed between frames,** without LushText's per-row
  bookkeeping and item disposal inside the chunk.
- **Several collapses drain bottom-up,** so the folder header's later roots go
  while unbound.
- **The expansion cap,** now that the profile puts small-directory expansion's
  over-budget dispatches in creating and styling about 200 rows.
- **A fix for `main`'s folder-header collapse,** which leaves a root expanded:
  the expansion-intent hook lived on row objects GTK discards, so the entry
  points now record intent themselves and the hook is installed at bind (new
  axiom, A114).

- **An owned, sliced collapse.** Every LushText route that collapses a
  directory goes through one section entry point:
  - the expander click, which LushText takes over from `GtkTreeExpander`'s
    own gesture. That gesture lives on the expander's arrow icon, not on the
    expander (`gtktreeexpander.c:193-210`), so LushText claims the sequence
    in the capture phase on a release inside the icon, where and when GTK
    claims it. LushText's existing bind-time loop that sets the
    "expander's" gesture phase finds no gesture on 4.24.1 and is corrected;
  - the expander's collapse keys, as far as they are reachable from
    LushText's rows today (A119);
  - activating a directory row;
  - the folder-header collapse and toggle;
  - the reveal's failure terminal;
  - the reorder drag's defensive collapse.

  A collapse whose subtree holds at most `DIRECT_COLLAPSE_ROWS` flattened rows
  is GTK's `set_expanded(false)`, as today. A larger one is sliced:
  1. The input dispatch only records the intent and shows it. The expander
     arrow and the accessible expanded state flip at once. Loads, scans and
     refresh targets under the directory stop, and selection and focus inside
     the subtree move to the directory row.
  2. Later turns remove the children GTK holds no widget for, in chunks.
     They then remove the bound children in units of at most
     `SIDEBAR_REALIZED_ROWS_PER_FRAME`, one step per frame, deepest
     expanded descendant first.
  3. Once at most `DIRECT_COLLAPSE_ROWS` rows remain, the row is collapsed.
     BL's listing retirement takes the emptied stores.

  Steps are paced by the list's frame clock (a tick callback, as
  `ui/list_growth.rs` paces a staged publish), at most one bound unit per
  frame across the window. A `G_PRIORITY_LOW` source takes over while the
  list is not mapped (sidebar hidden, section body collapsed, window
  unmapped), and while its frame clock is starved (a minimized or occluded
  window), so a collapse always finishes. Several collapses at once, such as
  the folder header collapsing every root, share that one unit per frame. Expanding the
  row again during a collapse, by any route, goes through an owned expand
  entry point: it cancels the collapse and refills the store through the
  existing refresh path. The row is still GTK-expanded at that point, so a
  bare `set_expanded(true)` would do nothing.
- **One pure step policy.** The choice of the next removal comes from facts:
  the subtree's stores, their lengths, the flattened position of the
  directory and the positions GTK holds rows for. It is a GTK-free,
  allocation-free function in `ui/sidebar/policy.rs`. It reuses
  `model::list_budget`'s proved unheld-range and turn rules rather than
  forking them. It is unit- and property-tested and proved by Kani in a new
  scheduled shard `core-sidebar-collapse`. The proofs cover termination, the
  per-step bounds, deepest-first order, and that the final collapse removes at
  most `DIRECT_COLLAPSE_ROWS` rows.
- **Expansion bounded the same way, if measurement confirms it.**
  Re-measure small-directory expansion on quiet `main`. If any dispatch
  exceeds its budget, the batches a directory's load installs are capped so
  that at most `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows enter the realized band
  per frame. Otherwise the figures are recorded and nothing changes.
- **Readiness covers a collapse in progress.** `workspace-refresh-complete`
  (`refresh_blocks_readiness`) stays blocked while a sliced collapse runs. The
  sidebar evidence surface reports the collapse's state, steps and rows left.
- **GTK axioms**, each with a probe, a sample and a ledger row. A118–A120
  are applied under those ids; the revision's fourth takes the next free id
  at apply time (A114, design R1).
  - A118: on a mapped `GtkListView`, a removal destroys the row widgets bound
    to the removed items inside `items-changed`, and creates fresh widgets for
    the rows that enter the band. Unclaimed widgets are never reused for other
    items. The next frame computes the new widgets' styles. Removing rows no
    widget is bound to destroys and creates none.
  - A119: the `GtkTreeExpander` disclosure routes and how an application can
    own them. That covers where its click gesture lives, which key bindings
    reach it from a focusable list row, and its AT-SPI action table (the
    generic widget vtable exposes its three class actions).
  - A120: a CSS rule on a class of the expander overrides the `:checked`
    arrow without changing the row's state.
  - A114: `GtkTreeListModel` keeps no `GtkTreeListRow` for a
    node nothing references, so `get_item` hands out a new row object
    without the object data and `notify::expanded` handler of the one that
    died (design R4).
- **Records corrected.** A91's row stops claiming the collapse's cost and
  keeps the node-teardown share it measured. BL's exception text in
  `docs/next/long-line-documents.md` and the archived design is annotated with
  the attribution above. BL's queued follow-ups are resolved:
  - the `is_expandable()` probe scan is not folded in here; it stays with
    `stop-expander-bind-time-child-scans`, with this change's evidence;
  - the 14–18 ms re-expansion outlier is attributed to it;
  - the probe scan's 37–50 ms publication is a named residual owned by that
    change.

## Non-goals

- **Not scanning the discarded `is_expandable()` probe store.** That is
  `stop-expander-bind-time-child-scans`, a designed change of its own
  (deferred materialization). Folding a partial fix in here would duplicate
  its design and conflict in the same files. This change measures the probe
  scan's publication and records it as a residual with that owner.
- **`LushtextRowText` sidebar rows** (text measured without shaping). The
  revision makes rows lighter by creating unused decorations on demand; a
  text-widget change stays a follow-up.
- **Asking GTK to reuse removed row widgets** for the rows that enter the band
  (`gtk_list_item_change_finish` destroys every unclaimed widget). Recorded as
  an upstream-report candidate. Nothing is filed without the maintainer's
  per-report approval.
- **Routes LushText cannot intercept.** These keep GTK's direct collapse and
  are measured as residuals:
  - an expanded directory removed by a refresh splice (the directory was
    deleted on disk);
  - a whole-model reset (section rebuild);
  - an AT-SPI `DoAction` on the expander. `gtkatspiaction.c`'s generic widget
    vtable (`:1063`) exposes its enabled, parameterless class actions
    `listitem.expand`, `listitem.collapse` and `listitem.toggle-expand`.
    Disabling a class action would also disable its keys, and a class action
    cannot be shadowed, so this route keeps `main`'s direct collapse. Row
    activation (`Enter`) gives assistive technologies the owned path;
  - GTK's drag-hover expansion, which does not open a row that is still
    collapsing.

## User-visible impact

- Collapsing a directory with more than about 64 visible children, by a
  LushText-owned route, no longer freezes the sidebar for 2–5 frames. The arrow flips in the first frame.
  The children then disappear in bands of at most
  `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows per frame while the rows below move
  up, every frame drawn and input handled throughout. *(Stage 1's estimate
  here, a finish slower than `main`'s, is superseded by the revision's
  completion bar below.)*
- Selection or keyboard focus on a child of a collapsing directory moves to
  the directory row as the collapse starts, on the direct path too. On `main`
  it lands wherever `GtkSingleSelection` puts it once the child is gone.
- No loss compared with `main`: the same rows, the same final state and the
  same accessible names, and the expander keeps every key and click it has
  today.
- **Revised:** the collapse finishes no later than `main`'s frozen one (plus
  two frames, the frame boundaries a paced collapse starts and ends on), and the folder header's Collapse leaves every root collapsed,
  which `main` does not when a large first root pushes the next one out of
  the realized band. Expanding a small directory shows the rows in view in
  the first frame and fills the rest of the band over the next few frames
  instead of freezing for one 40–60 ms frame.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `main-thread-responsiveness`: adds the requirement that collapsing a
  sidebar directory fits the interaction budgets at every size, with or
  without rows after it, and that directory expansion fits them at small
  sizes.
- `workspace-tree-virtualized-rendering`: adds the requirement that a sliced
  collapse keeps the tree consistent. That covers every route, selection and
  focus, the scroll position, re-expansion mid-collapse, an ancestor collapse,
  loads and refreshes, accessibility and readiness.
- `gtk-axiom-ledger`: adds the requirement that the list view's row-widget
  churn and the tree expander's disclosure routes are ledger axioms.
- `dbus-automation-spine`: adds the requirement that the automation snapshot
  reports `collapse_pending` beside `refresh_blocks_readiness`.

## Impact

- **Code:**
  - `crates/lushtext-core/src/ui/sidebar/workspace_section/`: a new
    `collapse_execution.rs` (coordination); `mod.rs` (`activate_file_at`);
    `row_factory.rs` (owning the disclosure click and keys, the arrow class);
    `folder_execution.rs`; `scan_execution.rs` (cancellation and readiness);
    `imp.rs` (collapse state).
  - `ui/sidebar/reveal_execution.rs`, `reorder_execution.rs` and `policy.rs`
    (the step policy and its Kani proofs).
  - `ui/sidebar/evidence.rs` and the application CSS (the collapsing arrow).
- **GTK Lush:** `crates/gtk-lush/axioms/` gets four probes, catalogue
  entries and samples, run by the adoption lab's `axiom_probes`.
- **Tests:** failing-first widget tests in
  `crates/lushtext/tests/widget/workspace_tree_virtualization.rs` (shard
  table entry unchanged), unit and property tests of the policy, and a Kani
  shard.
- **Gates:**
  - `make check`, `make check-gtk-axioms`, the new axiom samples on GNOME 51
    and the GNOME 50 floor, and `make check-workflow-boundaries`;
  - `make check-widget-shards`, `make check-kani-shards`, the new shard once,
    and the widget shard owning `workspace_tree_virtualization`;
  - `make check-accessibility-policy`, `make check-visual-proof-policy`, and
    the headless visual, visual-geometry
    (`workspace-tree-rendered-row-anchors`) and accessibility smokes. The
    touched sidebar files are row-anchor keys, and `collapse_execution.rs`
    becomes one. Every visual-sensitive edit therefore lands in one commit
    after those proofs (design D16).
- **Data integrity:** none at risk. The collapse removes rows from in-memory
  child stores only. No file is read, written, renamed or deleted, and no
  persisted state changes.
- **Dependencies:** none. It is independent of `rust-1-100-adoption` (Clippy
  sweep), which may land first. A merge from `origin/main` is expected.
