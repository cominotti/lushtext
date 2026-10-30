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
| C4 | `bound-window-construction-dispatch` | Removes every GSettings write from window construction and interaction dispatches (coalescing settings writer, pure split-view restore plan, axioms A38–A41); construction 135–151 → 22–24 ms (fresh keyfile profile) | Window construction within B_settle per dispatch: **not met** (22–26 ms residual of `present` and template; keyfile flush 12.8–17.9 ms); maintainer decisions recorded under "C4" below. |
| C4b | `first-frame-window-shell` (proposed 2026-10-20, revised after review the same day) | The window built whole in one dispatch with the content the first frame does not need invisible and its breakpoints installed before map, then realized, presented, and revealed in bounded, frame-gated dispatches (axioms A46–A50; content fill-in accepted up to 40 ms) | Every dispatch from `open` until composition complete within B_settle on runs meeting the reference precondition, or the A46 realize floor recorded as not met with an upstream report. **Met (idle verdict 2026-10-29):** every dispatch ≤ 10.7 ms in every profile and repetition (before C4b: 22.5–25.9 ms before the first frame). **Open:** content fill-in 51–61 ms against the accepted 40 ms, a maintainer decision ("C4b" below). **Corrected 2026-10-29 (C4c):** its first-frame figure was an unseen frame on the unmapped window (A51); the real first frame was ≈ 46 ms from the request, no earlier than before C4b, and the honest fill-in 29–39 ms. |
| C4c | `shorten-window-content-fill-in` (2026-10-29) | Realize and present in one dispatch (no frame on the unmapped window, axiom A51), one reveal dispatch after the first frame, the first frame measured on the mapped window everywhere (evidence, settings writer gate, lane), the fill-in counted to the first frame after the activation's opens ran, and a populated lane profile | Every dispatch until composition complete (and activation shown) within B_settle in all five profiles; fill-in ≤ 40 ms per run and median ≤ 3 reference frames; first painted frame earlier than C4b's. **Met (interleaved verdict 2026-10-29):** largest dispatch 13.2 ms, fill-in 20.4–25.9 ms (median 21.4), first painted frame 30.0–33.2 ms against `main`'s 45.1–47.9 ms ("C4c" below). |
| C5 | `explore-editing-long-paragraphs` (optional) | Exploration of editing giant paragraphs | Maintainer decision. |
| C6 | `bound-post-open-settle` (proposed 2026-10-30) | Bound every dispatch from the activation turn until the opened document is settled, including the validation of dense documents of short lines that stay editable (handed over by C2's revision, open question 11). The proposal's attribution, exploratory on `main` b7f56d45: small `.txt` post-open dispatches are already ≤ 14.9 ms. The over-budget sources are the HTML language-definition parse in the activation dispatch (36–40 ms; the "first highlight"), the highlight attach after a sliced install (71 ms at 5 MB), GtkSourceView's background analysis slices (40–47 ms), and the validation of dense 710-character lines (21.6 ms). Every LushText projection costs < 0.3 ms. | Every post-open dispatch until `document-settled` ≤ B_settle and every input during settle dispatched within B_interact, interleaved with `main`, 4 profiles × 7 repetitions (design D8). Input-to-frame reported, not gated, pending a maintainer decision. Does not block C1–C4. |

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

> **Superseded in part on 2026-10-13.** The idle re-confirmation below corrected
> three derivation rules and changed `L_search_highlight` (6,700 → 10,000) and
> `L_minimap` (50,000 → 22,000); every other value was confirmed. C1 and C2 read
> the final values in "Idle re-confirmation (2026-10-13)"; this table stays as
> the loaded run's record.

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
| `N_inspect` bound | **not derived** (literal); **500** from `L_protect_stall` | — | `min(N_visible, L_protect)`; C2 chooses at or below `min(N_visible, L_protect_stall)` = 500 (the idle re-confirmation kept both inputs) until the user-session validation says otherwise |
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
- **Correction (2026-10-13).** The premise above is false for short
  dispatches on this machine: on an idle machine a full screen of text
  scrolled 2–3× slower than under background load (see "An idle machine is
  not the fast one" below). Load still only adds latency to long stalls, but a
  pass under load is not a pass idle, and the idle re-confirmation is the
  authority.
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

The current brackets are committed beside this record
(`long-line-documents-brackets.json`; since 2026-10-13 they are the idle
re-confirmation's final brackets below, and `V_batch` always re-measures the
dense reference case with them). A point that flips from fail to pass raises
its threshold (re-run the sweep with `--prior` and `--only` for that
threshold, bisecting upward). A passing point **can** flip too: the idle
re-confirmation showed that an idle machine is not the fast one (below), so a
pass→fail flip is investigated before any value is lowered, and a real one is
bisected downward the same way.

### Idle re-confirmation (2026-10-13)

**These values supersede the loaded ones where they differ.** Run
`rr-c0-reconfirm-20261013`: the idle re-measurement
(`rr-20261013T202507Z-toolbx-1259663` baseline, 3 variants × 3 repetitions,
and six `idle-bracket-*` parts, plain × 3, 17:25–17:49 -03, 1-minute load 5.4
falling to 1.5, nothing heavy except a 220 % `java` burst during the find part),
then a follow-up for the flipped points (`--prior` the re-confirmation,
`--only L_search_highlight,S_syntax_interact,V_batch`, 17:57–18:27) and three
idle-gated final parts (`final-dense` 18:59, `final-dense120` 19:11,
`final-minimap` 19:16; each waited for 30 s of load < 4 with no process at
≥ 80 % CPU). Total measurement time 44 minutes. Same toolkit, kernel, headless
lane and affinity (E-cores 8–23, inherited) as the reference run; watchdog tick
2 ms, idle slack ε 295 µs; a fresh `make gtk-axioms` (A31: 112 rows per
batch); 0 integrity violations. The machine was shared with another session's
bursty `java` jobs (up to 1,355 % CPU on the same E-cores) during the
follow-up: parts that ran beside them are marked below, the four that ran
beside the worst of them were discarded and re-run, and the final parts were
gated. Artifacts: `build/smoke/responsiveness-reconfirm{,-followup,-final}`,
merged report `build/smoke/responsiveness-reconfirm-final/merged/report.md`.

| Start (-03) | Part | Run id | Load (1 min) | Co-running heavy processes (≥ 1 CPU) |
|---|---|---|---|---|
| 17:25 | idle-baseline | rr-20261013T202507Z-toolbx-1259663 | 5.40 | none |
| 17:29 | idle-bracket-caret | rr-20261013T202928Z-toolbx-1274828 | 2.51 | shellcheck 101 % |
| 17:31 | idle-bracket-find | rr-20261013T203138Z-toolbx-1300170 | 2.41 | java 220 % |
| 17:33 | idle-bracket-layout | rr-20261013T203305Z-toolbx-1371739 | 4.28 | du 101 % |
| 17:42 | idle-bracket-layout-large | rr-20261013T204243Z-toolbx-1566928 | 3.20 | none |
| 17:43 | idle-bracket-dense | rr-20261013T204359Z-toolbx-1568698 | 1.83 | none |
| 17:46 | idle-bracket-total | rr-20261013T204613Z-toolbx-1638271 | 1.53 | none |
| 17:57 | followup/r0-anchors-find | rr-20261013T205720Z-toolbx-1729302 | 2.06 | none |
| 17:58 | followup/r0-anchors-dense | rr-20261013T205806Z-toolbx-1732064 | 1.82 | none |
| 18:07 | followup/r0-anchors-total (re-run) | rr-20261013T210720Z-toolbx-1795335 | 4.25 | none |
| 18:08 | followup/r1-bisect-dense (re-run) | rr-20261013T210822Z-toolbx-1801755 | 2.91 | none |
| 18:09 | followup/r2-bisect-dense (re-run) | rr-20261013T210918Z-toolbx-1806020 | 10.00 | java 547 % |
| 18:10 | followup/rc-bracket-find (re-run) | rr-20261013T211014Z-toolbx-1812323 | 6.66 | java 342 % |
| 18:13 | followup/rc-bracket-dense | rr-20261013T211333Z-toolbx-1829325 | 13.99 | java 1,355 % |
| 18:19 | followup/rc-bracket-total | rr-20261013T211908Z-toolbx-1896753 | 1.64 | none |
| 18:59 | final-dense | rr-20261013T215906Z-toolbx-2716045 | 2.50 | none at start; java from 19:02:55 |
| 19:11 | final-dense120 | rr-20261013T221124Z-toolbx-2863700 | 1.61 | none |
| 19:16 | final-minimap | rr-20261013T221619Z-toolbx-2882340 | 0.92 | none |

The discarded first runs of the four re-run follow-up parts ran beside `java`
at 720–1,100 %; they are kept in `target/reconfirm/contaminated/` only as the
evidence for the warning below.

| Value | Loaded (`rr-c0-reference-20261011`) | Final (idle, corrected rules) | Bracket (pass / first fail) |
|---|---|---|---|
| `L_syntax` | 2,000 | **2,000** (unchanged) | 2,000 / 3,000 |
| `L_minimap` | 50,000 | **22,000** (lowered; the loaded value was a rule artifact) | 22,000 / 50,000 |
| `L_invisibles` | 50,000 | **50,000** (unchanged) | 50,000 / 74,000 |
| `L_search_highlight` | 6,700 | **10,000** (raised) | 10,000 / 15,000 |
| `N_visible` | 500,000 | **500,000** (unchanged) | 500,000 / 710,000 |
| `L_protect` | not derived | **not derived** (frame half still fails at 500) | — / 500 |
| `L_protect_stall` | 500 | **500** (unchanged) | 500 / 710 |
| `V_batch` | 56,000 | **56,000** (unchanged; dense line 500 × 112 rows) | dense 500 / 710 |
| `R_batch`, `N_dense` | 224, 250 | **224, 250** (unchanged) | — |
| `N_inspect` bound | 500 | **500** (unchanged) | — |
| `S_syntax` | not derived | **not derived** (still fails at 16,000) | — / 16,000 |
| `S_syntax_interact` | 16,000 | **16,000** (unchanged; the idle "pass" at 31,000 was a rule artifact) | 16,000 / 31,000 |

**What flipped, and why.**

- **`L_search_highlight` (fail → pass at 10,000), raised to 10,000.** Find at
  10,000 characters stalled at most 4.1 ms idle (plain × 3) and 4.7 ms in the
  follow-up's bracket (3 variants × 3, beside a 342 % `java` burst); the loaded
  failures there were load. 15,000 fails on an idle anchor run (10.3 ms) and in
  the bracket (9.8 ms), so the bracket is 10,000 / 15,000 (ratio 1.5, no
  further bisection).
- **`S_syntax_interact` (fail → pass at 31,000) was not a real pass.** The rule
  subtracted the **largest** `.txt` stall over all interactions from the
  largest `.html` one, so one `.txt` outlier hid an `.html` cost in another
  phase: at 44 kB idle a 9.8 ms `.txt` click masked 13.8 ms `.html` typing
  against 2.9 ms `.txt` typing. The lane now takes the difference **phase by
  phase** (driver `attributable`, failing-first self-test). Per phase, 31,000
  fails idle (typing 12.3 ms `.html` against 6.0 ms `.txt`) and loaded, so the
  value stays 16,000.
- **The same pooling had decided `L_syntax` and `L_minimap`.** Their settle
  half pooled `open` and `background`, so the 60–85 ms window-construction
  `open` stall on both sides hid every background cost below it. Per phase:
  - `L_syntax`'s `.html` background carries the size-independent first
    highlight (C6) at every length, so it would fail everywhere; the rule now
    discounts it, measured as the `.html` minus `.txt` background of the
    smallest short-line `total-size` document (42 ms loaded, 36 ms idle, the
    stall that leaves `S_syntax` underivable). With that, `L_syntax` stays
    2,000 in both runs (at 3,000 the script flavour's `.html` background
    reaches 111 ms idle and 129 ms loaded).
  - `L_minimap` is a real cost the pooling hid: minimap on minus off, the
    background phase at 50,000 is 21 ms loaded and 17 ms idle, over
    B_settle. Corrected, the loaded run gives 10,000 / 50,000 and the idle
    final part 22,000 (6.2 ms over the minimap-off background) / 50,000.
    **`L_minimap` = 22,000** (a lower bound; [22,000, 50,000) is not bisected).
- **`V_batch` (pass → fail at dense 500) is neither a lower `V_batch` nor the
  excess rule.** The background excess over the `baseline/small` post-open
  stall was 0 ms idle (15.2 against 15.4 ms). What failed was the **scroll**
  phase, read as measured: 5–7 ms per dispatch, over B_interact once the 2 ms
  tick is added, in 2 of 3 wrap-off repetitions. Bisecting downward found no
  boundary: on an idle machine every dense length scrolled alike, 120 (17 of
  18 runs at 4.0–7.1 ms), 240 (4.9–6.7 ms, 18 runs), 350 (5.5–7.5 ms in its
  first 7 runs) and 500, while a 2,000-character paragraph that does not fill
  the screen scrolled in 1 ms.
  It is the cost of painting a **full screen of text**, which every document
  pays, not validation. The rule now reads scroll and Page Down as their
  excess over a **dense reference case**, a full screen of 120-character
  lines (`DENSE_REFERENCE_LENGTH`, added to the `full` dense grid; failing-first
  self-test), as the background half already reads the post-open stall. With
  it, idle passes 240, 350 and 500 and fails 710 on the background half (21.8
  against 15.4 ms post-open, 6.4 ms of validation): **`V_batch` stays 56,000**.
  `N_visible` deliberately does not discount the reference: it bounds what is
  painted, paint included.

**An idle machine is not the fast one.** The record assumed load only adds
latency, so a loaded pass would hold idle. It does not hold for short
dispatches on the reference machine: in the fully idle windows (1-minute load
about 1) almost every full-screen scroll took 5–7 ms, while the loaded
reference run never exceeded 3.5 ms on dense 500 (36 runs). The mode can
switch within a part: `final-dense` scrolled dense 350 in 5.5–7.5 ms for its
first 7 runs and in 1.9–4.2 ms (10 of 11 under 3 ms) for the last 11, from
19:02:29 (the `java`
jobs resumed at 19:02:55). The baseline's worst interaction stall is 5.6 ms
idle against 2.3 ms loaded. Power management on the E-cores (governor `powersave`,
EPP `balance_performance`) is the likely cause; it is not established. Read
together, a full screen of text costs 5–7 ms per scroll dispatch on the idle
reference machine, which leaves almost none of B_interact for anything else.
It is not a length threshold (no line or paragraph length removes it), so it is
recorded here, with N_visible's idle rows at 10,000 and 22,000 (wrap on, 4.8–6.7
ms, a screen of wrapped text) that the absolute rule reads as fails, and handed
to the candidate list in `long-line-evolution.md`.

**Budget verdict, idle (baseline cases, 3 variants × 3 repetitions):**

- **B_interact = 8 ms: within.** Worst interaction stall 5.6 ms (bounds
  5.3–7.6 ms), `baseline/small` AT, type-middle.
- **B_settle = 16 ms: indeterminate.** Worst settle stall 15.4 ms (bounds
  15.1–17.4 ms), the `baseline/small` post-open stall (probe IM repetition 2);
  the loaded "over" (19.6 ms) was partly load.
- **Input-to-frame ≤ 2 frames: over.** Worst 24.5 ms, the first-wheel
  overlay-indicator frame on the empty document, as loaded: structural.
- Scroll p95 frame gap (informational): 8.6 ms.

The lane's literal verdict is again **revise** (it proposes
`B_SETTLE_US = 18000`). The maintainer decision of 2026-10-11 stands: the
targets stay 8 ms, 16 ms and 2 reference frames, and C6 owns the baseline's
settle and frame stalls.

