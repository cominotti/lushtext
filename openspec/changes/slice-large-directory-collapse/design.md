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
- A collapse never finishes later than `main`'s: its completion (the input
  to the frame that draws the rows collapsed) is at most `main`'s plus two
  frames, or 100 ms where `main` is faster than that ("Design revision after
  stage 4", R3).
- The picture the user sees is final no later than `main`'s where it can
  be: where no row follows the collapsing subtree in the flattened list and
  the directory row is on screen, its rows draw nothing from the first frame
  after the input; otherwise the picture is final within the completion bar
  (R11, D22).
- The collapse's final state is `main`'s. That covers rows, expansion intent,
  watch targets, retired stores, the accessible state, and the scroll
  position when the directory row is on screen.
- Small-directory expansion meets the existing requirement that every
  dispatch applying a sidebar directory's rows fits `B_SETTLE_US` (quiet
  measurement confirmed it is violated on `main`; R5, D10).
- `main`'s folder-header collapse that leaves a root expanded is fixed (R4).
- BL's queued follow-ups are resolved with evidence: the A91 attribution, the
  re-expansion outlier and the `is_expandable()` probe scan.

**Non-Goals**

- Removing the `is_expandable()` probe scan. `stop-expander-bind-time-child-scans`
  owns it; see D12.
- Reuse of removed row widgets by GTK (a follow-up; see D4). Lighter sidebar
  rows were a non-goal until the revision: R2 shows nothing else meets D2 and
  the completion bar together, so D17 brings the on-demand decorations in.
- Collapses LushText does not start: a refresh splice that removes an expanded
  directory, a section rebuild, a model reset, or an AT-SPI `DoAction` on the
  expander's class actions (D5). They keep GTK's direct collapse and are
  measured as residuals.

## Measured baseline

### Verdict campaign (stage 4, 2026-11-15; tasks 1.2 and 1.5)

**Protocol.**

- The verdict test of task 1.1
  (`test_directory_collapse_and_small_expansion_dispatches_fit_the_budgets`),
  identical on both sides: the `main` copy is the task 1.1 patch applied to
  `main` `72c039ef` (`.claude/handoffs/slice-large-directory-collapse/main-tree`),
  and it differs from the committed branch copy only in formatting, a named
  `TimedDispatch` instead of a tuple, and the debug-build size cut, which an
  optimized build does not take. The branch is `e2427b2f`, which merged
  `main` `14332d69`; the `main` side is therefore older than the `main` the
  branch merged, by 64 commits. Their sidebar changes are the rename
  durability fix `9fa727d4` (`file_execution.rs`, rename only) and lint
  rewrites from the Rust adoption; none is on an expansion or collapse
  path, and `rust-toolchain.toml` pins 1.98.0 on both sides.
- Optimized widget build (`CARGO_PROFILE_DEV_OPT_LEVEL=2
  CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false`), `COLLAPSE_VERDICT_REPS=3`,
  CPUs 8–23, headless Mutter, harness renderer **cairo only**.
- Four rounds interleaved `main` → branch → `main` → branch through
  `scripts/measure.sh`, run by the coordinator's quiet-gated runner (labels
  `slc-v2-r<round>-<side>-c1`; logs and `measure.log` in
  `.claude/handoffs/slice-large-directory-collapse/impl/campaign2/`).
  Constants at their design candidates (32, 32, 8,192, 128 ms).
- Load (1 / 5 / 15 minutes) before → after each round:
  r1 `main` 3.29 / 2.52 / 2.68 → 3.02 / 2.44 / 2.55;
  r2 branch 3.09 / 2.71 / 2.65 → 2.54 / 3.22 / 2.99;
  r3 `main` 2.64 / 2.90 / 2.90 → 2.24 / 2.16 / 2.37;
  r4 branch 3.28 / 4.55 / 6.91 → 3.08 / 3.63 / 6.03.
  Round 4's 15-minute figure is the decaying tail of earlier load (its
  1-minute load met the protocol at both ends; `localsearch-3` ran at nice 19
  at its start). **No round is contaminated**: round 4 agrees with round 2
  in every case. Only the two fastest cases ran slower (0:64 and 0:256,
  worst frame 8.8–10.5 against 5.3–5.9 ms), both still within
  `B_SETTLE_US`, and every case that fails fails in both branch rounds.
- **Not measured:** GL frame figures (D2 asks for them beside cairo), and a
  step's own time apart from the frame that carries it (the timeline records
  whole dispatches, so a frame-paced step is inside its frame's figure). The
  failing frames below are layout-phase time (paint 0.5–1.4 ms everywhere),
  so the renderer cannot change those verdicts; GL is owed by task 7.2's
  final-code campaign, and the step's own time by task 7.1's instrument.

**Figures** (ms; each cell spans the six repetitions of a side, three per
round, over its two rounds; "frames over 16 ms" counts a repetition's frame
dispatches over `B_SETTLE_US`; the last column but one lists non-frame
dispatches over `B_SETTLE_US` and whether the section published a scan
terminal in them; "until settled" is the timeline's `done`: collapsed, or
loaded, and `refresh_blocks_readiness` false).

| case | rows after | N | side | input dispatch | worst frame (layout) | frames over 16 ms per rep | other dispatches over 16 ms (terminal publication) | until settled |
|---|---|---|---|---|---|---|---|---|
| collapse | 0 | 64 | main | 4.7–10.8 | 0.6 (0.2) | 0 | none | 0.7 |
| collapse | 0 | 64 | branch | 0.1–0.3 | 3.8–9.4 (3.2–8.7) | 0 | none | 8.2–14.2 |
| collapse | 0 | 256 | main | 15.4–21.9 | 0.9 (0.5) | 0 | none | 1.2 |
| collapse | 0 | 256 | branch | 0.2–0.4 | 5.3–10.5 (4.7–9.9) | 0 | none | 94.5–105.7 |
| collapse | 0 | 4,096 | main | 14.8–20.8 | 0.9–1.4 (0.5–0.6) | 0 | none | 27.2–28.1 |
| collapse | 0 | 4,096 | branch | 0.2–0.4 | 6.3–8.4 (5.9–7.8) | 0 | none | 137.2–159.3 |
| collapse | 0 | 16,385 | main | 15.7–17.9 | 1.0–1.1 (0.5–0.6) | 0 | none | 47.7–51.4 |
| collapse | 0 | 16,385 | branch | 0.2–0.4 | 7.9–12.1 (7.5–11.3) | 0 | none | 185.5–202.5 |
| collapse | 0 | 131,072 | main | 23.7–28.3 | 1.2–2.0 (0.6–0.7) | 0 | 32.8–44.6 (yes) | 438.5–502.8 |
| collapse | 0 | 131,072 | branch | 0.2–0.4 | 9.3–10.2 (8.9–9.8) | 0 | 33.7–47.9 (yes) | 839.0–1114.6 |
| expand-cold | 0 | 64 | main | 0.1–0.3 | 11.3–16.3 (10.8–15.5) | 0–1 | none | 27.0–31.2 |
| expand-cold | 0 | 64 | branch | 0.1–0.3 | 11.7–13.0 (11.1–12.5) | 0 | none | 26.4–28.5 |
| expand-after-collapse | 0 | 64 | main | 0.1–0.4 | 11.4–13.4 (10.8–12.7) | 0 | none | 7.3–14.0 |
| expand-after-collapse | 0 | 64 | branch | 0.1–0.3 | 11.9–14.5 (11.3–13.7) | 0 | none | 7.7–26.8 |
| expand-cold | 0 | 256 | main | 0.1–0.3 | 41.4–45.8 (40.3–44.8) | 1 | 20.6–25.1 (yes) | 64.3–70.8 |
| expand-cold | 0 | 256 | branch | 0.1–0.3 | 42.1–46.3 (41.1–45.2) | 1 | 21.0–24.9 (yes) | 67.0–72.4 |
| expand-after-collapse | 0 | 256 | main | 0.1–0.4 | 41.4–45.3 (40.7–44.6) | 1 | 20.8–26.4 (yes) | 21.4–28.0 |
| expand-after-collapse | 0 | 256 | branch | 0.2–0.4 | 41.2–61.6 (40.6–60.4) | 1 | 22.2–30.2 (yes) | 23.2–31.9 |
| expand-cold | 0 | 1,024 | main | 0.1–0.3 | 15.9–42.6 (15.7–42.4) | 0–2 | none | 89.2–95.4 |
| expand-cold | 0 | 1,024 | branch | 0.1–0.3 | 16.1–44.1 (15.9–43.9) | 1–2 | none | 86.9–135.2 |
| expand-after-collapse | 0 | 1,024 | main | 0.1–0.4 | 15.2–42.8 (15.0–42.5) | 0–2 | none | 87.9–95.1 |
| expand-after-collapse | 0 | 1,024 | branch | 0.1–0.2 | 15.6–40.1 (15.5–39.9) | 0–2 | none | 87.0–93.1 |
| collapse | 400 | 64 | main | 11.5–18.8 | 18.0–20.2 (17.4–19.5) | 1 | none | 18.3–20.6 |
| collapse | 400 | 64 | branch | 0.2–0.3 | 16.8–22.6 (16.2–22.0) | 1–2 | none | 35.1–40.6 |
| collapse | 400 | 256 | main | 36.7–55.7 | 56.1–72.6 (55.5–71.6) | 1 | none | 56.7–73.4 |
| collapse | 400 | 256 | branch | 0.2–0.4 | 18.6–24.3 (17.9–23.7) | 4–6 | none | 122.7–135.3 |
| collapse | 400 | 4,096 | main | 35.9–43.8 | 57.7–65.8 (57.0–64.8) | 1 | none | 84.6–96.9 |
| collapse | 400 | 4,096 | branch | 0.2–0.4 | 18.6–22.9 (18.0–22.2) | 6 | none | 172.2–184.9 |
| collapse | 400 | 16,385 | main | 38.2–45.0 | 59.9–61.1 (59.3–60.4) | 1 | none | 95.4–96.2 |
| collapse | 400 | 16,385 | branch | 0.2–0.4 | 17.9–18.5 (17.3–17.9) | 6–7 | none | 199.3–211.3 |
| collapse | 400 | 131,072 | main | 48.2–56.0 | 62.7–65.0 (61.9–63.3) | 1 | 34.0–43.1 (yes) | 449.6–474.7 |
| collapse | 400 | 131,072 | branch | 0.2–0.4 | 18.7–22.0 (18.2–21.4) | 6 | 33.5–43.7 (yes) | 831.0–888.2 |
| expand-cold | 400 | 64 | main | 0.1–0.3 | 16.3–22.8 (15.6–21.9) | 1 | none | 30.9–36.6 |
| expand-cold | 400 | 64 | branch | 0.2–0.3 | 16.0–17.4 (15.3–16.8) | 0–1 | none | 33.9–36.4 |
| expand-after-collapse | 400 | 64 | main | 0.1 | 16.2–17.5 (15.6–16.8) | 1 | none | 4.1–6.1 |
| expand-after-collapse | 400 | 64 | branch | 0.2–0.4 | 16.0–16.9 (15.4–16.3) | 1 | none | 6.6–9.3 |
| expand-cold | 400 | 256 | main | 0.1–0.3 | 47.1–50.8 (46.4–50.1) | 1 | none | 64.2–70.9 |
| expand-cold | 400 | 256 | branch | 0.1–0.3 | 48.0–62.4 (47.3–61.6) | 1 | none | 65.6–76.7 |
| expand-after-collapse | 400 | 256 | main | 0.1–0.3 | 34.8–45.5 (34.2–44.5) | 1 | none | 9.8–16.6 |
| expand-after-collapse | 400 | 256 | branch | 0.2–0.4 | 49.7–57.4 (49.0–56.5) | 1 | none | 9.6–16.5 |
| expand-cold | 400 | 1,024 | main | 0.1–0.3 | 25.5–27.6 (25.2–27.4) | 2–3 | none | 85.9–116.9 |
| expand-cold | 400 | 1,024 | branch | 0.1–0.3 | 26.9–31.9 (26.7–31.7) | 2–3 | none | 86.0–112.2 |
| expand-after-collapse | 400 | 1,024 | main | 0.1–0.2 | 46.2–49.7 (46.0–49.4) | 1 | none | 36.8–40.0 |
| expand-after-collapse | 400 | 1,024 | branch | 0.1–0.2 | 47.3–49.6 (47.0–49.4) | 1 | none | 35.5–37.7 |
| header-two-roots | 0 | 4,096 | main | 50.3–55.1 | 26.7–28.1 (26.5–27.9) | 3 | none | not reached in 240 s |
| header-two-roots | 0 | 4,096 | branch | 2.1–4.9 | 24.3–29.9 (23.7–29.2) | 5 | none | 272.7–297.2 |
| reexpand-mid-collapse | 0 | 131,072 | main | 0.2–0.4 | 75.1–94.9 (74.8–94.2) | 1–2 | 39.8–66.7 (yes) | 996.0–1121.0 |
| reexpand-mid-collapse | 0 | 131,072 | branch | 4.0–6.7 | 1.0–1.2 (0.7–0.8) | 0 | none | 295.3–339.7 |
| residual-expander-action | 400 | 256 | main | 44.1–47.8 | 97.4–97.6 (96.7–96.9) | 1 | none | 98.2–98.3 |
| residual-expander-action | 400 | 256 | branch | 42.5–44.3 | 99.9–100.2 (99.1–99.5) | 1 | none | 100.5–101.0 |
| residual-hidden-sidebar | 400 | 256 | main | 42.3–46.0 | 98.9–99.1 (98.8–99.0) | 1 | none | 99.7–99.9 |
| residual-hidden-sidebar | 400 | 256 | branch | 0.2 | 19.8–35.8 (19.7–35.7) | 5–6 | none | 175.5–179.0 |
| residual-refresh-splice | 0 | 256 | main | 22.0–24.1 | 1.7–1.8 (1.5–1.5) | 0 | none | 2.4–5.9 |
| residual-refresh-splice | 0 | 256 | branch | 19.2–20.3 | 1.7–1.8 (1.5–1.5) | 0 | none | 6.0–7.1 |
| residual-refresh-splice | 0 | 131,072 | main | 179.8–180.0 | 4.1–5.2 (1.5) | 0 | none | 121.3–121.5 |
| residual-refresh-splice | 0 | 131,072 | branch | 181.6–187.4 | 2.2–4.9 (1.6–1.7) | 0 | none | 121.3–121.5 |
| residual-section-rebuild | 0 | 256 | main | 0.1–0.2 | 2.1 (1.6) | 0 | 21.8 (no) | 41.1–48.4 |
| residual-section-rebuild | 0 | 256 | branch | 0.1–0.2 | 2.1–2.2 (1.6–1.7) | 0 | 17.4–20.8 (no) | 43.9–46.7 |

**Reading.**

- **Collapse with nothing after the directory: passes the `B_SETTLE_US`
  half on the branch** at every size (input 0.1–0.4 ms; every frame
  3.8–12.1 ms). The `B_INTERACT_US` half for the steps is open, not passed:
  at 131,072 entries dozens of 8–10 ms frames each carry a step, and the
  step is not timed apart from its frame. `main`'s
  14.8–28.3 ms input dispatch from 256 entries up. Time until settled grows
  from 1.2 to 95–106 ms at 256 entries, and from 0.44–0.50 to 0.84–1.11 s at
  131,072.
- **Collapse with 400 rows after the directory: fails on the branch.** The
  input dispatch is gone (36.7–56.0 ms on `main` → 0.2–0.4 ms), and `main`'s
  one 56–73 ms frame becomes four to seven frames of 16.8–24.3 ms (layout
  16.2–23.7 ms). Each such frame carries one 32-row bound unit. The
  hypothesis, from A118 and not yet profiled, is that the unit destroys 32
  widgets and creates and styles 32 for the rows entering the band, where
  the same unit with nothing after the directory, which brings no row into
  the band, costs 5–12 ms. At unit 32 the pass bar is missed in every
  repetition of both rounds; D2's exception covers only unit 8 against
  `B_INTERACT_US` and does not apply.
- **The folder header's two-root collapse fails on the branch**: five frames
  of 20.0–29.9 ms per repetition (input 2.1–4.9 ms, against `main`'s
  50.3–55.1 ms). Not attributed; the frames run 2–5 ms above the
  single-directory unit frames, consistent with the second root's unbound
  chunks sharing a frame with the first root's bound unit. On `main` the
  case never reached `done` within its 240 s budget (about 3,000 dispatches
  a repetition, three frames over 16 ms); not attributed.
- **Re-expansion in the middle of a 131,072-entry collapse passes on the
  branch** (input 4.0–6.7 ms, every frame 1.0–1.2 ms, refilled in
  0.30–0.34 s). On `main`, where no collapse is pending, the same input
  re-expands after a direct collapse: a 39.8–66.7 ms dispatch with a scan
  terminal publication (D12), a 16.0–17.0 ms frame and a 75.1–94.9 ms frame
  without one.
- **D12's probe publication** shows in every 131,072-entry collapse on both
  sides: one 33.5–47.9 ms non-frame dispatch, flagged with a terminal
  publication, 0.40–0.45 s after the input, 12 of 12 repetitions on each
  side. It stays the named residual. The flag in the expansion rows is a
  different publication: the expanded directory's own load.
