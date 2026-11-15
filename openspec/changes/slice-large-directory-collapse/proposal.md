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
   the nearly empty row last. Every step took 6–8 ms and every frame 4–10 ms,
   where the bare collapse took about 40 + 38 ms (400 rows after, 256 or
   16,385 entries; 131,072 entries with nothing after). The directory row's
   rendered position never moved, and the viewport slice bin made no
   correction.
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

- **An owned, sliced collapse.** Every LushText route that collapses a
  directory goes through one section entry point:
  - the expander click, which LushText takes over from `GtkTreeExpander`'s
    own gesture;
  - the expander's collapse keys;
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
     `COLLAPSE_REALIZED_ROWS_PER_FRAME`, one step per frame, deepest
     expanded descendant first.
  3. Once at most `DIRECT_COLLAPSE_ROWS` rows remain, the row is collapsed.
     BL's listing retirement takes the emptied stores.

  Expanding the row again during a collapse cancels it and refills the store
  through the existing refresh path.
- **One pure step policy.** The choice of the next removal comes from facts:
  the subtree's stores, their lengths, the flattened position of the
  directory and the positions GTK holds rows for. It is a GTK-free function in
  `ui/sidebar/policy.rs`, unit- and property-tested and proved by Kani in a new
  scheduled shard `core-sidebar-collapse`. The proofs cover termination, the
  per-step bounds, deepest-first order, and that the final collapse removes at
  most `DIRECT_COLLAPSE_ROWS` rows.
- **Expansion bounded the same way, if measurement confirms it.**
  Re-measure small-directory expansion on quiet `main`. If any dispatch
  exceeds its budget, the batches a directory's load installs are capped so
  that at most `COLLAPSE_REALIZED_ROWS_PER_FRAME` rows enter the realized band
  per frame. Otherwise the figures are recorded and nothing changes.
- **Readiness covers a collapse in progress.** `workspace-refresh-complete`
  (`refresh_blocks_readiness`) stays blocked while a sliced collapse runs. The
  sidebar evidence surface reports the collapse's state, steps and rows left.
- **GTK axioms**, each with a probe, a sample and a ledger row. Provisional
  ids are A112–A114; the final ids are the next free ones at apply time.
  - A112: on a mapped `GtkListView`, a removal destroys the row widgets bound
    to the removed items inside `items-changed`, and creates fresh widgets for
    the rows that enter the band. Unclaimed widgets are never reused for other
    items. The next frame computes the new widgets' styles. Removing rows no
    widget is bound to destroys and creates none.
  - A113: the `GtkTreeExpander` disclosure routes and how an application can
    own them.
  - A114: a CSS rule on a class of the expander overrides the `:checked`
    arrow without changing the row's state.
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
- **Lighter sidebar rows** (fewer widgets and CSS nodes per row, or
  `LushtextRowText`). They would lower every per-row cost, but they cannot
  make about 200 replacements fit one dispatch. Recorded as a follow-up.
- **Asking GTK to reuse removed row widgets** for the rows that enter the band
  (`gtk_list_item_change_finish` destroys every unclaimed widget). Recorded as
  an upstream-report candidate. Nothing is filed without the maintainer's
  per-report approval.
- **Routes LushText cannot intercept.** These keep GTK's direct collapse and
  are measured as residuals:
  - an expanded directory removed by a refresh splice (the directory was
    deleted on disk);
  - a whole-model reset (section rebuild);
  - `GtkTreeExpander` exposes no AT-SPI action in 4.24.1 (`gtkatspiaction.c`
    handles `GtkExpander` only), so there is no assistive-technology route to
    own.

## User-visible impact

- Collapsing a directory with more than about 64 visible children no longer
  freezes the sidebar for 2–5 frames. The arrow flips in the first frame.
  The children then disappear in bands of at most
  `COLLAPSE_REALIZED_ROWS_PER_FRAME` rows per frame while the rows below move
  up. A 256-entry directory with rows after it is fully collapsed in about
  8 frames, and a 131,072-entry one in roughly 16–24. This is slower to finish
  than `main`'s frozen collapse (about 80 ms for the former), but every frame
  in between is drawn and input is handled.
- Selection or keyboard focus on a child of the collapsing directory moves to
  the directory row as the collapse starts. On `main` it lands wherever
  `GtkSingleSelection` puts it once the child is gone.
- No loss compared with `main`: the same rows, the same final state and the
  same accessible names, and the expander keeps every key and click it has
  today.

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
- **GTK Lush:** `crates/gtk-lush/axioms/` gets three probes, catalogue
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
  - `make check-accessibility-policy`, `make check-visual-proof-policy` and
    the headless visual and accessibility smokes.
- **Data integrity:** none at risk. The collapse removes rows from in-memory
  child stores only. No file is read, written, renamed or deleted, and no
  persisted state changes.
- **Dependencies:** none. It is independent of `rust-1-100-adoption` (Clippy
  sweep), which may land first. A merge from `origin/main` is expected.