**A fixed defect reappeared once.** In the follow-up's discarded
`rc-bracket-find` part (`rr-20261013T210157Z-toolbx-1753909`, beside a
900 % `java` burst), `paragraph/15000` wrap on, find, plain repetition 1 logged
`Trying to snapshot GtkGizmo … without a current allocation` (A37) during the
find phase's worst stall (10.8 ms), and the lane failed the run as designed.
It did not reproduce in 94 further runs (67 of the same case), all with the
ring-buffer shim from the original investigation and 76 of them beside 14
busy threads.

**Explained and fixed (2026-10-14, `explain-a37-residual`).** The find
phase opens the search bar, whose 150 ms slide shrinks the editor viewport
frame by frame. Every page-size change schedules the end-of-document
overscroll idle (`ui/editor_page/overscroll.rs`), which sets the view's bottom
margin. `gtk_text_view_set_bottom_margin`, like every setter of a property
that changes the view's default text style, invalidates every line and queues
no resize, allocation or redraw; `frame_validation.rs` hooked none of them.
Normally the validation idle (`GTK_PRIORITY_RESIZE - 2`) runs right after the
overscroll idle. But every frame of the slide starts from the frame clock's
`flush_idle` (`GDK_PRIORITY_EVENTS + 1`), because GTK synthesizes a motion event
after each relayout (`gtk_window_native_layout` → `gdk_surface_request_motion`),
so when the main thread is preempted right after the overscroll idle — the
900 % `java` burst — the next frame is already due and wins. After the slide's
last frame that frame no longer allocates the view; if it redraws it,
`gtk_text_view_paint` flushes the validation and the new margin grows the
vertical `upper` inside the snapshot (symbolized: `gtk_text_view_paint` →
`gtk_text_view_set_vadjustment_values`), and the vertical scrollbar's trough
logs the warning, ~45 ms after the slide ended (+195 ms into the phase).