- **Expansion is over budget on `main` and on the branch alike** (D10's
  hypothesis confirmed): at 256 entries a 20.6–30.2 ms non-frame dispatch
  (the directory's load terminal) and then one 41–62 ms frame; at 1,024
  entries one to three frames of 16–50 ms; at 64 entries with 400 rows after,
  frames of 16.0–22.8 ms. The branch changes nothing here. D10's cap is
  therefore in scope (tasks 1.3 and 6.1), and its attribution is owed first.
- **Residual routes** (task 1.5; recorded, not judged). AT-SPI-equivalent
  `listitem.collapse` on 256 entries with 400 after: 42.5–47.8 ms input and
  one 97–100 ms frame on both sides. Collapse while the sidebar is hidden
  (256 + 400): `main` 42.3–46.0 ms input and one 99 ms frame; the branch
  paces it through the fallback source in five or six dispatches of
  19.8–35.8 ms, settled in 176–179 ms. Refresh splice removing an expanded
  directory: input 19.2–24.1 ms at 256 entries and 179.8–187.4 ms at
  131,072, on both sides. Section rebuild with a 256-entry directory
  expanded: one 17.4–21.8 ms non-frame dispatch on both sides.

### Stage 1 figures (history, non-verdict)

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
  *Superseded by R13 (D25):* no step runs inside the frame clock's dispatch;
  a bound unit runs in a step-source dispatch of its own between frames,
  read against `B_INTERACT_US`, and the frame that lays out its rows runs no
  step and is read against `B_SETTLE_US`.
- Scope: the routes LushText owns (D5). The residual routes are measured and
  recorded, not judged.
- Exception: if the smallest measured unit (8) still cannot keep a step within
  `B_INTERACT_US` because of the toolkit's per-widget cost, that dispatch is
  recorded as a measured toolkit-floor exception citing A118. This is the
  existing clause of "Workspace list dispatches fit the settle budget". The
  unit then stays at 8. Nothing over budget is left unexplained.
- The first frame after the input shows the collapse: the arrow and the
  accessible state, D6.
- *Revised (R3, R9):* completion is enforced: the branch's worst completion
  is at most `max(main's worst + 33.3 ms, 100 ms)` per case, plus
  `max(0, R − 16,384) × 0.5 µs` for a subtree of `R` flattened rows (R9);
  the settled time is printed and recorded, not judged. The paragraph below
  is the stage 1 text it replaces.
- *Revised (R11):* the picture-final figure is enforced beside completion
  (the first frame where no row follows the subtree in the flattened list
  and the directory row is on screen, else the completion bar), and
  constants and go/no-go rounds select only a configuration one frame
  (16.7 ms) inside every completion-bar figure; the final campaign is judged
  at the bars themselves.
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

*Revised (R5):* `COLLAPSE_DESTROY_ONLY_ROWS` joins them (D18), and every
value is chosen by task 10.7 on the reworked code, then judged once by the
final campaign. The stage 1 text follows.

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

*Revised (R5):* unbound chunks leave the frame clock and lose LushText's
per-row bookkeeping (D19), and a bound step that brings no row into the band
takes the larger destroy-only unit (D18). The order below is unchanged.

For each step, the policy (D9) picks one removal:

1. **Deepest expanded descendant first.** An ancestor's collapse removes
   every descendant row with it, in GTK's one call. So descendants under the
   collapsing directory are drained before their parent, and each drained
   descendant is then collapsed directly.
2. **Within a store:**
   - first every row no widget is bound to, in chunks, one range at a time:
     the rows after the band, then the rows between the directory row and
     the band. The latter exist only when the user scrolled into the
     children. Per A110 and A118, this destroys and creates no widget. The
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
     units never share a frame. Unbound chunks create no widget (A118), so a
     step that takes only chunks needs no frame of its own. They still run
     on ticks, which keeps one source. *Superseded by D19 (R5):* unbound
     chunks run from a `G_PRIORITY_DEFAULT_IDLE` source between frames, and
     the tick takes bound units only. *Superseded by D25 (R13):* no step
     runs on the tick; a bound unit runs from its own step-source dispatch
     between frames, and the frame after it styles its rows.
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
     A118's probe records the unmapped head removal with rows after it.
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
The rows before the band therefore go before any unit, not after. A118 pins
the mapped case. *Fallback*, if A118 shows that a mapped list moves its band
or realizes rows when rows before the band are removed: those rows go last,
as in this design's first draft, and the probe records why.

*Rejected.*
- **Lighter rows alone.** *(Still rejected as the whole fix; R5's D17 takes
  lighter rows as margin inside each unit frame, and R2 replaces the per-row
  figures below with 0.45–0.6 ms a row replaced and styled, quiet.)* Fewer widgets and CSS nodes per row would cut each
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
| expander click | The `GestureClick` is **not on the expander**. `update_for_list_row` creates it on the expander's `expander` icon child (a `GtkBuiltinIcon`, `gtktreeexpander.c:193-210`), and destroys and re-creates the icon whenever expandability changes. Its `released` handler (`:150-165`) activates `listitem.toggle-expand` and claims the sequence. The expander's only own controller is the drop-motion controller (`:753`). LushText's bind-time loop over `expander.observe_controllers()` (`row_factory.rs:338-357`) therefore finds no `GestureClick` on 4.24.1. It is inert, and its comment describes an older toolkit | LushText adds one capture-phase `GestureClick` to the expander at setup. It lets the press through, as GTK's own gesture does, so the row's press-side behaviour (focus on click, selection, double-press activation) is `main`'s. On a release inside the current `expander` icon's bounds it claims the sequence in the capture phase, before the icon's bubble gesture sees the release, so GTK's `released` handler never runs, whatever icon GTK has created since. That is the moment and the area GTK claims, by construction. Then expanding activates `listitem.scroll-to` and calls the expand entry point, and collapsing calls the collapse entry point. A release elsewhere is left alone. A119 pins where the gesture lives, that the release claim denies it, and that the row's press-side behaviour is unchanged. The inert loop is removed, or re-aimed if A119 shows a toolkit where it acts |
| expander keys | `listitem.expand`: `plus`, `KP_Add`, `asterisk`, `KP_Multiply`. `listitem.collapse`: `minus`, `KP_Subtract`, `slash`, `KP_Divide`. By text direction: `Shift+Right`, `Shift+KP_Right`, `Ctrl+Shift+Right` and `Ctrl+Shift+KP_Right` expand in LTR and collapse in RTL; the `Left` and `KP_Left` four do the opposite. `listitem.toggle-expand`: `Ctrl+space`, `Ctrl+KP_Space` (`:654-690`). Class bindings fire only for key events that reach the expander, i.e. with focus on it or inside it. `GtkTreeExpander` is focusable (`:746`), but LushText leaves `GtkListItem:focusable` at its default `TRUE`, which the expander's own documentation (`:49-50`) says puts keyboard focus on the row instead. A row is the expander's ancestor, so a key with focus on the row never reaches it. These keys may therefore be unreachable today except with focus on a focusable descendant such as the Focus Folder button. That is open question 4, settled by A119 | the capture-phase key controller handles exactly the keys A119 shows GTK honours for the focus it sees, no more, so it adds no keyboard behaviour: collapse keys call the collapse entry point; expand keys on a collapsing row call the expand entry point; everything else propagates. If A119 shows none is reachable, no controller is installed and the row is recorded as "no key route" |
| expand callers | the reveal's `advance_expansion` (`reveal_execution.rs:344`), the new-item flow (`file_execution.rs:119`), the restore (`scan_execution.rs:1664`), folder expand / toggle / load-time auto-expand (`folder_execution.rs:399, 409, 636`) | each calls the expand entry point, so none acts on a draining store |
| row activation | `activate_file_at` toggles | collapse calls the entry point |
| folder header collapse / toggle | `folder_execution.rs` `set_expanded(false)` per root | the entry point per root |
| reveal failure terminal | `reveal_execution.rs` collapses what it expanded | the entry point; the reveal's terminal does not wait for it |
| reorder drag hover | `reorder_execution.rs` defensive idle collapse of an inert, empty model | the entry point (always direct: an inert model has no rows) |

Class actions on `GtkTreeExpander` cannot be overridden by an inserted action
group, and the type is final, which is why the gesture and the keys are taken
over rather than the actions. A119 pins this. Not owned, and measured as
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
  A119 records the action table on both toolkits, and D2 measures the route
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

*Reviewed (R5, D21):* the revision's proposal to run the first step in the
input dispatch is rejected (D21): the first frame's tick already runs the
first step, with no frame lost. The stage 1 text below stands.

The input dispatch does no row work. It:
- adds `lushtext-collapsing` to the row's `GtkTreeExpander`, whose CSS shows
  the collapsed arrow over `expander:checked`, both text directions (A120);
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

*Fallback*, if A120 is refuted on either toolkit: the first step removes a
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
- **Several collapses at once.** *Revised (R5):* the queue runs them in
  descending flattened position, so a later root drains while unbound and an
  earlier root's units bring in only collapsed rows. `collapse_folders` and `toggle_folders`
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
against a model of GTK's band (A118) to the end.

### D10. Expansion, measurement first

*Decided (R2, R5):* the profile puts the over-budget dispatches in row
creation (60 % of the load terminal) and CSS validation (67–77 % of the
frame), so the cap applies (task 6.1).

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

*Added by R11 (2026-11-18):* **A123**, from the change's reserve
A123–A126 (free on `main` and on every pushed branch, checked at the
revision): hiding a presented `GtkListView` row's child by opacity 0 and
`can-target` false is paint-only. It runs no measure, setup or teardown,
leaves the band and the list's allocation where they are, draws no pixel of
the row, lets the pointer pick the list instead, and the row child's
accessible `hidden` state reads back. Probe and sample on GNOME 51 and the
GNOME 50 floor (task 2.7); D22 relies on it.

**New**, provisional ids (final = next free at apply time). They were
written as A112–A114, became A113–A115 at apply time, because `main` took
A112 (`restore-main-ci`) first, and became **A118–A120** at the post-stage-4
design revision: `main` took A113 (`diagnose-ci-only-reflow-guard-failure`,
PR #111), and the open branch `quiet-headless-session-services` holds
A115–A117, so A118 is the first id from which three in a row are free on
`main` and on every pushed branch (2026-11-15). A114 stays free for whoever
allocates next. Ids are never reused:

- **A118**: on a mapped, presented `GtkListView` over a `GtkTreeListModel`,
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
- **A119**: `GtkTreeExpander`'s disclosure routes and their ownership, on
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
- **A120**: a CSS rule on a class of `GtkTreeExpander` overrides the arrow
  icon its `expander:checked` node shows, in both directions, without
  changing the row's expanded state or the node's `:checked` state, on
  Adwaita 1.10 and on the GNOME 50 floor.

- **A114** (R4; the free gap, taken at apply time): `GtkTreeListModel` keeps no `GtkTreeListRow`
  for a node nothing references; `get_item` then hands out a new row object,
  without the object data or signal handlers set on the one that died, and a
  `set_expanded` through it notifies only the new object. Probe: hook a row,
  drop every reference, read it back far outside any band, and record the
  identity, the data and the notify.

All four run on GNOME 51 and the GNOME 50 floor. A118's sample includes the
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
  node teardown, about 10 % of a collapse. It points to A118 for the row
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

## Implementation decisions

Recorded at stage 3 (implementation), each with its evidence.

1. **D4's order is kept (A118 holds).** Removing unbound rows after the band
   or between the directory row and the band destroys and creates no widget
   and leaves the band in place on a mapped list (2.1 ms each for 8,000 rows,
   the rb-tree work), so unbound chunks go first and the bound units after.
   On an unmapped list the head removal still replaces its 64 widgets in the
   call, which is why the fallback source keeps the per-unit bound.
2. **D5's click route (A119 holds).** On 4.24.1 and 4.22.2 the expander
   carries no `GestureClick`; the toggle gesture is on the re-created
   `expander` icon. The bind-time `observe_controllers()` loop was inert and
   is removed (memorial appendix in `docs/memorials/tree-expander-activation.md`).
   One capture-phase gesture per expander, installed at setup, claims the
   release inside the current icon. The click leaves selection and focus on
   the item they were on, as GTK's own toggle does (the probe compares the
   selected item, not its position, which the expansion shifts).
3. **D5's key route (open question 4).** In a row built like LushText's (the
   list item's child is a `GtkOverlay` holding the expander), the focus the
   list gives a row (a click, `scroll_to` with `FOCUS`, arrow keys) stays on
   the `GtkListItemWidget`, from which no class binding reaches the expander;
   Tab from the row moves the focus onto the expander, from which all 13
   injectable bindings fire (the keypad direction keys and `Ctrl+KP_Space`
   have no keycode in the virtual keymap and are taken from
   `gtktreeexpander.c:654-690`). The key route is therefore installed: a
   capture-phase key controller on the list view's slice bin, its parent
   (`collapse_execution::setup_disclosure_key_route`) matches the 18 class
   bindings as GTK's triggers do (`gdk_key_event_matches`), reverses the
   direction bindings in right-to-left text, and only for the focused
   expander's directory row takes a collapse key (or `Ctrl+space`) on a row
   that shows expanded into the collapse entry point (origin `Keys`) and an
   expand key (or `Ctrl+space`) on a collapsing row into the expand entry
   point. Everything else propagates, so GTK's own expansion and every other
   key are `main`'s. Widget test
   `test_expander_keys_take_the_collapse_entry_point` drives it with real key
   input through the compositor.
4. **D6's primary path (A120 holds).** The first candidate, a
   `-gtk-icon-transform` rule, did not draw the collapsed arrow: Adwaita
   swaps the icon source on `expander:checked`. The shipped rule sets
   `-gtk-icon-source` back to `pan-end-symbolic` (`pan-end-symbolic-rtl`
   under `:dir(rtl)`), pixel-identical to the collapsed arrow in both
   directions on both toolkits.
5. **The accessible half of A119 is source-read.** Under `GTK_A11Y=none`,
   which the axiom lane and the widget harness set, GTK records no accessible
   state, so the probe skips that half there and says so;
   `gtktreeexpander.c:220-228` and `:278-301` are the evidence, and the
   widget tests assert the projection LushText writes.
6. **Test pacing is one enum seam** (`test_policy::CollapsePacingForTest`:
   `FrameClock`, `NoFrameClock`, `Hold`) instead of separate switches, held
   by a drop guard in the tests so a failing assertion restores production
   pacing.
7. **Strong row references.** A `GtkTreeListModel` keeps no row object for a
   node no widget shows, so a `WeakRef<TreeListRow>` outside the band dies at
   once; the collapse record and its descendant list hold strong references
   for the collapse's lifetime, released at its terminal.
8. **The direct path stops subtree work too.** A collapse of at most
   `DIRECT_COLLAPSE_ROWS` rows first cancels the scans, loads and queued
   refresh targets under the directory, as the sliced path does; without it a
   load kept applying batches to a collapsed directory's retained store.
9. **`expand_directory` falls through to GTK's `set_expanded(true)`** for a
   collapsed row inside a collapsing subtree (a reveal's next ancestor), after
   ending the covering collapse.
10. **One window-wide unit token keyed by frame counter**, so two sections
    collapsing in one window take one bound unit per frame between them
    (`test_collapsing_several_directories_takes_one_unit_per_frame`).
11. **The bound-step tests count both sides of a unit.** GTK may rebind a
    removed row's list item to an entering row instead of creating a widget,
    so the tests count widgets shown anew (new or rebound) and widgets
    destroyed per dispatch, each against the unit, and include the input
    dispatch: `main`'s direct collapse of 256 entries with 400 rows after
    changes 203 each way inside the input dispatch.
12. **The key route sits on the slice bin, not on the list view.** The full
    suite found twelve Space-peek tests failing once the route was on the
    list view: their helper (`common::emit_key_pressed_on_focus`) emits a
    synthetic `key-pressed` on the first key controller of the nearest
    ancestor that has one, and the route became that controller, ahead of
    the peek's. Real key events reach every controller, so no user path was
    broken. On the bin, the list view's parent, capture still reaches the
    route first, and the list view keeps the peek as its first key
    controller.
13. **The A119 probe does not type `Ctrl+KP_Space`.** The compositor's
    virtual keymap has no keycode for it, and Mutter's warning failed the
    axiom lane; the binding is read from the source like the keypad
    direction keys.
14. **A119 stays off the sandbox skip list** (`REMOTE_INPUT_AXIOMS`): its
    probe checks `in_sdk_sandbox()` itself and skips only the input half, so
    the GNOME 50 SDK run keeps its gesture, accessible-state and class-action
    verdict. The list's unit test now names exactly the probes that start a
    RemoteInput session without that guard.
15. **A step drops its store references before GTK collapses the row.**
    The full suite caught `test_collapse_stops_loads_and_refreshes_under_it`
    timing out (passing on retry): the step's facts held a strong reference
    to the drained store while `set_expanded(false)` ran, so the expansion
    hook's `retire_child_store` saw a second reference and skipped the
    retirement. The drained listing's mirror and path entries (its direct
    rows and a "Loading entries…" row) outlived the store, and the evidence
    reported that stale listing beside the re-expanded one; which of the two
    a reader found depended on hash order. The step now clones the row,
    drops its facts and then collapses, and the test asserts that no listing
    of the path keeps a loading row once the collapse has settled. The test
    also moved to 32,768 entries, starting the collapse at the shown store's
    first batch (asserted), because at 131,072 entries the re-expansion
    peaked at 2.4 GiB in a debug build with GTK's `is_expandable()` probe
    store still holding a whole listing (design D12's residual, owned by
    `stop-expander-bind-time-child-scans`), past the widget shard's 2 GiB
    scope.
16. **The verdict test's debug run is small.** A debug build judges no
    timing, and the full matrix (131,072-entry cases, three sizes of
    expansion, the residual splices) helped push the `surfaces` shard (CI
    record 11.3 minutes) to 36.7 minutes over two attempts; it now walks every case at 64 and 256 entries
    (4,096 for the large cases) unless `timing_verdicts_enforced()`, which
    the campaign's optimized build is.
17. **D18's destroy-only condition also bounds the rows before the band**
    (stage 3 continuation). A bound removal takes the destroy-only unit only
    when no row is listed after the realized band *and* at most one unit
    (`SIDEBAR_REALIZED_ROWS_PER_FRAME`) lies before its first realized row
    (`CollapseFacts::rows_before`, `policy::collapse_removal_is_destroy_only`).
    With nothing after the band, GTK keeps the band full by realizing rows
    from above (A1, A118), so a 64-row destroy-only removal at the tail of a
    long list brought 64 rows in: the widget guard measured
    `RowChange { shown: 64, destroyed: 64 }` and the band-model property test
    shrank to 34 rows entering in one step before the condition was added.
    Kani's collapse harnesses take `rows_before`/`rows_after` symbolic.
18. **The first-frame scenario under D19 (decided in design R9, 2026-11-17:
    accepted, and the scenario reworded).** "The first frame takes the
    first step" says the first step runs no later than the first frame
    "(… or unbound chunks from the between-frames source), so no frame
    passes before the work starts". With the band at the top of a large
    listing, the first frame's tick finds an unbound step and removes
    nothing; the between-frames source removes rows right after that frame,
    and they are gone by the second. The guard test (task 10.9) asserts
    that behaviour (first frame at most one unit and the D6 projection,
    second frame under 4,096 rows left). Whether that satisfies the scenario
    or the scenario must say "the first step runs in the gap after the
    first frame" was left to the reviewing stage; R9 took it.
    *Revised by D23 (R11, 2026-11-18):* the input dispatch now removes one
    unbound chunk, so where that chunk covers the subtree's unbound rows the
    first frame's tick takes the first bound unit; the between-frames
    source still takes whatever unbound rows remain after it.
19. **The inline-rename CRITICAL flood is pre-existing, and fixed here
    (task 9.12, 2026-11-17).** The rename's focus-leave handler removed the
    entry from its row inside GTK's focus crossing; the crossing's IN-phase
    loop (`synthesize_focus_change_events`, gtkwindow.c, GTK 4.24.1) then
    walks parents from a widget that has none and spins on
    `gtk_widget_get_parent: assertion 'GTK_IS_WIDGET (widget)' failed`. The
    same flood reproduces on `main` (`c16d6fdc`), so it is not this change's;
    `preexisting-blockers.md` puts it in scope anyway. Two triggers, each with
    a deterministic test: any focus move out of the entry, and a background
    child-state restore that scrolls with `FOCUS`. Fix: the teardown runs from
    an idle and only if the window's focus is no longer in the entry (a check
    of `GtkEventControllerFocus`'s `contains-focus` failed: window
    deactivation also emits `leave`), and `restore_selection` selects and
    scrolls without `FOCUS` while the focus is inside a row. The widget
    harness's `LogCapture` now aborts a run after 4,096 captured warnings, so
    a flood fails one test instead of filling the disk. Latent and harmless,
    recorded for the follow-ups: the rename's focus controller is attached
    after `grab_focus`, so in a window not activated since the rename began a
    click elsewhere does not end the rename.
20. **The expansion cap goes through the batch path BL already had (task
    6.1).** A load into an empty listing of more than one unit no longer
    takes the direct splice (which put up to 256 rows into one frame, 203
    row widgets measured), and every warming-up batch waits past a frame on
    BL's 20 ms timer, which already gives one frame per batch (two painted
    frames per batch in the test's trace), so `ui/list_growth.rs`'s tick
    pacing is not needed. `FIRST_BATCH_ROWS` is now derived from
    `SIDEBAR_REALIZED_ROWS_PER_FRAME`, so task 10.7 decides one unit for
    collapse steps and expansion batches; the attribution hook does not move
    it (compile-time), so 10.7 measures the capped expansion at unit 32 only.
    A refresh that changes at most 256 rows of a listing that is not empty
    keeps the direct splice: it is not an expansion.
21. **The verdict timeline reads no evidence before the completing frame
    (task 10.1).** The convention forbids a per-field `*_for_test` getter, so
    instead of a scalar probe the surface's own cost moved: a listing's
    `directory_listing_evidence` reads only its placeholder tail (placeholders
    only ever end a listing), and the timeline attributes scan terminals per
    dispatch only from the completing frame on, counting earlier ones once
    (`terminals_before_shown`). The D12 residual lands 430-480 ms after the
    completing frame in every 0:131,072 rep of `rev/attr2/slc-d19a-2.log`,
    so no residual attribution is lost. The `done` predicates read the row's
    expansion first and the evidence only once the row is collapsed.
22. **Task 10.4 was not started in the 2026-11-17 stage**, which spent its
    quiet-machine-independent time on 9.12, 9.1, 6.1 and the instruments;
    its queued 10.7 round is marked NOT READY until the next stage puts the
    lighter rows in the round's binary, because 10.7's frame margin is
    10.4's.
23. **D17 as built (task 10.4, 2026-11-18).** A realized depth-one file row
    now carries 9 widgets, down from 16 (the overlay, the expander and its
    three internal icons, the content box, the open indicator, the icon and
    the label; failing-first `rev5/failing-first-10-4.log`, passing
    `rev5/t104.log`). Where D17 said "on demand", the build decides when:
    - *Top-level workspace-folder rows get the drag handle, the shield and
      the insertion-line surface at bind*, whether or not the handle shows
      (a one-folder workspace keeps it hidden). They are one row per folder,
      and creating them at bind keeps the reorder, handle-visibility and
      accessibility tests' contract on those rows unchanged.
    - *Every other realized row gets only the shield, and only while a
      reorder drag is active*: at the drag's start
      (`sync_folder_reorder_shields_for_active_drag`) and when bound during
      it. The shield must own drag hover on every row before
      `GtkTreeExpander`'s drop controller can auto-expand it, so it cannot
      wait for hover. The insertion line shows only on top-level rows, which
      already have it. The drag start's GTK time, with the slice bin's band
      realizing about the viewport's rows (18 in the 800 px test window, not
      A1's 200), printed 2.3 and 6.3 ms for 18 rows in a debug build at a
      load of 10 (`test_a_reorder_drag_gives_every_realized_row_a_shield`,
      `rev5/t104.log`; not a verdict). An optimized figure is queued with the
      go/no-go round.
    - *The Focus Folder button and its hover controller are created at bind
      of a focusable row* (a non-root, non-empty directory), not at the
      first hover: a hover-time creation needs a motion controller on every
      row, which is part of the cost D17 removes. A directory-dense listing
      still pays the button on its directory rows.
    - *A recycled row keeps what it was given*, hidden, inert and without
      `can-focus`: removing a controller-bearing widget at unbind would cost
      the churn D17 saves. So after one drag the realized rows carry shields
      until GTK destroys them (a collapse or a scroll replaces them with
      fresh, light rows).
    - Every lookup reads a row's children by type or CSS class
      (`row_icon_and_label`, `content_box_drag_handle`) and resolves the bound
      row from the widget tree at event time (`expander.list_row()`), so no
      closure captures the setup-time `GtkListItem`.
    - Real pointer drag-and-drop stays covered by the simulate seams only, as
      before: no widget test drives a live GTK drag, so "a shield created
      mid-drag receives hover" rests on GTK picking the widget under the
      pointer at each motion event (the shield is a sibling above the
      expander, as before). The reviewer should treat live DnD as unproven
      by this change's tests.

## Stage 4 verdict (2026-11-15): pass bar not met, back to implementation

The quiet campaign ("Measured baseline", "Verdict campaign") judges the
branch at the candidate constants. Not merged. What it settles and what it
leaves owed (items 2, 3 and 5 are answered by "Design revision after stage
4", R2 and R5; item 2's `main` header case by R4; item 4 by task 10.1):

1. **Passes:** every collapse with nothing after the directory, at every
   size, for its frames against `B_SETTLE_US` (the steps' own time against
   `B_INTERACT_US` is not measured, item 4); the re-expansion in the middle
   of a 131,072-entry collapse; the input dispatch of every owned route (0.1–0.4 ms; 2.1–4.9 ms for the
   two-root folder header; 4.0–6.7 ms for the mid-collapse re-expansion).
2. **Fails at `SIDEBAR_REALIZED_ROWS_PER_FRAME` = 32:** every collapse with
   rows after the directory (four to seven frames of 16.8–24.3 ms per
   collapse from 256 entries up, one or two from 64), and the folder
   header's two-root collapse (five frames of 20.0–29.9 ms). A unit that
   brings rows into the band costs about twice one that does not.
   `main`'s two-root case never settled within 240 s (not attributed; it
   may be a readiness problem on `main` and needs attribution before the
   rework relies on the comparison).
3. **Owed by task 1.3:** units 8 and 16 measured on the same cases (the
   linear reading of the 32-row frames predicts about 9–12 ms at 16, which
   is a prediction, not a measurement), the constant chosen from them, and
   a profile of the two-root frames before deciding whether a frame that
   takes a bound unit may also take unbound chunks of another collapse.
4. **Owed by task 7.1:** the step's own time against `B_INTERACT_US`. The
   verdict test records whole dispatches, so a frame-paced step is not
   separable from its frame; the test needs a step-time instrument (for
   example a timestamp pair around `run_collapse_step` read through the
   evidence) before the pass bar can be enforced.
5. **Owed by tasks 1.3 and 6.1 (D10):** expansion is over budget on quiet
   `main` and unchanged on the branch (256 entries: a 20.6–30.2 ms load
   terminal and a 41–62 ms frame). The profile comes first; the cap, if the
   profile puts the cost in row creation and styling, follows it with a
   failing-first test.
6. **Owed by task 1.4:** the store the probe scan publishes to is not
   printed, and `main`'s re-expansion after a direct 131,072-entry collapse
   has two over-budget frames (16.0–17.0 and 75.1–94.9 ms) without a
   terminal publication, which the task says to attribute before going on.
7. **Owed by task 7.2:** the final-code campaign, four rounds interleaved,
   with GL figures beside cairo.

## Stage 4 review findings (2026-11-15, owed by the rework)

A read-only review of `origin/main...4e979b5b`. Findings 1 and 3 were checked
against the code by the stage 4 agent; the others are the reviewer's, with
the scenario it gave. None is fixed yet: every fix touches a visual-sensitive
path, which D16 lands only after the smokes, and the rework of "Stage 4
verdict" edits the same files. Each needs a failing-first test. The revision
decides each (R6) and makes each a task in group 9.

1. **Blocker: the key route takes keys typed into inline rename.**
   `focused_expander` (`collapse_execution.rs`) walks up from the focus to
   the first `GtkTreeExpander`, and the rename `GtkEntry` sits inside the
   expander's content box (`file_execution.rs`). Typing `-`, `+`, `*`, `/`
   (and the keypad forms), `Shift`/`Ctrl+Shift` with Left/Right or
   `Ctrl+space` while renaming an expanded directory collapses it and drops
   the character (a wrong name can be committed). On `main` the entry
   handles them first. Fix: no expander when the focus is an editable or a
   button, as `focus_allows_peek_shortcuts` (`peek_execution.rs`) does;
   test: type `-` into the rename box of an expanded folder.
2. **A starved mapped collapse stalls.** With a mapped window whose frame
   clock stops, a starved step stores `counter + 1` and the counter never
   advances, so after one bound unit every step breaks at `!unit_allowed`;
   the queue never empties and `workspace-refresh-complete` stays false
   until the window draws again (against D4). `NoFrameClock` skips the
   mapped branch entirely, so the starvation test exercises only the
   unmapped path.
3. **The unit token is keyed by the frame clock's address and compared with
   `<`.** A closed window's entry (for example 50,000) survives; a new
   clock at the same address starts at 0, so no unit is free for that many
   frames and the tick keeps the starvation fallback from firing. Fix:
   treat a stored value above `frame + 1` as stale, or key it by a weak
   reference.
4. **A taken-over descendant keeps the collapsed arrow and `expanded=false`
   after its ancestor is re-expanded**: `finish_collapse` re-projects only
   the record's own row. Re-project every realized row whose path left
   `collapsing_paths`.
5. **An expanded descendant of a collapsing directory reads collapsed to
   LushText but shows expanded** (no class): its arrow or Enter routes to
   `expand_directory`, which cancels the ancestor's collapse, the opposite
   of the user's intent; on rebind the overlay and the expander disagree.
   Project the whole subtree at intent, or make the routes a no-op there.
6. **New File / New Folder from a file row inside a collapsing subtree**
   appends the pending-rename item to the store being drained: a stray file
   on disk, no inline rename, `is_new_item` stuck. Expand the target first
   (as the `was_collapsed` branch does) or refuse.
7. **GTK's own expansion of a collapsed descendant inside a collapsing
   subtree** (drag hover, AT-SPI `listitem.expand`) is not in the record's
   descendants: the step's flattened range goes wrong and `expanded_paths`
   keeps a path under a collapsed directory, which a later restore expands.
   End the covering collapse, or collapse the row again, from the expansion
   hook.
8. **Flake risk:** `test_expander_keys_take_the_collapse_entry_point` and
   `test_the_arrow_click_survives_an_icon_recreation` read state after real
   input with fixed delays only; wrap each check in `wait_until`.
9. **Tests that miss their names:**
   `test_collapse_moves_selection_and_focus_to_the_directory` asserts no
   focus; `test_removing_a_collapsing_directory_cancels_the_collapse` runs
   under `Hold` and reaches `Removed` only through a whole-model reload,
   never through `step_facts` noticing the removed row.
10. **Nit:** when the subtree span is unknown, `move_selection_and_focus_to(row,
    u32::MAX)` moves a selection from anywhere below the directory; skip the
    move instead.
11. **Nit (Kani):** C5 is argued in a comment, not proved (no decreasing
    measure; sibling stores, two nesting levels and multi-row bound removals
    are property-tested only). Assert a measure, or restate C5 as
    termination.

## Design revision after stage 4 (2026-11-15)

Stage 4 blocked the change ("Stage 4 verdict"). This revision attributes
the failure before changing the design, adds the completion bar the
maintainer asked for (user experience first: a collapse must not feel
slower than `main`), brings D10's expansion cap into scope, classifies
`main`'s never-settling folder-header collapse, and turns each stage 4
review finding into a task. Evidence is in
`.claude/handoffs/slice-large-directory-collapse/rev/`: the measurement
patch (`attribution.patch`, applied to the branch for the build and
reverted, never committed), the runner (`attr-rounds.sh`, `run-bin.sh`),
the rounds and profiles (`attr/`), the window-cut profile reader
(`perf-windows.py`) and the `main` diagnosis (`main-header-diag*.log`,
`main-tree-header-diagnostic.patch`).

### R1. Axiom ids

`main` took A113 (`diagnose-ci-only-reflow-guard-failure`, PR #111) and
the open branch `quiet-headless-session-services` holds A115–A117. The
merge commit `cb637ceb` moved this change's axioms to **A118** (row-widget
churn), **A119** (the expander's disclosure routes) and **A120** (the
collapsing arrow) everywhere, and the samples of A113 and A118–A120 hold on
GNOME 51 after the merge (D13). A114 is left free. The new axiom of R4 takes
the next free id at apply time (provisionally A121). Being a single id, it
counts the gap: it is A114 if no branch holds A114 by then, and every
artifact naming A121 is renamed with it. *Applied (2026-11-15, task 2.6):* no
local or remote branch held A114 or A121, so the axiom is **A114**, and every
artifact was renamed. The pre-renumber stage 3 text that wrote this change's
click-route axiom as "A114" (now A119; the history above and the handoff's
`impl/impl-decisions.md`) is history, not this axiom.

### R2. Attribution: the unit is not the lever, the per-row cost is

**The fit.** The stage 4 reading guessed a fixed per-splice cost (a 32-row
frame of 16.8–24.3 ms "above a linear prediction of 10–12 ms"). That
prediction used stage 1's figures (load 7–13, a scratch harness, the call
and the next dispatch apart). Fitted on the quiet campaign's own `main`
rows, same harness, input plus its frame:

| rows after | N | rows replaced | `main` input + frame | per row |
|---|---|---|---|---|
| 400 | 64 | 64 | 29.5–36.8 ms | 0.46–0.58 ms |
| 400 | 256 | about 207 (the band, A1) | 93.0–121.1 ms | 0.45–0.59 ms |

Both lines pass through the origin within a few milliseconds, so a 32-row
unit predicts 14.4–18.8 ms, and the branch measured 16.8–24.3 ms. Stage 1's
lower per-row figure (0.19 + 0.185 ms) split the cost across dispatches its
harness did not all count. **No dominant fixed per-splice cost exists with
rows after the directory.** With nothing after it a unit only destroys, and a
small fixed cost does show (R2's unit table: about 2.9 ms plus 0.16 ms a row
per frame, against `main`'s 0.07–0.11 ms a row inside one call).

**The unit sweep** (branch only, one round per unit, `SLC_UNIT` read by the
measurement patch, labels `slc-rev-u<unit>-ng`, 1-minute load 2.9–3.6 at the
start of each round, so attribution and not a verdict; three repetitions;
"completion" is the new instrument of R3, from the input):

| case | rows after | N | unit 8: worst frame / completion | unit 16 | unit 32 |
|---|---|---|---|---|---|
| collapse | 0 | 256 | 3.7–4.7 / 408–421 ms | 4.7–9.7 / 196–203 ms | 5.7–10.9 / 95–106 ms |
| collapse | 0 | 4,096 | 6.4–6.6 / 425–446 ms | 5.1–7.1 / 206–229 ms | 8.4–9.0 / 110–116 ms |
| collapse | 0 | 131,072 | 9.5–10.6 / 702–775 ms | 9.9–20.0 / 482–542 ms | 9.3–9.8 / 385–470 ms |
| collapse | 400 | 64 | 6.6–9.3 / 114–123 ms | 8.6–9.0 / 51–59 ms | 13.8–14.9 / 29–31 ms |
| collapse | 400 | 256 | 6.4–6.7 / 413–423 ms | 10.0–11.3 / 203–208 ms | 15.1–22.1 / 114–141 ms |
| collapse | 400 | 4,096 | 8.1–10.0 / 430–447 ms | 12.7–13.1 / 218–220 ms | 14.7–18.7 / 128–138 ms |
| collapse | 400 | 131,072 | 9.3–9.5 / 703–709 ms | 9.8–10.5 / 487–498 ms | 14.9–15.9 / 391–438 ms |
| header, two roots | 0 | 4,096 | 9.0–10.6 / 460 ms | 12.3–12.7 / 247–255 ms | 18.2–19.3 / 222–229 ms |

(Full table with layout phases and "until settled": `rev/attr/unit-table.md`.)

Reading:

- Units 8 and 16 keep every frame within `B_SETTLE_US` (one 20.0 ms frame at
  16 in the 131,072-entry case, an unbound turn), and 32 does not with rows
  after the directory (15.1–22.1 ms at 400:256).
- **Completion is frame-quantized:** one unit per frame, so the time is about
  `ceil(band / unit)` frames of 16.7 ms whatever each frame costs. Halving the
  unit doubles it: 400:256 takes 413 / 203 / 114–141 ms at 8 / 16 / 32
  against `main`'s 93–121 ms, and 0:256 takes 408 / 196 / 95–106 ms against
  `main`'s 17–23 ms.
- So no fixed unit meets D2 and R3 together. The work a collapse must do (about
  207 replacements at 0.45–0.6 ms each, about 100 ms) does not fit fewer than
  seven 16 ms frames, which is already `main`'s frozen time. The levers are the
  per-row cost, and not paying it for rows that are about to go (R5).
- The 131,072-entry cases spend 16 frames on unbound chunks (one chunk of
  8,192 per tick, about 8.5 ms each) before the first bound unit, so their
  completion is 385–775 ms at any unit, against `main`'s 24–30 ms.

**The profiles** (`perf record -k CLOCK_MONOTONIC -e cpu-clock:u -F 4000
--call-graph=fp --user-callchains`, through `scripts/measure.sh`, labels
`slc-rev-p-*-ng`, load 3.6–5.2, attribution only; the samples are cut to the
dispatches the verdict test printed over a limit, by their CLOCK_MONOTONIC
window, and to the GTK main thread, `perf-windows.py`; the frame-pointer
unwind does not name every Rust caller):

- **A unit frame, 400:256 at unit 32** (36 frames over 8 ms, 1,950 samples):
  the step's `items-changed` 39 % (destroying the removed rows' widgets 19 %,
  creating the entering rows 20 %, of which factory setup 12 %), CSS
  validation of the new nodes 26 %, the list's measure across its band 8 %
  (`gtk_list_view_measure_across`, labels' Pango extents), AT contexts and
  accessible state 9 %, LushText's `drain_child_store` 4 %. By widget:
  overlay subtrees 19 %, the expander 11 %, labels and Pango 7 %, buttons 5 %,
  images 4 %. The cost is spread over the row's widgets, not one call.
- **The expansion, 256 entries** (18 dispatches over 16 ms): the load
  terminal (20–28 ms) is 60 % row creation (factory setup 36 %, AT contexts
  15 %, buttons 11 %); the frame after it (21–38 ms) is 67–77 % CSS
  validation and 13–19 % labels. D10's hypothesis holds: creating and styling
  about 200 row widgets at once.
- **The two-root header frame** (16.2–16.5 ms frames, 196 main-thread
  samples): the unit frame's split (`items-changed` 31 %, CSS 25 %, AT 9–11 %),
  plus `drain_child_store`'s unbound chunks sharing the frame (11 %). The
  `compare_directory_entries`, `canonicalize` and palette-index samples in the
  same windows are on worker threads (the probe scan), not on the GTK thread.
- **An unbound chunk, 0:131,072** (16 frames of 8.1–9.6 ms, 1,559
  main-thread samples): `drain_child_store` 33 %, of which the store splice's
  `items-changed` into the tree list model 24 % (rb-tree node frees 18 %);
  `FileTreeItem` disposal 16 %; and a `PathBuf` equality loop (21 %) and path
  SipHash (9 %) on the GTK thread whose callers the unwind does not name
  (`[unknown]`): task 10.3 names them first with a DWARF-unwound profile.
  GTK's own collapse removes the same 131,072 nodes in about 10 ms of a
  24–28 ms call, so the chunked removal costs several times the toolkit's.

### R3. The completion bar (new, part of D2)

**Instrument.** *Completion* is the time from the start of the input dispatch
to the end of the first frame dispatch after which the directory row is
GTK-collapsed, i.e. the frame that draws the final rows. On `main` that is
the input plus the next frame; on the branch the frame whose update phase ran
the final step. The verdict test prints it beside the existing figures (the
measurement patch's `shown_us`, ported by task 10.1); the folder-header case
waits for every root.

**Bar.** For every owned case, the branch's worst completion is at most
`max(main's worst completion + 33.3 ms, 100 ms)`.

- *Why two frames of slack, and no more:* `main` does all its work inside the
  input dispatch and draws it in the next frame. A paced collapse has the
  same work to do (R2: about 100 ms with rows after the directory), spread
  over frames that each hold at most one unit, so it pays two frame
  quantizations `main` does not: the first frame boundary its work starts
  from, and the last one it completes on (two 60 Hz frames, the headless
  monitor's refresh). Within that, the branch is never slower than `main`,
  and it responds a whole collapse earlier: the arrow and the accessible state
  flip in the first frame, and input is handled throughout, where `main`
  freezes for all of it.
- *Why 100 ms:* below about a tenth of a second a response reads as
  instantaneous (Miller 1968; Card, Moran and Newell 1983; Nielsen 1993). Where
  `main` freezes for 17–30 ms (no rows after the directory), the branch shows
  the arrow in the first frame and must finish within that limit, not within
  `main`'s frozen time.
- It is a verdict-test figure (`out: time`), not a production threshold.

**Where `main` stands** (from the quiet campaign, input plus the end of its
first frame over 8 ms, or plus the call when no frame is): 0:256 17–23 ms;
0:4,096 15–21 ms; 0:131,072 24–30 ms; 400:64 30–37 ms; 400:256 93–121 ms;
400:4,096 94–104 ms; 400:131,072 113–124 ms; the header's two roots
108–115 ms. `main`'s header case is taken at the first frame that draws both
roots collapsed, which comes before its deferred restore expands the second
root again (R4); this is expected from the diagnosis (both roots read
collapsed right after the call), and the instrument records it.

**Where the branch stands** (unit 32, R2): it fails the bar at 0:256 and up,
at 400:4,096 and up, in the header case and in every 131,072-entry case.

**Can the revised mechanism meet it?** In frames of 16.7 ms, after R5's
D18, D19 and the bottom-up order, with a unit of 32 rows where rows enter
the band and a destroy-only unit of 64 where none do, the band of about 207
rows needing `ceil(175 / unit) + 1` frames (the last for the direct collapse
of at most 32 rows), and the unbound rows taking whatever wall time D19
leaves them:

| case | `main` worst | bar | frames the bar allows | frames needed | slack |
|---|---|---|---|---|---|
| 0:256 | 23 ms | 100 ms | 6 | 4 | 2 |
| 0:4,096 | 21 ms | 100 ms | 6 | 4 + unbound (under 1) | 1 |
| 0:131,072 | 30 ms | 100 ms | 6 | 4 + unbound (32 ms: 2) | 0 |
| 400:64 | 37 ms | 100 ms | 6 | 2 | 4 |
| 400:256 | 121 ms | 154 ms | 9 | 7 | 2 |
| 400:4,096 | 104 ms | 137 ms | 8 | 7 + unbound (under 1) | 0–1 |
| 400:16,385 | 106 ms | 139 ms | 8 | 7 + unbound (under 1) | 0–1 |
| 400:131,072 | 124 ms | 157 ms | 9 | 7 + unbound (32 ms: 2) | 0 |
| header, two roots | 115 ms | 148 ms | 8 | about 5 (both roots' unbound rows, then destroy-only) | 3 |

So the bar is reachable only if every unit frame fits 16.7 ms (the frame
count is the completion), the first unit runs in the first frame (it does
today: the first unit frame starts 17–42 µs after the input, D21), and D19
removes 131,072 unbound rows within about two frames. With rows
after the directory there is no slack for a smaller unit, and lighter rows
(D17) buy margin inside each frame, not fewer frames. Task 10.7 re-derives
this table from its measured costs before choosing the constants, and if any
case has negative slack the change returns to design with that round. No
case is excepted in advance. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)*

### R4. `main`'s folder-header collapse never settles: a real expansion-intent bug

Reproduced on current `origin/main` `0ff77683` (debug build, `heavy.sh`,
`rev/main-header-diag*.log`): after `collapse_folders()` both roots read
collapsed (`(false, 0)` and `(false, 1)`), and within the next turns the
second root is expanded again; readiness settles (`refresh_blocks_readiness`
false, no scan, no pending path), but `expanded_paths` still holds the second
root. A backtrace of its `notify::expanded` shows the re-expansion comes from
`schedule_child_state_restore` (`scan_execution.rs:1664`), which re-applies
`expanded_paths`.

So it is **neither a readiness hang nor a harness artifact**: the verdict
predicate never holds because `main` really leaves the second root expanded.
The cause: the expansion-intent hook (`install_expanded_watch_hook`,
`workspace_section/watch.rs`) lives on `GtkTreeListRow` objects, installed
at model install and at splices. `GtkTreeListModel` keeps no row object for
a node nothing references (implementation decision 7), so `item(i)` for a
row no widget shows, here the second root 4,097 rows down, hands out a
**fresh** row object without the hook. Collapsing through it emits
`notify::expanded` on an object nobody listens to, `expanded_paths` keeps
the path, and the next deferred restore expands it again. The backtrace
shows the restore, not what scheduled it; its two schedulers are a child
scan's terminal (`scan_execution.rs:1425`, expected here: the probe scan
the first root's collapse starts, D12) and a refresh
(`refresh_execution.rs:505`). The same holds
for any LushText expansion or collapse through a row object created after
the hooked one died (a reveal's ancestors, the restore itself, a row
scrolled out and back). The branch's sliced path prunes `expanded_paths`
itself at intent, which is why its header case settles, but its direct path
(`collapse_now`) and `expand_directory` still rely on the hook.

**Fix (D20, task 10.5).** The hook stays as GTK's route for transitions on
bound rows, and is installed at factory bind as well as at splices, so every
row object a widget shows has it. The two entry points record the intent
themselves, on every path (the sliced collapse already does): the direct
collapse prunes the subtree from `expanded_paths` and releases its retained
stores; the expansion inserts the path and retains the shown store; both
are idempotent with the hook. The hook has a second duty the entry points
take over too: it refreshes the row's own workspace-watch contribution
(`refresh_workspace_watch_row`, `watch.rs`), which for a non-root directory
depends on `is_expanded()` (`watch_contribution`). The model's
`items-changed` mirror removes the children's contributions but not the
row's own, so a non-root directory collapsed through a fresh row object (a
reveal's failure terminal, the restore, the sliced path's final
`set_expanded(false)` on a record whose row object no widget showed) would
stay watched. Both entry points therefore refresh the row's contribution
after GTK's transition, on every path; roots are watched at any depth-0
state, so the header case does not depend on it. A new axiom (A114, task 2.6)
records that `GtkTreeListModel` hands out a new `GtkTreeListRow` for a node
whose previous row object died, without its object data or handlers.
Failing-first test: `test_the_folder_header_collapses_every_root`, two roots,
the first holding 4,096 entries and the second 8 (so its collapse is
direct), both GTK-collapsed and absent from `expanded_paths` one second
after the header's collapse. It is expected to fail on `main` (the diagnosis
reproduces the cause) and on the branch's direct path (not run yet: task
10.5 runs it first).

