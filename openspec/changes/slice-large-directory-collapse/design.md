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
- The picture the user sees is final no later than `main`'s: where nothing
  follows the collapsing subtree in the realized band, its rows draw nothing
  from the first frame after the input, and otherwise the picture is final
  within the completion bar (R11, D22).
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
  (the first frame where nothing follows the subtree in the band, else the
  completion bar), and constants and go/no-go rounds select only a
  configuration one frame (16.7 ms) inside every bar; the final campaign is
  judged at the bars themselves.
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
     the tick takes bound units only.
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
case is excepted in advance.

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
  than `main`. Row lightening was a recorded follow-up (D4 "Rejected"); it
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
  takes bound units only. Their cost is cut to the toolkit's: LushText's
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
advance.

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
- for the new picture-final figure, its bar (below).

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

#### Decision

(a) + (c), as four decisions. Their mechanisms are fixed here; their
thresholds are pre-registered to the rounds below and to task 10.7.

- **D22 (new). The collapsing subtree's realized rows are hidden at intent,
  paint-only.**
  - *What:* in the input dispatch, the entry point sets, on every realized
    row in the collapsing subtree (the band read of D6 already finds them):
    the row child's opacity to 0 (`gtk_widget_set_opacity`), `can-target`
    false, and the accessible `hidden` state, true on the row child that
    D6's projection already writes.
  - *Bind:* the bind path re-applies the hidden state to a row rebound into
    a collapsing subtree, and clears it on a row rebound anywhere else.
  - *End:* the collapse's end clears it on every realized row it set, by any
    end (`Completed`, `ReExpanded`, `Removed`, a model reset, dispose).
  - *Why paint-only:* anything that changes a row's measure (visibility, a
    size request, a CSS class whose rules change size) would make the list
    realize more rows to fill the viewport (A1). That is the cost this change
    exists to bound, and it is why D4 rejected hiding or detaching the
    list, which tears every row down. Opacity and `can-target` change
    neither the row's measure nor its binding. `set_opacity` is used rather
    than a CSS class: a class change revalidates the CSS of about 200
    subtrees, and CSS validation was 26 % of a unit frame (R2).
  - *Axiom:* A123 (task 2.7) pins it on GNOME 51 and the GNOME 50 floor,
    with a probe:
    - hiding a presented list's rows this way runs no measure, setup or
      teardown, and leaves the band and the list's allocation unchanged;
    - a hidden row draws no pixels;
    - the pointer picks the list, not the row;
    - the accessible state reads `hidden`.

    If A123 is refuted on either toolkit, the change returns to design with
    the probe's figures.
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
  - *Pointer:* a click or a hover on the hidden region selects, activates,
    expands and peeks nothing, because the rows are not targets.
  - *Assistive technologies:* hidden rows report `hidden`. The directory row
    keeps D6's `expanded` false, and the accessibility smoke and
    `docs/accessibility.md` say so (task 8.3, reopened).
- **D23 (new; revises implementation decision 18). The input dispatch
  removes at most one unbound chunk.**
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
    round Q5 shows such a chunk's turn within `B_INTERACT_US`.
  - *Bound:* the input dispatch stays within `B_INTERACT_US`. At the
    skip-cache rate (about 0.18 µs a row) one 8,192-row chunk is about
    1.5 ms and a 16,384-row chunk about 3 ms, beside the 0.2 ms input and
    D22's projection. If task 10.7's round shows the
    input over `B_INTERACT_US`, the chunk shrinks to the largest power of
    two that fits, and the frame table is re-derived.
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
  - *Condition, pre-registered:* D24 is taken if round Q2 shows the
    path-cache splice costing at least one frame (16.7 ms) of 104's median
    0:131,072 completion. If it costs less, D24 is void, and the
    largest LushText cost that round Q1's profile names in the unbound phase
    is cut instead, under the same failing-first discipline.
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
  wherever nothing follows the subtree.

#### Instruments and bars

- **Completion** (`shown_us`, R3) is unchanged, and so is its bar, R3's plus
  R9's allowance. It still records the frame after which the directory row
  is GTK-collapsed, which is when the scrollbar and the model are final. The
  change does not pass by redefining it.
- **Picture final** (`picture_final_us`, new; task 10.16).
  - *Snapshot:* after each frame dispatch, the verdict test records the rows
    the viewport draws. A row counts when it is a list-view child with
    opacity above 0 that intersects the viewport; it is recorded with its
    path and its rendered top relative to the viewport.
  - *Figure:* the end of the first frame dispatch from which every later
    snapshot until settled equals the last one. The scrollbar is excluded,
    because completion owns it. `main`'s side runs the same instrument; with
    nothing hidden there, it equals `main`'s completion.
  - *Bar, nothing after the subtree in the band at intent:* the first frame
    dispatch after the input, within `INPUT_TO_FRAME_US`.
  - *Bar, otherwise:* the same case's completion bar.
  - *Printed:* each case's distance to `main`'s completion, the figure this
    revision is judged by.