Reproduction: a temporary trace build that records each frame phase and
whether `first_validate_callback` is pending (found through
`g_main_context_find_source_by_funcs_user_data` on the view), and simulates
the preemption with a 12 ms busy wait in the bottom margin's `notify` plus a
redraw of the view: the lane's `paragraph/15000` wrap-on find case logged the
warning in 2 of 3 runs and failed as designed (in all 3 the vertical
`upper` changed inside the paint). Fix: `frame_validation.rs` queues the
view's allocation on the `notify` of every default-style property
(`DEFAULT_STYLE_PROPERTIES`); the same simulation then gave 0 in-paint
changes and no warning in 15 runs (plain, at, probe-im × 5), and 48
unsimulated runs under induced load (14 and 24 busy threads on the lane's 16
CPUs, three variants) gave no pending validation at a paint and no warning.
The same in-frame change in a widget test counted 20 adjustment emissions
inside the paint without the fix and 0 with it. A37's ledger entry, probe (a
margin run: 24 inside the paint, 0 with the workaround) and sample now cover
default-style properties, and the regression test is
`editor_page::test_bottom_margin_changes_in_frames_keep_adjustment_changes_out_of_the_snapshot`.
C1's task 8.4a holds again: nothing is left for C1.

## C4 `bound-window-construction-dispatch` (2026-10-15)

**Status: settings writes removed from window construction and from every
interaction dispatch; the remaining construction cost and the keyfile flush
are over B_settle and need a maintainer decision (below).** Branch
`bound-window-construction-dispatch`, PR #58 on `main`.

### Preconditions (tasks 1.1, 1.2)

- C0 applied: budgets in `model/responsiveness_budget.rs`, watchdog and frame
  timing in `ui/responsiveness_probe.rs`, the lane `make responsiveness-smoke`,
  last ledger id A37 (C4 adds A38–A41).
- **The shipped Flatpak runs on the keyfile backend.** Inside the installed
  `dev.cominotti.lushtext` 0.8.3 (runtime `org.gnome.Platform//50`, GLib
  2.88.0), `GSETTINGS_BACKEND` is unset, the runtime ships no dconf GIO module,
  and `G_MESSAGES_DEBUG=all gsettings get …` reports
  `Found default implementation keyfile (GKeyfileSettingsBackend) for
  'gsettings-backend'`; the settings file is
  `~/.var/app/dev.cominotti.lushtext/config/glib-2.0/settings/keyfile`
  (checked non-interactively with `flatpak run --command=sh`). The Snap check
  waits for the platform snap.

### What was measured

The lane's `baseline/small` case now runs on four settings profiles (fresh
or steady-state, keyfile or dconf backend; `scripts/responsiveness-smoke-driver.py`
`CONSTRUCTION_PROFILES`/`CONSTRUCTION_BACKENDS`). A steady-state profile is one
a previous, gracefully quit launch wrote. The construction cases keep their XDG
state on the artifact filesystem (btrfs on the reference machine): the first
baseline, with state on the temp scope's tmpfs, hid the fsync entirely
(`migrate_split_view` 0.3 ms). The dconf cases point the private bus's
`dconf-service` at the case's own config directory before seeding it, so the
user's real dconf database is never reached. The probe records `span` lines
(`startup_toolkit`, `startup_lushtext`, `window_construction`, every
construction stage, `first_frame`, `settings_flush`), and the report adds a
"Construction spans against B_settle" table.