### R5. The revised mechanism

The order and the owned routes of D4–D7 stay. What changes:

- **D17 (new). Lighter sidebar rows.** The row decorations most rows never
  show are created on demand instead of in every row's setup: the reorder
  drag handle (a button and its image, root rows during a reorder only), the
  Focus Folder button (a button and its image, shown on hover of a
  non-root directory row), and the three reorder drop surfaces (shield,
  target, indicator), with their controllers, which only workspace-folder
  rows use. That leaves the list item, the overlay, the expander, the content
  box, the open indicator, the icon and the label. The per-row cost (R2,
  0.45–0.6 ms replaced and styled) is what every collapse unit, every
  expansion frame and every scroll pays; the profile spreads it over the
  widgets a row carries. **Acceptance:** a branch-only attribution round on
  the lighter rows (task 10.4) shows unit 32's worst frame at 400:256 at most
  12 ms (a quarter of margin under `B_SETTLE_US`). If it does not, the change
  returns to design with that round, rather than shipping a collapse slower
  than `main`. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)* Row lightening was a recorded follow-up (D4 "Rejected"); it
  is in scope now because R2 shows nothing else can meet D2 and R3 together.
  Its visual, focus and accessibility behaviour must stay `main`'s: the
  accessibility and visual smokes and the reorder and Focus Folder widget
  tests guard it.
- **D18 (new). Destroy-only units.** A bound removal that brings no row into
  the band (nothing after the subtree beyond the band) only destroys, at about
  a third of a replacement's cost. The policy reads that from its facts (the
  flattened rows after the band) and takes up to
  `COLLAPSE_DESTROY_ONLY_ROWS` (cap, rounded down; candidate 64, decided by
  task 10.7) for such a step; a step that brings rows in keeps
  `SIDEBAR_REALIZED_ROWS_PER_FRAME`. Each step still fits `B_INTERACT_US`.
  Needed for R3's 100 ms cases: 207 rows at 32 a frame take seven frames.
- **D19 (revises D3/D4). Unbound removal is cheap and off the frame clock.**
  Unbound chunks create no widget (A118), so they need no frame: they run
  from a `G_PRIORITY_DEFAULT_IDLE` source between frames, several per frame
  interval, each turn within `LIST_TURN_SLICE_US` plus one chunk; the tick
  takes bound units only (*R13, D25:* no step runs on the tick any more;
  bound units run from their own step-source dispatch between frames). Their cost is cut to the toolkit's: LushText's
  per-row bookkeeping in the chunk's `items-changed` handlers (the `PathBuf`
  loop, the path hashing, the palette index and the `canonicalize` of R2) is
  made O(chunk) or skipped for a collapsing subtree, and the removed
  `FileTreeItem`s are handed to BL's listing retirement instead of being
  disposed inside the chunk. `COLLAPSE_UNBOUND_CHUNK_ROWS` is then re-measured
  (task 10.7; Prototype A's plain splices removed 16,384 rows in
  4.9–6.6 ms under load, so it may grow) under the same rule: a chunk's turn fits
  `B_INTERACT_US`. Target: the 131,072 unbound rows gone within about 32 ms of
  wall time, so the 0:131,072 case can meet R3's 100 ms (R3's table: no
  slack at 32 ms).
- **D7, revised: several collapses drain bottom-up.** When one input
  collapses several directories (the folder header), the queue runs them in
  descending flattened position. A later root's subtree is then removed
  while unbound, and the earlier root's bound units bring in only rows
  already collapsed (destroy-only, D18), instead of creating widgets for the
  next root's children only to remove them. The one-unit-per-frame token is
  unchanged.
- **D21 (considered at the revision, rejected at its review). The input
  dispatch does not take the first step; the first frame does, as today.**
  The revision proposed running the first step inside the input dispatch on
  the premise that waiting for the first tick costs a frame `main` never
  pays. The attribution rounds refute the premise: in every sliced case of
  `rev/attr/slc-rev-u{8,16,32}-ng.log` whose first unit frame was over
  8 ms, that frame starts 17–42 µs after the input dispatch (for example
  400:256 at unit 32: 18, 23 and 42 µs), so the tick the input arms already
  runs in the first frame, the same frame that draws `main`'s collapse.
  Moving the unit into the input dispatch therefore saves no frame. It would
  put 6–9 ms of row churn (39 % of a 15–22 ms unit frame, R2) into the
  input dispatch against `B_INTERACT_US`, and the first frame's own tick
  would take a second unit unless the token were marked, which is the
  stage 4 failure (two units styled in one frame). D6 stands as written:
  the input dispatch does no row work, and the first step runs from the
  first frame's tick. Task 10.9 keeps that as a guard test.
- **D10, decided: cap the expansion.** The attribution (R2) puts the
  over-budget expansion dispatches in creating and styling about 200 row
  widgets, so a load's batches bring at most `SIDEBAR_REALIZED_ROWS_PER_FRAME`
  rows into the band per frame (task 6.1). The first batch is that unit
  (BL's `FIRST_BATCH_ROWS` is already 32), which covers the viewport at the
  verdict window's height, so the rows the user sees appear in the first
  frame there, as today. *Decided at review:* in a viewport taller than the
  unit the cap still holds, and the rest of the visible rows fill top-down
  over the next frames, one unit a frame (about 17 ms for each further 32
  rows, against `main`'s single 40–60 ms frozen frame before any row shows).
  Covering a tall viewport in one batch would bring that many rows into one
  frame, which is the cost the cap exists to bound.
  Expansion is pre-existing over budget on `main` and the branch alike; it is
  in scope under `.agents/rules/preexisting-blockers.md`. The probe-scan
  publication stays D12's residual.
- **D2, extended.** The pass bar adds R3's completion bar, and the step's own
  time is read from a step timer in the evidence (task 10.1;
  `collapse_step_us_max`), so the `B_INTERACT_US` half is enforced apart from
  the frame that carries it.
- **D3, revised.** The constants are chosen by task 10.7 from one branch-only
  attribution round on the reworked code (lighter rows, D18, D19), then
  judged by the final verdict campaign (task 7.2), which runs only the final
  values. Candidates: `SIDEBAR_REALIZED_ROWS_PER_FRAME` 32,
  `COLLAPSE_DESTROY_ONLY_ROWS` 64, `DIRECT_COLLAPSE_ROWS` 32,
  `COLLAPSE_UNBOUND_CHUNK_ROWS` from D19's measurement,
  `COLLAPSE_FRAME_STARVATION_MS` 128.

*Alternatives considered and rejected.*

- *A smaller fixed unit (8 or 16).* Passes the frame bar, doubles or
  quadruples completion (R2): 203–423 ms at 400:256 against `main`'s
  93–121 ms.
- *A unit sized at run time from the measured frame cost.* It would keep
  frames within budget under load, but with today's rows it settles at 16 and
  completion stays near 200 ms; it adds a load-dependent controller to a
  Kani-proved policy for no completion gain. Not taken.
- *Several units per frame interval in idles between frames.* The next
  frame would style every unit's new rows at once, which is the stage 4
  failure again.
- *Collapsing the remainder directly once only the band is left.* That is
  `main`'s collapse of the band (15–22 ms without rows after, about 100 ms
  with them).
- *Waiting for GTK to realize fewer rows* (A1's 200-row band is fixed in
  4.24) or to reuse removed widgets (`gtk_list_item_change_finish`): not in
  LushText's control; the upstream-report candidate of task 8.4 stands.

### R6. The stage 4 review findings

Each finding is a task in group 9 with a failing-first test, or a recorded
refutation. Decisions where a finding leaves a choice:

1. *Key route and inline rename* (verified): the route takes a key only when
   the focus widget **is** the expander (the focus A119 shows the bindings
   reaching from), never a descendant. A key typed into the rename entry
   goes to the entry first, as on `main`; a key the entry does not consume
   and that reaches GTK's binding keeps GTK's direct collapse, a residual
   route.
2. *A stopped frame clock*: a starved step takes a unit whatever the token
   holds, and the token records "a starved unit ran" as a flag the next tick
   reads (that tick takes no unit), instead of storing `counter + 1`. A test
   pacing mode keeps the list mapped while suppressing its ticks, so the
   mapped-and-starved branch is the one exercised.
3. *The token's key* (verified): the token is keyed by a weak reference to
   the frame clock (one entry per live clock, dropped with it), and a stored
   counter above the clock's current one is stale; a pure helper owns that
   decision and is unit-tested.
4. *A taken-over descendant after its ancestor re-expands*: the user
   collapsed the descendant first, so it ends as on `main`: collapsed. The
   cancelled ancestor re-queues every taken-over descendant's own collapse
   (direct if small), and every realized row whose path left the collapsing
   set is re-projected.
5. *An expanded descendant inside a collapsing subtree*: the whole subtree is
   projected collapsing at intent (the class and `expanded = false` on every
   realized expanded directory row in it), and the disclosure routes (click,
   keys, activation) on a row strictly inside a collapsing subtree do
   nothing: on `main` those rows are already gone.
6. *New File / New Folder inside a collapsing subtree*: the target directory
   is expanded first through the expand entry point (which cancels the
   covering collapse and refills), and the item is appended after the refill,
   as the `was_collapsed` branch does.
7. *GTK's own expansions inside a collapsing subtree* (drag hover, AT-SPI
   `listitem.expand`): the expansion hook collapses that row again directly
   (its store holds at most a loading row) and records no intent, so the
   covering collapse's facts and `expanded_paths` stay those of `main`'s
   direct collapse.
8. *Fixed delays after real input*: every check after real input waits with
   `wait_until` on the asserted state.
9. *Tests that miss their names*: the selection test asserts the list's focus
   row, and the removal test removes the directory on disk while steps run
   (frame pacing, not `Hold`), so `step_facts` sees the row leave; the
   whole-model reload stays a second case.
10. *Unknown span*: when the subtree's rows cannot be located, selection and
    focus are left where they are.