- **Selection** uses the margin above on both figures; the final campaign
  uses the bars.

#### Attribution rounds queued (branch only; `MEASURE-QUEUE.md`, "R11 design attribution")

All five use the saved binary `rev5/bin/widget-attr-104`, whose switches
need no build. They run in the coordinator's quiet window, one round at a
time, at a 1-minute load below 4.

The perf-prefixed form has never run through the fixed `run-bin.sh`, which
now goes through `scripts/headless-session`: a launch failure there is
void, not a result. Task 10.12 reads the rounds before any code of D22–D24. Q4 runs last, because its gate reads Q2 and Q5.

| round | factor varied | cases | purpose and pre-registered use |
|---|---|---|---|
| Q1 `slc-r11-pdw-104` | none (DWARF profile, 4 kHz, 16 KiB stacks) | 0:131,072, one rep | 104's split by phase: unbound, tail, retirement, evidence. It names D24's target if Q2 voids the path-cache one |
| Q2 `slc-r11-skipcache-104` | `SLC_SKIP_CACHE=1` | 0:131,072 and 400:131,072, three reps | the path-cache splice's share on the real code. D24 is taken if the median 0:131,072 completion falls by at least 16.7 ms |
| Q3 `slc-r11-d128-104`, `slc-r11-d256-104` | `SLC_DESTROY=128`, then `256` | collapse, 0 rows after, at 256, 4,096 and 131,072, three reps | whether a larger destroy-only unit cuts the tail by whole frames with the step within 8 ms and the frame within 12 ms. It bounds task 10.7's candidates |
| Q5 `slc-r11-c16k-104` | `SLC_CHUNK=16384` | collapse at 0:131,072 and 400:16,385, three reps | a 16,384-row chunk's turn (the worst step) against `B_INTERACT_US`, which decides whether D23's chunk can cover 400:16,385 |
| Q4 `slc-r11-base-104` | none (constants 32 / 64 / 8,192) | every collapse case and the header, three reps | the real code's completion per case. **Gate:** for each case, predict the reworked completion as Q4's worst, less one frame where D23 applies (the subtree has unbound rows, and they fit one chunk of the size Q5 admits: 16,384 if its worst step fits `B_INTERACT_US`, else 8,192), less Q2's median gain where D24 applies (subtrees above 16,384 rows). If any prediction exceeds bar − M, the change returns to design before D22–D24 are coded. Task 10.4's 12 ms acceptance is read here too (unit 32, worst frame at 400:256) |

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
  class nor its CSS, because hiding uses no CSS.

## Risks / Trade-offs

- **Lighter rows touch every row** (D17). The reorder drag, the Focus Folder
  button and the drop surfaces are created on demand, so their tests, the
  accessibility smoke and the visual smokes guard them; if the acceptance
  round of task 10.4 misses its figure, the change goes back to design.
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
  slack anywhere, the change returns to design.
- **A very large directory finishes later than `main`'s frozen collapse**
  (R9): above 16,384 flattened rows the bar allows 0.5 µs a further row
  (57 ms at 131,072 rows), a trade stated in the proposal. The arrow and the
  accessible state still flip in the first frame and nothing freezes. The
  400:131,072 figure under the reworked bookkeeping is not measured yet
  (queued); if it misses, the change returns to design with the slice-model
  or band-hiding alternatives of R9.
- **Hiding the band (D22) shows an empty region while the model drains.**
  With rows after the subtree, the region below the directory row is empty
  until they arrive (at or a frame before completion). When the directory
  row is above the viewport, the whole viewport is empty until completion.
  The scrollbar describes the subtree until completion in every case.
  Accepted: no row about to disappear is drawn, the wait is bounded by the
  completion bar, and `main` shows a frozen, stale picture for the same
  cases. The visual and accessibility smokes, and D22's focus, pointer and
  assistive-technology scenarios, guard it. If A123 does not hold on a
  toolkit, the change returns to design.
- **Held navigation keys (D22) change when a key acts, not what it does.**
  A Down pressed during a collapse acts when the collapse ends, as on
  `main` it acts when the frozen collapse returns. A collapse that never
  ends would hold the keys forever, which D4's starvation fallback rules
  out.
- **The selection margin (R11) can leave no configuration.** If no
  configuration clears every bar by one frame, the change returns to design
  again; the margin is not relaxed to let one through.
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
