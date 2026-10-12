# Long-line documents — programme record (C0–C5)

> Planned-work record for the long-line responsiveness programme. The origin
> proposal, with its review and the reasoning behind the split, is
> [`long-line-responsiveness.md`](long-line-responsiveness.md); this record is the
> one that tracks the programme's state. `probe-long-line-toolkit-costs` (C0)
> created it and filled in the measured values; every later change updates it in
> the same change that advances it.

LushText must stay instantly responsive (open, scroll, every interaction) with
documents of any admitted size, and must never change a file's bytes. A 5 MB
HTML file with a 1.9-million-character line froze the app for seconds. The cost
that freezes it is **paragraph length**, not file size.

## Sequence and gates

| # | Change | Delivers | Gate before the next change |
|---|---|---|---|
| C0 | `probe-long-line-toolkit-costs` | Axioms A23–A37, the synthetic fixture generator, the smoke-only in-process probe, `make responsiveness-smoke`, the budget constants, and the measured values below | Values and the budget verdict recorded here (this record). |
| C1 | `document-scale-policy` | Paragraph profile on the worker, one GTK-matching paragraph-boundary function (fixes the `\n`-only slicer), `DocumentScale` replacing `FileSizeCheck`, gates for syntax/minimap/invisibles/search highlighting at load and on edits, the minimap-detach CRITICAL fix | The context-engine stall and both CRITICALs gone, verified on the lane. |
| C2 | `long-line-protected-view` | Protected View (view-only, caret-free) for giant-paragraph documents, the display-only fold, the Line Inspector, find on a worker | Every interaction offered on these documents within the budgets. |
| C3 | `sliced-protected-install` (conditional) | Sliced install of a giant protected paragraph | Adopted only if the lane's `install` case, run against C2, exceeds B_settle. |
| C4 | `bound-window-construction-dispatch` | Removes the window-construction stall | Window construction within B_settle per dispatch. |
| C5 | `explore-editing-long-paragraphs` (optional) | Exploration of editing giant paragraphs | Maintainer decision. |
| C6 | `bound-post-open-settle` (candidate, recorded by C0's budget verdict) | Bound the first main-loop turns after load readiness: the small-document post-open stall, the HTML first-highlight stall, the first overlay-indicator frame | The baseline fits B_settle and 2 reference frames on an idle reference run. Does not block C1–C4. |

C1–C4 are drafted; none may be **implemented** before this record holds the
values they use (`responsiveness-measurement` spec).

## Maintainer decisions (2026-10-11)

1. Protected View now: giant-paragraph documents are view-only; editing them is
   deferred to an optional C5.
2. The syntax total-size cutoff S_syntax is measured by C0, not fixed in
   advance.
3. Budgets, stricter than the origin proposal's 16/50 ms: **B_interact = 8 ms**
   per interaction dispatch, **input-to-presented-frame ≤ 2 frames**, and
   **B_settle = 16 ms** per settle dispatch. They are read against **main-loop
   stalls** (the longest run of dispatches without returning to poll).
4. A **fixed 120 Hz reference frame** (8,333 µs) for every frame-relative limit:
   input-to-frame ≤ 16,666 µs, scroll p95 frame gap ≤ 8,333 µs.
5. The **reference machine** is the maintainer's desktop (`sophia`, inside its
   toolbox, headless, at most 16 CPUs); CI runs are trend evidence only and never
   fail on a budget.
6. L_protect is a fixed constant (not a GSettings key, for now).
7. Fixtures are synthetic only; the reproducer file is never committed or copied.

The constants live once in `crates/lushtext-core/src/model/responsiveness_budget.rs`
(`B_INTERACT_US`, `B_SETTLE_US`, `REFERENCE_FRAME_US`, `INPUT_TO_FRAME_FRAMES`,
`SCROLL_P95_FRAME_GAP_US`), with Kani-proved classification arithmetic. Later
changes read them and never restate the numbers.

## Measured values (reference run)