11. *Kani C5*: a harness asserts a ranking function (rows left in the
    subtree's stores plus expanded stores left) strictly decreases under each
    step applied to a model of the facts, and is bounded by D2's frame count,
    replacing the comment-only argument.

### R7. What `stop-expander-bind-time-child-scans` must build on

Its proposal says `row_factory.rs` is unchanged; this change makes that
false. It must build on: `collapse_directory` and `expand_directory`
(`workspace_section/collapse_execution.rs`), the only LushText callers of
`TreeListRow::set_expanded` under `ui/sidebar/` (a
`make check-workflow-boundaries` rule); `row_factory.rs`'s setup-time
capture-phase `GestureClick` on the expander (`install_arrow_gesture`) and
its bind-time re-application of the collapse projection
(`apply_collapse_projection`); the removal of the inert bind-time
`observe_controllers()` loop; the lighter rows of D17; the expansion hook at
bind of R4; and `collapse_blocks_readiness` inside
`refresh_blocks_readiness`. Recorded in `docs/next/long-line-documents.md`.

### R8. Review of the revision (2026-11-15)

An independent review of R1–R7 against the code, the attribution logs and
the rules. It found the change sound and its stop conditions honest, and
corrected, in place:

1. **D21 rejected.** Its premise (a frame lost waiting for the first tick)
   is refuted by `rev/attr/`: the first unit frame starts 17–42 µs after
   the input. The spec scenario is now "The first frame takes the first
   step", and task 10.9 a guard test.
2. **R3's 400:64 row** applied `main + 33.3 ms` (70 ms) where the bar is
   `max(…, 100 ms)`: 100 ms, 6 frames, slack 4.
3. **R4's fix missed the hook's second duty**, refreshing the row's own
   watch contribution; the entry points take it over (task 10.5, with a
   non-root case in its test; the spec scenario says so).
4. **The expansion cap and "the viewport in the first frame"** contradicted
   each other for a viewport taller than the unit. Decided for the cap
   (D10 in R5, task 6.1, spec scenario).
5. **Stale stage 1 and stage 3 text** that the revision contradicts is
   marked superseded: the proposal's "slower to finish than `main`", D4's
   "unbound chunks still run on ticks" and its lighter-rows figures, and the
   risk that a collapse finishes later than `main`'s.
6. **The proposal's axiom list** gains the fourth axiom, and R1 says a
   single new id counts the A114 gap.
7. **Task order across groups** is stated in `tasks.md`, since the group
   numbers predate the revision: 2.6 first, the landing commit 7.7 after
   every visual-sensitive edit and its smokes, the verdict campaign last.
8. **D17's gain is recorded as a hypothesis** (Risks): the decorations are
   invisible in most rows on `main`, so their share of CSS validation is not
   separated by the profile; task 10.4's acceptance round decides.
9. **Task 10.7** also measures the capped expansion, which the same unit
   governs, and reads an unbound chunk's budget as its whole turn.

### R9. The D19 go/no-go (stage 1 reconciliation, 2026-11-17)

The stage 3 continuation stopped with the D19 go/no-go open (tasks 10.3,
10.4, 10.7): the rule in its queue entry was that if the skip-cache upper
bound misses R3's completion bar for the 0:131,072 collapse, the change
returns to design. The coordinator ran the queued rounds on 2026-11-17
(`rev/attr2/`, labels `slc-skipcache-1`, `slc-d19a-2`, `slc-d19a-pdw`,
`scripts/measure.sh`, recorded 1-minute load 2.9–3.3 at their ends, saved
binaries `rev/bin/widget-attr-{skipcache,d19a}`; attribution, not
verdicts). This stage merged `origin/main` `c16d6fdc` first (A113 and
A115–A117, A122 are `main`'s; this change keeps A114 and A118–A120, and
needs no new id).

**The figures** (0:131,072, three repetitions each; *completion* is R3's
instrument, `shown_us`; *settled* is `done_us`, until the section's
readiness holds):

| build | completion | settled | worst step | worst frame | worst other dispatch |
|---|---|---|---|---|---|
| `d19a` (the WIP's D19 code) | 194 / 231 / 299 ms | 658–741 ms | 7.9–8.2 ms (**over `B_INTERACT_US` in two reps**) | 6.5–7.1 ms | 30.5–33.7 ms (probe-scan publication, D12) |
| `skipcache` (D19 code, `drain_child_store` skipping the path-cache splice) | 112 / 126 / 153 ms | 589–617 ms | 5.7–6.0 ms | 6.7–7.3 ms | 29.0–37.0 ms (the same residual) |
| `main` (quiet campaign, stage 4) | 24–30 ms | 438.5–502.8 ms | (one direct collapse) | | 32.8–44.6 ms (the same residual) |

**The rule fires.** R3's bar for 0:131,072 is `max(30 + 33.3, 100)` =
100 ms, and the skip-cache upper bound reaches 112–153 ms. On the settled
reading it misses as well (589–617 ms against `main`'s 0.50 s + 33.3 ms).
The change therefore returns to design at this point, and this section is
that decision. The `d19a` build also misses D2: its worst step is 8.1–8.2 ms
in two repetitions, over `B_INTERACT_US`; task 10.3's per-row cut must close
that as well.

**Where the time goes** (`slc-d19a-pdw`, DWARF-unwound, read by leaf symbol
because the Rust frames do not unwind: `rev3/leaf.py`; 318 GTK-thread
samples of 0.5 ms in the 182.7 ms from the input to the completing frame,
so the GTK thread is busy 87 % of the window: the completion is CPU-bound,
not frame-quantized):

- LushText's path bookkeeping, 37 %: `PathBuf` equality 16.4 %, the path
  SipHash 16.3 % (`DefaultHasher::write` 9.4 %, `Path::hash` 6.9 %), the
  `item_locations` `remove_entry` 2.5 % and `release_child_occurrences`
  2.2 %. These are the callers R2 left `[unknown]`; the skip-cache build
  removes most of them.
- Allocation frees 10–12 % (`_int_free_*`, the dropped `PathBuf`s and mirror
  rows), GObject reference and lock traffic about 12 %, GTK's own removal
  (rb-tree, store splice) a few per cent.
- **Instrument bias, 5.7 %:** `directory_listing_evidence` and its reverse
  scan of the 131,072-row mirror, which the verdict test's timeline loop
  runs through `workspace_section_evidence()` after every dispatch (84–111
  dispatches in the branch's window, a few on `main`'s). About 9 ms of the
  window is the measurement, not the collapse. Task 10.1 replaces it with a
  scalar probe.

**What the skip-cache bound is not.** It is not the floor of the mechanism:
it still pays, per 8,192-row chunk, the O(store) mirror count, a
`store.item()` per removed row, the removed paths' collect and free, and
5.7–6.0 ms a chunk, against GTK's own collapse removing the same 131,072
nodes in about 10 ms (R2). And it is not the floor of every mechanism:
removing the unbound tail through one model change (a slice model over
the child store) or hiding the band at intent were considered and not taken
(below). So the evidence does not show that no mechanism can meet R3; it
shows that R3's 100 ms cannot be met by trimming D19's bookkeeping, and
that R3, written for subtrees whose unbound removal costs under one frame
(R3's table: "unbound (under 1)" up to 16,385 entries), did not price the
linear part of a very large subtree at all.

**Decision: a revised, user-experience-first completion bar (not a new
mechanism).** User experience first, taken as what a user perceives of a
collapse: when it acknowledges the input, whether anything freezes, and when
the picture is final.

1. *Acknowledgement* (unchanged, D6): the first frame after the input shows
   the collapsed arrow and the accessible state, in every case. That is
   earlier than `main`'s completion in every measured case (`main` draws
   nothing until its 24–124 ms of frozen work are done).
2. *No freeze* (unchanged, D2): every step within `B_INTERACT_US`, every
   dispatch within `B_SETTLE_US`, except the named residual (D12) and a
   recorded toolkit floor.
3. *Completion*, revised: for every owned case, the branch's worst
   completion is at most

   `max(main's worst completion + 33.3 ms, 100 ms) + max(0, R − 16,384) × 0.5 µs`

   where `R` is the collapsing subtree's flattened rows at the input; when
   one input starts several sliced collapses (the folder header), `R` is the
   sum of their subtrees' rows, since they share one unit per frame and one
   between-frames source (R10). Up to
   16,384 rows the allowance is zero and the bar is R3's, unchanged; it is
   continuous in `R`, so no size steps the bar.
4. *Settled* (`done_us`): printed for both sides and recorded, not judged.
   After the completing frame the section retires the listing (BL's
   4,096-row slices at `G_PRIORITY_LOW`) and, 0.40–0.45 s after the
   collapse on both sides, publishes the probe scan (the residual owned by
   `stop-expander-bind-time-child-scans`); the settled time's excess over
   `main`'s is mostly the completion's.

*Why 16,384.* It is the size up to which R3 is reachable on the frame
argument: R3's table puts the unbound removal of up to 16,385 entries under
one frame, so their completion is the bound units' frames. Past it the
unbound rows' removal grows linearly with `R` at a per-row cost (the
toolkit's per-node removal plus LushText's item hand-off), which no frame
budget prices. A power of two, as `rust.md` asks of a threshold; it is a
verdict-test figure, not a production constant.

*Why 0.5 µs a row: a declared, bounded trade, not a figure taken from
`main`.* `main` has no comparable per-row cost to anchor it: its direct
collapse removes the subtree inside one 24–30 ms dispatch, and its settled
time (438.5–502.8 ms at 131,072) is not a per-row cost either, but the
probe-scan publication of D12, which comes 0.40–0.45 s after the collapse on
both sides (on the branch 431–442 ms after the completing frame). So the
allowance is a choice this change declares and bounds: 0.5 µs (`1 / 2` µs,
a power of two) lies inside what the skip-cache build spends a row past the
frame-bound cases, its 0:131,072 completion (112–153 ms) less `d19a-1`'s
0:4,096 one (71–82 ms, load 8, non-verdict; no skip-cache 0:4,096 figure
exists) over the 126,976 rows between them, 0.24–0.65 µs a row. It is a bar
that requires work: the WIP's `d19a` build misses it by 37–141 ms at
0:131,072, and only the skip-cache upper bound meets it, by 4–45 ms before
the instrument bias is removed. Meeting it requires task 10.3's per-row cut,
which this section's profile now names, and the margin is thin (Risks).

*A second anchor: the chunked-removal rate (R10).* Stage 1's Prototype A
removed unbound rows in plain 16,384-row splices at 4.9–6.6 ms each (load
7–13, non-verdict), 0.30–0.40 µs a row including GTK's per-node removal and
the mirror bookkeeping of that time. The skip-cache build's 5.7–6.0 ms per
8,192-row chunk is 0.70–0.73 µs a row, about twice that, which fits the
per-chunk work listed above ("What the skip-cache bound is not", the
O(store) mirror count among it) that task
10.3 removes. At Prototype A's rate the roughly 130,800 unbound rows of the
0:131,072 case take 39–52 ms of GTK-thread time; with the first frame's
wait, the frames that interleave with the between-frames source and the
band's four destroy-only frames (D18), the reworked completion is estimated
at roughly 110–150 ms. That is over R3's 100 ms even at that rate, so R3
was out of reach for any chunked removal of a subtree this size, not only
for D19's bookkeeping; and it is under R9's 157.3 ms, though not by much.
It is an estimate from non-verdict figures, not evidence: task 10.7's round
decides.

| case | `main` worst | R3 bar | allowance | revised bar | evidence today |
|---|---|---|---|---|---|
| 0:16,385 | 17–19 ms (input 15.7–17.9 ms plus its 1.0–1.1 ms frame, campaign table) | 100 ms | 0 | 100 ms | (R3's frame argument) |
| 0:131,072 | 30 ms | 100 ms | 57.3 ms | 157.3 ms | skip-cache 112–153 ms; `d19a` 194–299 ms |
| 400:16,385 | 106 ms | 139 ms | 0 | 139 ms | (R3's table) |
| 400:131,072 | 124 ms | 157 ms | 57.3 ms | 214.6 ms | **provisional:** no skip-cache figure; `d19a-1` 285–345 ms at load 8 (non-verdict) |

*Measured since (R11, task 10.10):* the old skip-cache binary completed
400:131,072 in 149.9 / 168.3 / 213.0 ms at loads of 4.1–4.9 (attribution),
inside 214.6 ms, with a worst step of 8.2 ms in one repetition. The
paragraph below is the R9 text it answers.

The 400:131,072 row is provisional: its skip-cache figure is queued
(`MEASURE-QUEUE.md`, "large-subtree bar attribution", round 1, with a
skip-cache profile as round 2), and task 10.7's round on the reworked code
decides it with the rest. **The negative-slack stop condition stands:** if
task 10.7's round or the final campaign misses the revised bar in any case,
the change returns to design with that round, and no case is excepted in
advance. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)*

*The trade is user-visible again, and stated.* R8 item 5 marked stage 1's
"finishes later than `main`" superseded by R3. For a subtree above 16,384
rows that is reinstated, bounded: the rows finish disappearing up to
`(R − 16,384) × 0.5 µs` (57 ms at 131,072 rows) after R3's bar, while the
arrow flips in the first frame and no frame freezes. The proposal, the
Risks and the completion requirement say so (the requirement's scenario
names are kept).

*Alternatives considered and not taken now.*

- *Keep R3 and change the mechanism.* (a) A `GtkSliceListModel` over every
  child store, so the unbound tail leaves in one `items-changed` whose
  per-node rb-tree removal GTK does without LushText's per-row work, and
  the store and its caches stay untouched until the terminal (which would
  also make D7's refill trivial). Its gain is unmeasured, and every reader
  that maps a tree row to its `gio::ListStore` (`child_store_key`, the
  mirrors, the scan flights) would change: a redesign of the section, not a
  revision of this change. (b) Hiding the band's subtree rows at intent:
  with nothing after the directory the picture is final in the first frame,
  but the scrollbar still describes 131,072 rows and the user can scroll
  into blank space; with rows after it, a gap opens above them. Both stay
  recorded follow-ups if task 10.7's round misses the revised bar.
  *Superseded by R11 (2026-11-18):* after the go/no-go missed, R11 takes
  R9's (b), hiding the band, as D22, for what the user sees, and keeps R9's
  (a), the slice model, as the recorded next step.
- *Judging the settled time.* On both sides it ends with the probe-scan
  publication 0.40–0.45 s after the collapse (`main`'s dispatch starts
  402–450 ms after the input in the stage 4 campaign, the branch's 431–442 ms
  after its completing frame), so its excess over
  `main` (589–741 ms against 438.5–502.8 ms) is the completion's excess,
  which the completion bar already judges; judging it again would add
  nothing the user sees.
- *Exempting the 131,072 cases.* Rejected: an exception is for a measured
  toolkit floor, and this cost is mostly LushText's (37 % of the window).

**Implementation decision 18, decided.** With the band at the top of a large
listing, the first frame's tick finds only unbound rows to remove and
removes nothing; the between-frames source removes them in the interval
after that frame. Accepted: the first frame shows D6's projection, which is
all a user can see change (unbound rows are not drawn), and the interval
that follows it is the earliest time a between-frames source can run. The
scenario "The first frame takes the first step" now says so; task 10.9's
guard test asserts it (the first frame takes at most one unit and shows the
projection, and before the second frame's tick the subtree holds fewer rows
than at the input, removed by that frame's tick or by the between-frames
source; R10).

**The open CRITICAL flood is a task.** The WIP's
`test_typing_into_inline_rename_keeps_every_character` floods
`gtk_widget_get_parent: assertion 'GTK_IS_WIDGET (widget)' failed` (36.5
million lines; the 4 GiB scope kills mutter) on every batch that contains
it (handoff `impl/impl-decisions.md`, decisions 16 and 19). It is a blocker
under `preexisting-blockers.md` whether or not `main` has it, so it is task
9.12: a deterministic reproduction run with bounded output, a run on
`origin/main` to classify it, and a failing-first fix (the leading
candidate is `restore_selection`, handoff `impl/wip-1700.patch`), before
task 9.1 can be proved.

**Reconciliation record.** The merge with `origin/main` needed two WIP
fixes to pass the hook, both in the uncommitted tree (they land with task
7.7): the section's `map`/`unmap` handlers moved from string-named
`connect_local` to the typed `connect_map`/`connect_unmap`
(`make check-signal-connections`, new on `main`), and `docs/thresholds.md`
gained `COLLAPSE_DESTROY_ONLY_ROWS`'s row (`make check-thresholds`). The WIP
snapshot after the merge commit is `rev3/wip-after-merge.patch`.

### R10. Review of R9 (2026-11-17)

An independent review of R9 against the logs (`rev/attr2/*.log` read with
`rev/summ.py`; the stage 4 campaign's `main` rounds), the WIP and the rules.
Every R9 figure was re-read and holds: skip-cache completion 112.6 / 125.6 /
153.3 ms, settled 589–617 ms, worst step 5.7–6.0 ms; `d19a` 194.1 / 230.9 /
298.7 ms, 658–741 ms, 7.9–8.2 ms; `d19a-1`'s 0:4,096 71.2–82.3 ms at load 8;
`main`'s 0:16,385 15.7–17.9 ms plus a 1.0–1.1 ms frame; the 0.24–0.65 µs
range and the 37–141 ms and 4–45 ms margins. The decision is sound: R3's
own table priced the 0:131,072 case at "unbound (32 ms: 2)" frames, which
no chunked removal reaches at the rates measured so far, so revising the
bar for the linear part is a correction of R3, not a relaxation of what R3
could have met; and the stop condition stays. Corrected, in place:

1. **`R` for several collapses at once** was undefined. It is the sum of the
   subtrees one input collapses (R9 item 3, the requirement, task 10.1),
   because they share the unit token and the between-frames source; for the
   measured header case (2 × 4,096) the allowance is zero either way.
2. **The 0.5 µs a row had one anchor across two builds and two loads.** A
   second anchor is added beside it, Prototype A's chunked-removal rate,
   with the completion it predicts; it shows both why R3 was out of reach
   at 131,072 rows and that R9's bar is reachable but thin.
3. **"The cut's best case" contradicted R9.** R9 says the skip-cache build
   is not the floor of the mechanism; Risks and task 10.3 called it the
   cut's best case. Both now say it is the best case of cutting the
   path-cache bookkeeping alone. Risks also records the repetitions'
   spread.
4. **The requirement said "no later than `main`'s"** in its body while
   allowing more; its body is reworded (the scenario name is kept, as R9
   decided), and "57 ms past the bar of a directory of 16,384 rows" now
   says "past the same case's bar without the allowance", since that bar
   depends on `main`'s time for the case, not only on the size.
5. **Implementation decision 18's guard** read "fewer than 4,096 of the
   subtree's rows left in the band's model range", which no instrument
   measures; it now matches task 10.9: the subtree holds fewer rows than at
   the input before the second frame's tick.
6. **The probe-scan delay** read 0.43–0.45 s in two places and 0.40–0.45 s
   in a third; `main`'s campaign rounds put its dispatch 402–450 ms after
   the input, so all three say 0.40–0.45 s.
7. **The attribution hooks had no removal task.** `SLC_UNIT`, `SLC_DESTROY`,
   `SLC_CHUNK` (read through `OnceLock`s inside `next_collapse_step` itself,
   so the pure, Kani-proved policy reads the environment, and in
   `collapse_execution.rs`'s direct-path test) and `SLC_SKIP_CACHE`
   (`scan_execution.rs`, which skips the path caches and so is wrong by
   design) are in the WIP. Task 10.7 needs them; new task 10.11 removes
   them after it, saved as a handoff patch, before the gates of 7.3 and the
   smokes of 7.4.
8. **The ticked boxes 9.4–9.7, 9.10 and 10.6** were proved before the merge
   with `c16d6fdc` and before the rest of groups 9 and 10; task 5.8 now
   reruns them on the final tree, in isolation as well.
9. **Task 10.10's profile** was ordered after task 10.3, which it is meant
   to inform; 10.3 now uses it if it arrives first and does not wait for it.

### R11. The go/no-go on the real code, and the design it returns to (2026-11-18)

**The result, recorded as it is.** The coordinator ran the queued go/no-go
on 2026-11-18 at 04:41 (`MEASURE-QUEUE.md`, "go/no-go on the real code",
label `slc-gonogo-104`, `scripts/measure.sh`, 1-minute load 3.86 at its end
and below 4 before it; saved binary `rev5/bin/widget-attr-104`, which is
`314b6187` plus task 10.4, opt-level 2; log `rev/attr2/slc-gonogo-104.log`).
At 0:131,072 the completion (`input_us + shown_us`) was **157.0 / 156.1 /
157.6 ms** against R9's 157.3 ms, which every repetition had to meet. The
third misses by 0.3 ms, so the result is **NOGO** by the pre-registered rule,
and this section does not reinterpret or round it. The same round recorded:
- worst frame 4.5–4.6 ms (layout 3.8–3.9 ms);
- worst step 5.9–6.0 ms, within `B_INTERACT_US`;
- settled 694–710 ms, with one unshown dispatch of 32.0–34.4 ms (the
  probe-scan residual of D12);
- 100–104 dispatches from the input until settled;
- input dispatch 0.2 ms.

The result is not circular: production code does not pace on R9's figure,
which only `evidence.rs` reads. The drag start that the same queue entry
measured is 1.9 ms for 18 realized rows (`slc-dragstart-104.log`;
implementation decision 23's open question, within `B_INTERACT_US` at that
band).

The queued R9 row is measured too (task 10.10). The **old** skip-cache
binary (`rev/bin/widget-attr-skipcache`: D19 code, 16-widget rows, the
path-cache splice skipped), at 400:131,072, completed in **149.9 / 168.3 /
213.0 ms** against R9's 214.6 ms, with:
- worst frame 12.3–14.7 ms;
- worst step 6.6 / 7.3 / **8.2 ms**, the last **over `B_INTERACT_US`**;
- settled 614–716 ms.

Its runs were at 1-minute loads of 4.94 and 4.10 at their ends, so they are
attribution, not verdicts (`slc-skipcache-400.log`, `progress.log`).

#### Attribution: where the time goes

**What was read, and what it covers.** The only profile of this window is
of the **old skip-cache binary**, `rev/attr2/slc-skipcache-pdw.data`:
- recorded 02:20 at load 2.57, `cpu-clock:u` at 4 kHz, DWARF call graphs of
  16 KiB, 0:131,072, one repetition;
- completion 111.3 ms, settled 584.5 ms;
- read by window, the input to the completing frame, on the GTK thread
  (tid 1627932), with `perf script` and `perf report --time`; the readers
  and their output are in the handoff's `rev/r11/` (`README.md`,
  `skipcache-window.txt`).

The GTK thread has 333 samples there: 83.3 ms of CPU in 111.3 ms of wall
time. `widget-attr-104`, the binary that missed, has **no profile and no
per-dispatch timeline**: its log prints only per-case summaries. Its split
below is therefore an inference from the two builds, and the rounds queued
at the end of this section measure it.

**The skip-cache window has two phases.** Samples in 5 ms buckets, by the
first matching frame in each call chain:

| from the input | what runs | GTK-thread CPU |
|---|---|---|
| 0–30 ms | the unbound chunks, back to back (119 of a possible 120 samples). Totals over the window, nearly all in this phase: the store splice's `items-changed` into the tree list model (rb-tree node removal) 8.5 ms, `store.item()` collecting the removed items 7.3 ms, the rest of the splice 3.0 ms, `drain_child_store`'s own work 2.3 ms, `PathBuf` drops 2.8 ms, the verdict test's evidence read 2.0 ms, unattributed 5.5 ms | 29.8 ms |
| 30–111 ms | five **frame-paced** band dispatches, at about 45, 60, 75, 90 and 107 ms, 15–17 ms apart: the destroy-only units and the terminal collapse (list item manager teardown 8.8 ms in the window, frame layout and paint 5.3 ms). Between them the `G_PRIORITY_LOW` listing retirement drops the removed `FileTreeItem`s, 22.8 ms in all | about 53 ms, of which 22.8 ms retirement |

So the coordinator's reading of the go/no-go ("about 100 dispatches before
shown, CPU-bound time slices back to back, not frame-quantized") holds for
the first phase only. The count of dispatches cannot tell the two readings
apart: the retirement source is a 1 ms `G_PRIORITY_LOW` timeout, which
yields many short dispatches between frames. In the skip-cache window the
**tail is frame-quantized**: about four bound frames after the unbound
phase. That is `ceil(207 / 64)` destroy-only units of D18 for the band of
about 207 rows, plus the terminal. Together they are about 65–80 ms of the
111 ms.

**Levers this refutes.**
- *The listing retirement.* Its drops run at `G_PRIORITY_LOW`. They fill the
  gaps between band frames, and the frames keep their cadence, so they are
  not on the completion path. Pausing them during a collapse would not
  shorten it. Not taken.
- *The evidence instruments.* About 3 ms in the window (12 samples). Task
  10.1 already removed most of the bias R9 measured. Not a lever.
- *Unattributed.* 33 of the 333 samples carry no call chain and are
  reported as such (8.3 ms).

**The `widget-attr-104` split (inference, to be measured).**
- The band tail cannot be cheaper in frames: 104 has the same destroy-only
  unit of 64, and its lighter rows shorten each frame (worst 4.5 ms against
  6.3–7.3 ms in the skip-cache build), not their number. With four or five
  frames, about 65–80 ms of its 157 ms are the tail, and the unbound phase
  is the remaining 75–90 ms.
- That is about 45–60 ms more than the skip-cache build's 30 ms. The one
  known difference on that path is the path-cache splice that
  `SLC_SKIP_CACHE` skips: `drain_child_rows_cache`'s per-row `item_locations`
  and `visible_path_counts` removals, a `PathBuf` hash and an equality test
  each. Task 10.3 cut it from R9's `d19a` figure (194–299 ms) but did not
  remove it.
- `widget-attr-104` still reads `SLC_SKIP_CACHE`, `SLC_DESTROY`, `SLC_UNIT`
  and `SLC_CHUNK`, so rounds Q1–Q2 below measure this split without a build.

**The three causes asked about.**
- *Per-row bookkeeping:* inferred to be the larger part of 104's unbound
  phase (the path-cache splice), plus about 9.5 ms of LushText's own work in
  the skip-cache build (the item collect and `drain_child_store`).
- *GTK's list-model removal:* about 11.5 ms (the tree list model's node
  removal and the store splice), close to GTK's own collapse of the same
  131,072 nodes in about 10 ms (R2).
- *The evidence instruments:* about 3 ms.

The time the user waits is the sum of a CPU-bound unbound phase and a
frame-quantized tail of four to five frames.

#### The pre-registered selection margin (fixed before the candidates)

**Rule.** A configuration is **selected** only if **every repetition of
every case is at most its bar minus one 60 Hz frame**, M = 16.7 ms
(16,667 µs). That holds for the constants round (task 10.7), for the
attribution gates below (Q4), and for any further go/no-go. "Its bar" is:
- for completion, R3's bar plus R9's allowance;
- for the new picture-final figure, the completion bar where that is its
  bar (below). Its first-frame bar is ordinal: the figure must fall in the
  first frame dispatch after the input, and `INPUT_TO_FRAME_US` (16,666 µs)
  less a frame would be nothing, so no margin applies to it.

The final verdict campaign (task 7.2) is still judged at the bar itself,
unrounded and unchanged. The margin chooses what goes into that campaign; it
does not move the line the campaign is judged on.

**Why one frame.**
- The tail moves in frame quanta (the timeline above): a configuration
  within one frame of the bar fails as soon as one frame slips, for example
  when one dispatch runs past a vsync. That is a coin flip across the four
  rounds and eleven cases of the final campaign.
- The repetitions of a configuration can sit very close together, and the
  bar can still split them. 104's three are within 1.5 ms of each other, and
  the bar falls inside that span. The old skip-cache build's spread 112–153
  and 150–213 ms. Neither kind of spread is what the margin is for. It is
  for the unit in which a configuration fails, which is one frame.
- One frame is also the smallest step D23 and D18 below buy, so a margin
  smaller than a frame could not separate the candidates.

**What it says about 104.** At 0:131,072 the selection bar is
157.3 − 16.7 = **140.6 ms**, and 104's 156.1–157.6 ms miss it by **15.5–17.0 ms,
a whole frame**, not by 0.3 ms.

Selection bars per case. `main`'s worst completions are R3's, from the
stage 4 campaign; `R` is the subtree's flattened rows.

| case | `main` worst | bar | bar − M |
|---|---|---|---|
| 0:64, 0:256, 0:4,096, 0:16,385 | 11–23 ms | 100 ms | 83.3 ms |
| 0:131,072 | 30 ms | 157.3 ms | 140.6 ms |
| 400:64 | 37 ms | 100 ms | 83.3 ms |
| 400:256 | 121 ms | 154.3 ms | 137.6 ms |
| 400:4,096 | 104 ms | 137.3 ms | 120.6 ms |
| 400:16,385 | 106 ms | 139.3 ms | 122.6 ms |
| 400:131,072 | 124 ms | 214.6 ms | 197.9 ms |
| header, two roots of 4,096 | 115 ms | 148.3 ms | 131.6 ms |

#### Candidates, by the phase each can touch

The three candidates are not exclusive. Each touches a different phase of
the timeline above, so they are judged by phase and in combination, by their
distance to `main`. On `main` a 0:131,072 collapse freezes the sidebar for
24–30 ms, and then the picture is final.

- **(a) Tuning the per-step work** touches both phases.
  - The unbound phase: the path-cache splice leaves the chunk (D24, below).
  - The tail's frame count: the destroy-only unit (D18's constant, 64 today),
    and whether the first bound unit runs in the first frame (D23, below).
  - The unit and the chunk size are secondary. R2 showed that the unit trades
    frame cost for frame count, and the chunk size moves only the unbound
    phase's slicing.
- **(b) A `GtkSliceListModel` over each child store** touches the unbound
  phase only. The unbound rows would leave through the slice's
  `items-changed`, without the store splice and without LushText's per-row
  work. Of the skip-cache build's 30 ms phase, GTK's node removal (8.5 ms)
  stays either way. So (b) can save at most about 20 ms over (a)'s D24,
  which removes the path-cache work too, and nothing in the tail. Its cost is
  a redesign of the section, as R9 recorded: every reader that maps a tree
  row to its `gio::ListStore` (`child_store_key`, the mirrors, the scan
  flights) changes, and D7's refill becomes a model change of up to 131,072
  rows. **Not taken.** If D24 does not reach the selection bar, it is the
  recorded next step.
- **(c) Hiding the band at intent** touches neither phase. It changes what
  the sidebar draws while both run: the collapsing subtree's realized rows
  draw nothing from the first frame on. It cannot move `shown_us`, so it
  cannot pass the completion bar alone. It is the only candidate that brings
  the picture the user sees to `main`'s, or ahead of it.

**What the user sees, frame by frame.** The band is the about 207 rows GTK
realizes around the viewport (A1). The verdict window's 800 px viewport
shows about 23 of them. Entering rows arrive at the band's tail, below the
viewport (A118). A sliced collapse has more than `DIRECT_COLLAPSE_ROWS`
(32) subtree rows, so at intent the rows after the subtree are below the
viewport at that height.

- **Nothing after the subtree** (0:N).
  - *Today:* the arrow flips in the first frame. Each band unit then removes
    the band's first 64 children, and the children below shift up, so the
    viewport shows a different set of children each frame, four or five
    times, until the terminal empties it. At 0:131,072 the children stay
    drawn for about 155 ms.
  - *With (c):* the first frame draws the directory row collapsed and
    nothing below it. That is the final picture, except the scrollbar, which
    keeps describing the subtree until the model completes.
- **Rows after the subtree** (400:N).
  - *Today:* the viewport shows the children shifting by a unit each frame,
    seven times at unit 32, and the rows after the subtree appear only when
    the last children leave the viewport.
  - *With (c):* the first frame draws the directory row collapsed and an
    empty region below it. The rows after the subtree enter at the band's
    tail and reach the viewport in the last unit or two, in their final
    places. The picture is final at or one frame before completion. Only in
    a viewport taller than the subtree do rows after it show at intent; a
    gap above them then closes unit by unit.
- **Scrolled inside the children** (the folder header's collapse with the
  directory row above the viewport).
  - *Today:* the children shift each frame, then the final scroll position
    of D8 shows the final picture.
  - *With (c):* the viewport is empty until completion, then the same final
    picture.

  The empty viewport is the trade (c) makes in this configuration, stated in
  Risks: no row that is about to disappear is drawn, and the wait is bounded
  by the completion bar.

**Distance to `main`, predicted from the timeline.** These are estimates,
not measurements; the rounds below and task 10.7 measure them.

| configuration | picture final, 0:131,072 | completion, 0:131,072 | completion, 400:4,096 |
|---|---|---|---|
| `main` | 24–30 ms (frozen until then) | 24–30 ms | 94–104 ms (frozen) |
| 104, measured | about 155 ms (children drawn, shifting) | 156.1–157.6 ms | not measured on 104 (Q4) |
| (a): D23, D24, destroy-only unit 64–128 | about 90–110 ms | about 90–110 ms | about 105–110 ms (7 frames, frame 1 included) |
| (b) on top of (a) | as (a), less up to 20 ms | as (a), less up to 20 ms | as (a) |
| (c) alone | the first frame (about 5–17 ms) | 156–158 ms: fails the selection bar | as today |
| **(a) + (c)** | **the first frame** | about 90–110 ms | about 105–110 ms |

Only (a) + (c) does both things: it ends the picture's wait in the first
frame, earlier than `main`, wherever nothing follows the subtree, and it
moves the completion under its selection bar. With rows after the subtree,
nothing can make the rows after it arrive sooner. About 207 rows must be
created at 0.3–0.6 ms each, which no 16 ms frame holds, so (a) packs them
into seven frames (D23). `main` also freezes for 94–124 ms in those cases.

**The frame count is not settled (R12; measured in "Attribution results" below: five band dispatches at 0:131,072, seven at 400:4,096).** The table's rows-after figures use
R3's count, `ceil(175 / unit) + 1` frames, which is what `next_collapse_step`
computes for a band of about 207 bound rows (bound units take the store down
to `DIRECT_COLLAPSE_ROWS`, then one terminal `CollapseRow`): at unit 32, six
units and the terminal, seven frames. The skip-cache timeline above has five
band dispatches where that count gives four at unit 64 (`ceil(175 / 64)` is
three, plus the terminal), and the reading above wrote it as
`ceil(207 / 64)` units plus the terminal. If that extra dispatch is in the
rows-after tail too, 400:4,096 takes eight frames even with D23, about
117–133 ms against its 120.6 ms selection bar, and nothing in (a) shortens
a rows-after tail except D23's frame and the unit, whose larger values R2
priced in frame cost. The predicted 105–110 ms are therefore not evidence.
Q4's gate does not use this table: it starts from Q4's measured completions,
which carry the real frame count, so the gate, not this table, decides; and
task 10.12 records the band dispatches Q1's profile counts against
`ceil(175 / unit) + 1` and corrects the table either way. If 400:N then
misses its selection bar, the change returns to design with that round, as
the gate already says.

#### Decision

(a) + (c), as four decisions. Their mechanisms are fixed here; their
thresholds are pre-registered to the rounds below and to task 10.7.

- **D22 (new). The collapsing subtree's realized rows are hidden at intent,
  paint-only.**
  - *What:* in the input dispatch, the entry point sets, on every realized
    row in the collapsing subtree (the band read of D6 already finds them),
    on the **row widget**, the list view's child that hosts the row (the
    overlay's parent, read as `mapped_row_widgets` already reads it, R12):
    opacity 0 (`gtk_widget_set_opacity`), `can-target` false, and the
    accessible `hidden` state.
  - *Why the row widget, not the overlay (R12):* the row widget is GTK's
    `row` CSS node, which draws the row's hover and selected background and
    its focus ring (`.navigation-sidebar > row`) outside the overlay's render
    node; it carries the list's click gesture (on `GtkListFactoryWidget`,
    the row widget's parent class: a press selects, a double press
    activates); and GTK presents it to assistive technologies as the list
    item, with its position in the set. With the overlay alone hidden, a
    hovered row would still paint its background, `gtk_widget_pick` would
    return the row widget and a click would select the hidden item, and the
    tree would keep about 200 empty list items. With `can-target` false on
    the row widget, the pointer picks the list view, which has no click
    gesture of its own (`gtklistbase.c`: only the rubber band, off here).
  - *Membership (R12):* a row is in the collapsing subtree when the
    collapsing directory's item is one of its `TreeListRow` ancestors' items,
    or by its flattened position inside the subtree's span, never by path
    prefix. The same path can be listed twice, under two overlapping
    workspace folders (task 9.10's fixture), and a path test
    (`path_is_collapsing`, as the R6.5 projection uses it today) would hide
    the other folder's rows, and that folder's own root row when it is the
    collapsing path, for the whole collapse. The R6.5 projection and
    `stop_subtree_work` take the same membership (task 10.13).
  - *GTK's own `hidden` writes (R12):* `gtk_widget_map` writes `hidden`
    false after the `map` emission returns, and `gtk_widget_unmap` writes it
    true (`gtkwidget.c`, `gtk_widget_map`/`gtk_widget_unmap`), and the list
    item manager binds a new row widget before inserting it, so mapping it
    (`gtklistitemmanager.c`, `gtk_list_item_base_update` then
    `gtk_widget_insert_after`). Neither a bind-time write nor a `map`
    handler outlasts that write on a created row, or on any row after the
    sidebar is unmapped and mapped again mid-collapse (the "collapse nobody
    draws" scenario). The state is therefore re-applied after insertion: by
    the collapse's tick of the next frame (or the low-priority step source
    when no frame comes) for every realized subtree row that does not read
    `hidden`, a read of at most the band, so such a row reports `hidden` from
    the end of the frame after the one that created or mapped it. Opacity and
    `can-target` are set at bind and GTK does not write them, so its pixels
    and pointer input are hidden from the start.
    *Revised by R13 (D25):* the re-apply runs at the end of the step that
    created the row (inserted rows are mapped inside the step), or from a
    one-shot step source armed by the row's bind or the list's `map`; no
    pass runs on the frame tick.
  - *Bind:* the bind path re-applies the opacity and `can-target` to a row
    rebound into a collapsing subtree, and clears all three on a row rebound
    anywhere else, because GTK recycles row widgets without resetting them.
  - *End:* the collapse's end clears it on every realized row it set, by any
    end (`Completed`, `ReExpanded`, `Removed`, a model reset, dispose). The
    `hidden` state is reset, not written false, so GTK's map writes govern it
    again.
  - *Why paint-only:* anything that changes a row's measure (visibility, a
    size request, a CSS class whose rules change size) would make the list
    realize more rows to fill the viewport (A1). That is the cost this change
    exists to bound, and it is why D4 rejected hiding or detaching the
    list, which tears every row down. Opacity and `can-target` change
    neither the row's measure nor its binding. `set_opacity` is used rather
    than a CSS class: a class change revalidates the CSS of about 200
    subtrees, and CSS validation was 26 % of a unit frame (R2).
  - *Axiom:* A123 (task 2.7) pins it on GNOME 51 and the GNOME 50 floor,
    with a probe on the row widgets:
    - hiding a presented list's rows this way runs no measure, setup or
      teardown, and leaves the band and the list's allocation unchanged;
    - a hidden row draws no pixels, including when it is selected, focused
      or under the pointer;
    - the pointer picks the list, not the row, on both toolkits; and, on
      GNOME 51 only (the GNOME 50 SDK run skips real input, implementation
      decision 14), a real press and double press over a hidden row select
      and activate nothing and leave the row without the prelight state;
    - whether the presented accessible tree, read under `GTK_A11Y=test`,
      omits the hidden rows and their descendants (if it keeps the
      descendants, `hidden` is also written on the overlay, D6's projection
      target), and that GTK's map write turns a created
      row's state back to not hidden (the record that makes the re-applying
      pass necessary), while a write after insertion holds until the next
      unmap;
    - the time to hide and to clear about 207 rows, the band's size.

    If A123 is refuted on either toolkit, the change returns to design with
    the probe's figures. *(Superseded by R13, G3: a refuted A123 drops D22
    without a design loop.)*
- **D22, focus, pointer and assistive technologies.** Specified as new
  scenarios with failing-first tests (tasks 10.13 and 10.14):
  - *Focus:* no painted frame shows the keyboard focus on a hidden row.
    D6 already moves the focus to the directory row. While a collapse is
    pending, the section's capture-phase key controller (implementation
    decision 12) holds the list-navigation keys pressed on the collapsing
    directory row or on a hidden row: Up, Down, Page Up, Page Down, Home,
    End, their keypad forms, with or without Shift and Ctrl. When the
    collapse ends, it applies them in order from the directory row, as
    `main` applies keys queued behind its frozen collapse. A Down pressed
    in the interval lands on the row after the directory.
  - *Order behind a held key (R12):* once a key is held, a later key whose
    effect depends on the focused row is held behind it too, not applied to
    the directory row ahead of it. Otherwise Down then Return would activate
    the directory row (a re-expansion, D7) where `main` activates the row
    after it. Those keys are, from the routes in the code today: Return and
    KP_Enter (row activation), Space (the peek controller on the list view,
    `peek_execution.rs`), and Menu and Shift+F10 (the context menu,
    `context_menus.rs`); the sidebar has no Delete or F2 key route. Every
    other key (Escape, Tab, window accelerators) propagates as on any other
    frame. Held keys replay in order when the collapse ends, navigation
    through the pure helper of task 10.14 and the others through the route
    each takes, on the row the replayed navigation reached; task 10.14
    re-reads the routes at apply time and holds exactly the row-dependent
    ones. Each replayed navigation key lands where GTK's own binding lands
    on the final tree (Page Up and Page Down by the list's page at that
    time), which task 10.14's parity test checks against GTK.
  - *Pointer:* a click or a hover on the hidden region selects, activates,
    expands and peeks nothing, and paints no hover background, because the
    row widgets are not targets and the list view under them has no click
    gesture.
  - *Assistive technologies:* hidden rows report `hidden`. The directory row
    keeps D6's `expanded` false, and the accessibility smoke and
    `docs/accessibility.md` say so (task 8.3, reopened).
- **D23 (new; revises implementation decision 18). The input dispatch
  removes at most one unbound chunk.**
  *Moved by R13 (D25):* the chunk runs in the first step-source dispatch
  after the input, not in the input dispatch, and is admitted by its own
  dispatch time (R13, G2); the input dispatch does no row work.
  - *What:* after D6's intent and D22's hiding, the input dispatch runs one
    `COLLAPSE_UNBOUND_CHUNK_ROWS` chunk of the unbound rows, if any. It runs
    no bound unit (D21 stays rejected) and no turn-slice loop. Unbound chunks
    create no widget (A118), so this adds no row churn to the input.
  - *Why:* where the subtree's unbound rows fit one chunk, the first frame's
    tick then finds bound rows and takes the first unit. Without it, the
    frame that implementation decision 18 accepted is lost. Without that
    frame 400:4,096 needs eight frames (about 122–127 ms, over its 120.6 ms
    selection bar), and with it seven (about 105–110 ms).
  - *Which cases it covers:* at the 8,192-row candidate, every subtree up to
    about 8,400 entries; at 16,384, also 400:16,385 (about 16,180 unbound
    rows), whose 122.6 ms selection bar eight frames would also miss. Task
    10.7 therefore takes the chunk from candidates of at least 16,384 when
    the deciding Q5 round (Q5b if D24 is taken, else Q5a) shows such a
    chunk's turn within `B_INTERACT_US`.
  - *Seams:* the input's chunk is the policy's next step, taken only when
    that step is an unbound removal (a bound unit or a collapse leaves the
    input dispatch to D6 alone), and it honours the test pacing seam: under
    `CollapsePacingForTest::Hold` the input removes nothing, so the ticked
    tests that hold a collapse (9.4–9.7) see the store they see today.
  - *Superseded text:* D6 ("the input dispatch does no row work") and R5's
    D21 rejection ("the input dispatch does no row work") now mean no
    **bound** row work; the one unbound chunk is D23's exception, which
    creates and destroys no widget (A118).
  - *Bound:* the input dispatch stays within `B_INTERACT_US`. At the
    skip-cache rate (about 0.18 µs a row) one 8,192-row chunk is about
    1.5 ms and a 16,384-row chunk about 3 ms, beside the 0.2 ms input and
    D22's projection. If task 10.7's round shows the
    input over `B_INTERACT_US`, the chunk shrinks to the largest power of
    two that fits, and the frame table is re-derived.
  - *Bound, corrected (R12):* the skip-cache rate is the rate only if D24 is
    taken. If D24 is void the chunk keeps the path-cache splice, and 104's
    inferred unbound phase (75–90 ms for about 130,865 rows) is
    0.57–0.69 µs a row: about 4.7–5.6 ms an 8,192-row chunk and 9.4–11.3 ms
    a 16,384-row one. And the input dispatch also carries D6's intent and
    D22's hiding of about 207 rows, which no round has measured yet. So the
    deciding Q5 round admits a chunk size for D23 only when its worst chunk
    turn, plus 104's worst input dispatch (0.245 ms), plus A123's measured
    time to hide the band, fits `B_INTERACT_US` (task 10.12); task 10.7's
    round on the reworked binary, which measures the input dispatch whole,
    has the last word.