Release build, `mutter --headless`, cairo, CPUs 8–23, 3 repetitions, plain
variant; all times in ms; "largest" is the largest dispatch before the first
frame (watchdog lateness).

**Baseline before the fix** (run `rr-20261015T021420Z`, load 7.7–11.0,
`btrfs-stall-report`: storage not a bottleneck, IO pressure 0.8 %):

| Profile / backend | construction | largest | first frame | dominant stages |
|---|---|---|---|---|
| fresh / keyfile | 135–151 | 136–152 | 182–183 | `migrate_split_view` 59–66 (five writes), `restore_*` 10–13 each, `preview_actions` 10–21 (the reset write), `present` 16, template 11–18 |
| steady / keyfile | 77–91 | 78–93 | 109–134 | `restore_*` 10–13 each, `preview_actions` 11–18, `present` 17–24, template 11 |
| fresh / dconf | 28–36 | 29–37 | 48–62 | `present` 15–21, template 11–14 |
| steady / dconf | 31–39 | 33–41 | 53–62 | `present` 17–21, template 11–13 |

GTK's own startup (`startup_toolkit`) is 39–51 ms in every case and LushText's
startup 3.4–5.9 ms. Each keyfile write cost 10–13 ms of `fsync` on the GTK
thread; a steady-state profile still made three of them (the two restores and
the preview reset), a fresh one eight. The failing-first widget gate
(`window_construction`) shows the same thing deterministically on the pre-fix
code: a steady-state construction replaced the settings file, a fresh one
wrote it before the first frame.

**After the fix** (run `rr-20261015T052216Z`, load 1.8–1.9,
`btrfs-stall-report`: storage not a bottleneck, IO pressure 0.9 %):

| Profile / backend | construction | largest | first frame | `settings_flush` (after the first frame) |
|---|---|---|---|---|
| fresh / keyfile | 22.7–24.1 | 23.1–24.5 | 38.2–41.4 | 12.8, 13.9, **17.9** |
| steady / keyfile | 22.0–23.6 | 22.4–24.1 | 37.8–39.0 | none (nothing to write) |
| fresh / dconf | 21.9–24.3 | 22.3–24.7 | 37.0–40.1 | 0.07–0.08 |
| steady / dconf | 21.9–26.1 | 22.3–27.3 | 37.2–42.1 | none |

A run under heavy load (19.9–24.8, `rr-20261015T025252Z`) measured 29–56 ms;
the idle run is the reference. What remains is `present` (GTK's realize and
first measure of the whole window, 11.8–14.4 ms) and template instantiation
(8.5–11 ms); every other stage is under 0.3 ms.

### GSettings axioms (tasks 2.x) and the flush path (D3)

A38–A41 (ledger, `gtk-lush-axioms`, all hold on GTK 4.22.5 / GLib 2.88.3): a
keyfile write replaces the file on the calling thread once per `set_*`, an
equal value included (A38); `delay()`/`apply()` writes N keys in one
replacement and the delayed instance reads its pending values (A39); the
keyfile backend takes **no lock** (source-cited), and a second backend object
over the same file stays stale until its file monitor reloads, about 25 ms
later (A40); `changed` reaches another instance inside the writer's `set_*`,
before the rewrite, a delayed instance notifies only itself until `apply()`,
and `apply()` does not re-notify it (A41).

**Flush path chosen: the GTK-idle fallback (D3.2).** A worker flush through
the shared default backend would race its unlocked key file; through a private
backend object it would leave every other reader stale for a monitor round
trip, and a write through the default backend meanwhile would overwrite the
file with stale values. The writer (`ui/settings_writes.rs`) therefore keeps
**one delayed `gio::Settings` instance that every LushText reader, binding and
subscriber shares** (the overlay of D3 is GSettings' own delayed backend, so
read-your-writes and enqueue-time notification hold across windows and editor
pages, A39/A41), drops a value equal to the current one, and flushes with one
`apply()` on a low-priority idle no earlier than the first presented frame, at
most once per 500 ms persistence window; the last window's confirmed close and
application shutdown flush synchronously. The batch algebra
(`model/settings_write_batch.rs`) is a per-key state machine; its Kani harness
found a real defect while being fitted (a value written back to the old value
while a flush was in flight was dropped as equal to the stale persisted
value), fixed by comparing against the in-flight value first.

### Imperative-write audit (task 3.1)

Every write now goes through `settings_writes::set_*`/`reset`, and every
reader and subscriber reads the shared instance (no `gio::Settings::new(APP_ID)`
remains outside the writer; `make check-settings-writes` enforces both).