**Reference set `rr-c0-reference-20261011`**: the baseline and dense groups of a
`full`-tier start (`rr-20261011T234802Z-toolbx-3121571`,
`rr-20261011T235228Z-toolbx-3134791`) joined with the 33 parts of the reduced
reference sweep (`scripts/responsiveness-reference-sweep.py`, design D7's
documented deviation; every part's run id, load and co-running processes are in
the table below). Headless `mutter 50.4 --virtual-monitor 1280x800@120`,
`GSK_RENDERER=cairo`, caret blink off, GTK 4.22.5, Libadwaita 1.9.3, Pango
1.57.1, GtkSourceView 5.20.0, GLib 2.88.3, kernel 7.2.9-200.fc44, Intel Core
Ultra 9 285K, affinity CPUs 8–23 (16 E-cores, inherited, never widened).
Watchdog tick 2 ms, idle slack ε 928 µs. 1,499 fixture-integrity checks, 0
violations; 2 of the bisection rows were storage-contaminated and excluded by
the lane. Axiom observations: a fresh `make gtk-axioms` on the same tree (A31:
112 rows per 2016 px step at the default font, wrap off).

| Value | Derived | Bracket (pass / first fail) | Rule (lane `report.md`) |
|---|---|---|---|
| `L_syntax` | **2,000** characters | 2,000 / 3,000 | `.html` minus `.txt` at the same length: scroll ≤ B_interact, open and background ≤ B_settle; worst flavour (`script`) decides; worst variant |
| `L_minimap` | **50,000** | 50,000 / 74,000 | minimap on minus off (`.txt`, same wrap): scroll ≤ B_interact, open and background ≤ B_settle |
| `L_invisibles` | **50,000** | 50,000 / 74,000 | invisibles on minus off while scrolling ≤ B_interact |
| `L_search_highlight` | **6,700** | 6,700 / 10,000 | every find and match-navigation stall ≤ B_interact |
| `N_visible` | **500,000** | 500,000 / 710,000 | plain `.txt` scroll stalls ≤ B_interact, wrap on and off |
| `L_protect` | **not derived** | fails at 500 (the floor) | caret, type and click stalls ≤ B_interact **and** input-to-frame ≤ 2 reference frames: the frame half fails at every length, because the 2-frame limit sits on the lane's double-buffered 120 Hz floor (an input landing just after a frame start waits one frame for the next frame and one to present, 16.7–19.3 ms), which the baseline exceeds too |
| `L_protect_stall` (informational) | **500** | 500 / 710 | the stall half of `L_protect` alone, worst variant; typing at the start of an unwrapped 2,000-character line already stalls 50 ms, and 1.26 s at 10,000 |
| `V_batch` | **56,000** visible characters per 2000 px batch | dense line 500 / 710 | (largest dense line length) × A31's 112 rows; scroll and Page Down as measured, background as its excess over the `baseline/small` post-open stall |
| `R_batch` | **224** rows | — | A31's 112 rows per batch at the default font, doubled for the 50 % minimum zoom |
| `N_dense` | **250** characters | — | `V_batch ÷ R_batch` |
| `N_inspect` bound | **not derived** (literal); **500** from `L_protect_stall` | — | `min(N_visible, L_protect)`; C2 chooses at or below `min(N_visible, L_protect_stall)` = 500 until an idle re-confirmation or the user-session validation says otherwise |
| `S_syntax` | **not derived** | fails at 16,000 bytes (the floor) | `.html` minus `.txt` over the same short-line bytes, every dispatch ≤ B_interact: the `.html` background phase carries a **size-independent** ~42 ms stall (57–61 ms against 14–17 ms for the same bytes as `.txt`, identical at 16 kB and 62 kB; the HTML language and first highlight in a fresh process), which no size cutoff removes |
| `S_syntax_interact` (informational) | **16,000** bytes | 16,000 / 31,000 | `S_syntax` without the background phase: scroll, Page Down, caret and typing differences ≤ B_interact (typing in 62 kB of HTML costs 12–16 ms against 2–3 ms as `.txt`) |

C3 (`sliced-protected-install`) is not decided by C0: the reduced sweep does
not run the `install` group, and the programme gate decides C3 from the install
case run against C2 (A36 records the btree cost evidence).