- **D24 (new). The path cache leaves the chunk.**
  - *What:* a chunk that drains a store whose parent's path cache that store
    owns no longer removes its rows' `item_locations` and
    `visible_path_counts` entries one by one. At the collapse's intent the
    parent's cache is detached whole: one map removal and a generation
    advance. The detached paths go to the listing retirement, which removes
    their entries in slices after the collapse completes, as it already
    retires the mirror rows.
  - *Correctness:* a lookup for a path under the detached parent misses from
    the intent on. A location records its cache generation, and a lookup
    confirms it against the live cache. The retirement removes an entry only
    if it still carries the retired generation, so a re-expansion that
    re-caches the directory before the retirement ends keeps its entries.
  - *The rest of the cache (R12).* `item_locations` is not the only map the
    splice keeps. `visible_path_counts` and `item_locations` are flat,
    section-wide maps keyed by path (`imp.rs`), and only `child_paths` is per
    parent, so "one map removal" detaches `child_paths[parent]` alone. D24
    therefore owes, before code, these invariants, each with a failing-first
    test in task 10.15:
    - *Counts describe live occurrences.* From the intent on, no reader of
      `visible_path_counts` (`visible_path_is_ambiguous`,
      `cache_item_location`, `forget_visible_path_occurrence`) counts an
      occurrence under a detached parent. Without it, a re-expansion before
      the retirement ends re-counts every child to 2, drops every location
      as ambiguous (`cache_item_location`), and the retirement's decrements
      then call `restore_unique_item_location`, a walk of every cached
      sibling list, once a path. A path the other overlapping folder still
      shows must be locatable during the collapse and after it.
    - *Every reader confirms the generation:* `rename_cached_item`,
      `remove_cached_item`, `shift_cached_indices`, the scan and refresh
      lookups, and `capture_subtree`.
    - *Order and nesting:* the intent detaches the cache of every store in
      the subtree (one removal each), after `capture_subtree` and D6's
      selection move have read the locations they need. A later collapse
      that takes over this one (task 9.4) finds the taken-over stores through
      the collapse records, not through the detached caches.
    - *No fallback per chunk:* a chunk on a store whose parent cache is
      detached skips the cache whole. It never reaches
      `drain_child_rows_cache`'s consistency fallback, which would see
      `child_paths[parent]` missing and re-cache the store from its mirror
      on every chunk: `cache_full_rebuilds` stays 0 across the collapse.
    - *No per-row work inside a step:* if no shape of the counts meets the
      first invariant without per-row work in the steps (duplicates arise
      only from overlapping workspace folders, which may bound that work to
      paths counted more than once), D24 is void, and the next lever that
      Q1 names applies, as below.
  - *Condition, pre-registered:* D24 is taken if round Q2 shows the
    path-cache splice costing at least one frame (16.7 ms) of 104's median
    0:131,072 completion. If it costs less, D24 is void, and the
    largest LushText cost that round Q1's profile names in the unbound phase
    is cut instead, under the same failing-first discipline.
  - *As built (task 10.15, 2026-11-19).* D24 is taken and implemented in
    `workspace_section/scan_execution/path_cache_detach.rs`, with these
    decisions, each recorded with its evidence in task 10.15:
    - *The cache generation is a section-wide epoch.* Every location carries
      the epoch it was cached at (`ItemLocation::epoch`; equality ignores
      it), and `detached_child_caches` maps a detached parent to the last
      epoch it retired. A location under that parent is live only when its
      epoch is newer. Every reader goes through one accessor
      (`live_item_location`) or the stale-unique rule below:
      `rename_cached_item`, `remove_cached_item`, `shift_cached_indices`,
      `capture_subtree`, and the splice and rebuild paths.
    - *Duplicates are excluded, not counted.* A subtree one of whose
      directories two live stores can list is not detached
      (`subtree_is_listed_twice`: another workspace folder's root lies
      strictly inside it, or it lies inside two folders, one inside the
      other): it keeps the per-step splice, which keeps those paths, counted
      twice, exact, and the cache of a shared directory with the store that
      stays. (The first version tested only the first shape; a review found
      the second, where the collapsing occurrence's store owned the cache the
      other occurrence still lists.) Every detached path therefore has a count of one, so the counts
      need one rule: a count of one whose location is stale (or absent under
      a detached parent) is dropped before it is read or re-counted
      (`drop_stale_unique`). The first invariant then costs nothing per row
      inside a step, and D24 is not void.
    - *An empty cache detaches nothing.* A store with no cached path records
      no detach, so nothing waits on a retirement that never runs (a
      property-test counterexample, now its regression seed).
    - *The re-expansion adopts the cache in slices (new invariant).* A
      re-expansion before the retirement ends would otherwise re-cache the
      whole mirror in one dispatch through the consistency fallback (40 to
      100 ms at 131,072 rows). Instead the refilled directory's store takes
      ownership at once and a `G_PRIORITY_DEFAULT_IDLE` source caches the
      mirror's rows again, `RETIRED_LISTING_ROWS_PER_UNIT` (4,096) per unit
      within `turn_continues`; the refill's scan result is applied after it
      ends. A mirror change abandons the adoption, and the consistency
      fallback then repairs the cache as before.
    - *The retirement keeps the re-cached entries.* It removes a location
      only while its epoch is at most the retired one, and a count only
      while no location is left behind a count of one, so a re-expansion
      that re-cached a path keeps it; a later detach of the same directory
      outlives an earlier retirement.
    - *The overlap invariant, read (2026-11-24).* "A path that a second
      overlapping folder still lists is located during the collapse and
      after it" holds for the first shape only: the other folder's root is a
      top-level row, located through `folder_paths`, which no child step
      touches. It does not hold for the second: a directory has one cache
      owner, the store that loaded last, so when that occurrence collapses
      its steps splice its paths out (nothing is detached), and the
      surviving occurrence lists every entry with no cache of its own until
      its next batch re-caches it from its mirror (the consistency check of
      `splice_child_rows_cache`). The same holds without D24, and on `main`,
      by construction. Not changed, for the user's sake: every row operation
      through the survivor still lands. A delete walks the stores behind a
      cache miss (`remove_from_model`), and a rename projects through the
      row it was issued for (`file_item.set_path`), the cache update being
      a no-op on a miss.
      `test_collapsing_one_of_two_listed_occurrences_detaches_nothing` now
      deletes two entries through the survivor after the collapse and
      asserts that each row leaves its listing and the survivor stays
      expanded (`impl1/overlap-survivor-2.log`; a first version that also
      asserted the survivor cached again after a delete failed,
      `impl1/overlap-survivor-1.log`: the store walk re-caches the listing
      but does not make the survivor the owner, so the evidence reports no
      owned cache until its next batch). The cost left is a store walk per
      operation on an uncached survivor, bounded by the directory's listing,
      in a case that needs two overlapping folders.
- **D18's constant, re-chosen under the margin.** `COLLAPSE_DESTROY_ONLY_ROWS`
  candidates are 64, 128 and 256. Task 10.7 takes the largest whose worst
  step fits `B_INTERACT_US` and whose worst frame fits the 12 ms of task 10.4.
  At 104's lighter rows a 64-row destroy-only frame is 4.5 ms, 3.9 ms of it
  layout, so 128 may fit; round Q3 measures it. Every frame it saves is
  16.7 ms of completion in every case with nothing after the subtree.

**What stays.**
- D4's order, D17–D19, D20, D21's rejection and the one-unit-per-frame
  token are unchanged.
- R3's and R9's bars stay the verdict's bars, unrounded.
- R9's statement that a very large directory finishes later than `main`'s
  frozen collapse stays true of the **model** (the scrollbar and readiness).
  With D22, the **picture** a user sees is final in the first frame
  wherever no row follows the subtree in the flattened list and the
  directory row is on screen.

#### Instruments and bars

- **Completion** (`shown_us`, R3) is unchanged, and so is its bar, R3's plus
  R9's allowance. It still records the frame after which the directory row
  is GTK-collapsed, which is when the scrollbar and the model are final. The
  change does not pass by redefining it.
- **Picture final** (`picture_final_us`, new; task 10.16).
  - *Snapshot:* after each frame dispatch, the verdict test records the rows
    the viewport draws. A row counts when it is a list-view child (the row
    widget D22 hides) with opacity above 0 that intersects the viewport; it is recorded with its
    path and its rendered top relative to the viewport.
  - *Figure:* the end of the first frame dispatch from which every later
    snapshot until settled equals the last one. The scrollbar is excluded,
    because completion owns it. `main`'s side runs the same instrument; with
    nothing hidden there, it equals `main`'s completion.
  - *Bar, when no row follows the subtree in the flattened list and the
    directory row is on screen at the input* (0:N in the verdict test): the
    first frame dispatch after the input, within `INPUT_TO_FRAME_US`. The
    condition is not "no row after it in the band": with 400 rows after a
    subtree that fills the band, those rows are outside the band at the
    input and still arrive only at completion (the walk above), and with the
    directory row above the viewport the viewport stays empty until
    completion.
  - *Bar, otherwise* (400:N, the header's first root, scrolled inside the
    children): the same case's completion bar.
  - *Printed:* each case's distance to `main`'s completion, the figure this
    revision is judged by.
- **Selection** uses the margin above on every completion-bar figure (not
  on the ordinal first-frame bar); the final campaign
  uses the bars.

#### Attribution rounds queued (branch only; `MEASURE-QUEUE.md`, "R11 design attribution")

All six use the saved binary `rev5/bin/widget-attr-104`, whose switches
need no build. They run in the coordinator's quiet window, one round at a
time, at a 1-minute load below 4.

The perf-prefixed form has never run through the fixed `run-bin.sh`, which
now goes through `scripts/headless-session`: a launch failure there is
void, not a result. Task 10.12 reads the rounds before any code of D22–D24. Q4 runs last, because its gate reads Q2 and Q5a/Q5b.

| round | factor varied | cases | purpose and pre-registered use |
|---|---|---|---|
| Q1 `slc-r11-pdw-104` | none (DWARF profile, 4 kHz, 16 KiB stacks) | 0:131,072, one rep | 104's split by phase: unbound, tail, retirement, evidence. It names D24's target if Q2 voids the path-cache one |
| Q2 `slc-r11-skipcache-104` | `SLC_SKIP_CACHE=1` | 0:131,072 and 400:131,072, three reps | the path-cache splice's share on the real code. D24 is taken if the median 0:131,072 completion falls by at least 16.7 ms |
| Q3 `slc-r11-d128-104`, `slc-r11-d256-104` | `SLC_DESTROY=128`, then `256` | collapse, 0 rows after, at 256, 4,096 and 131,072, three reps | whether a larger destroy-only unit cuts the tail by whole frames with the step within 8 ms and the frame within 12 ms. It bounds task 10.7's candidates |
| Q5a `slc-r11-c16k-104` | `SLC_CHUNK=16384` | collapse at 0:131,072 and 400:16,385, three reps | a 16,384-row chunk's turn (the worst step) against `B_INTERACT_US`, with the path-cache splice still in the chunk |
| Q5b `slc-r11-c16k-sc-104` | `SLC_CHUNK=16384 SLC_SKIP_CACHE=1` | the same | the same turn without the splice D24 removes. **Which decides:** Q5b if D24 is taken (Q2), because that is the chunk the reworked code runs; Q5a if it is not. The deciding round admits a 16,384-row chunk for D23 when its worst step fits `B_INTERACT_US` |
| Q4 `slc-r11-base-104` | none (constants 32 / 64 / 8,192) | every collapse case and the header, three reps | the real code's completion per case. **Gate:** for each case, predict the reworked completion as Q4's worst, less one frame where D23 applies (the subtree has unbound rows, and they fit one chunk of the size the deciding Q5 round admits: 16,384 if its worst step fits `B_INTERACT_US`, else 8,192), less D24's credit: Q2's median gain for the same rows-after count at 131,072 rows, and that gain scaled by `R / 131,072` for a smaller subtree (the splice it removes is per row), so 16,385 rows get about an eighth of it, and, for a case with no row after the subtree, less Q3's credit when Q3 admits a larger destroy-only unit (R12): Q4's median less Q3's median at the same case for the largest admitted unit, with 0:16,385 taking the 0:4,096 figure (the same band, so the same frames saved) and 0:64 none (it has no destroy-only unit). If any prediction exceeds bar − M, the change returns to design before D22–D24 are coded. Task 10.4's 12 ms acceptance is read here too (unit 32, worst frame at 400:256) |

The queued "constants round (task 10.7)" on 104 is superseded: its first
and fourth rounds are Q4 and Q3, and the rest would choose constants for code
this revision changes. Task 10.7 runs on the reworked binary.

#### Ticked boxes this revision voids

- **Task 10.9** (the guard that the input dispatch does no row work, and
  implementation decision 18's interval) is reopened: D23 has the input
  dispatch take one unbound chunk, and the guard now asserts that and that
  the first frame takes the first unit where the chunk covers the unbound
  rows.
- **Task 8.3** (accessibility and automation docs) is reopened for D22's
  hidden rows.
- **Task 10.10** is closed: its two rounds are recorded above.
- Tasks 3.2, 5.2, 5.7 and 9.5 stay true as written. D22 adds tests and code
  beside them (tasks 10.13 and 10.14), and changes neither the expander
  class nor its CSS, because hiding uses no CSS. One test in 3.2's list
  changes with D23: `test_collapse_shows_in_the_first_frame` asserts the
  projection "before any row is removed", which becomes "before any bound
  row is removed"; task 10.9 revises it with D23's guard.
- D6 and R5's D21 rejection are annotated where D23 is decided (above);
  the decisions themselves are unchanged for bound rows.

#### Attribution results (task 10.12, 2026-11-18): the gates, and the return to design

**The rounds.** The coordinator ran Q1–Q5b and Q4 on 2026-11-18 between
07:49 and 08:24 on `rev5/bin/widget-attr-104`, through `scripts/measure.sh`
on the reference CPUs 8–23. Every kept try has a 1-minute load below 4
before and after it (`rev/attr2/measure.log`, `progress.log`). Q2 and Q4
each had one try discarded because its load reached 4.02 or 4.27. Their logs
hold the second try (08:09 and 08:22, as their mutter timestamps show). No
round failed to launch. The perf-prefixed Q1 ran through the fixed
`run-bin.sh` and printed its verdict line.

The readers are in the handoff's `rev/r11/`:
- `gates.py` applies the gates below to the logs, and `gates-output.txt` is
  its output;
- `q1cls.py` classifies Q1's samples, and `q1-104-window.txt` is its output,
  with the extracts `q1-104-leaf.txt` and `q1-104-chain.txt` (GTK thread
  tid 838398, window 924329.206817–924329.370666 s).

**Q1: 104's 0:131,072 window.** Completion is 164.2 ms. It runs under
`perf`, so it is attribution only: its 11.7 ms step comes 19 µs after the
input, the first unbound chunk, and is perf's overhead on that turn. The GTK
thread has 558 samples, 139.5 ms of CPU. 233 of them carry no call chain
(perf: "lost 1 chunks") and are classed by their leaf symbol alone.

| phase, ms from the input | what runs | CPU |
|---|---|---|
| 0–97, unbound | the path-cache splice: `PathBuf` equality, the path SipHash, `Path::components`, the `item_locations` `remove_entry` (10 samples), and a `remove_entry` on a `HashMap<PathBuf, gio::ListStore>` (14 samples, 3.5 ms; the section's only such map is `retained_child_stores` in `imp.rs`, which is not one of D24's maps, and its callers are not in the chains). 222 samples | 55.5 ms |
| 0–97, unbound | GTK's tree-list-model node removal and the store splice | about 20 ms |
| 0–97, unbound | frees, and other work with no attributable caller | about 17 ms |
| 97–164, band tail | five band dispatches, at 97.6, 114.9, 132.6, 144.6 and 161.4 ms, the last the terminal `collapse_row`. Between them, frees with no call chain (the listing retirement's drops, by their leaves `g_object_unref` and `_int_free_chunk`) | about 24 ms of frees, 7.3 ms band teardown, 6.8 ms frame |
| whole window | the evidence instruments: none among the 325 samples with a call chain (a floor, since evidence and retirement are classed by chain) | 0 |

So the unbound phase is about 97 ms here, against the skip-cache build's
30 ms. More than half of it is the path-cache splice, which confirms R11's
inference (75–90 ms) and names D24's target.

**The tail's frame count, against R3's.** At 0:131,072, destroy-only unit 64,
Q1 counts **five** band dispatches. That is four units and the terminal, one
more than R3's `ceil(175 / 64) + 1` = 4, as in the skip-cache window. With
rows after the subtree, Q4's frames over 8 ms at 400:4,096 (unit 32) fall at
about 13, 26, 43, 59, 76 and 110 ms, with one more at about 93 ms under 8 ms.
That is consistent with seven band frames, R3's `ceil(175 / 32) + 1` = 7.
`over_interact` lists only dispatches over 8 ms, so the frame at about 93 ms
is inferred from the 17 ms cadence, not read from the log.

The log has no frame over 8 ms in the first 13 ms after the input at
400:4,096, and the first two recorded frames are only about 13 ms apart. At
400:64, which has no unbound rows, a frame over 8 ms follows the input at
once (at about 0 and 16 ms). That is consistent with R11's reading for D23:
at 400:4,096 the first frame's tick finds only unbound rows, which wait for
the between-frames source, so that frame takes no unit, and the input's
chunk would buy it back. The log does not show that light first frame
directly, and 400:4,096 passes the gate only on that credit, so task 10.9's
guard test (the first frame's tick takes the first bound unit) and task
10.7's round on the reworked code must confirm it.

R11's frame table is corrected by these counts:
- 0:N takes `ceil(207 / unit) + 1` band frames;
- 400:N takes `ceil(175 / unit) + 1`, plus the lost first frame without D23;
- the "(a)" row's 400:4,096 completion is Q4's prediction below, 105.2 ms
  with D23, not R11's 105–110 ms estimate.

**Q2: the path-cache share. D24 is taken.** At 0:131,072, Q2's median is
95.1 ms.
- Against the go/no-go's median of 157.0 ms (the queue entry's reference) the
  gain is **62.0 ms**, at least 16.7 ms.
- Against Q4's median of 158.9 ms (the same session) it is **63.9 ms**.
- The gate below uses Q4's figure, because only Q4 has a 400:131,072
  baseline. With 62.0 ms, 0:131,072 predicts 88.0 ms, which also passes.
- At 400:131,072: Q4's 216.8 ms against Q2's 142.2 ms, a gain of **74.6 ms**.

Q2's worst step is 7.200 ms. Its worst frame is 15.7 ms at 400:131,072 in
one repetition, and 9.4–9.6 ms in the other two.

**Q3: the destroy-only unit. 128 is admitted; 256 is not.**
- Unit 128: worst step 7.325 ms and worst frame 8.487 ms, within 8 and
  12 ms. Medians are 55.7 ms at 0:256, 55.4 ms at 0:4,096 and 144.5 ms at
  0:131,072, against Q4's 71.2, 77.8 and 158.9 ms.
- Unit 256: worst frame 8.159 ms. Its worst step is 8.925 ms, with 8.402 ms
  in another repetition, both at 0:131,072, so it is over 8 ms by the rule
  as written.
- Those two steps are the first between-frames dispatch, 14–25 µs after the
  input. That is the first 8,192-row unbound chunk with the path cache in it,
  the turn Q4 also shows at 8.8 ms with unit 64, not a destroy-only unit.
  D24 removes that work, so task 10.7 may measure 256 again on the reworked
  code. Until then, Q3 bounds its destroy-only candidates to 64 and 128.

**Q5: D23's chunk. 16,384 is not admitted.**
- Q5a (path cache in): worst step 15.370 ms.
- Q5b decides, because D24 is taken. Its worst step is 7.985 ms (400:131,072).
  Plus 104's worst input dispatch (0.245 ms) that is 8.230 ms, over
  `B_INTERACT_US` before A123's hiding time is added.
  - `step_us_max` cannot tell a chunk turn from a bound unit in that case.
  - The rule reads the worst step, so it is applied as written.
- The fallback is 8,192, under the same sum: Q2's worst step 7.200 ms plus
  0.245 ms is 7.445 ms. So 8,192 is admitted only if A123 measures the band's
  hiding at 0.555 ms or less. Otherwise the chunk is the next smaller power
  of two that fits.
- 400:4,096's unbound rows (about 3,889) fit a 4,096-row chunk too, so its
  D23 credit below holds for any admitted chunk of 4,096 rows or more.

**Q4: the completion gate passes in every case.** Each prediction is Q4's
worst completion, less:
- one frame (16.667 ms) where D23 applies: unbound rows that fit one chunk,
  taken here at 8,192;
- D24's credit: Q2's gain for the same rows-after count, scaled by
  `R / 131,072`;
- for 0:N, Q3's median gain at unit 128 (0:16,385 takes 0:4,096's, 0:64
  none).

| case | Q4 worst | D23 | D24 | Q3 | prediction | bar − M | |
|---|---|---|---|---|---|---|---|
| 0:64 | 7.7 | – | 0.0 | – | 7.6 | 83.3 | pass |
| 0:256 | 75.5 | 16.7 | 0.1 | 15.6 | 43.1 | 83.3 | pass |
| 0:4,096 | 79.5 | 16.7 | 2.0 | 22.4 | 38.5 | 83.3 | pass |
| 0:16,385 | 78.3 | – (does not fit 8,192) | 8.0 | 22.4 | 48.0 | 83.3 | pass |
| 0:131,072 | 164.3 | – | 63.9 | 14.4 | 86.1 | 140.6 | pass |
| 400:64 | 26.8 | – | 0.0 | – | 26.8 | 83.3 | pass |
| 400:256 | 123.2 | 16.7 | 0.1 | – | 106.4 | 137.6 | pass |
| 400:4,096 | 124.2 | 16.7 | 2.3 | – | 105.2 | 120.6 | pass, **only with D23** (121.9 without it) |
| 400:16,385 | 125.9 | – | 9.3 | – | 116.6 | 122.6 | pass |
| 400:131,072 | 218.3 | – | 74.6 | – | 143.8 | 197.9 | pass |
| header, two roots of 4,096 | 92.6 | – | – | – | 92.6 | 131.6 | pass |

400:4,096's pass depends on D23's chunk being admitted at 4,096 rows or
more, and so on A123's measured hiding time (Q5 above). That dependency is
stated here, not assumed.

**Task 10.4's acceptance is missed, and the change returns to design.**
- **The rule:** R11's Q4 row reads task 10.4's acceptance, which R5's D17
  pre-registered: "unit 32's worst frame at 400:256 at most 12 ms. If it
  does not, the change returns to design with that round".
- **The result:** Q4's worst frames at 400:256 are **10.973 / 12.787 /
  12.166 ms**. Two repetitions and the median are over 12 ms; layout is
  12.0 ms of the 12.8 ms frame. R11 defines no credit for this figure, and
  none is taken.
- **What the lighter rows bought:** in the stage 4 campaign (before task
  10.4), the rows-after collapses at unit 32 had frames of 16.8–29.9 ms. The
  rows are lighter now, but not by D17's margin.
- **The other rows-after frames in the same rounds,** recorded as data for
  the design stage, not as a waiver:
  - Q4 (unit 32): 400:64 9.6–14.7 ms, its first frame;
    400:4,096 11.1–12.8 ms; 400:16,385 10.6–12.2 ms;
    400:131,072 13.2–14.5 ms;
  - Q5a and Q5b: 400:16,385 9.4–9.9 ms;
  - Q2: 400:131,072 9.4 / 9.6 / 15.7 ms.

  Every one is within `B_SETTLE_US`; none of the 0:N frames reaches 9 ms.

**Consequence.** Following the rule as written:
- No code of D22–D24 and no A123 probe is written.
- Task 10.12 is done; task 10.4 stays open with this miss.
- The change returns to design with these rounds.

The design stage owns the next step: the frame figure D17 promised, or a
re-registered acceptance with its rationale. Every completion prediction
passes, D24 and unit 128 are admitted, and D23's chunk waits for A123's
hiding time. These results carry over to whichever mechanism the design
keeps.

### R12. Review of R11 (2026-11-18)

Reviewed against the code, the GTK 4.24.1 sources in the handoff
(`gtk-src-4.24.1/`), the go/no-go and skip-cache logs and the queue entry.
The recorded figures hold: the go/no-go's three completions are
`input_us + shown_us` = 157.0 / 156.1 / 157.6 ms at 1-minute loads of 3.38
before and 3.86 after (`rev/attr2/measure.log`); the skip-cache window and
its buckets are `rev/r11/skipcache-window.txt`; the six rounds are PENDING
and the old constants round SUPERSEDED in `MEASURE-QUEUE.md`. The decision
(a) + (c) stands. Corrected in place:

1. **D22 hid the wrong widget.** The overlay is the list item's child; the
   row widget around it draws the hover, selected and focus styling, owns the
   click gesture and is the presented list item. Opacity on the overlay
   alone would leave a hovered hidden row painted and clickable, and
   `pick` would return the row, refuting the axiom by construction. D22,
   A123, the delta specs and tasks 2.7, 10.13 and 10.16 now name the row
   widget, and A123 proves the pointer with a real press and hover.
2. **GTK writes `hidden` itself.** `gtk_widget_map` writes it false after
   the `map` emission, and a created row is bound before it is inserted, so
   bind-time writes do not survive on created rows or after an unmap and
   map. D22 re-applies it from the collapse's next pass, A123 records the
   write, and task 10.13 tests a created row and an unmap–map cycle. A123's
   read-back of a state it just set proved nothing; it now reads the
   presented tree.
   *(R13, D25: the re-apply runs at the end of the step that created the
   row, or from a one-shot step source its bind or the list's `map` arms;
   no pass runs on the frame tick.)*
3. **Membership by path prefix.** Overlapping workspace folders list one path
   twice; D22's hiding, the R6.5 projection and `stop_subtree_work` decide
   by tree position instead (task 10.13, with the two-folder fixture).
4. **Keys after a held key.** Down then Return would have activated the
   directory row; a row-dependent key after a held key is held behind it,
   every other key propagates (D22, task 10.14), and replayed navigation has
   a parity test against GTK's bindings, Page Up and Page Down included.
5. **D23's admission.** It quoted the skip-cache rate only; without D24 a
   16,384-row chunk is about 9.4–11.3 ms on its own, and the input also
   carries D22's hiding. The deciding Q5 round now admits a size only with
   the input's other work added (task 10.12).
6. **D24 covered `item_locations` only.** The counts, every reader, the
   subtree's nested caches, the order against `capture_subtree` and D6, the
   take-over and the per-chunk fallback are now invariants with tests (task
   10.15), or D24 is void.
7. **The frame count.** R3's `ceil(175 / unit) + 1` and the five measured
   tail dispatches disagree by one; the 400:N predictions are marked as not
   evidence, and the measured gate decides (above).
8. **Q4's gate omitted Q3.** A larger destroy-only unit, which Q3 measures,
   saves whole frames in the 0:N cases; the gate now credits it, so a case
   Q3 rescues is not sent back to design.
9. **Spec text D23 contradicted.** "Unbound rows leave between frames" and "A
   huge directory" now name the input's chunk; task 8.1 declares the new
   evidence counters, and task 8.3 adds the Orca checklist.

### R13. The rows-after frame: the step leaves the frame (2026-11-18)

**What this answers.** Task 10.4's acceptance, pre-registered by R5's D17
("unit 32's worst frame at 400:256 at most 12 ms"), missed in round Q4
(10.973 / 12.787 / 12.166 ms; R11, "Attribution results"), and every other
400:* frame there sat at 9.6–14.7 ms. This revision attributes that frame
from four rounds the coordinator ran for it, decides the mechanism, keeps the
12 ms bar, pre-registers the margins for every case, and writes the fallback
ladder and the closing rule so that a failed gate does not cost another
design loop. `origin/main` had not moved since `bc109b04` (already merged).

#### The rounds read

The coordinator ran four branch-only rounds on `rev5/bin/widget-attr-104`
on 2026-11-18, 08:47–08:53 (`MEASURE-QUEUE.md`, "frame attribution with rows
after the subtree"), each through `scripts/measure.sh` on the reference CPUs
8–23 at a 1-minute load of 2.80–3.98 before and after
(`rev/attr2/measure.log`). None failed to launch. The readers and their
output are in the handoff's `rev/r13/` (`README.md`, `summ-output.txt`,
`trend-output.txt`, `pdw-400-256-dispatches.txt`).

| round | factor | case: worst frame per rep (ms) | worst step per rep (ms) | completion per rep (ms) |
|---|---|---|---|---|
| `slc-r13-pdw-400-256` (DWARF, 1 rep) | none, under `perf` | 400:256: 8.7 | 4.2 | 115.4 |
| `slc-r13-u16-400` | `SLC_UNIT=16` | 400:64: 5.5 / 10.2 / 11.0; 400:256: 9.7 / 10.6 / 6.6 | 2.7 / 7.2 / 7.5; 6.4 / 4.6 / 3.1 | 45.7–51.2; 211.9–214.9 |
| `slc-r13-u64-400` | `SLC_UNIT=64` | 400:64: 8.3 / 8.9 / 9.0; 400:256: 15.6 / **17.8** / 16.0 | 4.1 / 4.6 / 4.5; 7.9 / **10.0** / 8.0 | 18.2–19.8; 72.6–76.9 |
| `slc-r13-d128-400` | `SLC_DESTROY=128` | 400:64: 8.3 / 8.7 / 8.3; 400:256: 9.0 / **14.4** / 9.8; 400:4,096: 9.4 / 9.5 / 9.5 | 4.0 / 4.2 / 4.1; 4.3 / **9.6** / 4.7; 4.5 / 6.5 / 7.5 | 17.4–18.1; 115.4–120.3; 116.4–117.6 |
| Q4 `slc-r11-base-104` (for comparison) | none | 400:64: 9.6 / 12.9 / **14.7**; 400:256: 11.0 / 12.8 / 12.2; 400:4,096: 11.1 / 12.0 / 12.8 | 4.0 / 7.2 / **9.1**; 4.3 / 5.4 / 5.8; 4.7 / 5.1 / 5.7 | 24.8–26.8; 119.3–123.2; 121.7–124.2 |

**What the instrument's "layout" spans.** The verdict test's dispatch
timeline (`workspace_tree_virtualization.rs`, `dispatch_timeline_shown`)
times a frame dispatch from the start of the main-loop iteration to the end
of the frame clock's `layout` signal, and the collapse's tick callback runs
in the clock's update phase before it (`collapse_execution.rs`,
`arm_collapse_source`). So `worst_frame_layout_us` contains the bound step:
a unit frame is the step (teardown of the removed rows, creation and bind of
the rows entering the band), then the layout proper (CSS, measure, allocate
of the new rows), then the paint. In GTK 4.24.1 the entering rows' widgets
are created inside the removal's `items-changed` emission
(`gtklistitemmanager.c`, `gtk_list_item_manager_model_items_changed_cb`
calls `gtk_list_item_manager_ensure_items`, which calls `create_widget` for
a tile with no widget), and removed rows' widgets are kept only for the same
item (`gtk_list_item_change_release` keys them by item;
`gtk_list_item_change_get` reuses a removed widget only for that item, else
one recycled from outside the tracked range, else creates one), so a
removal that brings rows in creates every entering widget in the step. An
inserted row is mapped at insertion when its parent is mapped
(`gtkwidget.c`, `gtk_widget_reposition_after`, "Enforce mapped
invariants").

**Attribution, by question.**

1. **The rows-after frame scales with the rows a dispatch brings into the
   band; it is not a fixed cost of the band.**
   - The profile (400:256, unit 32, 209 GTK-thread samples at 0.25 ms) has
     seven band dispatches, at 13.3, 25.7, 41.5, 57.4, 73.7, 90.1 and
     106.9 ms: five units of 32 rows, one of 14 rows (the rest of the 174
     bound rows above `DIRECT_COLLAPSE_ROWS`), and the terminal collapse of
     the last 32. The six 32-row dispatches hold 7.50–8.50 ms of CPU each,
     the 14-row one 3.75 ms: about 0.24 ms a row and about 0.4 ms fixed.
   - Across adjacent rounds in fresh processes (the advisor's reading, not
     a fit through `u16`): unit 32 (`d128`, median 9.8 ms) against unit 64
     (median 16.0 ms) at 400:256 is about 0.19 ms a row over about 3.6 ms.
     The two readings differ in the fixed part (0.4 against 3.6 ms) but
     agree that most of a 32-row frame is per row.
   - Each 32-row dispatch's samples split as about 25–35 % step
     (`items-changed`, `drain_child_store`, `set_expanded` in the chain),
     40–50 % layout (allocate, measure, CSS), 5–10 % paint, and about 20 %
     with a truncated chain; widget teardown sits under `items-changed` in
     29 of 32 attributable samples. The profile is of a fast repetition
     (8.7 ms worst frame), so it attributes a typical frame, not a slow one.
   - `u16` is confounded and is not a fallback: its 16-row steps took up to
     7.2–7.5 ms where `d128`'s 32-row steps took 4.0–4.2 ms at 400:64, and it
     completes 400:256 in 211.9–214.9 ms against a 154.3 ms bar. `u64` fails
     the frame (15.6–17.8 ms) and the step (7.9–10.0 ms). Neither unit is a
     rung of the ladder below.
2. **Destroy-only 128 does nothing with rows after, by construction.**
   `collapse_removal_is_destroy_only` (`ui/sidebar/policy.rs`) requires
   `rows_after == 0`, the flattened rows after the last realized one
   (`collapse_execution.rs`, `step_facts`), which 400 rows after the subtree
   never reach. `slc-r13-d128-400` is therefore a second unit-32 baseline
   for 400:*. That gives seven unit-32 repetitions at 400:256 (8.7; 9.0 /
   14.4 / 9.8; 11.0 / 12.8 / 12.2 ms) and six single-unit repetitions at
   400:64 in fresh processes (8.3–9.0 ms) against Q4's 9.6 / 12.9 / 14.7.
3. **Q4's frames are higher than the same work in other rounds, and case
   order is the leading hypothesis (not established).** Every band frame of
   Q4's 400:* series is 9.0–14.7 ms (its `over_interact` lists), against
   8.0–9.8 ms, with one 14.4 ms, for the same unit-32 work in
   `slc-r13-d128-400`, where 400:64
   is the process's first case. 400:16,385 is 9.4–9.9 ms as the third case
   of Q5a and Q5b and 10.6–12.2 ms as Q4's ninth; 400:4,096 is 9.4–9.5 ms as
   `d128`'s third and 11.1–12.8 ms as Q4's eighth. The verdict test builds
   one window per case (`tree_with_siblings`, through `LushtextWindow::new`
   on the test application) and ends each case with `drop(tree.window)`,
   which drops a Rust reference only: nothing closes or destroys the
   window, so every earlier case's toplevel, with its sidebar, stays alive
   and presented. Whether those windows (or LushText state they leave)
   cost the later cases' frames, or the difference is the session, is not
   separable from these rounds; round Q6 below separates order from
   session, and a build-side round separates the windows from LushText's
   own state.
4. **Process isolation alone would not meet 12 ms.** Even in the
   fresh-process round, one of three repetitions at 400:256 has a 14.39 ms
   frame, carrying a 9.585 ms step.
5. **Bound steps already break `B_INTERACT_US` at unit 32 (new finding).**
   R11's gates read the worst step of Q2, Q3 and Q5 only. The bound-unit
   steps in the 400:* cases reach 9.07 ms (Q4 400:64 rep 3, a case with no
   unbound rows, so a bound unit or the terminal collapse) and 9.585 ms
   (`d128` 400:256 rep 2, the first band dispatch, 12.9 ms after the
   input); typical steps are 4.0–6.0 ms. D2 requires every step within
   `B_INTERACT_US`, so unit 32 as built fails D2 in its tail, apart from the
   frame figure. The tail is not attributed: the profile has no slow
   repetition, and `u16`'s 7.2–7.5 ms 16-row steps suggest a part that does
   not scale with the rows.
   *(R14, read from the same logs.)* In `d128` 400:256 rep 2, the worst step
   and the worst frame are the same dispatch (both at 12.9 ms after the
   input), and the frame less its step is 4.8 ms (14.39 − 9.585). So under
   D25 it is the step tail, not the frame, that is most likely to miss.
   Q4 also has larger steps in non-frame dispatches right after the input:
   10.56 ms at 400:16,385 rep 1 (offset 0.1 ms), and 9.97, 8.1 and 9.1 ms at
   400:131,072. These are unbound chunk turns, which still carry the
   path-cache splice that D24 removes. They are G2's and G5's to re-read,
   not the bound tail's. The tail profile G0b (below) is queued now, on the
   binary that shows the tail, so S3's attribution does not wait for G1's
   build.

#### Decision D25 (new). The bound step runs in a dispatch of its own, between frames

- **What.** No collapse step runs inside the frame clock's dispatch. A bound
  unit (`RemoveBound`, `CollapseDescendant` of bound rows, the terminal
  `CollapseRow`) runs from a one-shot step source at `G_PRIORITY_HIGH_IDLE`
  (100), above the frame clock's `GDK_PRIORITY_REDRAW` (120) and below
  input (`GDK_PRIORITY_EVENTS`, 0). The source is armed:
  - by the input dispatch (D6), so the first unit runs before the first
    frame;
  - by each frame's `after-paint` on the list's frame clock while the
    collapse is pending, so the next unit runs right after that frame;
  - by the between-frames idle (D19) when its turn leaves the next step
    bound, so a unit that becomes next mid-interval runs before the next
    frame, as the tick takes it today;
  - by its own dispatch (R14): a dispatch that removed D23's chunk, or (under
    rung S1) a step that left rows in the interval's budget, re-arms the
    source before it returns when the next step is bound and the token is
    free. Without this, the first bound unit after D23's chunk would wait for
    the between-frames idle, which runs below the frame clock, and so would
    miss the first frame that D23's credit at 400:4,096 counts on.
  One source dispatch runs one step, plus D22's re-apply for the subtree rows
  that step created, and nothing else. The `after-paint` handler is the
  section's, connected to the list's current frame clock when the collapse
  arms its sources on a mapped list, at most one per section. It is
  disconnected when the queue empties, when the list unrealizes (the frame
  clock can change across a re-realize), when the section is disposed, and on
  every path that stops the tick (`stop_collapse_sources`). It is connected
  again on `map` while a collapse is pending (R14). The token is
  unchanged in meaning and key (R6.3): at most one unit of bound rows per
  frame interval, read from the list's frame clock counter (a unit runs only
  if no unit ran since the clock's last frame). The frame then lays out that
  one unit's rows and runs no step.