| Key(s) | Writers | Readers and `changed` subscribers |
|---|---|---|
| `window-width`, `window-height`, `window-maximized` | window notify handlers (`ui/window/imp.rs`) | window construction |
| `workspace-sidebar-visible`, `properties-sidebar-visible` | `actions.rs` toggles, restore plan | restore plan, geometry evidence |
| `workspace-sidebar-width-fraction` | restore plan, `set_workspace_sidebar_preset`, Preferences preset | window subscription (`imp.rs`), allocation reconciliation, geometry evidence, Preferences |
| `properties-sidebar-width-fraction`, `split-view-layout-migrated` | restore plan | restore plan, geometry evidence |
| `preview-pane-visible` | `preview.rs` (construction reset, toggle) | — |
| `search-panel-visible` | `search.rs` | — |
| `color-scheme` | theme selector (`window/mod.rs`; also sets `StyleManager`) | app startup |
| `zoom-level` (missed by the design's list) | `zoom.rs` actions and buttons | zoom controls, `theme.rs` font CSS |
| `invisible-characters-mode` | `encoding/execution.rs` | editor pages, encoding dialogs |
| `workspace-show-hidden-files`, `show-minimap` | `register_boolean_setting_toggle_action` (`app.rs`) | the actions, sidebar, editor pages, Preferences binds |
| `workspace-excluded-names` | `workspace_visibility.rs`, Preferences reset | sidebar, palette index, search, Preferences |
| `style-scheme`, `custom-font` | Preferences | `theme.rs`, editor pages, Markdown preview |

**Preferences binds (task 3.5):** a keyfile write measured 10–18 ms on the
reference machine, above B_interact, so the bound rows bind against the
shared delayed instance (all of them do, since it is the only instance) and
are flushed by the writer.

### D4 (staged construction): not implemented — maintainer decision

The residual (22–26 ms idle) is GTK's `present` of the whole window and
template instantiation, not a stageable secondary surface. Timing the custom
children in isolation (release widget build): sidebar 0.4, properties panel
0.9, command palette 0.8, Markdown preview 0.5, search panel 0.9, Open popover
0.2 ms — staging every hidden one saves about 3–4 ms of template and little of
`present` (removing the revealers' children before `present` changed it by
0–3 ms in a noisy probe). The first paint after construction is a further
~15 ms dispatch. Meeting B_settle per dispatch needs a different change: a
minimal first-frame shell (header bar, empty content, status bar) presented
first, with the editor shell built in later bounded dispatches, which
restructures `LushtextWindow`'s template and every `imp()` template-child
access. Proposed as a follow-up candidate (**C4b `first-frame-shell`**) in
`long-line-evolution.md`. Until then the lane reports the construction
verdict, and `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1` makes it fail the run.

### Open questions, answered or escalated

1. **dconf access for the Flatpak: escalated (blocker per design D3).** The
   keyfile flush is one `apply()` per persistence window, but on the reference
   machine it measured 12.8–17.9 ms, so it does not always fit B_settle. The
   options are dconf access for the Flatpak (permission-posture change, one-time
   keyfile import) or accepting a flush after the first frame that can exceed
   B_settle by a few ms on a slow `fsync`. Maintainer decision.
2. **Persistence window:** 500 ms (`settings_writes::PERSISTENCE_WINDOW`),
   trailing; a resize burst writes once after it ends.
3. **Toolkit startup:** `startup_toolkit` is 30–51 ms and reported, not
   budgeted. An upstream report and a portal on the reference lane remain open.
4. **Preferences binds:** moved (task 3.5 above); their effect on editors stays
   immediate because every reader shares the delayed instance.

Other consequences: values equal to a schema default are no longer written as
explicit user values (a fresh profile now persists only the keys whose values
differ from their defaults); a crash before a flush loses at most one
persistence window of preference and layout values, never content.

## C4b `first-frame-window-shell` (2026-10-28)

**Posture: implemented (b73b9663); idle budget verdict 2026-10-29 met, content fill-in above the accepted 40 ms awaits the maintainer (below).** The window is built whole in
one dispatch with the content the first frame does not need invisible, its three
breakpoints installed before it is realized, and then realized, presented and
revealed in bounded, frame-gated dispatches: W (one text layout and the shell's
icon lookups), C (the template, wiring, restores and breakpoints), R (realize,
alone), P (present), F1 (the chrome and the real split views' rail and pane
edges), then B1 (content stack and the activation's files, four per turn), B2
(sidebar) and B3 (properties), each after the frame that showed its predecessor
painted. `open`/`activate` only record a pending request and hold the
application. The design (`openspec/changes/first-frame-window-shell/design.md`)
holds the 2026-10-20 breakdown, the adversarial review and the implementation
notes.

### What the implementation found

- **Axioms A46–A50** (ledger and `gtk-lush-axioms`): all hold on GTK 4.22.5 /
  Adw 1.9.3. A46's magnitude in an application-less minimal `AdwWindow` is about
  0.2 ms (fresh process: first realize 266 µs, second 90 µs), so the ~10 ms the
  2026-10-20 breakdown attributed to "the first realize" is not a toolkit floor:
  it is per-application or per-template work in LushText's realize, still to be
  attributed (task 1.6 would profile it). That moves the task 1.4 escalation
  premise from A46 to the measured stage R span.
- **The visual-geometry lane found four defects the widget gate had not**, all
  fixed failing-first: (1) the first frame showed what is behind the window in
  the content area, because the window background is transparent (background
  opacity) and the staged content is what paints there; `content_box` is now
  opaque until B1. (2) A never-revealed sidebar has no rect at all, a state no
  unstaged window has; B2 and B3 are now always scheduled, revealing a hidden
  rail's content into the hidden rail, so the composed window equals an
  unstaged one. (3) A CLI open drew the tab strip a frame after B1; the strip
  now counts queued activation opens. (4) The scenario's status-bar anchor
  cannot be met without a tab (the bar draws nothing then, and its toggle icon
  is not drawn in any headless screenshot, before this change too).
- **The code review found** (each fixed failing-first): sidebar actions acting
  on an invisible sidebar before B2; a request between C and P presenting the
  window inside the handler and stalling composition; CLI tabs landing after the
  restored session (the restore now waits for queued activation opens);
  bookmark navigation focusing an invisible editor; files admitted to a
  torn-down window; the `frame_layout` span missing on frames without an
  `update` phase; a process-wide largest dispatch reported as the window's.
- **The crash-recovery smoke found two more.** On `main` it already failed
  before reaching this change: the shared AT-SPI helpers walked at most 30
  levels from the desktop and the editor text now sits 31 deep (after the
  preview document alert), so `atspi-set-text.py` found nothing (fixed on
  `main` by #64, which raised the default depth to 44). With that fixed it found a defect of this change: a CLI open
  into an empty session was never written to `session.json`, because the tab
  now opens in B1 while the startup restore suppresses session saves, and the
  empty restore ended without saving; it now saves the tabs it held back.
- **One flaky widget test** (`window_composition::test_session_restore_completion_issued_before_each_stage_matches_a_composed_window`,
  a retry pass in the headless run) was a test race: the window's own startup
  restore could land after the test's and replace it. The forced-path harness
  now waits for the startup restore before issuing an operation.

### Budget verdict (idle, 2026-10-29)

**Met: every dispatch from `open` until composition complete is within
B_settle, in every profile and repetition. Open: the content fill-in is above
the accepted 40 ms (maintainer decision below).**

- **Runs.** Run directory `.claude/handoffs/c4b-verdict-20261029-0923/`
  (local, not committed). Baseline `rr-20261029T122411Z-toolbx-1782972` on
  `main` a0adc5e2 (before C4b), 2026-10-29 09:24:10–09:26:41 −03:00; branch
  `rr-20261029T122740Z-toolbx-1787430` on b73b9663 (C4b), 09:27:39–09:30:09.
  `make responsiveness-smoke`, tier `full`, `baseline/small*`, 4 profiles ×
  7 repetitions, `RESPONSIVENESS_VARIANTS=plain`, release `responsiveness-probes`
  build, cairo, `1280x800@120`, CPUs 8–23, GTK 4.22.5 / Adw 1.9.3. Both
  summaries `passed`, 0 integrity violations over 57 checks.
- **Reference precondition (D9), from `runs.log`:**
  - before: `quiet: load 1.65 2.21 2.57 at 2026-10-29T09:24:10-03:00; no build/test/proof process; no process >= 80% CPU`;
  - between: `quiet: load 0.85 1.62 2.26 at 2026-10-29T09:27:39-03:00; no build/test/proof process; no process >= 80% CPU`;
  - after: `quiet: load 0.69 1.11 1.91 at 2026-10-29T09:31:07-03:00; no build/test/proof process; no process >= 80% CPU`.
- **Storage.** `btrfs-stall-report`, both windows: "Storage was NOT a
  bottleneck" (baseline: IO pressure 0.7 %, longest commit 159 ms; branch:
  IO pressure 0.9 %, longest commit 61 ms; 0 stall episodes, no swap, no
  device errors).

Per profile, 7 repetitions each, ms, min–max (median). "Before first frame" is
the largest watchdog lateness up to the first frame's paint; "composition" is
the largest up to `composition_complete`; "first frame" and "construction" are
the probe's spans; fill-in is `composition_complete − first frame`.

**Correction (2026-10-29, `shorten-window-content-fill-in`, C4c), made in
place.** The "first frame, after" figures this verdict first recorded
(16.2–18.2 ms from construction start, median 16.9–17.1) were an **unseen
frame**: stage R realized the window in a dispatch of its own, and the frame
the realize scheduled ran before stage P's default-idle source, on the
**unmapped** window. It was never presented, and the frame clock then paced
the first visible frame a full 60 Hz `FRAME_INTERVAL` (16.7 ms) behind it
(ledger A51). The probe's `first_frame` span, and the settings writer's
first-frame gate, fired on that frame in all 28 runs. Re-derived from the same
run artifacts as the first frame painted after `composition_present`, the
**real first frame** was 38.0–41.2 ms from construction start (median 39.2) and
**44.5–47.9 ms from the window request (median 46.1)**; the pre-C4b build
presented its first frame at 42.3 ms median from the request. **C4b bounded
every dispatch, but it did not make the first pixels earlier.** The first-frame
and fill-in columns below are corrected; the honest fill-in (from the real first
frame to composition complete) was 29.4–38.5 ms (median 36.8), and the 51–61 ms
the maintainer accepted is the figure measured from the unseen frame. The "first
frame layout plus paint 3.2–4.0" below is that unseen frame's too; the real
first frame's layout and paint took about 6.6 ms (C4c's `x7-base` measurement).

| Profile / backend | before first frame, before → after | composition dispatch, after | first frame (from construction start; corrected: the first mapped frame), before → after | construction, before → after | fill-in, after: corrected (from the first mapped frame) / as first reported (from the unseen frame) |
|---|---|---|---|---|---|
| fresh / keyfile | 23.1–24.6 → 8.0–8.6 (8.5) | 9.3–9.9 (9.5) | 38.5–41.1 → 38.0–40.4 (38.9) | 22.8–24.3 → 9.9–10.1 | 29.4–37.8 (37.5) / 51.2–59.9 (58.6) |
| fresh / dconf | 22.9–24.7 → 7.9–8.6 (8.5) | 9.4–10.6 (9.7) | 37.4–39.6 → 38.0–40.0 (38.7) | 22.5–24.3 → 9.9–10.0 | 29.5–38.5 (37.8) / 51.0–60.5 (59.4) |
| steady / keyfile | 23.2–25.9 → 8.0–9.2 (8.5) | 9.3–10.7 (9.8) | 37.9–41.4 → 38.4–41.2 (39.1) | 22.9–25.5 → 9.9–10.6 | 29.8–36.9 (35.5) / 52.1–59.9 (57.6) |
| steady / dconf | 22.5–23.6 → 7.9–8.7 (8.5) | 9.6–10.3 (10.0) | 37.6–39.0 → 38.6–40.6 (39.3) | 22.1–23.3 → 9.9–10.1 | 29.8–38.1 (36.8) / 52.0–60.6 (59.0) |

Over all 28 C4b runs: stages W 5.9–6.6, C 9.9–10.6 (template 8.5–8.9),
R 4.8–6.6, P 0.4–0.9, B1/B2/B3 ≤ 0.07 each; first frame layout + paint
3.2–4.0; every dispatch ≥ 4 ms is listed in `branch/report.md` and the largest
is 10.7 ms. Before C4b, `present` alone was 12.0–15.3 ms and the construction
dispatch was over B_settle in all 28 runs; after it, no dispatch is. Fill-in
over all 28: 51.0–60.6 ms, median 58.9 ms. The post-composition `background`
phase is unchanged and outside this verdict: median 9.7–10.2 ms per profile
(before: 8.1–10.4), maximum 11.4 ms. The lane's overall verdict stays
`indeterminate`, for the unchanged background phase and the unmeasured
input-to-frame, not for composition. The headless lane renders with cairo, so
paint and present are conservative relative to a GPU (D9); the CPU-side
phases are judged as measured.

**Maintainer decision (2026-10-29): the measured fill-in is accepted.** The
measured fill-in is 51–61 ms (median 58.9 ms over 28 runs, 57.6–59.4 per
profile), above the 40 ms (5 reference frames) accepted on 2026-10-20 (design
D7). The facts: no chrome moves during it (the per-frame allocation trace and
`first-frame-shell-chrome` prove it), every dispatch in it is within B_settle,
and it includes the B2 and B3 frames that are always scheduled (implementation
notes, D1/D4). The maintainer accepted the measured 51–61 ms on 2026-10-29;
it replaces the 40 ms of D7 as the accepted fill-in for this design.

**Lane finding (not part of this verdict; fixed by `959018d2`, PR #66).** On b73b9663 the probe writes no
`frame` records (0 in every C4b case, 25 per baseline case), so the report's
"first frame … after spawn" lines read `None` and the `Frame p95 gap` column is
empty; the construction and composition figures above come from `span` and
`late` records and are unaffected. Input-to-frame and scroll frame-gap
verdicts of later changes (C1, C2) read those records, so this must be fixed
before their lane runs.

**Fixed (`fix-probe-frame-records`, 2026-10-29).** Not the short-lived phase
hooks: since C4b the window's first frame (counter 0) never receives
presentation feedback (it reports only a frame time; frame 1 now carries the
16.7 ms fallback interval frame 0 used to carry), and the probe wrote frames
strictly in order, so that one frame held every later frame until it left the
clock's 16-frame history, which a short case never reaches.
`services::responsiveness_log::FrameTimingsQueue` now writes a frame once any
later frame is complete, since presentation feedback arrives in commit order.
A short lane run on the fix wrote 143–145 `frame` records per `baseline/small`
case with "first frame … after spawn" 85.6–91.2 ms. The driver now fails a run
in which a measured case wrote no `frame` record (`frameless_cases` in
`summary.json`), instead of reporting `None`; on the C4b verdict artifacts it
fails all 28 measured cases and passes the a0adc5e2 ones.

## C4c `shorten-window-content-fill-in` (2026-10-29)

**Posture: implemented (PR #67); verdict 2026-10-29: met (below).** C4b's accepted fill-in (51–61 ms)
was frame pacing plus a measuring error, not work (design, "Measured
breakdown"): stage R's realize ran a frame on the unmapped window that was
never presented and paced the first visible frame 16.7 ms later (A51), and each
of B1, B2 and B3 waited a frame cycle for a 0.03 ms dispatch. The change:

- **R and P in one dispatch**, so the window's first frame is its first
  presented one;
- **one reveal dispatch** after the first frame (content, sidebar, properties,
  one span each), composition completing on the next painted frame;
- **the first frame is the first frame painted on the mapped window** in the
  construction evidence, the settings writer's gate (whose first flush stays a
  500 ms persistence window and a `LOW` idle after it) and the lane, which
  derives it the same way for every build (the first `frame_paint` after
  `composition_present`) so `main` compares honestly;
- **activation shown**: the first frame painted after the activation's file
  opens drained (`window_construction.activation_shown_us`); the fill-in is
  `max(composition complete, activation shown) − first frame`, and the lane
  budgets every dispatch up to that later point;
- the fill-in is judged against the maintainer's 40 ms per run (failing the
  lane only under `RESPONSIVENESS_CONSTRUCTION_ENFORCE=1`) and a median target
  of 3 reference frames (24,999 µs);
- a fifth construction profile, `baseline/small/steady-populated` (steady
  keyfile, one restored workspace folder of 300 synthetic entries, both side
  panes shown);
- a lane reporting fix found on the way (pre-existing): GDK may run a frame's
  layout phase more than once, and the report paired the i-th `frame_layout`
  with the i-th `frame_paint`, so a frame's second, near-empty layout run was
  attributed to the next frame. Layout runs now belong to the paint that
  follows them.

Exploratory figures (the proposal's A/B instrumentation, load 1.7–2.1, not the
reference precondition): first painted frame 46.7 → 31.4 ms from the request,
composition complete 82.1 → 52.9 ms, honest fill-in 36.1 → 21.6 ms.

### Verdict (2026-10-29): met

**Met, on the interleaved set:** every dispatch until the later of
composition complete and activation shown is within B_settle in every profile
and repetition (largest 13.2 ms); every fill-in is within 40 ms (20.4–25.9 ms),
and the median, 21.4 ms, is within the 3-reference-frame target (24,999 µs); the
first painted frame is earlier than `main`'s in every run (30.0–33.2 ms against
45.1–47.9 ms from the request), and so is the first presentation (31.9–40.3
against 47.9–53.8 ms).

- **Runs.** Run directory `.claude/handoffs/c4c-verdict/interleaved/` (local,
  not committed). The maintainer decided on 2026-10-29 to measure under
  accepted load rather than wait for an idle window (dota2 running), then
  stopped dota2. The coordinator asked that the official verdict be an
  **interleaved** set in one window: `main` (1e2556a2), branch (ca8cf39c),
  `main`, branch, 17:27:47–17:36:01 −03:00, 4 + 3 repetitions per side (one
  branch case got an eighth repetition, which the lane adds to a noisy case).
  `make responsiveness-smoke`, tier `full`, `baseline/small*`, five profiles,
  `RESPONSIVENESS_VARIANTS=plain`, `RESPONSIVENESS_PHASES=background`, release
  `responsiveness-probes` builds, cairo, `1280x800@120`, CPUs 8–23, GTK 4.22.5 /
  Adw 1.9.3. Both sides were measured with **this branch's lane driver**, so
  `main`'s first frame is derived the same honest way. Each run held the
  measurement flock alone. Every report `passed`, with 0 integrity violations
  and no frameless case.
- **Load, per run** (sampled every 10 s, recorded, never gating, by the
  maintainer's decision): `main` round 1 load1 0.99–1.57, round 2 0.60–0.95;
  branch round 1 0.60–1.13, round 2 0.52–0.98. The busiest other processes
  were chrome (up to 85 %) and btrfs (up to 73 %). No other agent's build,
  test or proof process ran during any run.
- **Storage.** `btrfs-stall-report`, all four runs: "Storage was NOT a
  bottleneck" (IO pressure 0.3–0.7 %, longest commit 36–47 ms).
- **The unseen frame, failing-first.** On `main` the probe's `first_frame`
  span ended before `composition_present` in all 35 runs (the realize frame on
  the unmapped window, A51). On the branch, in none of 36.

Per profile, ms from the request, min–max (median). "First painted" is the
first frame paint after `composition_present` (the same derivation for both
builds); fill-in is `max(complete, activation shown) − first painted`, and on
`main`, which has no activation-shown span, `complete − first painted`.

| Profile | first painted, main → branch | first presentation, main → branch | complete, main → branch | fill-in, main → branch | largest dispatch, main → branch | R+P, branch | reveal frame (layout + paint), branch |
|---|---|---|---|---|---|---|---|
| fresh / keyfile | 45.4–47.7 (46.2) → 30.4–33.2 (31.4) | 48.3–51.2 (49.8) → 32.5–37.8 (34.0) | 75.3–85.7 (83.7) → 51.5–54.7 (52.7) | 29.4–38.0 (36.7) → 20.7–22.4 (21.5) | 9.5–10.4 → 9.7–10.5 | 6.5–9.3 | 8.1–9.8 |
| fresh / dconf | 45.5–46.1 (45.8) → 30.9–33.0 (31.4) | 47.9–48.8 (48.4) → 33.4–35.6 (34.0) | 75.1–83.9 (83.7) → 51.7–56.3 (52.7) | 29.3–38.0 (37.9) → 20.5–23.3 (21.1) | 9.5–10.0 → 9.8–10.3 | 7.2–7.8 | 7.9–10.2 |
| steady / keyfile | 45.3–47.1 (45.9) → 30.0–33.1 (31.4) | 48.1–53.8 (49.4) → 31.9–40.3 (37.9) | 75.4–84.2 (82.5) → 50.5–55.7 (52.7) | 29.6–37.9 (36.2) → 20.5–22.6 (21.4) | 9.5–10.3 → 9.6–11.0 | 6.9–9.2 | 8.1–9.0 |
| steady / dconf | 45.1–47.8 (46.2) → 30.2–31.8 (30.9) | 47.9–53.0 (49.8) → 33.2–38.5 (35.4) | 78.7–84.7 (81.6) → 51.2–53.5 (52.3) | 31.0–38.5 (35.8) → 21.0–21.8 (21.4) | 9.6–10.4 → 9.5–10.3 | 6.7–7.8 | 8.5–9.1 |
| steady-populated / keyfile | 45.6–47.9 (46.4) → 30.0–32.2 (31.1) | 47.9–51.4 (48.9) → 33.2–39.1 (36.8) | 76.1–86.1 (82.0) → 51.5–58.1 (54.2) | 30.1–38.2 (36.1) → 20.4–25.9 (23.3) | 9.5–10.0 → 9.5–13.2 | 6.6–8.1 | 8.2–9.9 |

Over all branch runs the fill-in median is 21.4 ms (38 runs), against 36.1 ms
on `main` (37 runs); composition completes about 30 ms earlier, and the first
pixels about 15 ms earlier. The merged R+P dispatch (at most 9.3 ms) and the
reveal frame (at most 10.2 ms) needed neither fallback (design D1, D2). The
populated profile's largest dispatch, 13.2 ms, is its activation turn; it is the
largest composition dispatch, as the design predicted (Trade-off).

**Supplementary, measured under maintainer-accepted load (dota2 running):** a
first pair, `main` 17:20:37–17:24:03 with dota2 at about 300 % CPU (load1
4.5), then the branch 17:24:26–17:27:42 after dota2 had mostly stopped (load1
2.1), so it favours the branch and is not the verdict
(`.claude/handoffs/c4c-verdict/{baseline,branch}`). Branch: fill-in 20.3–26.7 ms
(median 22.0), largest dispatch 13.3 ms, first painted 30.2–35.1 ms; `main`:
fill-in 35.3–49.7 ms (median 41.4, 23 of 36 runs over 40 ms), largest dispatch
18.3 ms, first painted 46.6–62.0 ms.

An exploratory branch run earlier the same day at load 6–7 (not a verdict)
measured the populated profile's first frame at 16.1 ms and its activation turn
at 17.5 ms; on the verdict set they are within 10.2 and 13.2 ms. The populated
profile's margin under heavy load is therefore the narrowest, and the
activation turn's one-time editor and language state (design, Non-Goals) is
the candidate if it needs widening.

## Axioms each change cites

| Change | Axioms |
|---|---|
| C1 `document-scale-policy` | A27 (tag changes do not dirty the buffer), A29 (context engine cut-off), A31 (2000 px validation batches, the source of R_batch), A32 (source-map detach CRITICAL) |
| C2 `long-line-protected-view` | A23, A24, A25, A26, A28, A30, A33, A34, A35 |
| C3 `sliced-protected-install` | A24, A35, A36 |
| C4 `bound-window-construction-dispatch` | A38 (keyfile write per `set_*` on the calling thread), A39 (`delay`/`apply` coalescing), A40 (no backend lock; a second backend stays stale), A41 (`changed` timing) |
| C4c `shorten-window-content-fill-in` | A51 (a realize frame runs on the unmapped window, is never presented, and paces the first visible frame one `FRAME_INTERVAL` later), A46–A50 unchanged |
| C4b `first-frame-window-shell` | A46 (one-time first realize; small in a minimal window), A47 (`present` measures nothing; invisible subtrees skipped), A48 (a reveal is measured in the next frame), A49 (concealed vertical revealers measure a visible child), A50 (breakpoint before map vs the snapshot swap), A21 (`window-removed` teardown) |
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
| Window construction stalls the main loop for about 190 ms on every open | C4: the settings writes are gone (135–151 → 22–24 ms on a fresh keyfile profile); the 22–26 ms residual is GTK `present` and template instantiation, removed by C4b (`first-frame-window-shell`: every dispatch until composition complete ≤ 10.7 ms, idle verdict 2026-10-29) |
| `Gtk-WARNING: Trying to snapshot GtkGizmo … without a current allocation`, the trough of the editor's horizontal or vertical scrollbar. **Fixed in C0** (pre-existing-blockers rule), so it is no longer owned by C1. Root cause, confirmed against GTK 4.22.5 and GtkSourceView 5.20.0 (ledger **A37**): `GtkTextView` validates invalidated lines from an idle at `GTK_PRIORITY_RESIZE - 2`, but a change made inside a frame — the key press of the smallest reproduction (one 500-character line, wrap off, Ctrl+Home and typing) is handled in the frame clock's flush-events phase — or just before a frame that input started (`gdkframeclockidle.c` runs it from `flush_idle` at `GDK_PRIORITY_EVENTS + 1`; find in dense and 1 MB documents, where GtkSourceView's search scan changes tags from an idle) leaves that idle pending at paint time. The view is not re-allocated in that frame's layout, so `gtk_text_view_paint` flushes the validation inside the snapshot (`gtk_text_view_flush_first_validate` → `gtk_text_view_update_adjustments`): a widened line or a queued scroll changes the scroll adjustments, `GtkRange` queues its trough's allocation, and `GtkScrolledWindow` snapshots the scrollbar after its child. Lines that fit the viewport never change `upper`, which is why they never warned. The `mark-set` handlers that run inside GtkSourceView's snapshot were ruled out: disabling LushText's Notes handler left the warning in place. Fix: `ui/editor_page/frame_validation.rs` queues the view's allocation on every edit and caret move, and before the paint (at `before-paint` or after the frame clock's layout handlers) after a tag change, so `gtk_text_view_size_allocate` flushes the validation in layout. Evidence: widget tests `editor_page::test_typing_in_a_long_line_keeps_adjustment_changes_out_of_the_snapshot` (20 adjustment changes inside the paint before the fix, 0 after) and `editor_page::test_tag_changes_in_frames_keep_adjustment_changes_out_of_the_snapshot` (8 without the tag handling, 0 with it), probe A37, and the lane: the minimal case and dense 500/2000/50000, total-size 1/5 MB and paragraph 500/15000/22000 (all phases) show no warning, nor do four more repetitions each of dense 500 and total-size 1 MB (before the fix dense 500 warned on find in 3 of 5 runs). The lane classifier now treats this warning as a **regression that fails the run** (`FIXED_DEFECTS` in `scripts/responsiveness-smoke-driver.py`), not as an owned finding. Residual: a size-changing tag that GtkSourceView applies inside its own snapshot (highlighting the visible region) is still validated in that paint; the editor validates it in the next frame's layout. **Observed once on 2026-10-13** (find in a 15,000-character wrapped paragraph beside heavy CPU load, `rr-20261013T210157Z-toolbx-1753909`) and not reproduced in 94 further runs; **explained and fixed 2026-10-14**: a second route, the overscroll idle's bottom-margin change (a default-style property: invalidates every line, queues nothing) just before a frame GTK started from its synthesized post-relayout motion, which redrew the view; `frame_validation.rs` now also queues the allocation on those properties' `notify` (see "Idle re-confirmation (2026-10-13)") | fixed in C0; residual fixed |



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

Two more derivation flaws surfaced on the idle re-confirmation (2026-10-13),
each fixed with a failing-first self-test case:

- **Pooled phases.** Every `.html`-minus-`.txt` and on-minus-off rule
  (`L_syntax`, `L_minimap`, `S_syntax`, `S_syntax_interact`) subtracted the
  largest counterpart stall over a group of phases from the largest measured
  one, so a counterpart stall in one phase hid the measured side's cost in
  another (the 60–85 ms `open` phase hid every settle cost below it; a `.txt`
  click outlier hid `.html` typing). The difference is now taken phase by
  phase. `L_syntax`'s background half discounts the size-independent first
  highlight (C6), measured on the smallest short-line `total-size` document.
- **The full-screen paint read as validation.** `V_batch` read dense scroll and
  Page Down as measured, so the cost of painting any full screen of text (5–7
  ms idle) failed every dense length. They are now read as their excess over
  the dense reference case (`DENSE_REFERENCE_LENGTH` = 120, a full screen of
  short dense lines, now in the `full` dense grid).

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
  owners (design Open Question 6): resolved 2026-10-12, C0 was archived with
  the remaining blockers owned by C1 and C4 (the one it could fix, the
  `GtkGizmo` warning, was fixed in C0).

Where this work goes next, in priority order, is in
[`long-line-evolution.md`](./long-line-evolution.md).