**Window construction (C4's evidence).** First frame 150–196 ms after spawn
(median 182 ms over 18 baseline launches); the open phase's worst stall
60–81 ms (median 71 ms). Owner: `bound-window-construction-dispatch` (C4).

### Budget verdict (design rule D7, literal)

Run `rr-c0-reference-20261011`, baseline cases (`baseline/empty`,
`baseline/small`; 3 variants × 3 repetitions each), window construction
excluded:

- **B_interact = 8 ms: within.** Worst interaction stall 2.26 ms (bounds
  1.3–4.3 ms).
- **B_settle = 16 ms: over.** Worst settle stall 19.6 ms (bounds 18.6–21.6 ms),
  the post-open stall of `baseline/small` (plain repetition 1).
- **Input-to-frame ≤ 2 frames: over.** Worst 25.3 ms: the first wheel step on
  the empty, unscrollable document draws only the overlay-scrollbar indicator,
  whose frame starts 13–17 ms after the handler and is never presented by the
  compositor (counted one reference frame after it started), in every
  repetition. Keys, clicks and Page Down: median 4–5 ms, p95 12 ms, maximum
  16.7–18.0 ms (the double-buffered floor plus jitter).
- **Scroll p95 frame gap (informational, not a D7 input): 9.7 ms** over
  presented frames, against 8.3 ms.

The lane's literal verdict is **revise** (it proposes `B_SETTLE_US = 22000`).
**Maintainer decision (2026-10-11): the targets stay at B_interact = 8 ms and
B_settle = 16 ms (and 2 reference frames); the constants in
`model/responsiveness_budget.rs` are not relaxed, and task 7.2 is recorded as a
maintainer deviation.** Every over-budget stall has an owner:

| Over-budget stall | Owner |
|---|---|
| Window construction (first frame 150–196 ms, open stall 60–81 ms) | C4 `bound-window-construction-dispatch` |
| Long-line costs: every threshold above, the context-engine cut-off, the minimap and invisibles costs, typing in long paragraphs | C1 `document-scale-policy` and C2 `long-line-protected-view` |
| The post-open settle stall of a small document (12–20 ms within 3 ms of load readiness, on a 400-byte `.txt`), the size-independent ~42 ms first-highlight stall of an HTML document in a fresh process, and the first-wheel overlay-indicator frame on an unscrollable document (21–25 ms) | **New candidate C6 `bound-post-open-settle`** (below) |

**New candidate C6 `bound-post-open-settle` (not yet proposed).** Bound the
work that runs in the first main-loop turns after a document's load readiness:
the 12–20 ms stall a 400-byte `.txt` shows within 3 ms of `file-open-complete`
(the first layout and paint of the opened document plus the post-publish
projections), the ~42 ms HTML language and first-highlight stall, and the
13–17 ms delay before the first overlay-scrollbar indicator frame. Gate: the
baseline fits B_settle and 2 reference frames on an idle reference run. It does
not block C1–C4.

### Load during the reference run, and what it can move

The run shared the machine with a game (`dota2`, about 2.5 CPUs on P-cores
0–7), a Windows VM (`qemu`, about 0.6 CPU) and, in later parts, `java`,
`pwsh` and Flutter test processes; the lane itself ran on E-cores 8–23. Load
only ever adds latency, so a length that **passed** under load also passes on
an idle machine: every derived value is a **lower bound** (conservative for C1
and C2), and an idle run can only raise a threshold or turn a fail into a pass.

- Every **passing** bracket point passed in every variant and repetition
  (`L_syntax` 10/10, `L_minimap` 10/10, `N_visible` 10/10,
  `L_search_highlight` 11/11, `L_protect_stall` 19/19, `L_invisibles` 10/10,
  `S_syntax_interact` 10/10, `V_batch` 18/18).
- Every **failing** bracket point failed in every run except three, which are
  the values contamination plausibly lowered: `L_search_highlight` at 10,000
  failed 6 of 19 runs, `L_protect_stall` at 710 failed 8 of 10, and
  `S_syntax_interact` at 31,000 failed 2 of 10. `L_syntax`, `L_minimap`,
  `N_visible`, `L_invisibles`, `V_batch` and `S_syntax` failed in every run, so
  load cannot have moved them.
- **Baseline spread.** `baseline/small` background (the B_settle decider):
  plain 14.3 / 17.9 / 19.6 ms, AT 13.7 / 15.4 / 15.9 ms, probe IM 12.4 /
  12.5 / 14.5 ms (max ÷ median ≤ 1.4, not noisy); 3 of 9 repetitions are within
  B_settle, 4 indeterminate, 2 over. The B_settle "over" therefore rests on
  the two worst repetitions and is plausibly load-sensitive. The input-to-frame
  "over" is structural (the overlay-indicator frame, in every repetition) and
  is not.

| Start (-03) | Part | Run id | Load (1 min) | Co-running heavy processes (≥ 1 CPU) |
|---|---|---|---|---|
| 20:48 | full/baseline | rr-20261011T234802Z-toolbx-3121571 | 9.01 | dota2 ~217% CPU, win11 VM (qemu ~63%) |
| 20:52 | full/dense | rr-20261011T235228Z-toolbx-3134791 | 6.05 | dota2 ~217% CPU, win11 VM (qemu ~63%) |
| 21:25 | reduced/r0-anchors-caret | rr-20261012T002526Z-toolbx-3423529 | 3.02 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:27 | reduced/r0-anchors-find | rr-20261012T002733Z-toolbx-3426828 | 3.14 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:28 | reduced/r0-anchors-layout | rr-20261012T002848Z-toolbx-3429219 | 3.49 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:37 | reduced/r0-anchors-total | rr-20261012T003714Z-toolbx-3441722 | 5.92 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:38 | reduced/r1-bisect-caret | rr-20261012T003842Z-toolbx-3444858 | 8.41 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:39 | reduced/r1-bisect-find | rr-20261012T003938Z-toolbx-3451338 | 10.93 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:40 | reduced/r1-bisect-layout | rr-20261012T004025Z-toolbx-3457547 | 10.02 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:42 | reduced/r1-bisect-layout-large | rr-20261012T004236Z-toolbx-3473456 | 11.06 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:43 | reduced/r1-bisect-total | rr-20261012T004329Z-toolbx-3487872 | 11.28 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:44 | reduced/r2-bisect-find | rr-20261012T004432Z-toolbx-3494852 | 8.59 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:45 | reduced/r2-bisect-layout | rr-20261012T004520Z-toolbx-3495997 | 7.15 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:47 | reduced/r3-bisect-layout | rr-20261012T004740Z-toolbx-3499771 | 5.61 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:48 | reduced/r3-bisect-layout-large | rr-20261012T004831Z-toolbx-3500943 | 5.68 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:49 | reduced/rc-bracket-caret | rr-20261012T004925Z-toolbx-3502309 | 6.40 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:52 | reduced/rc-bracket-find | rr-20261012T005228Z-toolbx-3507155 | 6.01 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 21:55 | reduced/rc-bracket-layout | rr-20261012T005546Z-toolbx-3519128 | 4.79 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 22:25 | reduced/rc-bracket-layout-large | rr-20261012T012524Z-toolbx-3624569 | 8.61 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 22:28 | reduced/rc-bracket-total | rr-20261012T012816Z-toolbx-3644797 | 9.62 | not sampled; dota2 (~250%) and the win11 VM ran throughout |
| 22:33 | reduced-2/r1-bisect-caret | rr-20261012T013357Z-toolbx-3785616 | 11.54 | dota2:257%,java:284%,java:236% |
| 22:34 | reduced-2/r1-bisect-find | rr-20261012T013452Z-toolbx-3793862 | 10.67 | dota2:257%,java:103%,java:293% |
| 22:35 | reduced-2/r1-bisect-dense | rr-20261012T013556Z-toolbx-3799630 | 9.63 | dota2:257%,java:350% |
| 22:36 | reduced-2/r1-bisect-total | rr-20261012T013652Z-toolbx-3815740 | 8.98 | dota2:257% |
| 22:37 | reduced-2/r2-bisect-caret | rr-20261012T013756Z-toolbx-3826433 | 10.51 | dota2:257%,java:278% |
| 22:38 | reduced-2/r2-bisect-find | rr-20261012T013850Z-toolbx-3830753 | 6.83 | dota2:257%,ps:200% |
| 22:39 | reduced-2/r2-bisect-dense | rr-20261012T013937Z-toolbx-3835566 | 6.72 | dota2:256%,java:273%,ps:200% |
| 22:40 | reduced-2/rc-bracket-caret | rr-20261012T014034Z-toolbx-3849226 | 5.61 | dota2:256%,java:395% |
| 22:46 | reduced-2/rc-bracket-find | rr-20261012T014606Z-toolbx-4021856 | 14.10 | dota2:252%,python:100%,java:370% |
| 22:49 | reduced-2/rc-bracket-dense | rr-20261012T014934Z-toolbx-4065603 | 13.43 | dota2:252%,java:188%,java:416%,ps:200% |
| 22:55 | reduced-2/rc-bracket-total | rr-20261012T015533Z-toolbx-4113948 | 17.58 | dota2:252%,dart:frontend_s:116%,flutter_tester:132% |
| 23:00 | reduced-3/r1-bisect-total | rr-20261012T020013Z-toolbx-36551 | 12.44 | pwsh:155%,dota2:253% |
| 23:01 | reduced-3/r2-bisect-total | rr-20261012T020117Z-toolbx-86765 | 11.85 | java:250%,pwsh:170%,dota2:254% |
| 23:02 | reduced-3/rc-bracket-total | rr-20261012T020222Z-toolbx-108535 | 12.18 | pwsh:121%,java:253%,dota2:254% |

### Re-confirm on an idle machine

One resumable command (about 30 minutes) re-measures the baseline (3 variants
× 3 repetitions) and every bracketing point above (plain, 3 repetitions), then
writes `reconfirm.json` comparing each point's idle verdict with the loaded one
and a fresh `report.md`. Run it with nothing else heavy on the machine (no
game, VM, build or test run); re-running the same command resumes it:

```sh
make responsiveness-build
scripts/responsiveness-reference-sweep.py --out build/smoke/responsiveness-reconfirm \
    --reconfirm docs/next/long-line-documents-brackets.json \
    --axiom-observations <a fresh make gtk-axioms log>
```

The brackets of this run are committed beside this record
(`long-line-documents-brackets.json`, the sweep's `brackets-final.json`).
A point that flips from fail to pass raises its threshold to the next bracket
(re-run the sweep with `--prior` and `--only` for that threshold); a passing
point cannot flip.

## Axioms each change cites

| Change | Axioms |
|---|---|
| C1 `document-scale-policy` | A27 (tag changes do not dirty the buffer), A29 (context engine cut-off), A31 (2000 px validation batches, the source of R_batch), A32 (source-map detach CRITICAL) |
| C2 `long-line-protected-view` | A23, A24, A25, A26, A28, A30, A33, A34, A35 |
| C3 `sliced-protected-install` | A24, A35, A36 |
| C0 itself (the scrollbar-warning fix below) | A37 |

Two probes corrected the designs that cite them, and C1/C2 must read the
ledger rows rather than their own earlier text:

- **A33.** GtkSourceView's `SearchSettings:visible-only` defaults to **true**:
  both the plain and the regex search skip invisible text by default. With it
  off, the plain search sees hidden text and the regex search still does not.
  C2 must set `visible-only` explicitly for whichever behaviour it relies on.
- **A29.** The context engine's 2000 ms cut-off fires at tens of thousands of
  characters, not near two million. On the reproducer's own lines (a
  single-line prefix highlighted inside its `<style>`/`<script>` context):
  CSS lines 8k → 128–131 ms, 16k → 280–430 ms, 32k → 0.96–1.58 s,
  64k → 4.3–5.4 s, 128k → cut-off; a script line 8k → 685 ms, 32k → cut-off;
  a markup line 8k → 345 ms, 32k → 2.5 s. Without its `<style>` context the
  longest line highlights in under 1 ms (it is then plain text). The synthetic
  `minified-html` longest line matches the markup and script lines (8k → 337 ms,
  32k → cut-off); the lane's `paragraph` kind has a `script` flavour calibrated
  to be at least as costly (~0.75 s at 8k, cut-off by 16k), and L_syntax takes
  the worst flavour.

Other recorded probe values (GTK 4.22.5, Adw 1.9.3, Pango 1.57.1,
GtkSourceView 5.20.0): A23 first layout of a 200k-character paragraph
177–217 ms wrapped, 34–42 ms unwrapped; A24 hiding a 200k tail 1.6 ms, showing
it 202–211 ms; A26 one log-attrs recompute in a 300k paragraph ≈ 17 ms; A28
Pango x positions overflow int32 at 238,319 monospace characters; A30 the
surrounding text is the whole paragraph (50,000 characters), requested on caret
moves, `move-cursor`, typing and scroll; A31 about 2016 px per validation step,
112 rows per batch at the default font with wrap off; A33 plain search
18–19 ms per match at 200k.

## Pre-existing blockers reproduced by C0

None is allowlisted; each is reported by the lane as a finding with its owner, and one
fixed here fails the lane if it reappears.

| Blocker | Owner |
|---|---|
| `GtkSourceMap` detach with a pending update tick emits `Gtk-CRITICAL` and `GLib-GObject-CRITICAL` (axiom A32) | C1 |
| The install slicer splits only on `\n`, so CR-only and U+2029 documents install in one GTK turn | C1 |
| Window construction stalls the main loop for about 190 ms on every open | C4 |
| `Gtk-WARNING: Trying to snapshot GtkGizmo … without a current allocation`, the trough of the editor's horizontal or vertical scrollbar. **Fixed in C0** (pre-existing-blockers rule), so it is no longer owned by C1. Root cause, confirmed against GTK 4.22.5 and GtkSourceView 5.20.0 (ledger **A37**): `GtkTextView` validates invalidated lines from an idle at `GTK_PRIORITY_RESIZE - 2`, but a change made inside a frame — the key press of the smallest reproduction (one 500-character line, wrap off, Ctrl+Home and typing) is handled in the frame clock's flush-events phase — or just before a frame that input started (`gdkframeclockidle.c` runs it from `flush_idle` at `GDK_PRIORITY_EVENTS + 1`; find in dense and 1 MB documents, where GtkSourceView's search scan changes tags from an idle) leaves that idle pending at paint time. The view is not re-allocated in that frame's layout, so `gtk_text_view_paint` flushes the validation inside the snapshot (`gtk_text_view_flush_first_validate` → `gtk_text_view_update_adjustments`): a widened line or a queued scroll changes the scroll adjustments, `GtkRange` queues its trough's allocation, and `GtkScrolledWindow` snapshots the scrollbar after its child. Lines that fit the viewport never change `upper`, which is why they never warned. The `mark-set` handlers that run inside GtkSourceView's snapshot were ruled out: disabling LushText's Notes handler left the warning in place. Fix: `ui/editor_page/frame_validation.rs` queues the view's allocation on every edit and caret move, and before the paint (at `before-paint` or after the frame clock's layout handlers) after a tag change, so `gtk_text_view_size_allocate` flushes the validation in layout. Evidence: widget tests `editor_page::test_typing_in_a_long_line_keeps_adjustment_changes_out_of_the_snapshot` (20 adjustment changes inside the paint before the fix, 0 after) and `editor_page::test_tag_changes_in_frames_keep_adjustment_changes_out_of_the_snapshot` (8 without the tag handling, 0 with it), probe A37, and the lane: the minimal case and dense 500/2000/50000, total-size 1/5 MB and paragraph 500/15000/22000 (all phases) show no warning, nor do four more repetitions each of dense 500 and total-size 1 MB (before the fix dense 500 warned on find in 3 of 5 runs). The lane classifier now treats this warning as a **regression that fails the run** (`FIXED_DEFECTS` in `scripts/responsiveness-smoke-driver.py`), not as an owned finding. Residual, not observed: a size-changing tag that GtkSourceView applies inside its own snapshot (highlighting the visible region) is still validated in that paint; the editor validates it in the next frame's layout | fixed in C0 |



## Headless measurement notes (task 6.0 spike)

- Mutter's `org.gnome.Mutter.RemoteDesktop` works inside the private
  `dbus-run-session`: keysym, relative motion, axis and button injection reach
  GTK with no screencast session. The first keyboard and pointer event after
  `Start` can be dropped, so the driver warms both devices up.
- Keys reach the window only after a click **inside the text area** (a
  header-bar click starts a compositor grab), so every case clicks first.
- `gdk_event_get_time()` is `CLOCK_MONOTONIC` in milliseconds, so the
  compositor timestamp is a valid latency origin (it lies between the injection
  and the handler).
- `--virtual-monitor 1280x800@120` is accepted. With `GSK_RENDERER=cairo` the
  frame clock presents at 8,333 µs; with the GL renderer the client got no
  presentation times and ran at 60 Hz, so the GL variant is optional.
- `ibus-daemon` is not installed on the reference host: the IBus variant is
  recorded as skipped, and the probe input method stands in for it (it requests
  the surrounding text at every entry point an IBus engine with that capability
  does). GTK 4.22 renamed the `ngl` renderer to `gl`.
- The probe input method needed two workarounds for gtk4-rs subclassing gaps:
  its `get_preedit_string` trampoline writes NULL out-parameters (GtkText passes
  NULL while realizing, which segfaulted), and `set_surrounding_with_selection`
  is not overridable, so GtkIMContext's own implementation swallowed the answer.

## Lane measurement corrections (before the reference run)

Quick run #4 (`rr-20261011T223041Z-toolbx-2000205`, non-reference: it ran
beside `make test`) exposed four derivation and measurement flaws. Each is now
fixed with a failing-first case in the driver's self-test:

- **Capped pairs read as passing.** When both sides of a difference (html and
  txt, minimap on and off) hit the 20 s interaction cap, the difference was about
  zero and read as "within", which produced L_syntax = N_visible = 250000. A
  capped phase on the measured side, or a phase never reached because the case
  hung, is now a failure.
- **The settle half ignored the background phase.** The context engine's
  2000 ms cut-off fires after load readiness, in the background phase; the settle
  half of L_syntax and L_minimap now reads `open` and `background`.
- **Input-to-frame attribution (the 102 ms / 13 frames on the empty
  document).** Two artifacts: the injected `Control_L` press of Ctrl+Home is its
  own input record and was matched instead of Home, and Ctrl+Home on an empty
  document changes nothing, so the next frame (caused by the following arrow key
  100 ms later) was charged to it. Modifier presses are now recorded as
  unmeasured injections, and a frame that starts after the next input was handled,
  while the main loop was free in between, is reported as `no-visible-change`.
  Two compositor effects of headless Mutter were also removed from the figure:
  - with the caret blinking, the blink's frames get no presentation feedback,
    so GTK paces them on its 60 Hz fallback timer and every input waits up to
    16.7 ms for the next tick (Page Down median 18.6 ms with blink, 10.8 ms
    without; lane experiments `exp-blink0`/`exp-blink1`). The lane now runs
    with `gtk-cursor-blink=false`;
  - after an idle period the compositor presents a lone frame only when the
    next commit arrives (a frame started 1 ms after its key press was
    "presented" 52 ms later). A presentation more than two reference frames
    after its frame started while the main loop was free now counts as one
    reference frame after the start; a presentation delayed by a stall (a long
    paint) keeps its real time.

  With these corrections the baseline input-to-frame is 9–11 ms for keys and
  clicks and at most ~17 ms (the double-buffered 120 Hz floor, an input landing
  just after a frame start waits one frame for the next and one to present)
  except the first wheel step on an unscrollable document (21–29 ms, the
  overlay-scrollbar indicator's first frame).
- **The ~17 ms frame-gap p95 under cairo at 120 Hz is a lane limitation, not an
  app verdict.** Across presented frames the gaps are 8.3 ms (p95 8.9 ms over
  434 gaps); every gap above 12 ms (193 of 240) touched a frame the compositor
  never presented, the caret-blink frames above. Frame gaps are now read between
  presented frames only, and a gap over four reference frames while the main
  loop was free is idle time, not a dropped frame. The scroll frame-gap limit
  remains informational in the budget verdict (D7 confirms 8/16 ms and 2
  frames).

A fifth flaw surfaced on the reference sweep's own anchors: **S_syntax read the
generic post-open stall as syntax cost.** A 400-byte `.txt` already shows a
12–20 ms stall right after load readiness, so "every dispatch ≤ B_interact"
failed at every size. S_syntax is now syntax-attributable like L_syntax: the
`total-size` group opens the same short-line bytes as `.html` and as `.txt`, and
the rule reads the difference.

## Carried-forward decisions

- L_protect as a GSettings key (decision 6 keeps it a constant for now).
- "Copy whole line" from the Line Inspector.
- Upstream reports: the `GtkSourceMap` `update_id` that survives
  `set_view(NULL)` (A32), Pango's int32 width overflow (A28), log attrs over
  invisible text (A26), the IM spot updated on every scroll (A30), search
  copying the line per match (A33), and the gtk4-rs `IMContextImpl`
  `get_preedit_string` trampoline that writes NULL out-parameters.
- Validation in the user's session (an isolated-XDG launch with consent) is a
  C2 precondition, not a C0 deliverable.
- Whether C0 may be archived while the reproduced blockers stay open with their
  owners (design Open Question 6).