- **What keeps the frames coming.** The tick callback stays registered while
  a collapse is pending on a mapped list, but takes no step: it keeps the
  frame clock updating every vsync, so every frame emits `after-paint` and
  arms the next unit even when a step removed nothing (a removal's own
  `gtk_widget_queue_resize` would request a frame only when rows left), and
  it refreshes the starvation clock of D4. A starved clock still hands the
  steps to the `G_PRIORITY_LOW` fallback after
  `COLLAPSE_FRAME_STARVATION_MS`, and a starved unit counts against the
  interval like any other (R6.2's flag reads the same counter).
- **Why it meets the frame figure.** It moves the step (about a third of a
  32-row frame's samples, and 4.0–9.6 ms of measured step time) out of the
  frame and leaves the frame the layout and paint of at most
  `SIDEBAR_REALIZED_ROWS_PER_FRAME` new rows. The frame count is unchanged:
  unit *k* is laid out by frame *k*, as now, because the input arms the first
  unit and every frame arms the next. The bars do not move.
- **What it does not cure.** The step's own time is unchanged, and its tail
  (9.07 and 9.585 ms) is over `B_INTERACT_US`. The step ladder below owns
  that.
- **It is not D21.** D21 put the first unit into the input dispatch (its
  `B_INTERACT_US` carried the row churn) and risked two units in the first
  frame. D25 runs the unit in its own dispatch with its own budget, after the
  input dispatch, and the token still admits one unit per frame interval.
- **The first frame.** The work between the input and the end of the first
  frame is the same as the current design's (input, D23's chunk, one unit,
  the layout of its rows), only split into separate dispatches; when the
  vsync is not yet due, the unit runs in the idle time before it, so the
  first frame ends no later than today's and usually earlier.
  `INPUT_TO_FRAME_US` is still checked by the verdict test (D6's scenario).
  *Superseded by rung T1 (2026-11-24, "Rung T1 as built"):* the first frame
  now lays out the intent alone; D23's chunk still runs before it, and the
  first unit runs in the interval after it.
- **D23, moved.** The one unbound chunk of D23 runs in the first step-source
  dispatch after the input, not in the input dispatch, so the input
  dispatch does no row work again (D6 and D21's rejection as written before
  R11). The chunk is admitted by its own dispatch time (the ladder below),
  no longer by the input's time plus A123's hiding time.
- **D22's re-applied `hidden` (revises R12).** A row created by a step is
  mapped at its insertion inside that step (above), so the step re-applies
  `hidden` to the subtree rows it created before it returns. A row created
  or mapped elsewhere (a scroll's band change, the sidebar mapped again)
  arms the same one-shot source from its bind or from the list's `map`, which
  re-applies it. No pass runs on the frame tick, so D22 adds no per-frame
  work, which was the per-frame headroom the 12 ms bar kept. A row reports
  `hidden` from the end of the dispatch after the one that created or mapped
  it, no later than R12's "end of the frame after".
- **Pacing seams.** `CollapsePacingForTest::Hold` holds the step source too;
  `NoFrameClock` and the starvation fallback (D4, R6.2) are unchanged: with
  no frame there is no `after-paint`, and the fallback steps.
- **A shared priority (R14).** GtkSourceView's first highlight update also
  runs at `G_PRIORITY_HIGH_IDLE` (ledger A69, A85). If the user types in a
  highlighted document while a collapse runs, one main-loop iteration can
  dispatch both sources back to back before the next input. No gate
  measures that combination. It is recorded as a risk, not a bar, and the
  step's own 6 ms margin is what bounds it.
- **Axioms.** D25 relies on three toolkit facts, pinned as **A124** and
  **A125** (task 2.8) on GNOME 51 and the GNOME 50 floor:
  - A124: A118 already records that a removal creates and binds the
    entering rows' widgets inside its `items-changed` and never reuses a
    removed one for another item. A124 adds what D25 needs beyond it: run
    from an idle outside any frame clock dispatch, the removal leaves every
    entering widget mapped, with accessible `hidden` false, when the
    emission returns, and the next frame runs no setup or bind for them and
    only styles, measures, allocates and paints them.
  - A125: a `G_PRIORITY_HIGH_IDLE` one-shot source armed in a frame's
    `after-paint`, in an input dispatch, or in its own dispatch (the re-arm
    above), runs before the next frame clock dispatch even when that frame is
    already due. Read in that source, `gdk_frame_clock_get_frame_counter`
    equals the counter of the frame just painted, and it advances only when
    the next frame starts. The token reads the counter outside a frame
    dispatch, and today's tick reads it only inside one (R14).
  If either is refuted on a toolkit, the change takes the closing rule
  below with the current in-frame step.

#### The bars, kept; the margins, pre-registered for every case

The 12 ms figure is kept as R5 registered it, and no bar is re-registered.
Its rationale was independent of any measurement: a quarter of
`B_SETTLE_US` kept for the frame's other work and its variance. Every
selection and go/no-go round from here on (G1–G5 below, task 10.7) applies
these margins to **every repetition of every case** (the collapse cases 0:N
and 400:N at 64, 256, 4,096, 16,385 and 131,072, and the folder header),
fixed before G1 runs:

| figure | selection bar | final campaign (task 7.2) |
|---|---|---|
| worst frame | 12 ms (`B_SETTLE_US` less a quarter, R5) | `B_SETTLE_US` |
| input dispatch, every step, every chunk turn | 6 ms (`B_INTERACT_US` less a quarter, the same rule) | `B_INTERACT_US` |
| completion (R3 + R9) | bar − M, M = 16.7 ms (R11) | the bar |
| picture final (R11) | as R11 | as R11 |

The 6 ms step margin is new here, by the same rule as the frame's, because
the final campaign judges the worst of four rounds at 8 ms and unit 32's
steps already reach 9.6 ms. It tightens R11 and R12's "fits `B_INTERACT_US`"
for selection only; it relaxes nothing.

**Predicted frames under D25 (not evidence; G1 measures them).** A 400:*
frame keeps its layout and paint, about 55–75 % of today's frame by the
profile's split (the step is 25–35 % of the samples, 20 % unattributed). On
Q4's frames, the highest measured: 400:64 14.7 → 8.1–11.0 ms; 400:256 12.8 →
7.0–9.6; 400:4,096 12.8 → 7.0–9.6; 400:16,385 12.2 → 6.7–9.2; 400:131,072
14.5 → 8.0–10.9; the 0:N frames, 4.6–8.5 ms today, fall further.

#### Gates and their fallback ladder

Each gate names its first measurement and the rung that follows a miss, so
a missed gate moves down its ladder and does not return the change to
design. Every round is queued in `MEASURE-QUEUE.md` with its commands, cases,
factor and bars, and read by the stage that queued it.

- **G0, case order (queued now, Q6, no build).** Purpose: whether a case's
  frames depend on the cases run before it in the process. Reading,
  registered now: per pair, the median of the per-repetition worst frames at
  each 400:* size, "after" against "alone". *Confirmed* if, in both pairs,
  "after" exceeds "alone" by at least 1.0 ms at 400:256 and at three or more
  of the five sizes; *refuted* if the difference at 400:256 is under 0.5 ms
  in both pairs; otherwise *inconclusive*. If confirmed, task 11.2 makes the
  verdict test destroy each case's window, on both sides (the `main` patch
  too), failing-first, and a build-side round (windows destroyed, same
  order) tells the windows from LushText's own state; LushText state that
  grows across collapses is then a bug with its own failing-first test. Until
  that fix lands, and whatever G0 says, G1 is judged in Q4's full case
  order, the worst order measured.
- **G1, D25's frame and step (first implementation task, task 11.3).**
  Build D25 with a temporary `SLC_STEP_ROWS` hook (below) and queue round G1
  on that binary: the full collapse case list and the header, 5
  repetitions, Q4's order. Pass: every worst frame at most 12 ms, every step
  and the input at most 6 ms, `INPUT_TO_FRAME_US` held, and every completion
  at most bar − M after the credits of R11's gate (D24's from Q2, Q3's for
  0:N). A "step" here, and in the final campaign's `B_INTERACT_US` reading,
  is the **whole step-source dispatch**, not the inner step timer
  (`collapse_step_us_max`, `collapse_execution.rs`). That dispatch includes
  D22's re-apply, the token and the re-arm. The collapse times the source
  callback as a whole, and the verdict timeline tags each step-source
  dispatch's `took_us` by kind (bound unit, terminal, chunk), because
  `worst_other_us` also holds other non-frame dispatches (R14). Ladder:
  - **S1 is the expected path, not a fallback.** At 32 rows the measured
    steps already exceed the 6 ms margin (Q4 7.2 and 9.1 ms at 400:64,
    `d128` 6.5–9.6 ms at 400:*, Q3's 7.325 ms at destroy-only 128, Q2's
    7.2 ms), so G1's binary carries the per-interval row budget and the
    temporary `SLC_STEP_ROWS` hook from the start, and G1, G1b (16) and G1c
    (8) are queued as one entry on one build in one quiet window.
  - *Frames pass, a step misses:* **S1**, steps of 16 bound rows. The
    interval keeps its budget of `SIDEBAR_REALIZED_ROWS_PER_FRAME` rows, now
    removed in steps of at most a new `COLLAPSE_STEP_ROWS` (cap, rounded
    down), each its own step-source dispatch before the next frame;
    `DIRECT_COLLAPSE_ROWS` drops to the step size, so the terminal collapse
    is one step. That is a policy change (`next_collapse_step` takes the
    rows the interval has left; Kani C1–C5 and the shard re-run, task
    11.4; task 11.3's binary carries the same budget behind the hook).
    Measured by round G1b (`SLC_STEP_ROWS=16`). Its steps miss:
    **S2**, 8 rows (`SLC_STEP_ROWS=8`); a step that still misses 8 ms at 8
    rows is D2's existing toolkit-floor exception (A118, A124), recorded,
    and its 6 ms selection miss is recorded as data.
  - *The step tail, read first (R14):* **S3**. The DWARF profile round G0b,
    queued now beside G0 on `widget-attr-104` (400:64 and 400:256, 10
    repetitions, the binary whose steps reach 9.07 and 9.585 ms), is read by
    task 11.1 before G1's build. For each repetition with a bound step over
    6 ms, it names what the slow step spends its time on beyond a typical
    one. A fixed LushText cost is cut under a failing-first test in task
    11.3's build, before G1 runs. A GTK cost is recorded as a toolkit floor
    (A118, A124). If no repetition is slow, S3 falls back to G1's per-step
    print: G1 and G1b print every step over 4 ms with its kind (bound unit,
    terminal, chunk) and its rows. If the tail does not scale with the
    step's rows (a 16-row step as slow as a 32-row one, as `u16` hints), a
    DWARF round of G1's slowest case at 10 repetitions follows. S1 and S2
    shrink only the part that scales with rows. If S3 leaves a tail over
    `B_INTERACT_US` in the final campaign, and that tail is not GTK's
    per-widget work, C3 sends the rows-after class to `main`'s direct
    collapse.
  - *A frame misses 12 ms:* **F1**, a DWARF profile round of that case. D25's
    frame holds only GTK's layout of the new rows and LushText's bind work
    for them; a LushText cost it names (bind, D22's projection, the
    accessible updates) is cut under a failing-first test, and G1 reruns
    once. A smaller interval budget is not a rung: `u16` misses completion
    by about 60 ms at 400:256. If F1 leaves a frame over 12 ms, the closing
    rule applies.
  - *Completion misses bar − M only for a lost first frame* (400:4,096, the
    case with no slack, R11): it is G2's.
  - *`INPUT_TO_FRAME_US` misses:* **T1**, the first bound unit is armed by
    the first frame's `after-paint` instead of the input, so the first frame
    shows the intent alone; the lost frame is re-read against the completion
    margin, and a case it pushes over bar − M goes to C1.
- **G2, the unbound chunk (in G1's entry).** `COLLAPSE_UNBOUND_CHUNK_ROWS`
  is the largest of 16,384, 8,192 and 4,096 for which, in every repetition,
  both D23's one-chunk dispatch and every between-frames turn
  (`LIST_TURN_SLICE_US`, 4 ms, plus one chunk) are at most 6 ms (`SLC_CHUNK`,
  measured at 0:131,072 and 400:16,385). The turn bounds the chunk to about
  2 ms: at D24's rate (about 0.18 µs a row, the skip-cache figure) that is
  8,192 rows or less, predicted, not measured. If 4,096 misses either
  figure, then:
  - the between-frames chunk is the largest smaller power of two that fits
    the turn;
  - D23 is void, because no single chunk then covers 400:4,096's unbound
    rows;
  - 400:4,096 keeps its measured completion without D23's credit (121.9 ms
    predicted against 120.6 ms), and the closing rule applies to that case's
    margin (R14).
- **G3, A123's hiding time (task 2.7, measured by its probe).** The input
  dispatch (intent and hiding) must be at most 6 ms: `0.245 ms` (104's worst
  input) plus the probe's time to hide the band. If it is over, the rows
  outside the viewport are hidden by the first step-source dispatch instead,
  before the first frame. A refuted A123 drops D22 without a design loop
  (C1): D22 is not built, task 11.5 removes the picture-final requirement's
  first-frame bar and D22's scenarios from the delta specs, and the rest
  proceeds.
- **G4, the destroy-only unit (in G1's round, 0:256, 0:4,096 and
  0:131,072).** 256 if its unit dispatch is at most 6 ms and its frame at
  most 12 ms in every repetition (D24 removed the path-cache chunk that
  failed Q3b), else 128 under the same bars, else 64. 128's Q3 figures
  (7.325 ms step, 8.487 ms frame) were taken with the step in the frame and
  at the 8 ms bar; G4 re-reads them under D25 and the 6 ms margin.
- **G5, D24 (task 10.15).** Taken (Q2). Its invariants hold with their
  tests, or D24 is void and the closing rule applies to 0:131,072's margin.

The constants round (task 10.7) runs on the binary built after G1's ladder,
D22 and D24, with the candidates the gates left, and the hooks go
(task 10.11) after it.

#### The closing rule, if this loop's gates also fail

This is the last design loop for the rows-after frame and step margins.
- **C1, no further loop.** If G1's ladder ends with a selection margin
  missed (a frame over 12 ms, a step over 6 ms, a completion over bar − M),
  the change does not return to design for it. It proceeds to the final
  campaign (task 7.2) with the last rung reached.
- **C2, it lands** if the final campaign meets D2's pass bar (input and
  every step within `B_INTERACT_US`, every dispatch within `B_SETTLE_US`,
  except D2's recorded toolkit-floor exceptions) and R3's completion bar
  with R9's allowance, in every round, at the bars themselves. A selection
  margin it missed is recorded as measured data in
  `docs/next/long-line-documents.md` (BL's follow-ups), with the levers
  left (below). A missed margin is not a budget exception, because the
  budget holds.
- **C3, scope reduction** if the final campaign misses a hard bar in some
  case:
  - What passes its own gates ships in every case: D24, the destroy-only
    unit G4 admits, D17's lighter rows, D22 if A123 holds, D10's expansion
    cap, R4's header fix and R6's fixes.
  - A collapse class (no row after the band at intent, or rows after it)
    keeps the paced collapse only if each dispatch over its budget is
    attributed by a profile to the toolkit's per-widget work (A118, A124)
    and is shorter than `main`'s worst dispatch in the same case, so no case
    gets worse than `main`. Each such dispatch is then a recorded
    toolkit-floor exception, an extension of D2's clause stated here before
    the campaign. Otherwise that class keeps `main`'s direct collapse, which
    is today's behaviour, and the deferral is recorded with its figures.
  - A class that misses R3's completion bar with R9's allowance keeps
    `main`'s direct collapse: a collapse slower than `main`'s by more than
    the bar is what R3 rules out, whatever its frames.
  - Main's own figures, for scale and not as a bar: 400:64's worst frame is
    18.0–20.2 ms with input 11.5–18.8 ms, and 400:256's is 56.1–72.6 ms in
    one over-budget dispatch (`impl/campaign2/slc-v2-r1/r3-main-c1.log`).
- **The deferral record** (C2 or C3) names the measured figure, the rung
  reached and the levers recorded below.

#### Alternatives considered

- *Re-registering the frame bar between 12.8 and 16 ms.* Rejected: it would
  be chosen from the data, and it would not help, because the final campaign
  is judged worst of four rounds at 16 ms and unit-32 tails already reach
  14.4–15.7 ms.
- *Process isolation alone (G0's fix).* Rejected as a mechanism: the
  fresh-process round still has a 14.39 ms frame. It is an instrument fix,
  if G0 confirms it.
- *Unit 16 in the frame.* Frames mostly under 11 ms, but completion
  211.9–214.9 ms at 400:256 against 154.3 ms. *Unit 64:* frames
  15.6–17.8 ms, steps up to 10.0 ms. Both rejected.
- *A larger interval budget under D25* (64 rows laid out per frame, removed
  in two step dispatches). It would halve the rows-after tail, but a 64-row
  frame's layout plus two steps is about 17–20 ms of main-thread work per
  16.7 ms interval, so the frame rate would drop during the collapse.
  Recorded as a lever for the deferral record, not taken.
- *GTK reusing a removed row's widget for an entering row.* GTK 4.24 keeps
  removed widgets only for the same item and recycles only rows leaving the
  tracked range (above). Reusing them in a removal would remove the creation
  cost; it is not in LushText's control and joins task 8.4's upstream
  report candidate.
- *Lighter rows still* (beyond D17). The profile spreads the per-row cost
  over creation, CSS and measure with no single heavy widget left, so there
  is no target. Recorded as a lever.
- *A `GtkSliceListModel`* (R11 (b)): unchanged, rejected; it touches only the
  unbound phase.

#### Attribution round queued

`MEASURE-QUEUE.md`, "slice-large-directory-collapse: case-order A/B (design
R13, G0)", PENDING: four rounds on `rev5/bin/widget-attr-104`, alone →
after → alone → after, 6 repetitions each, branch only, factor the cases
that run before the 400:* cases in the process. G1 and its rungs are queued
by task 11.3 once their binary exists. Added by R14: "slice-large-directory-
collapse: bound-step tail profile (design R14, G0b)", PENDING. It is one
DWARF round on the same binary, 400:64 and 400:256 with 400 rows after, 10
repetitions, profiled as the R13 profile was. Task 11.1 reads it.

#### What this revision changes in the tasks

- **New:** 2.8 (A124, A125); 11.1 (read G0); 11.2 (the verdict test's
  windows, if G0 confirms); 11.3 (D25 and gate G1); 11.4 (rungs S1–S3, T1 and
  F1, as G1 requires); 11.5 (the closing rule's record).
- **Revised:** 10.4 keeps its 12 ms acceptance, now read by G1 on D25 for
  every case; 10.7 takes R13's margins and candidates; 10.9's guard and D23
  move to the first step-source dispatch; 10.13's re-applied `hidden`
  follows D25; 8.3 gains D25's wording.
- **Annotated:** D2's step-in-the-frame bullet, D4's and D19's "the tick
  takes bound units only", D22's re-apply, D23's input chunk and R12's
  frame-tick re-apply are superseded by D25 where they are written. Every
  forward-looking "returns to design" exit written before R13 (R3's and
  R9's negative-slack stop, D17's acceptance, D22's refuted A123, R11's
  selection-margin risk, the Risks bullets, tasks 2.7, 10.4 and 10.7) is
  superseded by C1 and annotated where it is written; the history sections
  that record earlier returns stay as written.
- **Unchanged:** D22–D24 and R11's completion gate.

#### Gates G0 and G0b read (task 11.1, 2026-11-18)

**G0, case order.** The coordinator ran the four rounds on
`rev5/bin/widget-attr-104` in the queued order (alone-1 10:07, after-1
10:22, alone-2 11:43, after-2 13:58), each kept round at a 1-minute load
below 4 before and after (3.81 / 3.03, 3.64 / 3.33, 2.32 / 2.67,
3.29 / 3.12, `rev/attr2/measure.log`; tries at a load of 4 or more, or
that `measure.sh` refused with 75, were discarded and rerun in the same
position). Every log holds 6 repetitions of each case it lists. Reader
`rev6/g0/g0.py`, output `rev6/g0/g0-output.txt`.

| 400:N | pair 1 frame median, alone / after (diff) | pair 2 frame median, alone / after (diff) | pair 1 step median, alone / after | pair 2 step median, alone / after |
|---|---|---|---|---|
| 64 | 10.956 / 10.193 (−0.763) | 9.417 / 11.783 (+2.366) | 6.439 / 4.400 | 4.745 / 5.877 |
| 256 | 13.936 / 11.057 (**−2.879**) | 9.648 / 14.002 (**+4.354**) | 9.029 / 4.661 | 4.765 / 7.738 |
| 4,096 | 9.559 / 13.052 (+3.492) | 11.154 / 11.806 (+0.652) | 6.090 / 5.499 | 5.236 / 5.220 |
| 16,385 | 9.886 / 11.624 (+1.737) | 10.031 / 11.740 (+1.709) | 8.811 / 5.264 | 4.989 / 4.968 |
| 131,072 | 12.112 / 13.213 (+1.101) | 12.793 / 13.532 (+0.740) | 7.449 / 6.501 | 7.171 / 7.606 |

**Reading, as pre-registered: inconclusive.** Pair 1's difference at
400:256 is −2.879 ms (not ≥ 1.0 ms), so the order effect is not
confirmed; pair 2's is +4.354 ms (not < 0.5 ms), so it is not refuted.
Both pairs have three sizes at ≥ 1.0 ms. By R13, task 11.2 (destroy each
case's window) runs only on *confirmed*, so it is not done, and G1 is
judged in Q4's full case order, the worst order measured, as R13 already
requires whatever G0 says. The window-per-case finding of R13 item 3
stays an untested hypothesis, recorded, not acted on.

**The step tail, per repetition (for S3).** The worst steps of the 400:*
cases across the four rounds (24 repetitions per size) spread 4.0–10.2 ms
at 400:64, 4.2–11.2 ms at 400:256, 4.3–9.1 ms at 400:4,096,
4.5–10.3 ms at 400:16,385 and 5.8–10.9 ms at 400:131,072; every size has
a repetition over 6 ms in at least three of the four rounds (400:4,096 and
larger in all four), and 26 of the 120 are over 8 ms (`g0-output.txt`
lists every repetition). The tail is
frequent, not rare: under D25 the 6 ms step margin is not met at 32 rows,
which is why S1 is G1's expected path.

**G0b, the tail profile.** One DWARF round, 10 repetitions each of 400:64
and 400:256, run at a load of 2.74 / 2.61 (`slc-r14-g0b-pdw-400`); readers
`rev6/g0/tail.py`, `diff.py` and `diff-first.py`, outputs
`tail-output.txt`, `diff-output.txt` and `diff-first-output.txt`, the GTK
thread's samples in `rev6/g0/wall.txt`. 13 of the 20 repetitions have a
worst step over 6 ms (5 at 400:64, 8 at 400:256; 7.5–13.0 ms).

- **Where the tail sits.** In all 13 slow repetitions the slow step is the
  collapse's **first bound-unit dispatch** (the first frame at 400:64,
  offset 0.01 ms; the first band dispatch at 400:256, 12.8–13.6 ms after
  the input). The later bound dispatches of those repetitions are 8.1–9.7 ms
  frames carrying 4–5 ms steps, with one exception: 400:64 repetition 1's
  second (terminal) dispatch took 17.8 ms. The first dispatch of the 7
  typical repetitions is 8.6–9.7 ms, and 11.4 ms in 400:256 repetition 1.
  So the tail is concentrated in, but not caused by being, the first
  dispatch: 7 first dispatches are not slow.
- **What the excess is.** Against the 7 typical first dispatches of the same
  cases (39.0 samples of 0.25 ms each on average; `diff-first-output.txt`),
  the 13 slow ones hold 59.2. The growth is inside the step's removal
  emission: `g_list_store_splice` → `gtk_tree_list_model_items_changed_cb`
  19.5 against 10.9 samples per dispatch; under it
  `gtk_list_item_manager_ensure_items` 9.3 / 3.6, row widget creation
  (`gtk_list_view_create_list_widget` 3.9 / 1.7, `gtk_widget_init` 9.6 /
  4.4, with GTK's per-widget `gtk_widget_add_controller` →
  `gtk_shortcut_controller_set_widget` 5.6 / 2.1) and
  `gtk_tree_list_model_clear_node` 3.6 / 1.4. Against the later dispatches
  (`diff-output.txt`) the picture is the same. The slow removals create
  about twice the row widgets a typical one does; why the count varies from
  one repetition to the next is **not shown** by the profile (GTK recycling
  a removed widget only from outside the tracked range, R13, is a
  hypothesis, not a finding). The layout share grows less (CSS lookups), and
  no new function appears.
- **Whose cost.** GTK's. LushText frames are in at most 2 samples of any
  slow dispatch (0 in 12 of the 13); the growth is not in the row factory's
  setup or bind. There is no fixed LushText cost to cut before G1, so task
  11.3's build carries no S3 cut. The excess is GTK's per-widget creation
  inside a removal (A118; A124 pins it from an idle), recorded for D2's
  toolkit-floor clause.
- **What follows for the ladder.** *Prediction, not evidence:* if the
  excess is per widget created, smaller steps (S1, S2) shrink it with the
  rest of the step. G1's per-step print therefore carries, for every
  step-source dispatch over 4 ms, its kind, its rows and the row widgets the
  factory created in it, so G1b and G1c decide it; if a 16-row step is still
  as slow as a 32-row one, S3's further DWARF round follows as R13 writes it.

#### A124 and A125 (task 2.8, 2026-11-18)

Both hold on GNOME 51 (GTK 4.24.1, host and `org.gnome.Sdk//51`) and on the
GNOME 50 floor (`//50`, GTK 4.22.2); logs `rev6/sample-A124-51.log`,
`rev6/sample-A125-51.log`, `rev6/axioms-runtimes-a124-a125.log`.

- **A124.** From a `G_PRIORITY_HIGH_IDLE` idle outside any frame, removing
  32 bound rows of a 256-entry directory with 400 rows after it ran 32
  setups and 32 binds inside the splice, all 32 entering rows mapped at its
  return, and the next frame ran no setup or bind (layout 2.4–2.5 ms, debug
  build). D25's premise holds: the frame after a step-source unit only lays
  out and paints.
- **A125.** A source armed in `after-paint` (working 4 ms, re-armed once)
  and one armed in a 20 ms default-priority dispatch while the next frame was
  due both ran before the next `before-paint`; each read the counter of the
  frame just painted (10, 10; 11), and the next frame read the next counter.
  The 4 ms source did not delay the next frame (16.65 ms gap against
  16.44 ms). The token can read the counter from the step source.
- **Nightly preview (not a restatement).** On `//master` (GTK 4.25.1) A124
  is violated: the splice runs 0 setups and 32 binds, so GTK 4.25 reuses the
  removed row widgets for the entering rows, and only 19 of the 32 entering
  rows are mapped when it returns. A125 holds there. Recorded in the ledger
  for the next toolkit adoption: the reuse removes the per-row creation cost
  that G0b names as the step tail, and rows left unmapped at the splice's
  return would change when D22's re-apply must run. A118 is violated on
  `//master` too (`rev6/axioms-runtimes-a118-a120.log`: the head removal
  runs no setup and keeps 16 of 16 removed widgets as survivors), so the
  nightly's reuse is one toolkit change that A118 and A124 both see, not a
  defect of A124's fixture.

D25 is built (R13: "If either is refuted on a toolkit, the change takes the
closing rule"; neither is refuted on a shipped or floor toolkit).

#### Task 11.3: D25 built, G1 queued (2026-11-18)

D25 is in `collapse_execution.rs` with the per-interval budget behind
`SLC_STEP_ROWS`; G1, G1b, G1c, G2 and G4 are queued as one entry on one
binary (`rev6/bin/widget-attr-d25`, `MEASURE-QUEUE.md`, "D25 gates G1, G1b,
G1c, G2 and G4"). Implementation decisions, each taken for the user's
frames first:

1. **The token counts rows, not units.** `FrameUnitToken` holds the counter
   of the interval and the bound rows removed in it
   (`policy::interval_rows_left`, `record_interval_rows`); a counter other
   than the stored one, earlier or later, starts a fresh interval (R6.3). A
   step takes `policy::bound_step_rows`: a bound removal at most
   `SLC_STEP_ROWS` and what the interval has left; a destroy-only removal
   only on an untouched interval, charging the whole budget (it creates no
   widget, D18); a direct collapse only when its rows fit what is left. With
   the hook unset, one 32-row step spends the interval, as before.
2. **A starved fallback unit spends its interval.** R6.2's flag is replaced
   by charging the whole budget to the current counter, so the frame that
   lays a starved unit out takes no other.
3. **No busy loop between frames.** A step-source dispatch that stopped at a
   bound step it could not take (the interval spent) does not arm the
   between-frames idle, which would arm the source again and stop at the same
   step: the first build ran 597 step dispatches over 6 frames at 400:256.
   `test_bound_units_run_outside_the_frame` bounds the idle dispatches per
   interval (at most two) and fails without the guard (10, 39, 33 and 53 per
   interval, `rev6/d25-loop-failing-first.log`).
4. **Unrealize is covered by unmap.** A widget is unmapped before it
   unrealizes, and `unmap` disconnects the `after-paint` handler through
   `arm_collapse_source`; `map` reconnects it to the clock the list has then.
5. **D22's re-apply is not in this build.** A123 (task 2.7) is not yet run,
   so no step re-applies `hidden`; G1 measures the step without it, and the
   whole-dispatch timer will include it once D22 lands.
6. **Evidence.** The section's evidence gains the step dispatches of the
   current run (kind, rows, row widgets created, time, frame counter,
   at most `STEP_DISPATCH_LOG_ROWS`), their longest time, whether the
   `after-paint` handler is connected, its runs and the factory's setups. The
   verdict print adds one `collapse-steps` line per case.

Tests, failing-first against the pre-D25 core
(`rev6/d25-failing-first.log`): `test_bound_units_run_outside_the_frame`
(rows left inside every frame dispatch, `B 31 203` to `A 31 171` and so on),
`test_the_step_source_runs_before_the_next_frame` (every interval's unit
left after the next `before-paint`) and the revised guard of task 10.9
(`[4096, 203]`: nothing left before the first frame).
`test_the_step_source_leaves_no_frame_handler` reads fields the old evidence
does not have. All four pass on D25 (`rev6/d25-new-tests.log`), the
strengthened first one also checks that D23's chunk and the first bound
unit both run before the first frame and that no interval removes more than
`SIDEBAR_REALIZED_ROWS_PER_FRAME` bound rows; the module passes
(`rev6/wtv-module-d25.log`, before the loop fix, and `make test` after it).

**G1's reading, made precise before it runs (2026-11-18).** G1's "every
step" is every step-source dispatch of kind Bound, Descendant or Terminal in
the `collapse-steps` line. Chunk dispatches (D23's chunk in the step source)
and between-frames turns are G2's: without D24 they still carry the
path-cache splice (5.4–8.5 ms at 131,072 in the dry runs). `INPUT_TO_FRAME_US`
is not printed by the verdict test on this binary, so G1 does not judge it
and rung T1 cannot be read from G1's logs; task 7.1's verdict print owes it.

The late 33–41 ms "unshown" dispatch at 0:131,072 (about 670–700 ms after
the input) is not D25's: `widget-attr-104` shows it in every G0 "after"
repetition. It is the terminal's, outside G1's kinds; G5 and task 7.2 read
it.

#### A123 and gate G3 read; A119 corrected (tasks 2.7, 2.9, 2026-11-18)

**A123 holds** on GNOME 51 (host, real input) and on the GNOME 50 floor
(`//50`, input half skipped), with `//master` as a preview: hiding the
band's subtree rows by opacity 0, `can-target` false and accessible `hidden`
on the row widgets is paint-only (0 measures, setups and teardowns; band and
allocation unchanged), draws no pixel of a hidden row whatever its
selection, focus or hover, picks the list, and real pointer input over a
hidden row does nothing. **G3 passes:** 204 rows hide in 0.29–0.33 ms, so
the input dispatch hides every realized subtree row and nothing moves to
the first step-source dispatch.

Two findings change how D22 is built, not what it does:

1. **A created row widget cannot be touched at bind.** The list binds a row
   widget it creates before inserting it, and the widget is still floating
   then (its child has no root). A Rust reference taken to it there sinks
   the floating reference, and dropping it finalizes the widget before GTK
   inserts it (the first probe run crashed in `gtk_widget_insert_after`). So
   the bind path hides a rebound row only when its child is rooted, and a
   created row is hidden after its insertion: by the end of the step that
   created it (inserted rows are mapped inside the step, A124) or, for a row
   created outside a step (a resize mid-collapse), by a one-shot
   `G_PRIORITY_HIGH_IDLE` re-apply the bind arms. Such a row may draw for
   the frame that created it; recorded, not hidden.
2. **Only the rows the viewport shows are mapped.** The band's rows outside
   the viewport stay unmapped (A10), and a row is mapped when it scrolls in
   without a bind. GTK's `gtk_widget_map` writes `hidden` false after the
   `map` emission, so D22 connects a `map` handler on each row it hides that
   arms the same re-apply source, which writes `hidden` again once the
   mapping dispatch has returned.

**A119's accessible half, corrected.** `gtk_tree_list_row_set_expanded`
notifies `children` right after `expanded`, and the expander re-runs its
bind-time projection for every notify but `expanded` and `item`, so a real
expansion writes the expander's state too. The old statement had never been
observed: the lane ran that half under `GTK_A11Y=none`. D6 is unaffected:
the sidebar's shown and model states differ only while a collapse is
pending, when the row is GTK-expanded and no `set_expanded` reaches it until
the terminal one.

#### D22 as implemented (task 10.13, 2026-11-18)

- **Membership.** A `collapsing_rows` list beside `collapsing_paths` holds each
  collapsing row strongly from intent to its end (the queue removes a record
  while its step runs, so binds inside a step would otherwise miss it). A row is
  collapsing when it is one of them or descends from one by
  `TreeListRow::parent`; the R6.5 projection, D22 and the disclosure routes read
  this. The refresh and scan paths keep `path_is_collapsing`: they key on paths
  by construction, and a refresh of a path refreshes every occurrence.
- **`stop_subtree_work`.** It cancels the stores of the collapsing row and its
  captured descendants, plus path-prefixed stores and queued refreshes that no
  other workspace folder root covers. On the direct path (nothing captured), a
  store of this occurrence under another folder root keeps its load until
  GTK's collapse detaches it, and its batches are dropped as any detached
  store's; the other occurrence keeps loading (the overlap test).
- **Re-apply.** The band is walked at intent, at the end of each bound step
  that leaves a collapse pending, at every end, and by a one-shot
  `G_PRIORITY_HIGH_IDLE` source armed by a created row's bind (its overlay has
  no root yet, A123) or a hidden row's `map`. No pass runs on the frame clock
  (`collapse_hidden_syncs` stays still between `before-paint` and
  `after-paint`).
- **Fallback re-arm.** The fallback now re-arms the frame pacing when the list
  is mapped and the `after-paint` handler is missing, not only when the tick
  is: a `map` that could not connect it (under the `Hold` seam) left the tick
  refreshing `last_step_us` forever with nothing arming the step source.

#### A126 and the held keys (task 10.14, 2026-11-18)

D22 holds the list-navigation keys while a collapse is pending and applies
them when it ends. Task 10.14 first named the mechanism as a pure helper that
maps each held key to a target position, applied through `scroll_to`. It now
**puts the held key events back on GDK's queue** (`gdk_display_put_event`)
once the collapse has ended and a frame has laid out the final tree, and GTK
routes them as it routes any press. Reasons:

- *It is `main`'s mechanism.* On `main` the frozen collapse leaves the keys
  in GDK's queue, and GTK delivers them afterwards against the final tree.
  The spec's bars ("as on `main` after its frozen collapse returned", "where
  GTK's own binding lands") then hold by construction.
- *A target-position helper cannot match GTK without copying it.* `Page_Up`
  and `Page_Down` read tile allocations (`gtklistbase.c`,
  `gtk_list_base_move_cursor_page_*`); a plain or Shift move on a focused,
  unselected row selects that row and does not move
  (`handle_selecting_unselected_cursor`); Ctrl moves focus without selecting.
- *The row-dependent keys have three routes:* Return, ISO_Enter and KP_Enter
  are the row widget's `activate-keybinding` (`gtklistfactorywidget.c`);
  Space and KP_Space with any Ctrl/Shift are `listitem.select` plus the
  sidebar's peek controller; Menu and Shift+F10 are the context menu's. A
  re-queued event takes each route unchanged.
- *The keypad forms are GTK no-ops.* `GtkListBase` binds no `KP_` navigation
  key and `gdk_key_event_matches` does not match `KP_Down` to `Down`, so a
  held keypad key replays as nothing, which is what GTK does with it.

**A126** pins the toolkit half: a stopped press, put back from a later
dispatch, comes back through the capture phase as the same event, in put
order, before a `G_PRIORITY_LOW` idle, and the list's binding acts on it
(GTK 4.24.1, twice; the 4.22.2 floor by source, `gdk_display_put_event`
unchanged). The API is deprecated since 4.10 with no replacement, so its one
call carries `#[expect(deprecated, reason)]` (`rust.md`, "Lint Suppression").
The pure half in `ui/sidebar/policy.rs` classifies a key (navigation,
row-dependent, other) and decides hold or propagate.

**Implementation decisions (task 10.14, 2026-11-18).**

- *The hold covers the section's whole tree, not only the collapsing
  directory row and its hidden rows.* Up from the row after the subtree, or
  a page key from anywhere above it, would otherwise land on a hidden row. So
  a navigation key pressed on any row of the section's list while a collapse
  of that section is pending is held. The spec scenario carries it as an AND
  line, with a failing-first test
  (`test_up_from_the_row_after_the_subtree_waits_for_the_collapse`).
- *Where it lives.* `workspace_section/collapse_execution/held_keys.rs`, a
  child of the collapse's `execution` role (it owns the hold, the replay
  wait and the delivery check), entered from the existing capture-phase key
  route on the slice bin before its disclosure-key handling. The policy is
  `classify_held_key` plus `held_key_action` in `ui/sidebar/policy.rs`
  (enums, no keysym constants, so `docs/thresholds.md` is unchanged).
- *Order.* Held events carry an order number. A put-back event that comes
  back through the route is checked again (A126: it is the same event): it
  is held again, at its own place, only while another collapse is pending
  or a key pressed before it is still held, so a later real press never
  overtakes it.
- *The replay wait.* When the section's last collapse ends, by any end
  (`finish_collapse` and `cancel_all_collapses` arm it), a one-shot
  `after-paint` handler on the list's frame clock puts the keys back once a
  frame has laid out the final tree, and disconnects itself; a
  `COLLAPSE_FRAME_STARVATION_MS` timeout replays without a frame, and an
  unmapped list replays from an idle. A `G_PRIORITY_LOW` delivery check
  added with the put clears the keys GDK delivered and re-arms if a replayed
  key started another collapse and was held again. Readiness
  (`collapse_blocks_readiness`, hence `workspace.collapse_pending` and
  `workspace-refresh-complete`) stays blocked while keys wait, and the
  evidence reports the waiting count and the held, replayed and dropped
  totals.
- *Focus.* A key is held only while the window's focus is on a row of the
  list and not in an editable (the inline rename entry keeps its keys). If
  the focus has left the list when the keys would be put back, they are
  dropped and counted, never delivered to the widget that now has it.
  Dispose drops them and removes every source.

**What the tests found about the harness (2026-11-18).**

- The headless compositor deactivates the window when a RemoteDesktop input
  session ends; GTK then clears the list's `focus-child` chain while the
  window's focus widget, which keys go to, stays. The tests read the focus
  from the window, and aim real presses only after an activation (which
  scrolls the list back to its focus row) can no longer move the target.
- **Mapping a popover on the inactive window segfaults mutter 51**
  (`meta_wayland_xdg_popup_apply_state`, `g_hash_table_iter_next`; coredumps
  of 2026-11-18 20:43–21:16). Discriminated: a fresh real `space` on an
  active window peeks without a crash; the same press stopped and put back
  after the input session ended crashes it, and so does
  `toggle_peek_for_selection()` called with no key at all on the inactive
  window. So the trigger is the inactive window, not `put_event`. On a real
  seat the window the user types into is active; whether a popover opened
  on a window the user has just left reaches the same mutter path is not
  tested here, and is recorded for an upstream mutter report (maintainer's
  decision). The Space case therefore stops the replayed `space` with a
  capture probe in front of the peek controller and checks which row it
  reaches, instead of letting the peek popover open; the peek route itself
  is covered by the peek tests.
- `test_replayed_navigation_matches_gtk` takes its reference by putting the
  very press the window saw back from the same start, scroll position and
  final tree with no collapse pending, rather than a second real press: the
  compositor's input sessions did not deliver a second press reliably (a
  Ctrl chord's key went missing). It asserts that the replay acted from a
  settled scroll position, because Page Up and Page Down move by the band the
  slice bin shows.

#### Gates G1, G1b, G1c, G2 and G4 read (task 11.3, 2026-11-19)

The coordinator ran the eleven rounds of `MEASURE-QUEUE.md`, "D25 gates G1,
G1b, G1c, G2 and G4", on `rev6/bin/widget-attr-d25` under the amended
quiet criterion (level 1, drain first): each round exited 0 with
`run_busy` and `post_busy` below 4 (`rev/attr2/gate.log`). They were read
here as pre-registered above, on every repetition of every case. The readers
and their output are in the handoff's `rev8/g1/` (`read_g1.py`,
`read_g1_reps.py`, `read_g24.py`; `summary.txt`, `reps.txt`, `g2-g4.txt`).

**How the reading splits the bound dispatches.** A `Bound` dispatch in the
`collapse-steps` line is one of two things:
- a step of at most `SLC_STEP_ROWS` rows. These are the rows the rung
  changes, and only these discriminate G1, G1b and G1c;
- a destroy-only removal of more rows, which brings no row into the band.
  Its size is the subtree's rows less the steps before it, up to
  `COLLAPSE_DESTROY_ONLY_ROWS` (64). This is G4's unit, read separately
  below.

**G1 (32 rows): steps miss.**
- Frames: every worst frame is at most 8.2 ms (the 400:* cases 5.6–8.2 ms,
  the 0:* cases and the header at most 2.4 ms).
- Inputs: at most 3.9 ms.
- Steps: the 32-row steps reach **9.7 ms** (400:256), 8.6–8.9 ms in four of
  the five 400:64 repetitions, and 6.2 ms at 400:131,072 and 0:256. Every slow
  400:* step created 32 row widgets.

**G1b (16 rows): steps miss.**
- Frames: at most 8.3 ms.
- 16-row steps: **7.7 ms** (400:64, one repetition) and 7.2 and 6.5 ms
  (400:256, two repetitions). Each created 16 row widgets.
- *The tail scales with the rows,* so S3 is not read. At 400:64, one 16-row
  step in five is over 4 ms, against four 32-row steps in five at
  8.6–8.9 ms.

**G1c (8 rows): every step passes.**
- Frames: at most **10.8 ms** (400:4,096).
- 8-row steps: at most **5.7 ms** (400:64 5.2 and 5.7 ms, 400:256
  5.0–5.6 ms). Each created 8 row widgets.
- No step is over 8 ms, so D2's toolkit-floor clause is not needed for the
  step.

**Completion against bar − M.** The credits applied are D24's from Q2 only.
Q3's credit is the gain of destroy-only unit 128, and G4 below keeps 64, so
that credit is not earned and is not applied. This corrects the readers' first
pass, which applied it.
- G1c passes in every case:
  - 0:16,385: 83.8 − 8.0 = 75.8 against 83.3 ms;
  - 400:16,385: 131.2 − 9.3 = 121.9 against 122.6 ms;
  - 400:131,072: 206.3 − 74.6 = 131.7 against 197.9 ms;
  - 0:131,072: 168.8 − 63.9 = 104.9 against 140.6 ms;
  - header: at most 72.9 against 131.6 ms.
- Where a `SLC_SKIP_CACHE` round measured the case directly, it agrees:
  - 0:131,072: 91.4–105.2 ms (G2, chunks 8,192 and 4,096);
  - 400:16,385: 106.5–113.8 ms.
- G1b misses at 0:16,385: 97.3 and 91.5 ms, less 8.0, against 83.3 ms.
  These completions are noise against G1 and G1c, not the rung, but they are
  recorded.

**The rung: S2, steps of 8 rows (task 11.4).** Frames pass at every rung,
and G1c is the smallest-change rung whose steps pass. So
`COLLAPSE_STEP_ROWS` = 8 and `DIRECT_COLLAPSE_ROWS` drops to 8.

**G2: `COLLAPSE_UNBOUND_CHUNK_ROWS` stays 8,192.**
- 16,384 misses: its between-frames turn at 0:131,072 is 5.9–6.5 ms, over
  6 ms in three of five repetitions (6,055, 6,291 and 6,508 µs).
- 8,192 passes: Chunk and Between turns at most 5,253 µs at 0:131,072 and
  5,990 µs at 400:16,385.
- D23 holds, because 8,192 rows cover 400:4,096's unbound rows.

**G4: `COLLAPSE_DESTROY_ONLY_ROWS` stays 64.**
- 256 misses: its 139-row destroy-only dispatch is 5.4–7.5 ms.
- 128 misses: 6.9 ms at 0:256, 6.0 at 0:4,096, 6.5 at 0:131,072.
- 64 is read from G1's round: its one 64-row dispatch (0:256) is 4.5 ms.

**Misses the ladder leaves, recorded as data under C1.** None is over a hard
bar in these rounds.
- *The destroy-only surplus at 0:64 under S2.* With steps of 8, the
  subtree's 56 remaining rows go in one destroy-only dispatch: **4.9–7.8 ms**
  in four of five repetitions of G1c, and 48 rows 6.5–7.8 ms in G1b. That is
  over the 6 ms margin and under `B_INTERACT_US`, and no row widget is
  created. At 32 rows, G1 took the same subtree in two 32-row steps of at
  most 4.1 ms. The surplus is new with S1/S2, and it is the smallest margin
  the final campaign (task 7.2) reads against 8 ms. The lever, if the
  campaign misses there: cap the destroy-only unit at the step size while
  the interval is untouched (G4 measured only 64 to 256).
- *The folder-header input in G1c: 7.9 and 7.0 ms* in two of five
  repetitions, against at most 4.0 ms in G1 and G1b. The input dispatch does
  not depend on the step size, and the ladder has no rung for it. It is under
  `B_INTERACT_US`.
- *G2's kinds in the G1 rounds, with the path-cache splice still in the
  chunk:* the Chunk dispatch reaches **10.7 ms** at 0:131,072 and the Between
  turn 9.2–9.7 ms at 16,385. That is over `B_INTERACT_US`. Only the
  `SLC_SKIP_CACHE` rounds, D24's stand-in, bring them to at most 6.5 ms. So
  0:131,072 and 400:16,385 pass only on that stand-in until D24 is built.
  **Gate G5 (task 10.15) is open,** and the constants round (task 10.7) and
  the final campaign cannot pass those cases without it.
- `INPUT_TO_FRAME_US` is not readable from this binary (above). T1 stays
  unread, owed to task 7.1's print.
- *G1c's 5.7 ms excludes D22.* G1's binary had no hidden-row re-apply
  (task 11.3, decision 5). D22 now walks the band at the end of each bound
  step: about 0.33 ms by G3, up to four times an interval under S2. So the
  production 8-row step is unmeasured and sits at about the 6 ms margin. The
  rung stands, and the constants round (task 10.7) measures it.
- *Picture final at 0:N (task 10.16's quick run).* At 0:64 and 0:256,
  `picture_final_us` equals completion, against R11's first-frame bar. The
  cause is open: D22 leaving a row drawn, or rows that move when the
  terminal collapse shrinks the list. Task 10.7's reading attributes it.

**S2 as built (task 11.4).**
- `COLLAPSE_STEP_ROWS` = 8, and `DIRECT_COLLAPSE_ROWS` is derived from it.
- `next_collapse_step` caps a bound removal that brings rows in at one
  step. A destroy-only removal keeps `COLLAPSE_DESTROY_ONLY_ROWS`.
  `bound_step_rows` still spends at most `SIDEBAR_REALIZED_ROWS_PER_FRAME`
  per interval, which is now four steps.
- The input dispatch's direct collapses are capped at one step of rows in
  all, where they were capped at one interval's. The ladder's text did not
  name this; it was decided for the input's budget. Several small
  directories under one folder header would otherwise make a 32-row input
  dispatch, the size of the 32-row steps G1 measured at up to 9.7 ms.

#### The constants round and gate G5 read (task 10.7, 2026-11-24)

The coordinator ran the seven rounds of `MEASURE-QUEUE.md`, "constants round
and gate G5 on the D24 code", Part A, on `rev9/bin/widget-d24` (3248a99a: D22
to D25, S2, the held keys and D24) on 2026-11-19 between 12:36 and 16:07.
Each round was kept under the level-1 drain-first criterion (exit 0, busy CPUs
below 4 during and after it, `rev/attr2/gate.log`). Logs:
`rev/attr2/slc-d24-{base,c16384-0,c16384-400,c4096-0,c4096-400,d256,d128}.log`
(A1 to A7; refused tries `*.try<N>.log` are not read). Reader
`impl1/partA/read_partA.py`, output `impl1/partA/partA-output.txt`, both in
the handoff. Every rule below was pre-registered in that entry and is applied
to every repetition of every case. A "step" is a whole step-source dispatch of
kind Bound with at most `COLLAPSE_STEP_ROWS` rows, Descendant or Terminal; a
Bound dispatch of more rows is a destroy-only unit, G4's; Chunk and Between
are G2's.

**The contamination rule** (the coordinator's note of 16:15, recorded before
the figures were read). A GDK attribution agent ran headless sessions outside
`heavy.sh` while A5, A6 and A7 ran. A margin met there stands; a margin missed
there makes the round contaminated, and it is repeated once in the same
position before it decides anything. A5, A6 and A7 each missed a margin, so
all three are queued again (`MEASURE-QUEUE.md`, "Part A repeats (A5-A7,
contamination rule) and two attributions on the D24 binary", rounds R5, R6
and R7, READY 3248a99a).

**Task 10.7's margins, A1 (chunk 8,192, destroy-only 64, steps of 8; 30
sliced-collapse repetitions and 3 of the header):**
- *Worst frame:* at most 9.3 ms (400:4,096; the 0:* cases at most 1.7 ms, the
  header 2.6 ms). Every collapse and header repetition meets 12 ms.
- *Steps:* at most 5.9 ms (400:256), the header's terminal 4.0 ms. Every
  collapse and header repetition meets 6 ms.
- *Chunk and Between turns:* at most 4.9 ms. Every repetition meets 6 ms.
- *Destroy-only unit (64):* 0:64's one 56-row dispatch reaches 7.6 ms (one of
  three repetitions), as G1c recorded (4.9–7.8 ms). Over the 6 ms margin and
  under `B_INTERACT_US`; no rung below 64 exists in G4's ladder, so it is
  recorded under C1.
- *Task 6.1's expansion cases* (task 10.7 scopes them in, at task 10.4's
  12 ms frame margin). `expand-cold` meets it (frames at most 10.0 ms).
  `expand-after-collapse`, whose collapse runs before each expansion, misses
  three ways, all recorded under C1:
  - one frame of **13.7 ms** (0:1,024, one of three repetitions; under
    `B_SETTLE_US`);
  - 8-row bound steps of **6.4–6.5 ms** in three of its repetitions (400:64
    and 400:1,024), over the 6 ms margin and under `B_INTERACT_US`; 8 rows is
    the ladder's last step rung;
  - the 56-row destroy-only dispatch at **7,993 µs** (0:64), 7 µs under
    `B_INTERACT_US`. **A second hard-bar risk for the final campaign.** The
    lever G1's reading recorded applies if the campaign misses there: cap the
    destroy-only unit at the step size while the interval is untouched.
- *Input dispatch:* **missed in 9 of 30 repetitions**, 6.5–8.1 ms, one of them
  (400:256, 8,070 µs) over `B_INTERACT_US`; the header's inputs are 4.3–5.9 ms.
  The distribution is bimodal and does not grow with the subtree: the median
  is 2.6 ms, the slow mode about 7.2 ms at 0:256 as at 400:131,072. G1c's
  binary (no D22 hiding, no D24 detach at intent) measured the same inputs at
  0.1–0.6 ms, so the floor of about 2 ms and the slow mode both came with
  tasks 10.13 to 10.15. G3's rung (hide the rows outside the viewport in the
  first step-source dispatch) is pre-registered for an input over 6 ms, but
  it moves only the hiding, which A123's probe timed at 0.3 ms for the band;
  it is not applied unseen. Round P1 of the queued entry profiles the slow
  mode (0:256 and 400:256, 10 repetitions each); a LushText cost it names is
  cut under a failing-first test, and G3's rung is applied only if it names
  the hiding. **This is the final campaign's hard-bar risk**: D2's pass bar
  holds the input to `B_INTERACT_US` in every round, and A1 already has one
  repetition over it.
- *Completion against bar − M:* **missed at 400:4,096** (124.6–136.0 ms
  against 120.6 ms) **and at 400:16,385** (123.1–132.7 ms against 122.6 ms) in
  every repetition; every other case passes (0:131,072 107.3–108.7 against
  140.6 ms; 400:131,072 151.7–156.4 against 197.9 ms; 400:256 118.2–132.1
  against 137.6 ms; the header 73.6–75.3 against 131.6 ms). The bars
  themselves are 137.3 and 139.3 ms: 400:4,096's worst repetition is 1.3 ms
  under its hard bar. A3 and A5, which run 400:16,385 alone, measured 113.2 to
  121.1 ms, so A1's full case order (the order R13 judges in) costs that case
  about a frame. No rung is registered for a completion miss other than a
  lost first frame (G2's), so both are recorded under C1 and the final
  campaign judges them at the bars.
- *Picture final:* `picture_final_us` equals `shown_us` within 0.1 ms in
  every repetition, so it meets each case's completion bar wherever
  completion does, and no 0:N repetition's picture is final in the first
  frame dispatch, which is 10.16's open decision below.

**G2: `COLLAPSE_UNBOUND_CHUNK_ROWS` stays 8,192.** 16,384 misses in a clean
round: A2's between-frames turn at 0:131,072 is 6,589 and 6,082 µs in two of
three repetitions. 8,192 passes both of its cases in A1 (Chunk and Between at
most 4,802 µs at 0:131,072, none over 4 ms at 400:16,385). 4,096 is not
needed, so A4 and A5 decide nothing; A5's contamination therefore holds
nothing up, and its repeat (R5) is read as data on the input only. D23 holds:
8,192 rows cover 400:4,096's unbound rows.

**G5 (D24): passes.** On the D24 binary the Chunk and Between kinds of
0:131,072 and 400:16,385 meet G2's 6 ms at 8,192 (above), so the path-cache
splice that kept them at 9.2–10.7 ms in G1's rounds has left the chunk, as
D24 intended. D24's invariants hold with their tests (task 10.15).

**G4: not decided.** A6 (destroy-only 256: 187-row dispatches of 7.3–8.9 ms)
and A7 (128: 6.0–6.8 ms in five of nine repetitions) both miss, and both ran
in the contaminated window, so neither decides until its repeat (R6, R7) has
run. `COLLAPSE_DESTROY_ONLY_ROWS` stays 64 meanwhile, the value G1's round
left and the value the ladder falls back to. G1's clean rounds missed at both
too (128 at 6.0–6.9 ms, 256 at 5.4–7.5 ms), so 64 is the expected result.

**Rung T1 applies.** `input_to_frame_us` is over `INPUT_TO_FRAME_US`
(16,666 µs) in 22 of 30 sliced-collapse repetitions and in all three of the
header (18.0–31.4 ms where it misses). The reading was registered as "a miss
applies T1", and T1 is built as R13 states it (below). Recorded beside it,
because it bounds what T1 can buy: the instrument is measured against a 60 Hz
headless compositor, and `expand-cold`, which runs no collapse code at all,
misses it in 12 of 18 repetitions (18.1–28.6 ms). The first frame after an
input waits for the next vblank, up to 16.7 ms, and its own layout comes on
top. T1 removes the step and its rows' layout from that first frame; the
vblank phase stays. Task 7.1 enforces `INPUT_TO_FRAME_US` on both sides of
the final campaign, so `main` is read against the same floor.

**Task 1.4 (D12's attribution): not confirmed.** In A1's three 0:131,072
collapses, the one dispatch over `B_SETTLE_US` (30.2–40.7 ms, 611–628 ms
after the input) is tagged `unshown` in every repetition, as D12 predicts.
The re-expansion case is not: its first repetition has a **67.1 ms frame**
(layout 66.5 ms), 311 ms after the re-expanding input, untagged, with two
other dispatches of 15.0 and 12.6 ms before it and the picture final at
378 ms; the other two repetitions have frames of at most 1.4 ms and the
picture final at 1.4 ms. The slow repetition is the one whose collapse had
removed every unbound row before the re-expansion (its fifth between-frames
turn took the last 24,373 rows; the others' took 16,384 and left rows). By
the pre-registered rule it is attributed before task 7.2: round P2 profiles
the re-expansion at 8 repetitions, and task 1.4 stays open until it is read.

**Task 10.16's open decision: the completion bar applies to every verdict
case.** R11's first-frame bar applies "where no row followed the subtree in
the flattened list and the directory row was on screen at the input". The
verdict fixture's 0:N trees hold `top.txt` after the subtree, so a row
follows it in every 0:N case, and their picture changes when the terminal
collapse moves that row up. So no verdict case meets the first-frame bar's
condition, and none exercises it; the timing verdict enforces the completion
bar, which every case meets where its completion does. The first-frame
picture itself is not a timing question: D22's widget tests prove it by
counter (`test_collapse_hides_the_subtree_rows_in_the_first_frame`: every
realized subtree row hidden after the input dispatch, before the first
frame). Decided for the user: a timing case for the first-frame bar would
need a fixture with nothing after the subtree, a third campaign case on both
sides, and it would measure what those tests already prove.

**What follows (task 10.7 as read).**
- The constants: `COLLAPSE_STEP_ROWS` 8 (G1c, unchanged), `DIRECT_COLLAPSE_ROWS`
  8, `SIDEBAR_REALIZED_ROWS_PER_FRAME` 32, `COLLAPSE_UNBOUND_CHUNK_ROWS` 8,192
  (G2), `COLLAPSE_DESTROY_ONLY_ROWS` 64 until R6 and R7 are read (G4). Their
  rustdoc and `docs/thresholds.md` rows carry these figures, and the hooks
  are gone (task 10.11; `rev3/attribution-hooks.patch` re-applies them).
- The misses recorded under C1, for the final campaign and BL's follow-ups:
  the input dispatch (pending P1), the completions at 400:4,096 and
  400:16,385, the 56-row destroy-only dispatch at 0:64 (7.6 ms in the collapse
  case, 7,993 µs before a re-expansion), and the expansion cases' 13.7 ms
  frame and 6.4–6.5 ms steps. None returns the change to design.
- Part B (task 7.2) is not READY: it waits on R6 and R7 (G4), P1 (the input's
  slow mode, which may cut code) and P2 (task 1.4's frame).

#### Rung T1 as built (task 10.7's reading, 2026-11-24)

- *Mechanism.* An input that starts a sliced collapse sets
  `first_paint_pending`; the section's `after-paint` handler clears it before
  it arms the step source. While it is set, the step source's interval has no
  bound rows (`interval_rows_left_for` answers 0 and the re-arm reads the
  same), so the step source the input armed takes D23's chunk only, and the
  first bound unit runs in the step-source dispatch the first `after-paint`
  arms. The between-frames idle takes no bound step anyway; the fallback
  ignores the flag, because an unmapped or starved list paints no frame to
  clear it. `stop_collapse_sources` clears it with the other per-run state.
- *Why a flag and not the interval token.* A flag the input sets and the
  next `after-paint` clears depends on no assumption about which frame's
  counter the input dispatch reads. Charging the interval at the input
  would: if GDK delivered the input inside a frame (its `flush-events`
  phase), the counter it reads could already be the one of the frame that
  paints the intent, whose `after-paint` would then still see a spent
  interval and hold the unit one frame longer. That delivery order is not
  pinned by an axiom here, so the design does not rely on it either way.
- *A second input* that starts another sliced collapse while one is pending
  sets the flag again, so the running collapse's next unit waits for that
  input's first frame too: one frame, so that frame shows the new intent
  alone.
- *Test, failing-first.* `test_bound_units_run_outside_the_frame` now asserts
  that before the first frame after the input only D23's chunk ran, no bound
  unit, and that bound units run from the first frame's `after-paint` on. With
  the flag never set it fails (`[Chunk, Bound, Bound, Bound, Bound]` before the
  first frame, `impl1/t1-failing-first.log`); with T1 it passes, and so do the
  first-frame, step-source, D23 and frame-handler tests
  (`impl1/t1-green.log`). Task 10.9's guard, as task 11.4 wrote T1's
  failing-first test, now also asserts that the first frame shows or
  destroys no row widget: without T1 it changed 32 and 23
  (`impl1/t1-guard-failing-first.log`), and it passes with T1
  (`impl1/t1-guard-green.log`). The delta spec's "The first frame lays out
  the first unit" is now "The first frame lays out the intent alone".
- *Unmeasured.* No round has timed a T1 binary: the final campaign (task 7.2)
  is T1's first measurement, unless the stage that reads the queued repeats
  queues a branch-only A1 round on the rebuilt binary first.
- *The lost frame, re-read against bar − M.* T1 moves the first interval's
  bound rows one frame later, so a case's completion grows by up to one
  frame. Read on A1's figures plus 16.7 ms: 400:64, 400:256, 400:131,072,
  every 0:N case and the header stay under bar − M; 400:4,096 and 400:16,385,
  already over it, go further (to about 141–153 ms against hard bars of 137.3
  and 139.3 ms). By T1's rule they go to C1: the final campaign reads them at
  the bars, and if they miss there C3 applies to the rows-after class as R13
  wrote it. This is a prediction from A1's figures, not a measurement; the
  final campaign measures it.

#### The repeats R5–R7 and the attributions P1 and P2 read (2026-11-24)

The coordinator ran the five rounds of `MEASURE-QUEUE.md`, "Part A repeats
(A5-A7, contamination rule) and two attributions on the D24 binary", on
`rev9/bin/widget-d24` (3248a99a, the binary Part A ran) on 2026-11-24
between 03:16 and 03:40. All five were kept under level 1 (exit 0, busy CPUs
below 2.4 before and below 4 during and after, `rev/attr2/gate.log`;
refused tries exited 76 before running and left no log), and no outside
session ran headless work during them as far as the coordinator knows. Logs:
`rev/attr2/slc-d24-{c4096-400,d256,d128}-rep1.log` and
`rev/attr2/slc-d24-pdw-{input,reexpand}.{log,data}`. Readers and outputs, in
the handoff's `impl2/`: `read_repeats.py` (Part A's reader, unchanged) with
`repeats-output.txt` and `p1-verdicts.txt`; `read_p2.py` with
`p2-output.txt`; `p2_chains.py` with `p2-chains.txt` (the GTK thread's samples
in each over-budget dispatch, `perf script --tid 3841516` per repetition,
`p2-r<n>-gtk.txt`). Each reading below is the one the entry pre-registered.

**R5 (data only): no input over `B_INTERACT_US`.** 400:16,385 at chunk 4,096,
three repetitions: inputs 2,308, 2,358 and 2,436 µs. A5's 12,681 µs input is
not reproduced in the clean window.

**G4: `COLLAPSE_DESTROY_ONLY_ROWS` is 64.** R6 (destroy-only 256) misses in
every repetition: its 187-row destroy-only dispatch takes 7,516–9,411 µs at
0:256, 0:4,096 and 0:131,072 (frames at most 1.8 ms). R7 (128) misses in five
of nine: 6,134–6,813 µs at 0:131,072, 6,457 µs at 0:256, 6,161 and 6,407 µs
at 0:4,096 (the other four 5,283–5,982 µs). Both repeats miss again in a
window without outside load, so they decide, with no further repeat: the
ladder falls to 64, the value the code already holds (G1's clean rounds had
missed at both too). The constant does not change; its rustdoc and
`docs/thresholds.md` now cite R6 and R7. 64's own 56-row dispatch at 0:64
(7.6 ms in A1) stays recorded under C1: no rung lies below 64.

**P1: no slow mode to attribute.** The 20 profiled collapses (0:256 and
400:256, ten each) have inputs of 2,360–2,795 µs, median 2,506 µs, with no
gap wider than 161 µs: one mode, so the split finds no slow repetition, and
no LushText or GTK cost beyond a fast repetition can be named. The repeats
agree: R5, R6 and R7's 21 collapses have inputs of 2,301–3,772 µs, none over
6 ms. Part A's slow mode (about 7.2 ms in 9 of A1's 30 repetitions, and in
most of A2–A7's, all on 2026-11-19) does not appear in 41 repetitions of the
same binary on 2026-11-24, so it belongs to that day's windows, not to the
code; what in those windows caused it is not known. Consequences, decided:
nothing is cut, G3's rung is not applied (P1 names no hiding cost), and the
risk stays where Part A put it: D2's pass bar holds the input to
`B_INTERACT_US` in every round of the final campaign, on both sides.

**P2: the slow re-expansion frame is attributed, and it is a LushText cost.**
Four of the eight re-expansions (r0, r1, r3, r7) reproduce it: a frame of
18.9, 22.5, 26.8 and 33.6 ms (layout 18.2–32.9 ms) 290–312 ms after the
re-expanding input, after two dispatches of 11.6–17.7 ms, with the picture
final at 311–339 ms; the other four have frames of at most 1.0 ms and the
picture final at 1.0–14.3 ms. None of these dispatches is tagged `unshown`.
- *Chain.* The two dispatches before the frame are
  `g_list_store_splice` → `items-changed` → `GtkTreeListModel` → the list's
  row widgets (`gtk_tree_expander_set_list_row`, `gtk_label_new`,
  `g_object_new`; frees and `unlink_chunk` in the first, `g_malloc0` and
  `calloc` in the second). The frame is `gtk_list_view_measure` /
  `gtk_list_base_allocate` over those new rows: CSS selector matching
  (`gtk_css_selector_tree_get_change`, `match_class`, `match_name`) and
  shaping (`pango_hb_shape`). The DWARF chains stop inside GTK; the splice
  that queued the work is the refill's reconciliation
  (`scan_execution::apply_scanned_children`, whose callsite appears in r0's
  window), after the refill's scan of 131,072 entries (667–686 samples on the
  scan worker, about 170 ms) in each slow repetition.
- *The condition.* "Re-expanded after the collapse's unbound rows had all
  left" does not decide it: r0, r4 and r5 re-expanded after the last unbound
  turn (24,373 and 7,989 rows), and r4 and r5 were fast. A widget test
  reproduces it by counter (the pre-registered fallback, done so the reading
  is not left open): `test_reexpanding_after_bound_units_refills_the_band_one_unit_a_frame`
  collapses a 4,096-entry directory, re-expands it once one bound unit has
  run, and counts row widgets per dispatch. The bound unit had removed the
  band's first 8 rows, so the store held `row-00008` to `row-00202`, which is
  no prefix of the scanned listing; the reconciliation plan's prefix and
  suffix were both empty, and it replaced the 195 kept rows: one dispatch
  destroyed 195 row widgets and a later one showed 203
  (`impl2/p2-failing-first.log`). In r4 and r5 no bound unit had run, the
  store still held the band from its first row, and the refill only
  appended below it.
- *The cut.* `plan_directory_reconciliation` now also reports where the
  removed current rows reappear unchanged in the replacement
  (`kept_at`, from the pure `kept_run_offset`, computed on the scan worker),
  and the reconciliation keeps them: it inserts the rows before and after the
  run instead of replacing it, so no row the store still lists is destroyed
  and created again. Rows around a kept run land in the band, so they go in
  one unit a frame, as into an empty store (D10's warm-up), and a kept run
  with at most one unit around it takes two direct splices. A refresh whose
  changes surround unchanged rows takes the same path, and keeps those rows'
  widgets and expansion too (a replacing splice collapses the rows it
  replaces). Unit and property tests pin the plan
  (`reconciliation_plan_keeps_rows_a_collapse_left_from_the_top`,
  `keeping_the_kept_run_reaches_the_desired_rows`); the widget test above
  fails without the cut and passes with it.
- *Task 1.4.* D12's attribution holds for the collapse: the one dispatch over
  `B_SETTLE_US` after each 0:131,072 collapse is the `unshown` probe-scan
  publication. For the re-expansion it does not: the slow frame is the
  refill's own publication, not the probe scan's, and it is the cost cut
  above. The re-expansion in the queued T1 round is the cut's first
  measurement.

**The C1 record, as it stands.** Missed margins, none over a hard bar on this
binary, carried to the final campaign and BL's follow-ups: the completions at
400:4,096 and 400:16,385 (over bar − M, under the bars, and T1 adds up to a
frame); the 56-row destroy-only dispatch at 0:64 (7.6 ms; 7,993 µs before a
re-expansion), with no rung below 64; the expansion cases' 13.7 ms frame and
6.4–6.5 ms steps; and the input dispatch's slow mode of 2026-11-19, not
reproduced. The re-expansion frame is no longer on this list: it is cut, and
measured next.

(2026-11-24, after the T1 round, "The T1 round read" below.) The record now
also holds: the 20 `INPUT_TO_FRAME_US` misses of the collapse routes listed
there (0:4,096 once, 0:16,385 twice, 0:131,072 six times, 400:16,385 four
times, 400:131,072 six times, the header once), T1 being the last rung; the
input dispatch's slow mode, back once at 8,052 µs (400:256); the 9,122 µs
two-chunk turn before a re-expansion; and the re-expansion frame again (two
frames of about 60 ms in six repetitions: the P2 cut does not hold). The
completions at 400:4,096 and 400:16,385 leave the record: the rows-after
class misses a hard bar and goes to C3, so its paced figures no longer ship.

**What follows.** T1 and the P2 cut are unmeasured. Before the final campaign
one branch-only round on the final code times them (`MEASURE-QUEUE.md`,
"T1 round", queued READY with its readings stated before it runs): A1's
cases, completion against R3's hard bars and `input_to_frame_us`. If the
rows-after class misses a hard bar there, it goes to C3 (`main`'s direct
collapse for that class), and the stage that reads the round builds C3
before Part B. Part B stays PENDING on that reading.

**Task 7.1's enforcement, as built.** Decisions, each with its reason:
- The verdict test judges every owned case at the bars as written
  (`judge_case`), and asserts once, after every case ran, on the optimized
  build: a round that misses still logs every case, so a reading never
  loses the cases after the first miss.
- R3's completion bar is computed from R3's `main` figures
  (`main_worst_completion_us`, the stage 4 campaign that R13's table also
  uses), not from the `main` side of the same campaign: the test cannot see
  the other side, and R13 fixed these bars before any round of Part B. If
  Part B's `main` side measures a worse completion than R3's, the reading
  may relax a bar by that figure; it never tightens one.
- `INPUT_TO_FRAME_US` is judged for the collapse routes (row activation and
  the folder header) only: rung T1 bounds the collapse's first frame (D6).
  An expansion's first frame waits for its scan and is judged by D2's
  dispatch bar.
- D12's residual (a dispatch over `B_SETTLE_US` that published into a store
  no row shows) is listed per case in `collapse-judged` and not judged; D2's
  recorded toolkit-floor exceptions are not excepted in advance: the reading
  of the round classifies a miss as one, with its profile.
- The `main` side (`rev/verdict-main.patch`, task 10.1) carries the same
  instrument and prints the same lines, and judges nothing.

#### The T1 round read (2026-11-24)

The coordinator ran `MEASURE-QUEUE.md`, "T1 round (rung T1 and the P2 cut,
branch only, A1's cases on the final code)", on `rev10/bin/widget-t1`
(7b88a169: task 7.1's enforcement, rung T1 and the P2 cut) on 2026-11-24 and
kept it on its fourth try under level 1 (busy CPUs below 2.4 before, 2.35
during and 2.51 after, `rev/attr2/gate.log`; tries 1 to 3 exited 76 before
running). The test missed bars, exited nonzero and was rerun once by the
widget harness in a fresh child, so the log holds two attempts of three
repetitions each; by the entry both count, and the exit status decides
nothing. Log `rev/attr2/slc-t1-base.log`. Readers in the handoff's `rev11/`:
`t1-table.py` (per case, output `t1-table.txt`) and `t1-reps.py` (per
repetition, output `t1-reps.txt`). Every rule below is the one the entry
registered before the round ran, applied to all six repetitions of every
case.

**Completion, at R3's hard bars with R9's allowance.** One case misses:
- *400:16,385:* completion 142,054 µs (attempt 1, repetition 2) and
  141,584 µs (attempt 2, repetition 2), picture final 142,083 and
  141,614 µs, against its bar of 139,300 µs. The other four repetitions are
  123.1–139.2 ms (139,213 µs in attempt 2's repetition 1, 87 µs under the
  bar).
- Every other case meets its bar in every repetition, with the picture final
  within 0.1 ms of completion: 400:64 at most 41.8 ms (bar 100 ms), 400:256
  135.9 ms (154.3), 400:4,096 **135.6 ms (137.3)**, 400:131,072 156.1 ms
  (214.6); 0:64 20.4 ms, 0:256 84.9 ms, 0:4,096 88.3 ms, 0:16,385 89.0 ms
  (each against 100 ms), 0:131,072 113.0 ms (157.3); the header 88.3 ms
  (148.3).
- **Reading: a rows-after case missed its completion bar, so the rows-after
  class (siblings 400, every size) goes to C3:** it keeps `main`'s direct
  collapse, as R13's C3 states ("a class that misses R3's completion bar with
  R9's allowance keeps `main`'s direct collapse, whatever its frames"). The
  no-rows-after class (0:N) and the folder header meet their bars in every
  repetition and keep the paced collapse. The stage that read the round
  builds C3 before Part B ("C3 as built" below), and Part B measures that
  code.

**`INPUT_TO_FRAME_US` (16,666 µs), the collapse routes.** 20 of 66
repetitions miss; T1 has no further rung, so each is recorded under C1, and
Part B judges the figure at the bar:
- 0:4,096: 19,658 µs (attempt 2, repetition 1);
- 0:16,385: 20,800 and 17,008 µs (attempt 1, repetitions 0 and 2);
- 0:131,072: all six, 17,755–23,033 µs (18,594, 23,033, 22,422; 21,579,
  17,755, 22,049);
- 400:16,385: four, 16,819–17,619 µs (attempt 1 repetition 2; attempt 2
  repetitions 0 to 2);
- 400:131,072: all six, 17,529–23,225 µs;
- the header: 19,478 µs (attempt 1, repetition 1).
The other cases meet it: 0:64 and 400:64 at 1.1–2.4 ms, 0:256, 0:4,096,
400:256 and 400:4,096 at 13.9–15.4 ms. The figures fall in three groups by
what runs before the first frame, which the attribution below follows.

**D2's pass bar (input and every step within `B_INTERACT_US`, every dispatch
within `B_SETTLE_US` except D12's residual).** Listed as data for Part B:
- the input dispatch of 400:256, attempt 2, repetition 1: **8,052 µs**; the
  other 65 inputs are at most 6,993 µs (five of them 5.5–7.0 ms, the slow
  mode Part A saw on 2026-11-19 and P1 did not reproduce);
- the re-expansion case, attempt 1, repetition 1: a between-frames turn of
  two chunks (16,181 rows) of **9,122 µs**; every other step-source
  dispatch and turn of the round is at most 5,823 µs;
- the re-expansion case, attempt 1 and attempt 2, repetition 0: a frame of
  **60,213 and 60,139 µs** (layout 59,653 and 59,594 µs), 281 ms after the
  re-expanding input, untagged (no scan terminal published in it), with the
  picture final at 341–342 ms; the other four repetitions have frames of at
  most 1.8 ms and the picture final at 1.7–15.1 ms;
- D12's residual (excepted, as registered): one `unshown` dispatch of
  30.4–46.2 ms 591–663 ms after the input in every 0:131,072 and 400:131,072
  collapse, the probe scan's publication;
- worst frames otherwise at most 8.8 ms (400:131,072), and no other
  dispatch over `B_SETTLE_US`.

**The P2 cut: does not hold.** Two of the six re-expansions have a dispatch
over `B_SETTLE_US` that is not D12's residual (the 60 ms frames above). By
the entry they are attributed before Part B, by a counter in the widget test
first, a DWARF round only if the counter is silent ("Attributions of the T1
round" below). Both slow repetitions are the ones whose collapse had run a
bound unit before the re-expansion: their between-frames turns ended 8.1 and
15.1 ms before the re-expanding input, and the fast ones' 0.7–9.3 ms before
it with rows still left or no unit yet.

### R14. Review of R13 (2026-11-18)

Reviewed against the round logs (`rev/attr2/slc-r13-*.log`,
`slc-r11-base-104.log`, read again with `rev/summ.py` and the verdict
lines' `over_interact` lists), the code (`collapse_execution.rs`,
`ui/sidebar/policy.rs`, the verdict test), and the GTK 4.24.1 sources in the
handoff.

**What holds.** R13's figures hold:
- the per-row fits: (8.0 − 3.75) / 18 rows, and (16.0 − 9.8) / 32 rows;
- the bars: 400:256's completion bar of 154.3 ms, and 400:4,096's 120.6 ms
  selection bar;
- Q4's 9.07 ms step at 400:64 rep 3;
- D18's `rows_after == 0`, which `collapse_removal_is_destroy_only` requires;
- the verdict case loop: one window per size, ended by `drop(tree.window)`
  only;
- GTK's creation inside `items-changed` and its reuse rule
  (`gtk_list_item_change_get`);
- the mapping at insertion (`gtk_widget_reposition_after`).

D25 is sound. A step armed after `after-paint` moves into the idle part of
the interval instead of delaying the next frame, and input can run between
the step and the frame. Corrected in place:

1. **The step tail is the likely gate, not the frame.** Where the worst step
   sits in the worst frame, the frame less its step is 4.8 ms. Q4's larger
   non-frame steps at 400:16,385 and 400:131,072 are chunk turns, which G2
   and G5 own. S3's attribution no longer waits for G1's build: the
   tail-profile round G0b is queued now and read by task 11.1, and S3 is
   read before S1 and S2 (R13, item 5 and the ladder).
2. **An arming gap.** D23's chunk and the first bound unit both had to run
   before the first frame. Nothing armed the source again after the chunk,
   except the between-frames idle, which runs below the frame clock. The
   source now re-arms itself when the next step is bound and the token is
   free, and under S1 within the interval's budget. Task 11.3 asserts this
   at 400:4,096.
3. **The bars read the inner timer.** `collapse_step_us_max` excludes D22's
   re-apply and the source's own work. The whole step-source dispatch is now
   timed and tagged, and that figure is what is read against 6 ms and
   `B_INTERACT_US`.
4. **A test that might not fail first.** `test_bound_units_run_outside_the_frame`
   read the store length in the clock's `update`. GTK connects its tick
   handler to `update` when the first tick callback is added
   (`gtkwidget.c`, `gtk_widget_add_tick_callback`), so whether the test's
   read comes before or after the step depends on connection order. The
   test now reads at `before-paint`, which precedes `update` in the same
   dispatch.
5. **Signal lifetime and the counter.** The `after-paint` handler's lifetime
   is stated: at most one per section; disconnected when the queue empties,
   on unrealize, on dispose and with the tick; connected again on `map`.
   A125 now also records the frame counter as read outside a frame dispatch,
   which the token now does, and the source's re-arm from its own dispatch.
6. **Spec wording.** The scenario "The first frame takes the first step"
   said that frame runs no step. All four delta files are ADDED-only, so it
   is renamed "The first frame lays out the first unit". "Every step runs in
   a dispatch of its own" contradicted the between-frames turns that run
   several chunks; it now says "every bound unit". S1, S2 and T1 change the
   scenarios they touch, and task 11.4 now updates the delta specs for the
   rung taken.
7. **Smaller.** G2's duplicated "if 4,096 misses" sentence is now one rule.
   D25 records that it shares `G_PRIORITY_HIGH_IDLE` with GtkSourceView's
   first update. Task 8.2 adds the step-source pattern to
   `.agents/rules/widget-wiring.md`. The tasks' Kani line names the
   harnesses C1–C5 apart from the closing rule's C1–C3.

## Risks / Trade-offs

- **Lighter rows touch every row** (D17). The reorder drag, the Focus Folder
  button and the drop surfaces are created on demand, so their tests, the
  accessibility smoke and the visual smokes guard them; if the acceptance
  round of task 10.4 misses its figure, the change goes back to design. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)*
  The gain is a hypothesis, not a profile: these decorations are
  `visible = false` in most rows on `main` (`row_factory.rs`), and the
  profile does not separate their share of the CSS validation (26 % of a unit
  frame) from that of the visible widgets, so on-demand creation may cut
  creation, teardown and AT-context work (buttons 5 %, images 4 %, part of
  the overlay subtrees' 19 %) more than styling. Task 10.4's round is what
  decides it.
- **The completion bar is new and tight at 400 rows after** (R3): seven
  packed frames are `main`'s frozen time, and R3's table leaves no slack at
  400:4,096 and up or at 0:131,072. If task 10.7's round shows negative
  slack anywhere, the change returns to design. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)*
- **A very large directory finishes later than `main`'s frozen collapse**
  (R9): above 16,384 flattened rows the bar allows 0.5 µs a further row
  (57 ms at 131,072 rows), a trade stated in the proposal. The arrow and the
  accessible state still flip in the first frame and nothing freezes. The
  400:131,072 figure under the reworked bookkeeping is not measured yet
  (queued); if it misses, the change returns to design with the slice-model
  or band-hiding alternatives of R9. *Superseded by R11:* the band is hidden
  (D22) and the 400:131,072 figure is measured (R11); the slice model is the
  recorded next step if R11's rounds or task 10.7 miss the selection bars.
- **D25 moves work between dispatches, not out of the main loop** (R13).
  A unit interval still holds one step and one frame, about 9–13 ms of the
  16.7 ms at unit 32, so the cadence holds; what changes is that neither
  dispatch carries both. The step source runs above the frame clock's
  priority, so a step that runs long delays the next frame by its own
  length; the step ladder (S1–S3) bounds it. A124 and A125 pin the toolkit
  behaviour it relies on; a refuted one takes R13's closing rule with the
  in-frame step.
- **Hiding the band (D22) shows an empty region while the model drains.**
  With rows after the subtree, the region below the directory row is empty
  until they arrive (at or a frame before completion). When the directory
  row is above the viewport, the whole viewport is empty until completion.
  The scrollbar describes the subtree until completion in every case.
  Accepted: no row about to disappear is drawn, the wait is bounded by the
  completion bar, and `main` shows a frozen, stale picture for the same
  cases. The visual and accessibility smokes, and D22's focus, pointer and
  assistive-technology scenarios, guard it. If A123 does not hold on a
  toolkit, the change returns to design. *(Superseded by R13, G3: D22 is
  dropped without a design loop.)*
- **D22's cost under a real assistive technology** (R12). The verdict test
  runs with `GTK_A11Y=test`, which emits nothing; with AT-SPI each `hidden`
  change of about 207 rows emits a state change and a children change, a cost
  the input dispatch then carries and no round measures. The accessibility
  smoke has no collapse case today; task 10.13 adds one (a 4,096-entry
  directory collapsed with the bus and a listener up, the tree asserted
  after it) and records its input dispatch once (from the smoke-only
  responsiveness probe's dispatch log, or a timer the case adds). If it is
  over `B_INTERACT_US` there, the hiding of rows outside the viewport moves
  to the first frame's pass.
- **Held navigation keys (D22) change when a key acts, not what it does.**
  A Down pressed during a collapse acts when the collapse ends, as on
  `main` it acts when the frozen collapse returns. A collapse that never
  ends would hold the keys forever, which D4's starvation fallback rules
  out.
- **The selection margin (R11) can leave no configuration.** If no
  configuration clears every bar by one frame, the change returns to design
  again; the margin is not relaxed to let one through. *(Superseded by R13's closing rule C1: a miss moves down R13's ladder or to C1–C3, not back to design.)*
- **R9's bar is met today only by the upper bound, and thinly.** The
  skip-cache build is the best case of cutting the path-cache bookkeeping
  alone (the further cuts task 10.3 lists, the O(store) mirror count, the
  per-row collect and the disposal hand-off, go beyond it, R9), and it
  clears 157.3 ms by
  4 ms at the worst of three non-verdict repetitions (by about 13 ms once
  the 9 ms instrument bias goes). Its three repetitions spread 112–153 ms
  (`d19a`'s 194–299 ms), so the worst of a four-round campaign can sit
  above them. A real cut that keeps any of the bookkeeping, or a quiet
  verdict that differs, can miss; task 10.7's round is the stop condition.

- *Superseded by R3's completion bar, which no longer accepts a collapse
  that finishes later than `main`'s plus two frames; kept as the stage 3
  record.* **The collapse finishes later than `main`'s frozen one** (D2): stage 1
  estimated about 130 ms at 256 entries with rows after the directory and
  about 220 ms at 131,072; the quiet campaign measured 123–135 ms (`main`
  57–73 ms) and 0.83–1.11 s (`main` 0.44–0.50 s); that the latter is
  mostly the unbound chunks and BL's listing retirement is a hypothesis,
  not a profile. Mitigation: the intent shows in the first frame
  (D6), input is handled throughout, and the constants are measured, so the
  per-frame unit grows if a later lighter-rows change lowers the per-row cost.
- **Owning the expander's click and keys.** If a future GTK adds a disclosure
  route, LushText would collapse directly through it, no worse than today.
  A119's probe fails on a toolkit that changes the routes, which is how a
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
3. Whether A120 holds on both toolkits, or D6's fallback is used (task 2).
4. Whether the expander's key bindings are reachable from LushText's rows
   today (A119, task 2). The capture controller mirrors exactly what is
   reachable, so the answer changes its key list, not the keyboard
   behaviour.
5. Whether A118 confirms that removing the unbound rows before the band
   leaves a mapped list's band in place, or D4's fallback order is used
   (task 2).

**Answers (revision after stage 4).**

1. Chosen by task 10.7 on the reworked code (R5); the sweep at 8, 16 and 32
   (R2) rules out a smaller fixed unit.
2. Yes, over budget on quiet `main` (256 entries: a 20.6–30.2 ms load
   terminal and a 41–62 ms frame), and the profile puts it in row creation
   and styling: the cap applies (D10, task 6.1).

**Answers (stage 3).**

3. A120 holds on GTK 4.24.1 / Adw 1.10.0 and on GTK 4.22.2 / Adw 1.9.0, with
   the icon-source rule (implementation decision 4); D6's primary path is
   used.
4. Not from the focus the list gives a row (the row widget keeps it), yes
   once Tab moves the focus onto the expander (A119, implementation decision
   3); the key route is installed for the 18 class bindings.
5. A118 confirms it on both toolkits: the band stays in place; D4's order is
   used (implementation decision 1).
