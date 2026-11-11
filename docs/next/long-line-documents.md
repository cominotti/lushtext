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
| C1 | `document-scale-policy` | Paragraph profile on the worker, one GTK-matching paragraph-boundary function (fixes the `\n`-only slicer), `DocumentScale` replacing `FileSizeCheck`, gates for syntax/minimap/invisibles/search highlighting at load and on edits, the minimap-detach CRITICAL fix | The context-engine stall and both CRITICALs gone, verified on the lane. **Status: implemented, in review** (branch `document-scale-policy`, PR #59); see "C1 `document-scale-policy` (implementation record)" below. |
| C2 | `long-line-protected-view` | Protected View (view-only, caret-free) for documents whose longest paragraph exceeds `L_PROTECT_EDIT` = 50,000 (maintainer, 2026-10-30), the display-only fold, the Line Inspector, find on a worker; a recorded toolkit-floor exception for editable paragraphs between `L_EDIT_STALL` (3,500) and 50,000 | Every interaction offered on these documents within the budgets. |
| C3 | `sliced-protected-install` (adopted 2026-11-02, both halves) | Sliced install and clear of giant protected paragraphs: hidden tails in 256 KiB slices separated by boundary marks (A76), the head after its tail (A77), the delimiter before it and a stand-in terminator after an undelimited last paragraph (A79, a GTK defect), forwards clears per paragraph | Every protected install, recovery, restore and clear dispatch within B_settle at 1.9M characters, 32 MiB and 100–500 MB. **Met** (interleaved verdict 2026-11-02, "C3" below): largest dispatch 8.6–11.2 ms against `main`'s 14–1,901 ms, the 32 MiB recovery 3.5 s per frame → 10 ms. |
| C4 | `bound-window-construction-dispatch` | Removes every GSettings write from window construction and interaction dispatches (coalescing settings writer, pure split-view restore plan, axioms A38–A41); construction 135–151 → 22–24 ms (fresh keyfile profile) | Window construction within B_settle per dispatch: **not met** (22–26 ms residual of `present` and template; keyfile flush 12.8–17.9 ms); maintainer decisions recorded under "C4" below. |
| C4b | `first-frame-window-shell` (proposed 2026-10-20, revised after review the same day) | The window built whole in one dispatch with the content the first frame does not need invisible and its breakpoints installed before map, then realized, presented, and revealed in bounded, frame-gated dispatches (axioms A46–A50; content fill-in accepted up to 40 ms) | Every dispatch from `open` until composition complete within B_settle on runs meeting the reference precondition, or the A46 realize floor recorded as not met with an upstream report. **Met (idle verdict 2026-10-29):** every dispatch ≤ 10.7 ms in every profile and repetition (before C4b: 22.5–25.9 ms before the first frame). **Open:** content fill-in 51–61 ms against the accepted 40 ms, a maintainer decision ("C4b" below). **Corrected 2026-10-29 (C4c):** its first-frame figure was an unseen frame on the unmapped window (A51); the real first frame was ≈ 46 ms from the request, no earlier than before C4b, and the honest fill-in 29–39 ms. |
| C4c | `shorten-window-content-fill-in` (2026-10-29) | Realize and present in one dispatch (no frame on the unmapped window, axiom A51), one reveal dispatch after the first frame, the first frame measured on the mapped window everywhere (evidence, settings writer gate, lane), the fill-in counted to the first frame after the activation's opens ran, and a populated lane profile | Every dispatch until composition complete (and activation shown) within B_settle in all five profiles; fill-in ≤ 40 ms per run and median ≤ 3 reference frames; first painted frame earlier than C4b's. **Met (interleaved verdict 2026-10-29):** largest dispatch 13.2 ms, fill-in 20.4–25.9 ms (median 21.4), first painted frame 30.0–33.2 ms against `main`'s 45.1–47.9 ms ("C4c" below). |
| KP | `close-protected-install-proof-strength` (2026-11-06; closes two queue entries: "C3 task 3.11 proof strength" and "provisional Kani CI figures"; merged (PR #86, `main` `55966a83`) and archived 2026-11-06 (`openspec/changes/archive/2026-11-06-close-protected-install-proof-strength/`)) | C3's whole-plan properties K1–K6, K8 and K9 proved by a **one-step induction** over a symbolic, well-formed `ProtectedInstallCursor` (five `step_*` harnesses, shards `core-protected-install-step` and `-step-visible`, documents of at most 8 bytes; 10 bytes measured and not adopted at 6.3 GiB), proof strength for `model/protected_install.rs` with both oracles and every survivor triaged, and every pending Kani shard budget replaced by two dispatched runs (`42169248534`, `42172535348`; the step shards, after the maximality assertion, by the largest of those and two more, `42281883363` and `42291228680`, which also raised eight older records); `check-kani-shards` now fails on a local figure no active change owes | No planner property only property-tested inside the domain, and no shard figure from a developer machine. **Met:** see `docs/next/formal-verification.md`, "Sliced protected install". No runtime behaviour changed, so no lane case or budget measurement applies. |
| C6b | `chunked-highlight-attach` (queued follow-up, maintainer decision 2026-10-31; not implemented in C6) | Attach the language to the **empty** buffer, then install the text in paragraph-aligned chunks of at most 8 KiB, letting each chunk's first context-engine update run before the next: no background analysis slice runs and every dispatch stayed at 7.7–11.9 ms at every size probed (16 KiB chunks already exceed B_settle; ledger A69) | Lifts C6's per-language total-size syntax gate for the ten languages with no per-key cost (markdown, rust, c, python3, js, typescript, yaml, xml, css, sh) to any size within the other `DocumentScale` limits. It **inverts D2's order**: the text appears only after the deferred attach and its A67 parse, so first-frame and fill-in must be re-measured. HTML and JSON **stay gated** by their mid-document per-key re-analysis cost, which chunking does not change. Needs a second, 8 KiB highlight cut inside the `V_SLICE` slice plan. **Implemented 2026-11-04, merged (PR #81, `main` `afdcc6a8`) and archived 2026-11-04** (`openspec/changes/archive/2026-11-04-chunked-highlight-attach/`): the chunked install (language attached after the first chunk, A84 coverage before each next 8 KiB chunk), after the re-review **for no shipped language**: on a worst-case document (no closer after the cursor) a flipping key, or an ordinary key typed after it, exceeds its budget at 128 KiB in c and at 256 KiB in markdown (inside an open `<!--`), below their C6 gates (84 KiB passes for c, 320 KiB fails for markdown), and rust, python3 and xml failed earlier; every language keeps its C6 gate, and the mechanism is kept, proved and tested under a model cap for C6c; js, typescript, css, sh and yaml re-analyse past every key on blank-line-free content (the pre-existing miss queued as **C6c**); see "C6b" below. |
| C6c | `bound-context-flip-keystrokes` (queued 2026-11-04, coordinator; from C6b's review) | Gate highlighting on the **longest blank-line-free run** (one more fold field in the profile pass, a carried conservative bound in the edit-time watch, Kani-proved against recomputation), plus a **runtime downgrade** when an edit's re-analysis needs a `G_PRIORITY_LOW` worker dispatch (detectable through A84's coverage and a low-priority probe). Fixes the pre-existing miss C6b measured: C6's gates for js (18 KiB), typescript (10.5 KiB), css (37 KiB), sh (39 KiB) and yaml (144 KiB) let keys cost 9–41 ms on blank-line-free content, and a context-flipping key costs worker-sized dispatches in rust, python3 and xml above 64–128 KiB; C6b's re-review adds markdown's own gate: at 320 KiB an ordinary key typed inside an open `<!--` with no closer after it costs 11.3–11.5 ms (passes at 128 KiB); and C at 64–84 KiB, which passes the dispatch criteria on the worst case, shows key-to-frame echo of 16.4–17.2 ms at the document start (8.4–11.0 ms in the middle), at or above the 16.7 ms input-to-frame budget  **Implemented 2026-11-04, merged (PR #82, `main` `e31c7cb1`) and archived 2026-11-05** (`openspec/changes/archive/2026-11-05-bound-context-flip-keystrokes/`): the planned run gate was measured on real files and **replaced** (real js re-analyses across blank lines; a run gate would unhighlight 5–46 % of real js/ts/sh/yaml files): every edit of a highlighted document is re-analysed in paced `ensure_highlight` steps before the first update (ledger A85), and a session that spends `B_SETTLE_US` without merging downgrades syntax (`EditReanalysis`) before any worker slice; every key and flip within `B_INTERACT_US` per dispatch on both families (plain 11–35 ms); recorded exceptions: echo up to about 23 ms on the keys that spend the allowance, TypeScript above about 6 KB and some js above 16 KB lose highlighting when edited near the start; allowance kept at `B_SETTLE_US` and chunked caps dormant (coordinator decisions, 2026-11-05); see "C6c" below | Every key and flipping key within the budgets on both fixture families, without removing highlighting from ordinary files |
| CF | `scope-font-zoom-restyle` (implemented 2026-11-05, merged (PR #83, `main` `256cd35a`) and archived 2026-11-05 (`openspec/changes/archive/2026-11-05-scope-font-zoom-restyle/`): zoom classes on two display providers, swapped on the registered font-following widgets; the file tree does not carry `.monospace` since `50cd2e91`, so C6's note that it follows the font was wrong; record: "CF `scope-font-zoom-restyle`" below) | Scope the custom monospace font `CssProvider`, today display-wide, to the editor and preview surfaces without losing the `.monospace` search results, peek, minimap and previews (the file tree, named here before, never followed it after `50cd2e91`), so a font change or zoom step stops revalidating the whole widget tree's CSS (32 % of the zoom dispatch; the font change's 17.1 ms dispatch, footnote ³ of C3) | The font change and zoom step within `B_SETTLE_US` on the dense fixtures, interleaved with `main`, with every `.monospace` surface still following the font; a GTK 4 per-widget provider is only available through the deprecated `GtkStyleContext` (C6, "Zoom step"), so the design must find a non-deprecated path or record the residual |
| C5 | `explore-editing-long-paragraphs` (rescoped 2026-10-30 under the hybrid policy; implemented 2026-10-31) | Lower the per-keystroke cost of typing in long **editable** paragraphs: release the retired `GtkSourceMap` that still laid out every edited paragraph (a pre-existing defect: 7.5 ms per key at 50k; A52), show paragraphs longer than `L_wrap_edit` (8,192) unwrapped by a no-wrap tag (its first character decides, A53) while the rest keep wrapping, answer the input method's surrounding-text request with a bounded window (GNOME's input path asked for the whole paragraph's word boundaries on every key; A54), make the keystroke sweep a maintained lane group, and record the floor (one whole-paragraph Pango layout in GTK's commit handler). Editing above `L_protect_edit` (50,000, C2's Protected View) is deferred. | Typing within B_interact up to `L_wrap_edit` wrapped and further unwrapped, on the plain and GNOME input paths, the rest a recorded toolkit-floor exception. **Met up to 20,000 characters** on both paths (`main`: about 12,000 plain, 5,000 on GNOME's path); 50,000 at the budget (7.7–11.1 ms) and 100,000 at 16–20 ms are the recorded exception ("C5" below), accepted by the maintainer ("editable up to ~50k", 2026-10-30); since C2 merged, 100,000 is in Protected View. |
| C6 | `bound-post-open-settle` (proposed 2026-10-30) | Bound every dispatch from the activation turn until the opened document is settled, including the validation of dense documents of short lines that stay editable (handed over by C2's revision, open question 11). The proposal's attribution, exploratory on `main` b7f56d45: small `.txt` post-open dispatches are already ≤ 14.9 ms. The over-budget sources are the HTML language-definition parse in the activation dispatch (36–40 ms; the "first highlight"), the highlight attach after a sliced install (71 ms at 5 MB), GtkSourceView's background analysis slices (40–47 ms), and the validation of dense 710-character lines (21.6 ms). Every LushText projection costs < 0.3 ms. | Every post-open dispatch until `document-settled` ≤ B_settle and every input during settle dispatched within B_interact, interleaved with `main`, 4 profiles × 7 repetitions (design D8). Every wrap toggle, zoom step and font change on a dense document is paced the same way. Input-to-frame is reported, not gated, against the documented double-buffered lane floor. Coordinator decisions of 2026-10-30 (the maintainer may override): budgets are not relaxed, so large HTML above `S_BG_HIGHLIGHT` loses highlighting through `DocumentScale`; a language parse that cannot leave the GTK thread is deferred to an input-quiet idle and recorded as a toolkit-floor exception with an upstream report. Does not block C1–C4. |
| SC | `spawn-completion-priority` (2026-11-02; queued from C2's `eviction/reselect` finding) | `gtk-lush-tasks` delivers `spawn_blocking_then` completions from one source at `GDK_PRIORITY_REDRAW + 4`, one per dispatch, above GtkTextView's validation idle (axiom A80), aging after eight completions or 16 ms (review); `ViewportSliceBin`'s pending-request evidence covers the re-slice (review, pre-existing); LushText's load and save admission drains and permit releases at the same priority; the `save/during-validation` lane case | No completion dispatch over B_settle and no input or frame delayed against `main`, interleaved; a save during a validation backlog lands promptly. `eviction/reselect` is **mostly unchanged** by design: its cause is EV, below |
| EV | `defer-background-tab-layout` (queued 2026-11-02, coordinator decision; **next after C3**, it touches C3's install files) | The real cause of `eviction/reselect`'s 6–11 s batches, attributed in SC (perf on `main` cdfff201: 69.5 % of the application's CPU in `incremental_validate_callback`): (1) the transient load permit (`8 × 8 MiB + 1 MiB` = 65 MiB of the 256 MiB budget, so three loads at a time) is held through an installation that C6 paces below the validation idle, so the next document is not even read until every validating view has laid out its whole buffer; (2) **background tabs validate fully**: restoring many large files spends the CPU laying out tabs nobody is looking at (A80: a view on a hidden `GtkStack` page validates its whole buffer) | The selected document of a restore of ten 8 MiB documents is usable within the open budget, background documents do not delay it, and no background tab lays out its whole buffer before it is shown (or the cost is recorded against a toolkit floor). **Implemented 2026-11-03, review fixes 2026-11-04** (PR #80, not merged): a hidden tab's view shows an empty placeholder buffer (A82, A83) except while an assistive technology listens or past 2^18 lines, background installs wait one priority step below the visible tab's (aging after 256 ms), the load permit keeps the decoded body plus the residency not yet installed after the read, a dense document's reattach is revealed in paced ranges; `eviction/reselect` completes (main: 20 s cap) with every background document laid out never and installed within ~0.5 s of the visible one; see "EV" below |
| DL | `bound-dense-long-line-editing` (queued 2026-11-05, coordinator, from CF's review; proposal not written) | Bound open, click, restyle and typing on **editable dense documents of long lines** (the full tier's dense/10000 and dense/50000: 160 lines of 10,000 or 50,000 characters). Identical on `main` `1d483b24` and CF: open 110–530 ms, click 53–262 ms, zoom step about 100 / 470–485 ms, font change 70 / 320–330 ms, typing about 1.2 s per burst, the lane failing `misdirected_typing` and `withheld_typing_frames` in 12 / 18 phases; the cost is GtkTextView validating 2000 px batches of such lines (A31), each dispatch laying out up to about a million characters | The full-tier lane passes its typing gates on these cases, and every dispatch is within the budgets or recorded against a toolkit floor (C5's per-keystroke exception covers one paragraph's re-layout, not a batch of them) |
| PT | `power-of-two-thresholds` (queued 2026-11-01, maintainer decision; after C2, C6 and C5 merge; implemented 2026-11-05, merged (PR #84, `main` `d59a1256`) and archived 2026-11-06 (`openspec/changes/archive/2026-11-06-power-of-two-thresholds/`); `bound-lists-by-budget-not-count` merged (PR #87, `main` `1af354a1`) and archived 2026-11-07 (`openspec/changes/archive/2026-11-07-bound-lists-by-budget-not-count/`); `bound-notes-browser-by-budget` merged (PR #88, `main` `4860fa2d`) and archived 2026-11-07 (`openspec/changes/archive/2026-11-07-bound-notes-browser-by-budget/`); `drill-down-palette-categories` merged (PR #89, `main` `4b567cdf`) and archived 2026-11-07 (`openspec/changes/archive/2026-11-07-drill-down-palette-categories/`); `bin-minimap-markers-per-row` merged (PR #90, `main` `9b8f9102`) and archived 2026-11-09 (`openspec/changes/archive/2026-11-09-bin-minimap-markers-per-row/`; see "BM"); `adopt-gnome-51` implemented 2026-11-10, lane verdict 2026-11-11, merged (PR #91, `main` `14649be0`) and archived 2026-11-11 (`openspec/changes/archive/2026-11-11-adopt-gnome-51/`; see "AG" below); `retest-upstream-fixes-on-nightly` **next**, then `lighten-notes-browser-rows`) | Convert every existing threshold to the rule in `.agents/rules/rust.md` ("Thresholds are powers of two, rounded the conservative way"): inventory every byte, character, paragraph, row and count constant; classify each as cap, coverage window, policy tier, or external fact; round caps down and coverage windows up (e.g. `R_batch` 224 → 256); re-derive the constants Kani ties together and rerun the affected shards; user-visible tiers become 8 MiB / 64 MiB / 512 MiB in IEC units (512 MiB only after measuring the exclusive-load memory peak); external facts stay exact. No measurement campaign: rounding the conservative way keeps every guarantee. The exceptions are slice sizes (`V_SLICE`, the direct-install threshold), whose rounding changes total time and is re-measured, and any value whose rounding costs a visible loss (`V_BATCH`, the per-language syntax gates), which may stay exact as recorded exceptions. **Coupled pair (C2b review):** `N_INSPECT` (500, the Line Inspector's window width) is tied to `L_PROTECT` (500, the head a fold keeps) by a Kani bound (`core-protected-view`: a window never exceeds the basis `L_PROTECT` was measured on); convert both together, to the same power of two, and rerun that shard. **Proposal written 2026-11-05** (branch `power-of-two-thresholds`, `openspec/changes/power-of-two-thresholds/`): about 95 in-scope constants classified; tiers 8 MiB / 64 MiB (512 MiB after task 3's memory peak); caps down, coverage windows (`R_BATCH` 256), retention tiers and retry budgets up (nothing stored is pruned); exceptions with corpus evidence (`L_SYNTAX`, the per-language gates and `S_SYNTAX_OTHER`: rounding down unhighlights 0.1–22 % of real files; `L_PROTECT_EDIT`, `V_BATCH`, the 100,000-file index, the 64 MP image cap); `L_PROTECT`/`N_INSPECT` 512 if a `dense/512` lane case holds, else 256; `V_SLICE`, the direct-install threshold and the 250-per-turn search caps re-measured; a `make check-thresholds` inventory gate. **Implemented 2026-11-05** (branch `power-of-two-thresholds`, PR pending, not merged): see "PT" below. Measured: the open tier is 512 MiB (memory peak within 1.074 × main's at 500 MB), `L_PROTECT`/`N_INSPECT` 512, `V_SLICE` 16,384, the direct-install threshold 512 KiB, the search per-turn caps 256; task 4.5's corpus count kept `L_MINIMAP`, `L_INVISIBLES`, `L_SEARCH_HIGHLIGHT`, `MAX_MARKDOWN_EVENTS` and the table-cell caps as recorded exceptions. **Amended 2026-11-05 (maintainer, PR #84 not merged):** the first version rounded every cap down and lost what `main` showed (palette 50 → 32, notes 500 → 256, Replace All 10 → 8 MiB, the 10,000 caps → 8,192, minimap markers 2,000 → 1,024, the minimap and per-edit history tier 10 MB → 8 MiB). New kind **presentation cap** (design D12): a cap on what the user sees or gets rounds **up** from its pre-PT value when its bound holds by measurement, else keeps that value as an exception; internal caps keep rounding down. **Implemented 2026-11-05 (stage 3, PR #84):** palette 64, notes 512, Replace All 16 MiB, the 10,000 caps 16,384 (the sidebar directory cap kept at 10,000 by measurement), markers 2,048, save-only history 16 MiB (the minimap tier kept at 10,000,000 bytes by measurement), `N_HEAD` and CSS back to 10,000 and 37 KiB, `N_DENSE` 256, Markdown 65,536 events and 1,024 cells, `L_SEARCH_HIGHLIGHT` 16,384 (`L_MINIMAP` kept at 22,000 by measurement, `L_INVISIBLES` at 50,000 by Protected View's invariant), the index 131,072, images 2^26 pixels; displayed sizes in IEC units (D14). Queued after PT merges, in order (D13): `bound-lists-by-budget-not-count`, `stream-replace-all`, `scale-file-index`, slicing the 59–60 ms largest-file open dispatch, then `avx2-health-scan-kernel` and DL. See "PT" below, "Amendment". | Every threshold is a power of two or a recorded exception with its measurement; the affected Kani shards and the lane pass. |

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

> **Measured with the workspace sidebar shown** (annotated 2026-11-02 by
> `investigate-first-keys-without-frames`). The lane did not yet seed
> `split-view-layout-migrated`, so every launch ran the one-shot migration of
> the legacy `sidebar-visible` (default true): the workspace sidebar was shown
> in every profile that asked to hide it (a 920 px editor), and the migration
> hid the document-properties surface, so the populated profile showed the
> sidebar alone. Since that change the populated profile shows both panes as
> asked, which at the lane's 1280 px puts the properties panel in the bottom
> sheet. These figures are not directly comparable with later runs.

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

> **Measured with the workspace sidebar shown** (annotated 2026-11-02 by
> `investigate-first-keys-without-frames`). The lane did not yet seed
> `split-view-layout-migrated`, so every launch ran the one-shot migration of
> the legacy `sidebar-visible` (default true): the workspace sidebar was shown
> in every profile that asked to hide it (a 920 px editor), and the migration
> hid the document-properties surface, so the populated profile showed the
> sidebar alone. Since that change the populated profile shows both panes as
> asked, which at the lane's 1280 px puts the properties panel in the bottom
> sheet. These figures are not directly comparable with later runs.

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

## C1 `document-scale-policy` (implementation record)

**Status:** implemented on branch `document-scale-policy` (PR #59, on main
after C4), in review; not archived.

**Values applied** (constants in `model/document_scale.rs`, each citing the idle
re-confirmation; budgets read from `model/responsiveness_budget.rs`):
`L_syntax` 2,000, `L_minimap` 22,000, `L_invisibles` 50,000,
`L_search_highlight` 10,000, `N_visible` 500,000, `R_batch` 224, `V_batch`
56,000; unchanged byte tiers `S_minimap` 10 MB, undo and history 50 MB,
save-only history 10 MB, refuse 500 MB.

**Two values remain open** (C0 could not derive them):

- `S_syntax`: **1 MiB applied**, the conservative candidate of the origin
  proposal's decision 4 (keep 10 MB or about 1 MiB). The size-independent ~42 ms
  first-highlight stall (C6's scope) is not removed by any cutoff.
- `L_protect`: **500 applied**, the stall basis (`L_protect_stall`), as C2's
  gate already takes it. (Superseded on 2026-10-30: C2's design D0 keys
  `protected_view` on `L_PROTECT_EDIT` = 50,000; see the C2 record below.)
  Consequence for C1: any paragraph over 500 characters
  (ordinary Markdown prose written one paragraph per line) marks the document
  for Protected View, and the reduced-features notice says so. A maintainer
  decision may raise the C1 notice threshold until C2's user-session
  validation.

**Apply-time finding — `GtkTextBuffer` lines depend on edit history (A42).** Text
inserted whole has Pango's lines, but an insertion splits only its own text
(a `\n` typed after a buffer `\r` starts a new line; text typed between `\r`
and `\n` stays inside the line) and a deletion joins exactly the lines whose
starts it removes (deleting the `\n` of a CRLF joins two lines with an embedded
`\r`; deleting the text between `\r` and `\n` removes no line). The edit-time
watch measures the buffer's own lines and its Kani model plays exactly this rule;
the design's "line-span delete trigger" finding was withdrawn (with
history-dependent lines the line-span and line-count tests agree). The minimap's
modified-line marks, created before the default handler ran, had only ever
marked a multi-line insertion's first line; they moved to an after-handler.

**Pre-existing blockers fixed in this stream:** the `\n`-only install slicer
(CR-only and U+2029 documents installed in one GTK turn); the modified-line marks
above; and an intermittent pixman `In pixman_region32_init_rect: Invalid
rectangle passed *** BUG ***` printed by C0's A32 probe (the viewless map being
drawn), which `scripts/run-gtk-axioms.sh` did not catch. The lane now fails on
`*** BUG ***` (self-test case added), and the A32 probe unmaps the map
(`set_child_visible(false)`, so it stays realized and the defect still logs its
CRITICALs) before detaching.

**Upstream GtkSourceView report (task 8.5): drafted, awaiting the maintainer.**
The defect: `gtk_source_map_queue_update` adds a frame-clock tick
(`priv->update_id`) that `disconnect_view` does not remove (gtksourcemap.c,
5.20.0: queue_update 580–591, disconnect_view 948–1017), so a scroll of the view
followed in the same turn by `gtk_source_map_set_view (map, NULL)` runs the tick
with no view, which logs `Gtk-CRITICAL gtk_scrollable_get_vadjustment: assertion
'GTK_IS_SCROLLABLE (scrollable)' failed` and `GLib-GObject-CRITICAL g_object_get:
assertion 'G_IS_OBJECT (object)' failed` (drawing the viewless map can also print
pixman's `Invalid rectangle passed`). **Re-drafting found it already fixed
upstream:** commit `8d679ccd` ("map: reduce scrolling overhead", 2026-07-26)
added `gtk_source_map_cancel_update` to `disconnect_view`, `dispose` and `unmap`,
and makes `queue_update` return without a view or while unmapped. It shipped in
**5.22.0** (2026-09-18) and is not on the `gtksourceview-5-20` branch, so the
GNOME 50 runtime and Fedora 44 (5.20.0) still have the defect; `make
gtk-axiom-sample AXIOM=32 CHECK=1` reads "holds" on 5.20.0 (2026-10-28). A new
bug report would duplicate fixed work, so the draft is now a **backport request**
for `gtksourceview-5-20` with a minimal C reproducer (8 of 8 runs reproduce
under headless Mutter); whether to file it is the maintainer's call, at
`https://gitlab.gnome.org/GNOME/gtksourceview/-/issues`, recording the link here.
The draft is kept outside the repository, in the maintainer's checkout at
`.claude/handoffs/c1-gtksourceview-upstream-report.md`. LushText does not wait
for it: the minimap is retired (unparented and dropped) instead of having its
view cleared (A45). A32 will read "violated" once the toolkit floor reaches
5.22, which is an axiom change to review then.

**Load-analysis cost (task 2.5, closed with a recorded residual,
2026-10-29).** Interleaved Criterion runs, main `1e2556a2` against the branch,
three alternating rounds per side, at load 1.1–2.9 (measured under
maintainer-accepted load; dota2 not running during these rounds); medians
(min–max):

| Benchmark | main | branch | branch / main |
|---|---|---|---|
| `editor_file_io/load_text_file/1MB` | 642 µs (640–650) | 733 µs (732–735) | 1.14× |
| `editor_file_io/load_text_file/10MB` | 8.06 ms (7.96–8.16) | 8.47 ms (8.43–8.52) | 1.05× |
| `editor_file_io/load_text_file/50MB` | 65.5 ms (64.6–65.6) | 68.0 ms (67.8–68.0) | 1.04× |
| `end_to_end_boundedness/buffer_replacement_policy_large_unicode_bytes` | 435 ps | 237 ps | 0.54× |
| `transient_file_load/install_boundaries/unicode_50_mib` | 2.26 µs | 3.77 µs | 1.67× |
| `transient_file_load/install_boundaries/single_line_ascii_50_mib` | 4.83 ms | 4.09 ms | 0.85× |
| `transient_file_load/install_boundaries/single_line_typographic_50_mib` | 4.90 ms | 5.13 ms | 1.05× |

The first comparison found the slicer at 23.6–44 ms for a 50 MiB body and the
profile pass at +63–80 % of `load_text_file`; the slicer now searches
backwards from the budget end and, after four `0xE2` bytes that do not start
U+2029, switches to a chunked `memchr2` + `memmem` search (SIMD exploration
H2), which brings the worst single-line slice on the GTK thread under
B_interact; the profile counts a paragraph at a time with
`str::chars().count()` (measured 9x faster than a per-byte count, and 4x
faster than that count built for AVX2 and dispatched at run time).
**Residual, accepted by the maintainer:** `load_text_file` is +4–5 % at
10–50 MB and +14 % (about 90 µs) at 1 MB. The cost is worker-side, off the
GTK thread, and comes from the extra profile pass (one break search and one
count over the decoded text). The planned remedy is fusing the profile with
the P2 file-health scan in the queued `simd-line-scans` change, which should
make loading faster than main overall.

**Residual closed (`simd-line-scans`, 2026-10-30).** The file-health scan now
skips between its lead bytes with `memchr3`; fusing it with the profile pass
(variant B) was measured and declined, because the separate passes already
re-read a slice from L2 and the fused form was no faster. Interleaved, one
toolchain (1.98.1) on every side, three rounds, median of round medians
(min–max), load 1.2–3.0:

| Benchmark | pre-C1 `1e2556a2` | main `0439d4c4` | branch |
|---|---|---|---|
| `editor_file_io/load_text_file/1MB` | 655 µs (642–656) | 698 µs (689–710) | 295 µs (293–296) |
| `editor_file_io/load_text_file/10MB` | 8.00 ms (7.96–8.07) | 8.41 ms (8.08–8.58) | 3.99 ms (3.95–4.11) |
| `editor_file_io/load_text_file/50MB` | 64.0 ms (61.4–64.2) | 64.1 ms (62.2–67.1) | 42.4 ms (40.8–44.4) |

The full `text_scans/*` table is in `docs/benchmarks/transient-file-load.md`.

**Queued follow-ups from `simd-line-scans`:**

- *Minimap analysis off the GTK thread.* `MinimapAnalysisAccumulator::inspect_str`
  exists, measured 9.7× (`many_short_lines`) and 18.3× (`long_line`) faster than
  per-character `inspect_slice`, but has no production caller:
  `minimap/analysis_execution.rs` still reads one `GtkTextIter` character at a
  time on the GTK thread (32 Ki per turn). Feeding it a per-turn `get_slice`, or a
  worker over a buffer snapshot, changes that coordination role (WFR minimap row)
  and gets its own change; the cost of `get_slice` per turn is unmeasured. Source:
  `.claude/handoffs/simd-exploration/REPORT.md`, P3.
- *AVX2 file-health kernel: closed (`avx2-health-scan-kernel`, 2026-11-05;
  merged as PR #85, `main` `504642a9`, and archived 2026-11-06,
  `openspec/changes/archive/2026-11-06-avx2-health-scan-kernel/`).*
  The 64-byte AVX2 block kernel (`text_scans/health_avx2.rs`, the first row of
  the explicit-SIMD allowlist, run-time dispatch with the `memchr3` scan as the
  measured fallback, NEON recorded unmeasured) brought the typographic health
  scan from 27.76 to 6.50 ms (4.27×) and its share of `load_text_file` from
  19.3 % to 5.2 % (148.3 → 124.0 ms); the typographic `editor_file_io` loads are
  1.17–1.23× faster, and no ASCII, multibyte, mixed-EOL or long-line figure
  regressed (the 1 MB ASCII load missed 1.03 on its first three-round run by
  noise, then met it on two independent five-round reruns, 1.006 and 0.955;
  design R1). Verdict table in `docs/benchmarks/transient-file-load.md`, "AVX2
  file-health kernel".

**Lane result (task 10.4, 2026-10-30): budget missed on both sides, owned by C2
(Protected View).** Interleaved `make responsiveness-smoke` runs (full tier,
`delimiters/{lf,cr,crlf,u2029}`: the 1,894,586-character minified-HTML fixture
with short lines and giant paragraphs, plus `baseline/small`; phases open,
background, click, type-start, type-middle; variants plain, at, probe-im;
main `b7f56d45` and C1 alternating, 3 rounds; load 0.5-1.6, measured under
maintainer-accepted load). Stall medians (max), input-to-frame max:

| Phase (1.9M fixture) | main | C1 |
|---|---|---|
| open | 2008 ms (2010) | 517 ms (528) |
| background | 769 ms (2008) | 769 ms (777) |
| click | 3.9 ms (6.8), 3 frames | 3.5 ms (6.6), 3 frames |
| type-start (Ctrl+Home, caret moves, typing) | 7.5 ms (12.7), 3 frames | 7.5 ms (10.7), 3 frames |
| type-middle (click, typing) | 8.5 ms (10.1), 3 frames | 8.6 ms (10.6), 3 frames |

Re-run after the rebase onto the Rust 1.98.1 pin (`3f1d9cc2`, both binaries
built with it, load 0.8-1.5): click 3.8 / 3.5 ms, type-start 8.4 / 7.3 ms,
type-middle 8.8 / 8.4 ms (3 frames on both sides), open 2008 / 518 ms.
`baseline/small` is the same on both sides (typing 2.2-3.3 ms, 2 frames). Open
shows no context-engine stall and no CRITICAL on C1; main logs `Highlighting a
single line took too much time` and stalls 2 s. Typing in the fixture's short
paragraphs misses B_interact (8 ms) and the 2-frame input-to-frame budget on
**both** sides, so it is not a C1 regression; it is the visible giant
paragraphs' layout, which C2's Protected View owns. The first comparison
found C1 worse (click 317 ms, type-start 765 ms): the reduced-features notice,
then an inline alert above the editor, reallocated the text view on every show
and hide, and GTK re-laid out each visible giant paragraph. A build without the
notice measured 3.5 / 8.2 ms; the notice is now an announced status-bar message
plus a document-properties row (never a reallocation, pinned by
`document_scale::test_reduced_features_never_resize_the_text_view`). The
watch's own share is not separable in the lane (no per-step metric); its
per-keystroke reads are bounded by the widget tests.

**C2 scope (declared degraded mode after C1):** the remaining stalls on giant
paragraphs are Pango layout of the paragraph itself and whole-paragraph buffer
costs while typing inside one (A26 log attrs, about 95 ms per keystroke on the
1.9M-character line). C1 does not claim them; C2 (`long-line-protected-view`)
removes them by making such documents view-only.

## C6 `bound-post-open-settle` (implementation record)

**Status:** in review (PR from branch `bound-post-open-settle`, with C2 and C5
merged from `main` `7f50d232`); implemented from `main`
`0439d4c4`, on which C1 (`document-scale-policy`) is merged and archived
(task 1.1: no C1 edits go to C1's branch; D4's derived `S_SYNTAX` and D6's
boundary are made here).

**Coordinator decisions of 2026-10-30 (task 1.2; the maintainer may override):**

- **Budgets are never relaxed.** Background highlighting above the measured
  `S_BG_HIGHLIGHT` is reduced through `DocumentScale` with the existing reason
  `TotalSize`.
- **`S_SYNTAX` is per language** (design open question 2, option (b)): the
  coordinator chose it on 2026-10-30. `DocumentScale` gains a language input,
  and C1's monotonicity harnesses are re-stated over it.
- **The language-definition parse** leaves the GTK thread if axiom S1 allows
  it. Otherwise the attach runs from a `G_PRIORITY_LOW` idle after the first
  editor frame and `INPUT_QUIET`, bounded by `ATTACH_DEADLINE`, and the parse
  is a toolkit-floor exception against S1, with the upstream draft
  `.claude/handoffs/c6-upstream-gtksourceview-language-parse.md`.
  The background-analysis slice has its own draft for the maintainer to file:
  `.claude/handoffs/c6-upstream-gtksourceview-worker-slice.md` (A69).
- **The dense end-of-document overscroll** uses the S7 fallback, the margin
  frozen at install; a resize-driven change over `B_settle` is a toolkit-floor
  exception against S7. A view that only becomes shorter (an inline alert, the
  search bar, a bottom sheet) keeps the larger margin while it fits: the
  "Restored from Local History" alert after an 11 MiB restore had shrunk the
  margin and re-laid out the whole document in ~18 ms validation dispatches
  (found 2026-10-31 while re-measuring the restore).
- **Input-to-frame is reported, not gated**, beside the documented lane floor.
- **The settle may be later than main's** (maintainer decision 2026-11-01): the deferred attach waits for input quiet, so activation-to-settled is reported, not gated; per-dispatch stages stay gated.
- **Floors found by the verdict** (perf from the lane): GIO's keyfile-backend monitor dispatch after any settings write parses the mount table (10-13 ms on the reference host), so LushText's flush now waits for 250 ms of input quiet, and the lane's keyfile-written wrap and font triggers carry that dispatch as their interaction; the dense zoom step's first dispatch is GTK's CSS revalidation, font load and visible re-layout (9.8-16.6 ms, both sides).
- **C6 owns user-started dense invalidations** (word wrap, zoom, font).

**Maintainer decision of 2026-10-31 (`S_SYNTAX`):** per-language
total-size syntax thresholds now. Each gate is min(attach threshold,
mid-document typing threshold), taken from the lower of two runs. The default
for unmeasured languages is the median of the measured thresholds, and plain
text keeps 1 MiB. The thresholds were re-measured quietly through
`scripts/measure.sh` before they were committed. The chunked
attach-before-install is queued as **C6b** (see "Sequence and gates") and is
not implemented in C6. Per-key recurrence mid-document was measured only for
HTML and JSON; the other ten languages showed none at their thresholds.

The committed gates (`model/document_scale.rs`; re-measured 2026-10-31
through `scripts/measure.sh`, host load 8–30 from work outside the lock
protocol, so the lower run is used; every run is in ledger A69):

| Language | Attach (lower run) | Typing (lower run) | Gate (bytes) |
|---|---|---|---|
| typescript | 10,752 | none | 10,752 |
| html | 47,104 | 16,384 | 16,384 |
| js | 18,432 | none | 18,432 |
| json | 24,576 | 20,480 | 20,480 |
| css | 37,888 | none | 37,888 |
| sh | 39,936 | none | 39,936 |
| any other language | median 48,128 | — | 48,128 |
| python3, python | 49,152 | none | 49,152 |
| c | 86,016 | none | 86,016 |
| xml | 135,168 | none | 135,168 |
| yaml | 147,456 | none | 147,456 |
| rust | 188,416 | none | 188,416 |
| markdown | 327,680 | none | 327,680 |
| plain text | no engine | — | 1 MiB (C1, unchanged) |

"None" means no per-key dispatch above 2.8 ms at the language's higher attach
threshold. JSON's typing gate is the lower of runs one and three. Its second
run failed already at 4,096 bytes, which every other run of either language
passed with every key under 6 ms, so that run (load 27 falling to 8) is
recorded as disturbed rather than used (coordinator, 2026-10-31). Taking it literally would have turned JSON highlighting off at
every size.

### Verdict (2026-11-01)

> **Measured with the workspace sidebar shown** (annotated 2026-11-02 by
> `investigate-first-keys-without-frames`). The lane did not yet seed
> `split-view-layout-migrated`, so every launch ran the one-shot migration of
> the legacy `sidebar-visible` (default true): the workspace sidebar was shown
> in every profile that asked to hide it (a 920 px editor), and the migration
> hid the document-properties surface, so the populated profile showed the
> sidebar alone. Since that change the populated profile shows both panes as
> asked, which at the lane's 1280 px puts the properties panel in the bottom
> sheet. These figures are not directly comparable with later runs.

Interleaved, `c6-baseline` against the branch at `d461e328`, release
`responsiveness-probes` builds, 4 + 3 repetitions per side, plain variant,
under the load there was (1-minute load 0.9–2.7 before and after each run;
`.claude/handoffs/c6-verdict/runs.log`). The first full round (22 cases,
`nverdict`) found the misses below; the affected rows (the five
`baseline/small` profiles and `dense/*`) were rerun after the fixes and both
merges (`rverdict`).

| Case | Worst background dispatch, median (base / branch, us) | Slowest key, median (base / branch, us) | Activation to settled, median (base / branch, ms) |
|---|---|---|---|
| baseline/small | 9912 / 6960 | 1601 / 1665 | 176 / 171 |
| baseline/small/fresh-dconf | 9737 / 7317 | 8039 / 1515 | 177 / 169 |
| baseline/small/steady-dconf | 9760 / 7657 | 6826 / 5087 | 174 / 172 |
| baseline/small/steady-keyfile | 9782 / 7094 | 5323 / 4504 | 175 / 171 |
| baseline/small/steady-populated | 10244 / 7758 | 2261 / 7392 | 176 / 174 |
| dense/120/wrap-off | 10110 / 6989 | 5092 / 9939 | 143 / 129 |
| dense/120/wrap-on | 9874 / 7287 | 6192 / 1782 | 147 / 140 |
| dense/500/wrap-off | 14914 / 7183 | 6280 / 1920 | 162 / 162 |
| dense/500/wrap-on | 10290 / 9269 | 2314 / 2237 | 176 / 185 |
| dense/710/wrap-off | 21520 / 8822 | 6084 / 1831 | 175 / 170 |
| dense/710/wrap-on | 14112 / 9459 | 3020 / 1667 | 197 / 201 |

| Case and restyle | Worst dispatch, median (base / branch, us) |
|---|---|
| dense/120/wrap-off font-change | 7783 / 8088 |
| dense/120/wrap-off wrap-toggle | 5012 / 6695 |
| dense/120/wrap-off zoom-step | 0 / 0 |
| dense/120/wrap-on font-change | 7929 / 8280 |
| dense/120/wrap-on wrap-toggle | 6470 / 6524 |
| dense/120/wrap-on zoom-step | 0 / 13437 |
| dense/500/wrap-off font-change | 11476 / 8467 |
| dense/500/wrap-off wrap-toggle | 7225 / 7008 |
| dense/500/wrap-off zoom-step | 0 / 0 |
| dense/500/wrap-on font-change | 9419 / 10381 |
| dense/500/wrap-on wrap-toggle | 5692 / 6908 |
| dense/500/wrap-on zoom-step | 0 / 0 |
| dense/710/wrap-off font-change | 10305 / 8220 |
| dense/710/wrap-off wrap-toggle | 6669 / 6507 |
| dense/710/wrap-off zoom-step | 15813 / 0 |
| dense/710/wrap-on font-change | 10203 / 11692 |
| dense/710/wrap-on wrap-toggle | 8654 / 5373 |
| dense/710/wrap-on zoom-step | 0 / 0 |

**Not met as written.** What remains over budget on the branch, each
attributed with `perf` from the lane:

- **Keys while settling** (9 of 77 branch rows, 8.6–14.8 ms; the baseline
  has more): each key waited behind one dispatch 200–300 ms after open, a
  whole-tree CSS revalidation (`gtk_css_node_validate` from
  `gdk_frame_clock_paint_idle`). *Corrected 2026-11-02 (independent review
  M4a):* not "the frame after the first key presses". The review attributed
  it to libadwaita's `enable_animations_cb` (the settings portal's first reply
  about 251 ms after `adw_init` setting `gtk-enable-animations`). That
  mechanism is real and pinned as ledger **A74** (a change of the setting
  restyles every widget: 21.3 ms against a 1.5 ms redraw on 1,600 rows), but
  it is not what the lane measured: a measurement build that recorded every
  `notify` of `GtkSettings` and `AdwStyleManager` saw none in that window, and
  window activation came at 8 ms. `perf` (32 samples over the baseline rows'
  dispatches 150–300 ms after open) places the revalidation in the frame
  clock's flush phase, under `gdk_surface_ensure_motion` →
  `gtk_main_do_event`: the motion GDK synthesizes after the lane's focus click
  moved the pointer into the window, whose `:hover` change restyles the
  widgets under the pointer. Once per pointer entry, GTK's restyle, on both
  sides; recorded as a toolkit floor of the lane's first pointer entry.
- **Zoom step** (4 rows, 13–16 ms; the baseline has them too): GTK's style and
  font change (CSS revalidation, fontconfig and FreeType load, the visible
  lines' re-layout). A toolkit floor.
- **Font change** (1 row, 11.5 ms) and the lane's wrap and font triggers in
  general: GIO's keyfile-backend monitor dispatch, which the lane's external
  keyfile write causes before LushText's restyle runs.
- **Activation to settled** is later in 12 cases of the first round: the
  accepted cost of the deferred attach (maintainer decision 2026-11-01).
- **The language attach** (31 ms) is the A67 toolkit-floor exception.

### Independent review fixes (2026-11-02)

An independent review of PR #74 found two data-loss defects, a CI failure
and four record corrections; every fix below is failing-first where a test
can fail.

- **Drag and drop under the edit lock** (data loss): the view stays editable
  while a load, replacement or save holds the edit lock, so GtkTextView's drop
  handler reported a refused drop as done (`insert_interactive` returns TRUE
  when the view is editable) and a MOVE drag's source deleted its text. The
  lock now takes the view's drop targets off while it is held, nested with
  Protected View's own detach (one holder bit each), and the last holder
  restores them.
- **Ctrl+Z during the save of an edit-entered Protected View document**: the
  lock refused the buffer's undo but the entry undo still tore Protected View
  down, leaving the giant paragraph unfolded and editable. The entry undo is
  unavailable while the lock is held, and the teardown runs only after the
  undo changed the buffer.
- **Bookmark reconcile** skipped while the lock is held or projections are
  suspended (a refused key press still brackets a user action, and the marks
  then sit in half-cleared text); the lock also refuses `insert-paintable` and
  `insert-child-anchor`; the lock is a move-only token, its release
  debug-asserts a holder; a reload leaving Protected View restores `editable`
  before the first slice instead of re-laying out the new body at the
  terminal.
- **GTK Axioms CI** (A70, A72): the CI container had no fixed-pitch font, so
  "Monospace" was proportional (Adwaita Sans), the paced inserts' random lines
  widened the view only three times in six and column 80 sat off both
  predictions. Reproduced locally with a fontconfig of Adwaita Sans alone. The
  axioms were not wrong; the probes now stop at a fixed-pitch control step
  (fixture invalid) and both axiom jobs install `adwaita-mono-fonts`. The
  pacing claim V_SLICE and the idle pacing rest on is unaffected (six
  widenings for six inserts with either pacing, locally and once the font is
  present).
- **Zoom step** (13–16 ms; M4b): `perf` over the four over-budget rows
  (50 samples) puts 32 % of the dispatch in whole-tree CSS revalidation
  (`gtk_css_node_validate`) after the display-wide font provider reloads, and
  most of the rest in Pango layout and paint of the visible lines at the new
  size. A provider scoped to the editor and preview was not adopted: GTK 4
  offers per-widget providers only through the deprecated `GtkStyleContext`,
  the zoom and custom font also restyle the `.monospace` search results and
  peek (and, this note said, the file tree: wrong, the tree has used the
  default font since `50cd2e91`; corrected by CF), which would stop following
  them, and removing every CSS
  revalidation would still leave about 9–11 ms. Recorded as a toolkit floor
  (the visible region's re-layout at a new font size); the in-process font
  change (below) is the same restyle and lands at the same figures.
- **The dense/710 wrap-off 8.5 ms background dispatch** at about 64 ms (M4c):
  `perf` (8 of 8 and 8 of 9 samples) attributes it to GtkTextView's
  `incremental_validate_callback`, validating the lines of the two slices
  inserted before the frame at about 48 ms together: a slice inserted while a
  frame is due waits for that frame's allocation (the A37 allocation queue)
  before its validation idle runs. It is under `B_SETTLE` (16 ms) and is
  flagged only by the stage rule because `main` installs that document in one
  dispatch. Pacing every slice by frames removes it but took 2.7 times longer
  to install (option A of 2026-10-31), so it stays, recorded.
- **Input-to-frame while typing during settle** (M4d): the frames after a key
  now always win over the paced loops: a slice, replacement turn or restyle
  reveal that would run while a counted input has not been followed by a
  painted frame of its window waits for that frame (at most 128 ms,
  `input_quiet::paced_idle`). The review's 42–63 ms keys were then traced:
  **every** key over 33 ms on either side is the first key after a 1.0–1.8 s
  stretch in which the window drew no frame at all (the frame counter is
  contiguous across it) and that ends about 2.03 s after activation, with no
  application dispatch of 1 ms or more between the key and its frame. Keys
  whose frame was presented match `main` (median 12.9 ms on both sides, maximum
  25–33 ms against 25–26 ms). The branch shows more such keys because its
  stretch starts earlier (about 230 ms after activation, once settled,
  against about 1,050 ms on `main`). Why the typed keys draw nothing in that
  stretch is a lane question recorded for follow-up; it is not pacing.
- **In-process restyle triggers** (S4): the lane toggles word wrap and
  changes the font through the measurement build's `app.probe-set-word-wrap`
  and `app.probe-set-font`, which write through the app's settings writer, so
  LushText's own restyle dispatch is measured against `B_INTERACT`
  (`RESPONSIVENESS_RESTYLE_TRIGGER=keyfile` keeps the external write, which
  the `main` baseline binary needs).
- **No-regression rule**: activation to *fully laid out* is gated (end of the
  last layout dispatch or vertical-extent change), exempting only the
  deferred language attach's wait, at most `ATTACH_DEADLINE_US`; activation
  to settled is reported. Constants: `FLUSH_INPUT_QUIET` 250 → 256 ms,
  `FLUSH_INPUT_DEADLINE` 5 s → 4,096 ms, `ATTACH_DEADLINE_US` 2 s → 2,048 ms;
  `V_SLICE`, the 704 KiB synchronous threshold, the per-language syntax gates
  and `S_SYNTAX_OTHER` are recorded exceptions deferred to
  `power-of-two-thresholds`.

#### Rerun of the affected rows (2026-11-02)

`c6-baseline` against the branch with every fix above
(`c6-bin/lushtext-branch-review1`), interleaved 4 + 3 repetitions per side,
plain variant, the five `baseline/small` profiles and `dense/*`; 1-minute load
0.7–2.0 before and after each run (`.claude/handoffs/c6-verdict/runs.log`,
runs `vbase1`, `vbranch1`, `vbase2`, `vbranch2`, merged as `vverdict`). The
baseline's restyles are triggered by the external keyfile write (its binary
has no in-process trigger), the branch's in process; "-" is a restyle that
recorded no interaction dispatch of 1 ms or more.

| Case | Worst background dispatch, median (base / branch, us) | Slowest key, median (base / branch, us) | Activation to laid out, median (base / branch, ms) |
|---|---|---|---|
| baseline/small | 10083 / 7615 | 1375 / 1709 | 69 / 72 |
| baseline/small/fresh-dconf | 9937 / 7184 | 6567 / 1590 | 70 / 70 |
| baseline/small/steady-dconf | 9707 / 7344 | 2632 / 2543 | 70 / 72 |
| baseline/small/steady-keyfile | 9656 / 7414 | 5958 / 4763 | 69 / 72 |
| baseline/small/steady-populated | 11311 / 7806 | 3493 / 4766 | 72 / 71 |
| dense/120/wrap-off | 9715 / 7873 | 1692 / 1738 | 36 / 35 |
| dense/120/wrap-on | 9756 / 7153 | 2074 / 1729 | 38 / 45 |
| dense/500/wrap-off | 14881 / 7096 | 6131 / 1780 | 62 / 52 |
| dense/500/wrap-on | 10337 / 9265 | 2954 / 2100 | 70 / 86 |
| dense/710/wrap-off | 21467 / 8864 | 6640 / 3355 | 69 / 75 |
| dense/710/wrap-on | 14138 / 10438 | 1759 / 1673 | 88 / 101 |

| Case and restyle | Interaction dispatch, median (base keyfile / branch in-process, us) |
|---|---|
| dense/120/wrap-off font-change | 6527 / - |
| dense/120/wrap-off wrap-toggle | 5514 / 3060 |
| dense/120/wrap-off zoom-step | 17726 / - |
| dense/120/wrap-on font-change | 5312 / 9637 |
| dense/120/wrap-on wrap-toggle | 6726 / 1598 |
| dense/120/wrap-on zoom-step | 14355 / 1983 |
| dense/500/wrap-off font-change | 6862 / 1001 |
| dense/500/wrap-off wrap-toggle | 4908 / 4077 |
| dense/500/wrap-off zoom-step | 19213 / 1045 |
| dense/500/wrap-on font-change | 6303 / 1186 |
| dense/500/wrap-on wrap-toggle | 6847 / 3010 |
| dense/500/wrap-on zoom-step | 14239 / - |
| dense/710/wrap-off font-change | 5074 / 1110 |
| dense/710/wrap-off wrap-toggle | 5030 / 2761 |
| dense/710/wrap-off zoom-step | 18995 / 1010 |
| dense/710/wrap-on font-change | 6021 / 1465 |
| dense/710/wrap-on wrap-toggle | 7251 / 7814 |
| dense/710/wrap-on zoom-step | 14680 / - |

**Not met as written; every remaining miss is attributed.**

- **Keys** (9 rows, 8.7–16.8 ms): each waits behind the pointer-entry
  restyle about 207–217 ms after open (above; 15.6–16.1 ms on dense/710
  wrap-off, where the visible dense lines are restyled too), on both sides.
- **Font change in process** (`dense/120/wrap-on`, 5 of 7 rows, 9.2–13.1 ms):
  LushText's own restyle dispatch, now measured: the display-wide font
  provider reloads and the next frame revalidates the CSS of every widget and
  re-lays out the visible lines at the new font, as the zoom step. The other
  dense cases change the font in 1.0–1.5 ms. Recorded with the zoom step.
- **Zoom step** (1 row, 9.6 ms; median 1.0–2.0 ms where recorded): as above.
- **Activation to laid out** `dense/500/wrap-on` 86 against 70 ms and
  `dense/710/wrap-on` 101 against 88 ms (one reference frame is allowed):
  wrapped dense documents install in `V_SLICE` slices, each validated in its
  own dispatch, so the last validation ends one or two frames later than
  `main`'s single install dispatch, whose own worst dispatch is 10.3–14.1 ms
  against the branch's 9.3–10.4. The other nine cases are within a frame.
- **dense/710 wrap-off unattributed 8.5 ms** (M4c above).
- **Input-to-frame**: keys whose frame was presented: median 12.8–12.9 ms on
  both sides, maximum 32.2–33.3 against 22.6–23.7 ms; one branch key above
  33 ms. Every other key above 33 ms (branch 19 of 42, baseline 8 of 13 keys
  measured inside a frameless stretch) is the first key after the 1.0–1.8 s
  stretch with no frame described above.


The 11 MiB dense document (release widget harness, 2 interleaved repetitions,
load 1.0–3.1): main installs in 1.92–1.95 s (open) and 2.39 s (restore), and
is fully laid out at 4.56–4.60 s and 4.41 s, with worst dispatches of
45–54 ms and 36–39 ms. The branch installs and is laid out at 3.87 s (open)
and 3.52 s (restore), with worst dispatches of 10.9–11.0 ms (open) and
32–35 ms (restore: the local-history dialog's close animation in the cairo
renderer, on both sides).

### Decisions on the remaining misses (2026-11-02, delegated by the maintainer)

The maintainer delegated programme decisions on 2026-11-01 ("decide without
asking, reviewed by a subagent, user experience first"). With the independent
review's must-fixes landed, the remaining misses are decided as follows:

- **Activation to laid out, dense/500 and dense/710 wrap-on, 13–15 ms later
  than main** (over the one-frame allowance): **accepted as a recorded
  exception.** The branch's worst dispatch on those cases is 9.3–10.4 ms
  against main's 10.3–14.1 ms. A user notices a stall; a settle that ends
  less than one frame later is invisible. Revisit when `power-of-two-thresholds`
  re-measures `V_SLICE`. **Revisited 2026-11-05 (PT):** at `V_SLICE` 16,384
  the branch is laid out no later than `main` on every dense case (dense/500
  wrap-on 85 against 88 ms, dense/710 wrap-on 102 against 202 ms), so the
  miss is closed.
- **Keys behind the pointer-entry restyle (~210 ms after activation, both
  sides)** and **the dense/710 wrap-off 8.5 ms two-slice validation**:
  recorded toolkit costs, as attributed above.
- **Font change and zoom on dense/120 wrap-on (9.2–13.1 ms, 9.6 ms):** the
  display-wide `CssProvider` restyle is LushText's own choice (32 % of the zoom
  dispatch is CSS revalidation). Recorded, with a queued follow-up to scope the
  font provider to the editor and preview surfaces without losing the
  `.monospace` search results and peek (CF in the table at the top; the file
  tree named here does not follow the font since `50cd2e91`). **Done by CF**,
  2026-11-05: see "CF `scope-font-zoom-restyle`" below.
- **Typed keys draw no frame for 1.0–1.8 s about 2 s after activation, on main
  and the branch:** not caused by C6, but if it is real outside the lane it is a
  severe pre-existing defect (no key echo). Queued as the **next change, top
  priority** (`investigate-first-keys-without-frames`): reproduce outside the
  lane's injection path, attribute with perf, fix or prove it a lane artefact. **Resolved by that change (2026-11-02, below).**

## `investigate-first-keys-without-frames` (2026-11-02)

The 1.0–1.8 s stretch without frames in `type-during-settle` was **a lane
artefact with a real app defect behind it**. Evidence is in the change's design
and in `.claude/handoffs/first-keys-verdict/`.

- **The lane typed into nothing.** The focus click, requested at screen (426,
  400), arrived at window (336–338, 400), inside the workspace sidebar. The
  sidebar was shown because the lane never seeded `split-view-layout-migrated`,
  so the app migrated the legacy `sidebar-visible` (true) over the case's
  `workspace-sidebar-visible=false`. All 60 keys went to the status bar's sidebar
  toggle, and none was inserted. A `WAYLAND_DEBUG` trace shows no commit
  damaging the text after the click. The frames on either side of the stretch
  are GtkScrolledWindow's overlay-indicator fades: a fade-in after load, then a
  fade-out from 2 s after the last adjustment change (`INDICATOR_FADE_OUT_DELAY`,
  polled every 500 ms). That is the fixed end at about 2.03 s. Every key over
  33 ms in the C6 verdict was attributed to the first fade-out frame.
- **Real defect:** a document opened from the command line, a file manager or
  the startup session did not take keyboard focus. GTK picks the initial focus
  at `gtk_window_show`, before any page exists, and nothing moved it to the
  document. Typing straight after opening a document went to the status bar's
  toggle, not late but never. This is fixed: the opened or restored document
  takes focus unless a dialog, a transient surface or the user's own focus
  holds it (`opened_document_takes_focus`). Widget tests failed first.
- **Lane gates.**
  - Editor clicks aim at an uncovered point of the snapshot's `source-view`
    rectangle and correct the pointer by the probe's new `motion` records. A
    requested screen point arrives up to 146 px off in window coordinates; 640
    arrives at 640. A click fails its phase unless the probe's press record
    names an editor.
  - Typing phases report `delivery`, and a run whose typing reached no
    document fails (`misdirected_typing`).
  - The lane seeds `split-view-layout-migrated`. **Lane figures from before this
    change were measured with the workspace sidebar shown (a 920 px editor);**
    they are not directly comparable with later ones.
- **Interleaved verification, first round** (main b6330aa3 / branch / main /
  branch, 2 repetitions each, `baseline/small` in its five profiles and
  `dense/{120,500,710}` wrap on and off, plain, this change's driver on both
  sides, loads 0.6–4.0 at start and 0.7–1.3 at end). `type-during-settle`,
  counting only keys with a measured latency:

  | Run | Keys typed | Measured | Median | p95 | Max | Over 33 ms | Excluded |
  |---|---|---|---|---|---|---|---|
  | main1 | 1,320 | 1,310 | 5.9 ms | 18.7 ms | 25.6 ms | 0 | 10 `no-frame` |
  | branch1 | 1,320 | 1,310 | 6.4 ms | 16.9 ms | 25.7 ms | 0 | 10 `no-frame` |
  | main2 | 1,320 | 1,285 | 6.6 ms | 19.3 ms | 32.9 ms | 0 | 9 `no-frame`, 26 `no-visible-change` |
  | branch2 | 1,320 | 1,310 | 6.0 ms | 17.1 ms | 26.3 ms | 0 | 10 `no-frame` |

  The exclusions: a `no-frame` key is the last key of a `baseline/small`
  phase, after which the log holds no frame (the probe writes a frame only
  once it or a later one has final timings, and headless Mutter completes a
  lone last frame only at the next commit); a `no-visible-change` key had its
  first frame start only after the next key was handled. The C6 verdict runs
  had 28 and 23 keys over 33 ms, all at the stretch. On the branch every
  typing phase delivered 60 of 60 keys to the editor, with 60 insertions.
  **main2's 26 `no-visible-change` keys, all in one repetition of
  `baseline/small/steady-populated` 3.2–4.5 s after activation, are the
  artefact's signature** (keys that drew nothing). `main`'s probe could
  neither aim its clicks nor say where keys arrived, so `main`'s delivery is
  unknown on every row, and its clean rows do not show that its keys reached
  the document.

### Review round (2026-11-02, after merging main 8cd1441a)

An independent review found the delivery gate vacuous for keys that leave no
input record, the focus policy too eager, and the lane blind to typed text
whose frame is withheld. Decided and implemented:

- **Focus policy.** A user-input latch: once the window has seen a key or
  button press since it was mapped, the handoff moves nothing (an Orca user
  tabbing through the chrome during a restore keeps their place). Focus in a
  popover or menu is never chrome (`focus.native() == window`), so the Open
  popover and the tab menus are user surfaces. An activation hands off only
  when it added or selected a page in this window; a restore only when it
  created one. The decision is the pure
  `ui/window/editor_focus/policy.rs::opened_document_takes_focus`. Six widget
  tests during a held composition (sidebar, dialog, palette, Open popover, a
  key into the chrome, and the untouched control); the popover and key cases
  failed first. A seventh proves the handoff precedes a protected document's
  one-time "View-Only: Very Long Lines" announcement, for a CLI open and a
  restore.
- **Lane gates.** Text keys with no input record count as `undelivered` and
  fail the phase whatever the probe's vintage. A text key into an editable
  editor is never `no-visible-change`: drawn only after the next key was
  handled (loop free), or after the phase, it is measured to that frame,
  flagged `withheld_frame`, and fails the run (`withheld_typing_frames`).
  Keys into a protected editor are reported apart
  (`protected_input_to_frame`). A recovery whose editor click cannot be
  proven ends the case; an editor click waits up to 3 s for a free point of
  the text view instead of falling back to a blind screen point.
- **Fresh profiles** that show no pane seed the legacy `sidebar-visible=false`
  unmigrated, so their launch still runs the one-shot split-view migration;
  every other profile is seeded migrated.
- **Interleaved rerun** (main 8cd1441a / branch / main / branch, the merged
  driver on both sides, same cases, plain, 2 repetitions;
  `.claude/handoffs/first-keys-verdict/r2/`). Loads at measurement start and
  end: main1 1.72 → 1.04, branch1 1.48 → 0.78, main2 1.23 → 0.65, branch2
  1.69 → 0.95; storage was not a bottleneck in any run. **main1 and branch1
  started with another agent's single-threaded `cbmc` at 99 % CPU**, outside
  the reference precondition, so only main2 and branch2 are verdict-grade.

  | Run | Keys typed | Measured | Median | p95 | Max | Over 33 ms | Excluded | Delivery |
  |---|---|---|---|---|---|---|---|---|
  | main1 (cbmc) | 1,320 | 1,310 | 6.3 ms | 18.9 ms | 24.9 ms | 0 | 10 `no-frame` | unverified |
  | branch1 (cbmc) | 1,260 | 1,251 | 6.7 ms | 17.9 ms | 27.4 ms | 0 | 9 `no-frame` | 1,260 / 1,260 inserted |
  | main2 | 1,320 | 1,310 | 5.7 ms | 17.9 ms | 25.3 ms | 0 | 10 `no-frame` | unverified |
  | branch2 | 1,260 | 1,251 | 6.6 ms | 18.0 ms | 24.5 ms | 0 | 9 `no-frame` | 1,260 / 1,260 inserted |

  No run had a withheld typing frame, an undelivered key, or a misdirected
  phase. Each branch run lost one of its 22 typing phases before typing: the
  populated profile's focus click found no free point of the text view right
  after launch and fell back to a blind point in the sidebar, which the gate
  rejected (`click-missed-editor`) rather than measure. The 3 s wait above
  fixes it: 4 of 4 populated repetitions on the branch then delivered and
  inserted 60 of 60 keys (median 5.5 ms, max 21.6 ms, none over 33 ms).
  Content fill-in medians: 17.5 / 17.8 / 17.4 / 17.5 ms, all within the
  three-frame target.

## `keep-keystrokes-during-install` (2026-11-02)

C6's buffer-level edit lock refused every edit while a load install, a
whole-buffer replacement or a save owned the buffer, and nothing reported it;
text typed while the file was still being read went into the buffer and the
install's clear then deleted it. On the lane's `type-during-settle`
(`total-size/1000000` and `total-size/5000000`, one key every 50 ms from load
readiness) 0–3 of 60 keys were inserted in the worst phases. Now a typed
insertion at the caret is **held** (4 KiB cap, Backspace edits the held text)
and inserted once nothing owns the buffer, in one settle dispatch, as one user
action (one undo action, like typing), into the document whose install
completed. **Held text is never discarded:** a broken episode, a failed load,
an evicted tab, a Protected View result or a refused insert opens it in a new
Untitled tab with a warning naming why, and every close collector counts it as
unsaved (the review fixes below). Every other edit is still refused, with a
rate-limited warning. Design and decisions:
`openspec/changes/keep-keystrokes-during-install/design.md`.

### Verdict (interleaved, 2026-11-02): met

main ebdd6aa1 / branch / main / branch, this change's driver on both sides
(the probe's `held-editor` target and the delivery gate `inserted >= to_editor
+ to_held_editor`), `baseline/small` in its five profiles,
`total-size/{1000000,5000000}` (`.json` and `.txt`) and
`dense/{120,500,710}` wrap on and off, plain, phase `type-during-settle`, 2
repetitions per run; `.claude/handoffs/keystrokes-verdict/`, merged in
`verdict/lane`. **The host was not idle:** a game (`dota2`) used 2.5–3.3 CPUs
throughout. Storage was not a bottleneck in any run.

| Run | Load at start → end | total-size keys typed | To the editor / held | Inserted | Phases with every key inserted | Lane |
|---|---|---|---|---|---|---|
| main1 | 3.92 → 3.99 | 480 | 480 / – | 280 | 3 of 8 | fails `misdirected_typing` (5 phases) |
| branch1 | 2.78 → 4.54 | 480 | 354 / 126 | 480 | 8 of 8 | passes |
| main2 | 4.49 → 5.53 | 480 | 480 / – | 287 | 1 of 8 | fails `misdirected_typing` (7 phases) |
| branch2 | 4.36 → 6.67 | 480 | 295 / 185 | 480 | 8 of 8 | passes |

`main`'s probe has no `held-editor` target, so its refused keys count as
delivered to the editor and fail the gate as not inserted. Its worst phases
inserted 0 of 60 (`total-size/5000000/txt` in three of four repetitions);
main also failed `withheld_typing_frames` in six `total-size/5000000` phases,
the branch in none. Every `baseline/small` and `dense` phase inserted 60 of 60
on both sides (1,320 keys per run); on the branch no `dense` key was held,
because those installs finish before the first key.

- **Key-to-frame.** Keys that were not held stayed within 17.1–22.8 ms
  (`input_to_frame_max`) on the branch's `total-size` rows, against 530 ms to
  1.2 s on main's `total-size/5000000`. A held key is drawn only at the replay,
  so the driver reports it apart (`held_input_to_frame`): median 9.4–16.7 ms,
  max 52.8 ms (branch1 `total-size/5000000`, 75 keys) and at most 23.8 ms in
  branch2.
- **The replay dispatch.** 13 `held_input_replay` spans across both branch
  runs: 193–758 µs for at most 60 held characters, against `B_SETTLE` 16 ms.
  The lane cannot hold more than its 60 keys, so the cap-sized case was timed
  separately (a throwaway widget probe, debug build, cairo renderer, under the
  measurement lock, load 4.5–5.0): a 4,096-byte held text replayed at the
  caret took 0.48–1.4 ms in one 710-character-line `dense` document (ASCII
  and four-byte characters) and in a 5 MB short-line document (one line),
  and 3.0–3.3 ms as 256 lines into the 5 MB document; 12 samples, none in
  Protected View. **The dispatch stays within `B_SETTLE` at the cap, so it was
  not split into chunks.** The frame that then lays out the inserted text costs
  what pasting the same 4 KiB costs: up to 14 ms, and 19.7–31.5 ms for 1,024
  emoji in one line (font fallback in the cairo renderer). That frame is the
  edit's own cost, not the replay dispatch.
- `c6_verdict` read `not met` on both sides in every run, from the same
  `background` dispatches: `post_open_language_attach` on every `total-size`
  repetition (33.9–46.1 ms, the A67 toolkit-floor exception recorded under C6)
  and, under this load, a first-editor-frame, editor-construct, validation or
  unattributed dispatch of 16.1–25.0 ms
  (main1 3, main2 4, branch1 1, branch2 2). This change does not touch them.

### Review fixes (PR #78, 2026-11-02)

An independent review found that held text could still be lost, and the fixes
make **the user's text is never lost** hold for it: (1) held text counted as
unsaved nowhere, so closing a tab or the window between a save's release and
the replay closed without asking; every collector now asks
`has_unsaved_changes()`, a close settles held text first (the Save Changes
dialog then asks and its Save writes it) or is refused while a workflow owns
the buffer, and `app.quit` (Ctrl+Q), which skipped every close confirmation
(pre-existing), now closes each window through its own; (2) discard verdicts
now rescue the text into a new Untitled tab (the Preserved Drafts "Open"
path, draft-dirty) with a notice naming the cause; (3) the drop targets were
back while a load planned or text waited, so a MOVE drag's source could
delete text the buffer never took; they are now off for the whole held window.
Also: a tab holding typed input is never evicted; the settle waits for a
pending draft restore; reloads keep the caret, and a save-only episode replays
where the first character was held; the preedit is read at the settle and reset
after 1,024 ms; the input method's surrounding text includes the held text and
a `delete-surrounding` inside it edits it; a tab moved to another window is no
longer torn down (pre-existing: its load was cancelled) and is adopted by the
receiving window; a refused replay is rescued; one user action overflowing the
cap is refused whole; the settle runs at priority 124, above line validation
(A70), and a load's terminals queue it directly. Each fix has a widget test that
failed with the fix reverted (11 tests, `held_typing.rs`).

**Budget evidence** (throwaway widget probe, debug build, cairo renderer,
measurement lock, load 5.9–6.9 with a game on 2.4 CPUs, 3 repetitions,
`.claude/handoffs/keystrokes-verdict/budget/`), each replay against a **4 KiB
paste control** (the same text pasted at the same caret into the same document,
loaded):

| Fixture | Held / pasted | Replay dispatch | Paste | Worst dispatch in the 200 ms after (replay / paste) |
|---|---|---|---|---|
| `dense` 710 × 400, ASCII | 4,096 B | 0.71–0.84 ms | 0.68–2.4 ms | 8.5–9.3 / 3.2–13.1 ms |
| `dense` 710 × 400, four-byte | 1,024 emoji | 0.66–0.72 ms | 0.57–0.61 ms | 23.8–29.0 / 18.3–49.2 ms |
| 5 MB short lines, one line | 4,096 B | 0.37–0.39 ms | 0.46–0.78 ms | 2.6–4.8 / 2.4–4.5 ms |
| 5 MB short lines, 256 lines | 4,096 B | 2.9–3.5 ms | 3.7–10.7 ms | 2.9–3.8 / 3.2–7.8 ms |
| `.rs`, the replay crosses `L_SYNTAX` (syntax off) | 4,096 B | 0.89–1.19 ms | 2.5–3.0 ms | 6.2–8.5 / 3.8–14.5 ms |
| 48,000-character paragraph, the replay crosses `L_PROTECT_EDIT` (enters Protected View) | 4,096 B | 2.7–3.1 ms (13.6 cold) | 2.2–6.8 ms | 5.5–10.3 / 4.3–10.2 ms |

The replay dispatch stays within `B_SETTLE` at the cap, including when it
downgrades the document's scale or enters Protected View, and the work after
it is the same size as after the paste control (the four-byte rows are font
fallback in the cairo renderer). "The frame after costs what a paste costs" is
now measured rather than assumed.

**Lane rerun (interleaved, non-verdict: loaded host).** The review fixes
change what the lane sees only through the settle dispatch's priority and
hook, so the affected rows were rerun against the previous branch build
(d8cfbd0e): review / previous / review / previous, 2 repetitions each, the same
cases and phase as the verdict above, under the measurement lock
(`.claude/handoffs/keystrokes-verdict/{review,prev}{1,2}/`, merged in
`verdict-review/lane`). **The host was far from idle**: load 11.2–25.6 at the
run boundaries, another agent's ten CPU-bound loops plus a game, CPU contention
22–37 % of the time; storage was not a bottleneck in any run. Every run passed
its lane, and every phase of every case inserted every key on both sides (1,800
keys per run). Whether keys were held depends on whether the install outlasted
the first key, which varied with the load on both sides: held keys in
`total-size` 0 (review1), 282 (prev1), 271 (review2) and 0 (prev2). Keys that
were not held: `input_to_frame` max 17.7–29.4 ms. Held keys, drawn at the
replay: median 10.7–12.1 ms; their maximum is the install's remaining time
under the load, 139 ms (prev1, load 20–23) and 295–449 ms (review2, load
24–26), not the replay. `held_input_replay` spans 133–888 µs (8 replays). No
regression is attributable to the change; a verdict-grade comparison needs an
idle host.

### Landing: merge of `main` 44b1f067 (2026-11-03)

`main` brought `spawn-completion-priority` (PR #79): task completions and the
load/save hand-offs at `gtk_lush_tasks::completion_priority()` (124), with
aging, and the lane's per-completion spans. The merge auto-resolved; the
driver keeps both feature sets (`held-editor` delivery gates and
`save/during-validation`), and the completion summary now also counts
`held_input_replay` spans and the worst watchdog lateness of an iteration
overlapping one (`replay_combined_max_us`).

**Settle priority: 123, one above the completions.** The hard-coded 124 became
a derivation from the tasks crate. At the completion priority itself a replay
and a completion ready together run in one GLib iteration (A81), and their
bounds add past `B_SETTLE`: a replay that enters Protected View took up to
13.6 ms (D8), a completion up to 4.8 ms (PR #79's verdict). So the replay runs
at `completion_priority() - 1`, still above line validation (A70), and its
iteration dispatches nothing else of the application's. The lane did not catch
a combined iteration on an idle host either way:

| Interleave (main 44b1f067 vs branch, 2 reps x 2, load 0.8-3.7) | Settle at | Branch: misdirected phases | Held keys | Replays (max) | Iteration over a replay | Completions max / combined (branch, main) |
|---|---|---|---|---|---|---|
| `L` | 124 | 0 (main 1) | 67 | 2 (170 us) | no late tick | 4.6 / 4.2 ms, 4.3 / 3.3 ms |
| `L2` | 123 | 0 (main 0) | 34 | 2 (162 us) | no late tick | 5.0 / 3.5 ms, 4.8 / 4.4 ms |

Cases `baseline/small`, `total-size/{1M,5M}`, `dense/{120,500,710}` (phase
type-during-settle) and `save/during-validation`, under the measurement lock
(`.claude/handoffs/keystrokes-verdict/{L,L2}-*`). Every hand-off dispatch was
within `B_SETTLE_US` on both sides; `save/during-validation` saved in
2.1-4.9 ms on both sides (stall 19.3-21.0 ms on both: the validation backlog,
not the save). On an idle host most installs finish before the first key, so
few keys are held; main's misdirected phase is the defect this change fixes.

**Data safety (pre-existing, design D12).** A tab mid-install reported
`is_modified()` (GtkTextBuffer's flag), so closing it asked Save Changes and
the window close offered it. Its Save and Ctrl+S were refused, but two other
routes **wrote a buffer that was not the document**: a Save As during a load
pre-empted it and wrote the still-empty buffer, **truncating the file** it named
when that was the file being opened; and a memory eviction that went stale after
its first clear turn left an emptied, `Loaded`, "modified" tab whose Ctrl+S
**truncated the file**. Now `is_modified()` excludes a workflow's changes while it
runs (a live sliced install, an eviction in progress), no save pre-empts a
load in flight, every save refuses a buffer that is not a whole document
(`buffer_holds_no_whole_document`, new `EditorSaveError::DocumentNotInMemory`),
a replacement cancelled after it mutated clears the modified flag, and a stale
eviction is an eviction (reloaded at once when shown). Five widget tests in
`held_typing.rs`; four failed on the pre-fix tree (the Ctrl+S one is a guard).

## C2 `long-line-protected-view` (implementation record)

### Entry threshold: measured, then decided by the maintainer (2026-10-30)

Task 1.1 measured **`L_EDIT_STALL`**, the shortest paragraph length at which
one keystroke inside it exceeds B_interact (8 ms), with a `keystroke` sweep
(one paragraph of the swept length between 30 short lines, CSS and prose
flavours, keystrokes at its start, middle and end, wrap on and off, C1's
reduced features applied; plain and probe-IM variants, 2 repetitions on the
coarse grid and 7–9 near the bracket; evidence in
`.claude/handoffs/c2-measure/`: `sweep-table.md`, `keystroke-sweep.patch`, runs
`coarse-*`, `near-*`, `off-*`).

| Wrap | `L_EDIT_STALL` bracket | Note |
|---|---|---|
| on (default) | **3,500–5,000** (plain: 0/7 repetitions over at 3,500, first over at 5,000; probe IM: 2,500–3,500) | ratio 1.43 |
| off | **10,000–14,000** | ratio 1.4 |

Median per-keystroke stall over positions and shapes (max in brackets):

| Length | wrap on | wrap off |
|---|---|---|
| 5,000 | 4.7–5.5 ms (8.8) | 0.5–3.5 ms (5.5) |
| 10,000 | 6.5–7.4 ms (12.9) | 1.3–4.8 ms (8.6) |
| 20,000 | 7.3–9.9 ms (13.6) | 2.8–6.7 ms (10.0) |
| 50,000 | 18.5–21.0 ms (24.0) | 7.7–11.6 ms (16.5) |
| 100,000 | 41.7–44.7 ms (51.0) | 16.1–19.0 ms (27.0) |
| 200,000 | 104.5–106.9 ms (115.0) | 34.0–37.1 ms (41.5) |
| 500,000 | 152.0–186.8 ms (195.8) | 83.4–123.8 ms (128.9) |

The cost is GtkTextView's re-layout of the whole edited paragraph (A23) and
the buffer's whole-paragraph log attrs (A26); no LushText code is on the path.
The value is far below the tens of thousands the design expected, so the
choice returned to the maintainer (design D0), who chose **the hybrid**:

- **Editable up to `L_PROTECT_EDIT` = 50,000.** Documents whose longest
  paragraph is at most 50,000 characters stay editable under C1's reduced
  features. 50,000 is the length at which the median keystroke cost reaches
  about 20 ms.
- **Toolkit-floor exception, recorded, not a relaxed budget.** Keystrokes in
  editable paragraphs between `L_EDIT_STALL` (3,500) and `L_PROTECT_EDIT`
  (50,000) miss B_interact by the amounts above. B_INTERACT_US is unchanged;
  the exception names its range and costs, rests on A23 and A26 (whose ledger
  rows cite it), is re-measured by the lane's `keystroke` group, and is owned
  by the optional C5.
- **Protected View above `L_PROTECT_EDIT`**, with the one bounded undo of the
  edit that caused entry.

### Applied (tasks 1.1a, 1.1b, 2026-10-30)

- **The re-key (1.1a).** `model/document_scale.rs` gained `L_EDIT_STALL`
  (3,500, documentation of the exception's lower bound, deciding nothing) and
  `L_PROTECT_EDIT` (50,000); `protected_view` holds exactly when the longest
  paragraph exceeds `L_PROTECT_EDIT`. Density no longer produces
  `ProtectedView`, and as it reduced nothing else, `ReductionReason::Density`
  and the "dense long lines" reason class were removed (the profile and the
  edit-time watch still track `worst_batch_chars`, which the fold will read).
  `L_PROTECT` (500) stays the profile's first threshold. Failing first: a unit
  test of this repository's `AGENTS.md` profile (170,427 bytes, longest
  paragraph 8,314, worst window 132,281) failed on C1's key. The
  `core-document-scale` Kani shard's `scale_protected_view_is_upward_closed`
  states the new rule, it and `scale_of_a_profile_join_is_the_join_of_scales`
  iterate the two remaining reason classes, and the other harnesses are
  unchanged; the shard verified 9 of 9 harnesses on 2026-10-30. The
  exception is recorded on A23 and A26 in the axiom ledger.
- **The lane (1.1b).** The task 1.1 sweep is now the `keystroke` group of
  `scripts/responsiveness-smoke-driver.py` and the `keystroke` fixture kind
  (with the `prose` flavour) of `scripts/generate-long-line-fixtures.py`:
  one paragraph of exactly the swept length between 30 short lines on each
  side, markers at its start, middle and end; per position a `place-*`
  navigation phase (find the marker, Escape, Right) and a `key-*` interaction
  phase of ten keystrokes 150 ms apart; a `verify` phase that fails the case
  when a burst did not land right after its marker. Grids: quick 3,500 and
  50,000, full 2,500, 3,500, 5,000, 10,000, 20,000 and 50,000, CSS/`.html`
  and prose/`.txt`, wrap on and off (`RESPONSIVENESS_LENGTHS=keystroke=...`
  for other sweeps); the nightly workflow runs it as its own job. The
  fixture self-test found that the measurement patch's minimum length (37)
  produced a paragraph shorter than requested; the minimum is now 46.

### Fold row window: `R_fold = R_BATCH` (task 1.2, 2026-10-30)

The fold's density window counts **`R_fold` = `R_BATCH` = 224** paragraphs; no
edge row is added. GtkTextView validates whole lines from a line start until a
batch reaches 2000 px (A31: each step grows `upper` by 2000 px plus at most one
line; the measured step is 2016 px = 112 × 18 px), so a batch holds at most
`ceil(2000 / h)` lines of minimum row height `h`, **the line straddling the
2000 px edge included**: C0's 112 is exactly `ceil(2000 / 18)`, and its double,
224, covers `ceil(2000 / 9)` = 223 at the 50 % minimum zoom. A 2000 px window
starting at an arbitrary pixel (task 3.4's widget check) touches at most
`ceil((2000 + h − 1) / h)` rows: 113 at 18 px, 224 at 9 px. `R_BATCH + 1` would
also break the fold precondition `R_fold · N_DENSE ≤ V_BATCH`
(225 · 250 = 56,250 > 56,000). The argument assumes a row of at least 9 px at
50 % zoom, which C0's doubling already assumed; task 3.4 should lay out one
zoom-50 % fixture to tie it to the toolkit. The zoom range is 50–400 %
(`ui/window/zoom.rs`); `model/protected_fold.rs` names both ends, and a
compile-time assertion in `zoom.rs` is left to the first C2 milestone that
edits that GTK module.

`A28_SAFE_CHARS` = 59,579 is A28's overflow at the **largest** zoom
(`floor(i32::MAX / (9011 · 4))`; the overflow length shrinks as the advance
grows, so 400 % binds, not the smallest zoom). `N_HEAD` was the provisional
15,000 of design D1 (an A23 extrapolation) until the verdict measured it (next
section); it is now **10,000**, so `head_cap` = 10,000.

### Pre-existing blocker found by task 1.1

Typing into a 500,000-character paragraph with word wrap off logged A37's
`Trying to snapshot GtkGizmo … without a current allocation` in 2 runs
(`coarse-500000`: CSS wrap-off rep 0, prose wrap-off rep 1). C2 owns it under
the pre-existing-blockers rule (task 1.7).

**Fixed and re-checked (2026-10-31).** The fix (c61d9a7f) queues the view's
allocation when a tag leaves the tag table. The lane check runs on a binary
from before Protected View (47cf4d7b), where a 500,000-character paragraph is
still editable: `keystroke/500000/*/wrap-off`, plain × 3, reproduced no
defect and the lane's `FIXED_DEFECTS` check passed
(`.claude/handoffs/c2-verdict/a37-prepv/`).

### Verdict diagnostics (2026-10-31)

Runs in `.claude/handoffs/c2-verdict/` (`runs.log` records the load before and
after each run; each run is one exclusive hold through `scripts/measure.sh`).
The measurement build records a span for each dispatch the attribution needs:
`open_document`, `load_request`, `load_install_begin`, `load_install_slice`,
`load_complete`, `replacement_turn` and `protected_tail_insert`.

**`N_HEAD` (task 1.3).** The first branch round, with the provisional 15,000,
put protected open and post-open dispatches at 16–42 ms and find at 9–11 ms
(machine load 14–23). The lane's `keystroke` sweep of 2026-10-30 is task 1.3's
measurement: it opens one visible paragraph of a swept length between short
paragraphs. Its worst post-open dispatch is 12.7–14.8 ms at 10,000 characters
(prose `.txt`, wrap off and on), 16.1 ms at 14,000 wrap off and 20.7 ms at
20,000 wrap on; about 11 ms of each is the size-independent settle of a
500-character document. `N_HEAD` is therefore 10,000. The wrap-toggle
re-layout was not measured separately.

**Renderer × `N_HEAD`** (idle machine, load 1–2; plain × 2; worst over
open and the post-open settle, ms):

| Renderer | `N_HEAD` | frame layout | frame paint | worst dispatch outside the tail insert |
|---|---|---|---|---|
| cairo | 10,000 | 7.0–7.5 | 3.5 (wrap off), 13.4–13.5 (wrap on) | 9.9–15.7 |
| cairo | 5,000 | 6.6–7.4 | 2.5 (wrap off), 11.8–12.3 (wrap on) | 10.2–12.5 |
| GL (`GSK_RENDERER=ngl`, got `GskGLRenderer`) | 10,000 | 7.8–8.5 | 169–171 | 172–174 |
| GL | 5,000 | 7.9–10.2 | 170–172 | 172–175 |

Headless Mutter does accept the GL renderer, but its first paint costs about
170 ms whatever the head (size-independent: shader and pipeline set-up on the
headless EGL device), so the headless lane cannot measure GPU text paint; that
column says nothing about a desktop's GPU renderer. On cairo, the head adds
about 1 ms between 5,000 and 10,000, and every protected dispatch outside the
tail insert fits `B_settle` on an idle machine. The earlier "frame over 16 ms"
was load: no toolkit-floor exception is asked for.

**Attribution of the remaining long dispatches.**
- The one-turn hidden-tail insert: 22.7–23.4 ms for a 1.9M-character tail, 2.8–3.0 ms
  for 250,000 (`protected_tail_insert`), plus the load completion in the same
  dispatch (3.7–6.0 ms). This is the declared envelope, C3's gate input.
- Restore of three protected documents: GLib dispatches every ready source of a
  priority in one main-loop iteration, so two documents' tail inserts ran back
  to back in one dispatch (52 ms). `ui/protected_fold.rs` now admits one tail
  insert per iteration process-wide (6761ebf0); each document's envelope then
  stands alone (25–32 ms including its completion).
- CR-only (`delimiters/cr`, a 5.2 MB minified `.html`): a 43–47 ms dispatch
  was `open_document` itself, loading the HTML language definition for a
  document whose scale then cleared it. A pending load no longer applies a
  language; the load publication does, only when syntax is allowed (6761ebf0).
- `eviction/reselect` exceeds its startup wait on `main` and the branch alike.
  The case opens ten 8 MiB short-line documents. Between batches the GTK thread
  runs about 90 dispatches a second of about 10 ms each with no frame and no
  LushText span: GtkTextView's background line validation (priority
  `GDK_PRIORITY_REDRAW + 5`) of the loaded documents. The loads' worker
  completions, delivered by `spawn_blocking_then` through
  `G_PRIORITY_DEFAULT_IDLE`, wait behind it, so the documents install in three
  batches about 11 s apart and `app-startup` readiness takes over 30 s. Every
  dispatch stays near 10 ms, so no budget is missed. The case cannot reach its
  reselect phase as written, so it is excluded from the C2 verdict.
  **Queued GTK Lush change (scheduled after C2):** the delivery priority of
  `spawn_blocking_then` completions (`gtk-lush-tasks`, `glib::idle_add_once`,
  `G_PRIORITY_DEFAULT_IDLE`) against GtkTextView's validation idle
  (`GDK_PRIORITY_REDRAW + 5`): about 11 s between load batches and over 30 s
  to `app-startup` readiness, on `main` as on the branch.
  **Done as SC (`spawn-completion-priority`, 2026-11-02), with a corrected
  premise.** Re-attributed on `main` cdfff201, after C6 merged, the
  completions were no longer what held this case back: each next load's
  installation began 26 ms after the previous permit's release. The batches are
  the load permit held through a validation-paced install, and hidden tabs
  validating fully. That cause is queued as **EV** (see "Sequence and gates").
  The completion starvation is real for any completion that arrives during a
  validation backlog (a save, a draft, a small file), so SC fixes that class
  and pins it as A80.

### Verdict (2026-10-31 – 2026-11-01)

Measured under the load there was, recorded before and after every run in
`.claude/handoffs/c2-verdict/runs.log` (maintainer rule: never pause or discard
a run for load); each run one exclusive hold through `scripts/measure.sh`,
`main` (713cad83; 10cad94a for the first rounds, a docs-only difference) and
the branch alternating. Toolkit: GTK 4.22.5, libadwaita 1.9.3, Pango 1.57.1,
GtkSourceView 5.20.0, GLib 2.88.3, Mutter 50.4, cairo renderer, 1280×800 at
120 Hz.

Runs: `V-*` (full set: plain 4+3, AT and probe IM × 3, the 100 MB install;
load 2–30), `O-*` (the cases whose single runs went over; load 9–37),
`F-*` (the same cases, plain 3+2, load 1.2–3.6) and `Q-atim-branch` (AT and
probe IM × 3, load 0.7–4). `eviction/reselect` is excluded (see above).

**Medians, `main` → branch, plain, `F-*` (ms):**

| Case | open | after open | scroll | Page Down | click | typing | find | other |
|---|---|---|---|---|---|---|---|---|
| 250,000 wrap on | 112 → 12.2 | 132 → 14.0 | 6 → 2.9 | 9 → 2.7 | 68 → 1.4 | 22,021 (cap) → 2.3 | 170 → 3.5 | |
| 1.9M wrap on | 930 → 12.2 | 1,119 → 28.6 ¹ | 19 → 3.3 | 41 → 2.5 | 580 → 2.3 | 22,021 → 2.3 | 1,455 → 4.2 | |
| 1.9M wrap off | 748 → 11.8 | 750 → 25.2 ¹ | 19 → 2.6 | 33 → 2.7 | 403 → 1.4 | 22,021 → 2.9 | 1,128 → 2.0 | |
| CR-only | 519 → 21.7 ¹ | 770 → 8.3 | 6 → 2.2 | 3 → 1.0 | 3 → 1.0 | 9 → 1.6 | 770 → 6.5 | |
| U+2029 | 517 → 20.9 ¹ | 769 → 10.5 | 6 → 5.2 | 2 → 1.0 | 2 → 1.4 | 7 → 1.9 | 766 → 6.5 | |
| restore of three | 1,648 → 25.8 ¹ | 338 → 12.4 | 5 → 5.1 | 41 → 1.0 | 575 → 1.7 | 22,009 → 6.3 | 1,454 → 2.9 | restore 2,890 → 30.9 ¹, tab switch 1,114 → 4.2 |
| reload 250,000 | 114 → 12.3 | 1 → 1.4 | 2 → 5.6 | 6 → 1.0 | 7 → 1.3 | 22,020 → 1.5 | 173 → 3.0 | reload 68 → 2.9 |

¹ The phase contains the declared envelope: one hidden-tail insert per
document per dispatch, 22.7–28.1 ms for a 1.9M-character tail plus its load
completion (3.6–7.0 ms), the `protected_tail_insert` span. Outside those
dispatches every median above is within budget.

**The envelope (C3's gate input).** 1.9M tail: 23–28 ms per dispatch; with AT
on, 31–49 ms. The 100 MB install (`install/100000000`, plain and AT × 2):
1.28–1.37 s, against `main`'s 73–76 s after open.

**Toolkit-floor exception (keystrokes, recorded 2026-10-30).** Re-measured:
`keystroke/49990` key-start 17–18 ms, the 50,000-character editable paragraph
inside the range; `keystroke/3500` 5.2–6.5 ms on the branch, 1.0 or 5.6–5.9 ms
on `main` (bimodal by run, both within `B_interact`).

**Single maxima over budget on the quiet host, each attributed** (span or
`perf record -e cycles:u --call-graph dwarf`, evidence only):
- Window construction in `open` (13–19 ms: `gtk_widget_init_template`,
  `gtk_window_realize` → `icon_list_from_theme`, CSS validation): C4/C4b's
  composition stages, the same on `main`.
- The first frame that shows a head (17–20 ms on 1.9M wrap on, plain and
  probe IM): layout of the 10,000-character head (`gtk_text_layout_wrap`,
  Pango shaping, 4–9 ms) plus the cairo paint of a full screen of text
  (`draw_text` → `_cairo_image_surface_glyphs`, FreeType autofitting of
  first-use glyphs, 13.5–16 ms). The paint does not depend on `N_HEAD`
  (5,000 saves about 1 ms) and the headless GL renderer cannot measure it
  (170 ms first paint, set-up). On 1.9M wrap on its median is 17.6 ms in
  `F-*` (14.0 ms on 250,000), 1.6 ms over `B_settle`. It is the first frame
  of any screen of wrapped text under the cairo renderer, not a Protected View
  cost. **Accepted maintainer exception (2026-11-01):** the maintainer accepted
  this 17.6 ms first head frame (1.9M paragraph, word wrap on, 1.6 ms over
  `B_settle`, the cairo text paint plus the head layout) as a recorded
  exception, to be re-evaluated when the lane can measure the GPU renderer.
- Restore of three documents (22 ms): `GKeyfileSettingsBackend`'s file monitor
  re-checking writability after the settings flush
  (`g_keyfile_settings_backend_keyfile_writable` → `g_local_file_query_info` →
  libmount re-parsing the mount table), GIO's keyfile backend (C4, A38–A41); and
  the first frame of three new editor pages (CSS validation of their widgets).
- The 100 MB install's after-open frame (22–24 ms, every load): line
  validation and Pango shaping of the head (`gtk_text_view_validate_onscreen` →
  `gtk_text_line_display_cache_get`); it grows with the hidden tail
  (7 ms at 1.9M on an idle host), so it belongs with C3's hidden-tail
  re-validation axiom (S2) and its gate.
- Find 8.0–8.3 ms and a 3,500-character click 8.1 ms: single runs, medians
  6.5–7.0 ms.

**Verdict.** Every Protected View interaction median is within `B_interact`
(10–100× under `main`). Settle medians are within `B_settle` outside the
declared envelope, except the first head frame on 1.9M wrap on (17.6 ms, the
cairo paint above), an accepted maintainer exception (2026-11-01; re-evaluate
when the lane can measure the GPU renderer). The keystroke toolkit-floor
exception for editable paragraphs between `L_EDIT_STALL` and `L_PROTECT_EDIT`
(maintainer, 2026-10-30, "editable up to ~50k"; owned by C5) is accepted, not
a blocker. Every remaining maximum is attributed above.

## C2b `protected-view-inspector-and-surfaces` (2026-11-01)

The reading surfaces C2 deferred: the Line Inspector (worker-read windows of
at most `N_INSPECT` characters, Previous/Next Window and Page Down/Up, Go to
Line and Column, Copy Window), Copy Whole Line (a lazy `GdkContentProvider`
over the shared source, one `COPY_CHUNK_BYTES` chunk per awaited write), the
Protected View menu, the fold-marker gutter, the one inline alert (Inspect
Line, or Undo Edit while the entry undo is available, merged with a recovered
draft's Discard and Save), the rate-limited pre-emption notice, navigation from
bookmarks, workspace search, local history and session restore through the
`protected-nav` mark (the restored cursor is kept for later session saves), the
local-history preview and note editors folded at insertion, the file-peek
sample cut at `HEAD_CAP`, and Save As refused for an unmodified document whose
decode does not round-trip (`LoadMetadata::decode_round_trips`).

### Protected heads and C5's wrap gate (step 0, measured)

C5 tags every paragraph longer than `L_wrap_edit` with no-wrap, protected heads
included, so since C5 a protected head is shown unwrapped; C2's accepted 17.6 ms
head-frame exception was measured wrapped. Interleaved, one exclusive hold per
run, `main` 7f50d232 against a build that exempts protected paragraphs from the
gate, plain, 3 + 3 repetitions each (runs `pv-inspector/wrap/{main,exempt}{1,2}`,
load 0.8–3.0 before, inside and after each hold):

| Case | Build | Worst head paint (ms) | Hidden-tail insert, declared envelope (ms) | Worst dispatch outside it (ms) |
|---|---|---|---|---|
| 1.9M wrap on | `main` (gate) | 3.3–3.7 | 13.7–13.9 | 8.7–12.5 |
| 1.9M wrap on | exempt (head wraps) | 13.3–13.9 | 18.7–18.9 | 12.2–16.2 |
| 1.9M wrap off | `main` | 3.3–3.8 | 13.8–14.0 | 8.7–12.5 |
| 250,000 wrap on | `main` | 3.8–3.9 | 1.7–2.2 | 9.6–14.2 |
| 250,000 wrap on | exempt | 13.8–13.9 | 2.4–2.5 | 12.4–13.8 |

The exempt build's hidden-tail insert is also about 5 ms slower than `main`'s
(18.7–18.9 against 13.7–13.9 ms on 1.9M wrap on). That difference is not the
head's wrapping, which runs only when the head is laid out, after the insert:
the exempt build's only change is an early return in the `insert-text`
handler, and the two binaries were built minutes apart from different trees,
so it is recorded as build-to-build variation of that one envelope, not as a
cost of wrapping.

**Decision: keep the gate.** Wrapping the head costs a full screen of text
painted under the cairo renderer (10 ms more per head frame), pushes one sample
over `B_settle` and makes C3's envelope 5 ms longer, to show text the Line
Inspector already shows wrapped. With the gate, C2's 17.6 ms head-frame
exception does not occur on `main`. Recorded as the spec requirement "Protected
heads follow the per-paragraph wrap gate".

**Follow-up idea (queued, not scheduled): a shorter wrapped head.** With word
wrap on, a protected head could keep fewer than `L_PROTECT` characters and wrap
them: a head short enough to fill a few wrapped rows would read like the rest
of the document while costing less than the unwrapped 500-character head's
paint. Needs its own measurement against `B_settle`; owner none yet.

### Verdict (2026-11-01)

Interleaved, one exclusive hold per run (`scripts/measure.sh`), `main` 7f50d232
and the branch alternating, the branch's lane driver measuring both binaries
(runs `pv-inspector/verdict/{branch,main}-plain{1,2}` and `-atim`; load
0.8–3.1 before, inside and after every hold, recorded in
`.claude/handoffs/pv-inspector/runs.log`, accepted, not gated). C2's protected
cases (250,000 and 1.9M wrap on, 1.9M wrap off, CR-only, U+2029, restore of
three, reload of 250,000), plain 3 + 3 repetitions per side; AT and probe IM
2 repetitions each on 1.9M wrap on. The driver gained two phases on protected
tabs: `inspect` (`win.inspect-line`, ten Page Downs, Escape) and `copy-line`
(`win.copy-whole-line`, then `wl-paste` reads the whole line from the private
session); a build without the actions records them unavailable. Toolkit as
C2's verdict (GTK 4.22.5, libadwaita 1.9.3, cairo renderer, 1280×800 at
120 Hz).

**The new interactions (branch, worst per repetition, ms)** — *superseded by the corrected, per-input verdict under "Review fixes and corrected verdict" below: these paging figures are stall windows, not inputs, and the plain runs predate the driver's 0.5 s wait after the dialog opens:*

| Case | Variant | reps | Inspector open transition | Paging, 10 windows | Close transition | Copy Whole Line, served to `wl-paste` |
|---|---|---|---|---|---|---|
| 1.9M wrap on | plain | 6 | 25.7–29.8 | 4.8–7.8 (median 6.0) | 24.7–30.6 | 1.0–2.0 |
| 1.9M wrap on | AT | 2 | 26.1–26.4 | 5.9–6.8 | 27.7–28.3 | 1.0 |
| 1.9M wrap on | probe IM | 2 | 26.2–27.0 | 6.3–6.7 | 27.1–27.4 | 1.0 |
| 1.9M wrap off | plain | 6 | 25.8–26.7 | 4.6–6.7 | 25.1–31.1 | 1.0–4.3 |
| 250,000 wrap on | plain | 6 | 25.6–31.7 | 4.9–6.4 | 24.8–29.5 | 1.0–5.6 |
| reload 250,000 | plain | 6 | 26.0–26.5 | 5.2–6.3 | 24.4–27.9 | 1.0–4.1 |
| CR-only | plain | 6 | 25.4–26.1 | 0.0–1.7 | 26.9–31.8 | 1.0–1.6 |
| U+2029 | plain | 6 | 25.8–26.4 | 0.0–1.4 | 24.9–29.9 | 1.3–6.3 |

(The restore-of-three case's active tab is the ordinary document, so its
`inspect` phase only posts the status message.) Every Line Inspector window
move is within `B_interact` (a worker reads the window; the GTK dispatch sets
at most `N_INSPECT` characters and repaints the region the dialog covers), and
Copy Whole Line's chunked writes are within `B_settle` while another client
reads the 1.9M-character line.

**Toolkit-floor exception: the dialog's open and close transitions.** Every
frame of `AdwDialog`'s open and close animation (about 15 frames each way)
takes 24–32 ms under the lane's cairo renderer, for any content. `perf record
-e cycles:u --call-graph dwarf` of the `inspect` phase (run `pv-inspector/perf1`)
puts 14 % of the application's cycles in pixman's
`bits_image_fetch_bilinear_affine_normal_a8` under `general_composite_rect`:
the transformed (scaled) A8 masks the dialog's scale transition composites in
software. It is Libadwaita's dialog transition under the cairo renderer, not
Protected View work (paging inside the same dialog costs at most 7.8 ms), and
**it applies to every `AdwDialog` LushText presents under that renderer**
(Preferences, the local-history browser, the note editors, every alert), not
only to the inspector: the cost is the transition's, whatever the content.
Like C2's accepted 17.6 ms head-frame exception, it is to be re-measured with a
GL renderer (`GSK_RENDERER=ngl` or `vulkan`) when the lane can run one
headless; owner: the lane, no change scheduled. Recorded, not fixed: replacing the
`AdwDialog` with a separate window would drop the HIG-standard dialog to work
around a software-rendering cost.

**C2's cases, `main` → branch, median / max of each repetition's worst (ms):**
open 14.4/14.6 → 14.6/15.3 (1.9M wrap on), 17.4/18.1 → 17.0/17.9 (CR-only,
the declared hidden-tail envelope), 17.6/18.7 → 17.0/18.9 (U+2029, the same);
restore of three 17.5/17.7 → 15.5/15.9 (the restored position now scrolls the
navigation mark instead of positioning an iterator inside a hidden tail);
every interaction phase (scroll, Page Down, click, typing, find, tab switch)
within `B_interact` on both sides, with one branch repetition of restore-three
`type-start` at 8.2 ms (median 5.5, `main` 6.4/6.8). AT and probe IM on 1.9M
wrap on: no phase over budget outside the dialog transition.

### Review fixes and corrected verdict (2026-11-02)

An independent review of PR #75 found three must-fix and seven should-fix
defects; all are fixed in the second commit, each bug with a test that failed
first (`.claude/handoffs/pv-inspector/review-ff-unit.log`,
`review-ff-widget-head.log`, the widget tests run against 04f702ae):

- **Folded surfaces copied visible text only (M1).** A copy across a folded
  paragraph in the local-history preview put 10,326 of 56,716 bytes on the
  clipboard, after GTK had first copied the whole selection, hidden tails
  included, into a second buffer on the GTK thread. The preview and the note
  editors now copy from their source on a worker (`ui/folded_text.rs`: Ctrl+C,
  Ctrl+X, the menu and the primary selection; a drag of a selection that
  reaches a hidden tail does not start).
- **The note editor profiled and installed up to 16 MiB in one dispatch (M3).**
  It now profiles on a worker and installs in bounded turns through the
  preview's slice step (`ui/window/notes/install_execution.rs`), measured below.
- **The paging evidence (M2) was not per input.** The probe never saw input
  delivered to an `AdwDialog`, so all eleven inspector injections were
  `no-input-record`, and the plain runs predated the driver's 0.5 s wait, so
  Page Downs 1–3 were counted in the open transition. The probe now stamps the
  input of every dialog the window presents, once per event.
- Should-fix: Copy Whole Line copies only the paragraph's bytes (S1); one
  insertion into a single-line entry is cut at 16 KiB with a notice (S1b; 16
  rather than 64 because `GtkEntryBuffer` itself stops at 65,535 characters);
  the round-trip check runs only for protected profiles, in 64 KiB chunks (S2);
  an unmodified protected document's Save As is a byte-exact copy of its file,
  refused only when edited with a decode that does not round-trip (S3); the
  fold-marker column has a fixed width from construction, so no entry or exit
  resizes the text area (S4, new axiom A75); the inspector steps between folded
  lines and states each line's hidden count, notices are rate-limited per
  reason, and the accessibility smoke drives Ctrl+Alt+I and Shift+F10 (S5);
  Copy Whole Line announces "Line N copied (X characters)" (S6).
- Found on the way, pre-existing: **UTF-16 documents were saved as UTF-8**
  after a UTF-16 byte-order mark (`encoding_rs`'s output encoding for UTF-16 is
  UTF-8); **a click in a protected document never focused it** (the capture
  gesture claimed the press before GtkTextView's own gesture could), so the
  keyboard kept acting on whatever had focus, which is why the earlier
  verdict's paging and typing on protected documents produced no frame or no
  visible change; and the smoke tooling's `atspi-key` never delivered a key
  under Wayland (AT-SPI sends keycode 0), replaced for chords by `rd-key:`
  (Mutter RemoteDesktop).

**With C6 (merged 2026-11-02).** The entry undo is unavailable while a
workflow holds the edit lock (C6), and the alert follows: taking or releasing
the lock republishes it with the action that works then (Undo Edit or Inspect
Line). `decode_inexact` is set by the publishing load terminal and cleared by a
failing or cancelled one. The fold-marker column never changes width, so it
needs no ordering against C6's `set_editable` before the first slice.

**Corrected verdict.** Interleaved, one exclusive hold per run, `main`
7f50d232 against the review-fix build (`bin/branch-review`), the committed
driver and the dialog-stamping probe, same cases and repetitions as above (runs
`pv-inspector/verdict2/{branch,main}-plain{1,2}` and `-atim`; load 0.5–1.5
before and after every hold, `runs.log`). Every inspector injection is now
matched (0 without a record). Per input, ms; a Page Down after the line's last
window changes nothing and is listed separately:

| Case | Variant | reps | Page Down input-to-frame (ms) | worst dispatch per Page Down (ms) | open transition (ms) | close transition (ms) | Escape input-to-frame (ms) | Copy Whole Line (ms) | Page Downs past the last window | inputs with no record |
|---|---|---|---|---|---|---|---|---|---|---|
| delimiters/cr | plain | 6 | 3.5–12.8 (med 10.8) | 1.2–5.7 (med 3.1) | 25.9–29.4 (med 28.4) | 28.6–31.7 (med 30.7) | 3.7–38.0 (med 11.3) | 1.2–3.9 (med 3.2) | 40 | 0 |
| delimiters/u2029 | plain | 6 | 2.7–13.2 (med 10.3) | 0.0–6.3 (med 3.5) | 27.6–29.5 (med 28.5) | 25.6–32.6 (med 31.2) | 9.5–40.5 (med 30.8) | 1.0–5.4 (med 3.7) | 30 | 0 |
| paragraph/1900000/css/txt/wrap-off/minimap-off | plain | 6 | 2.4–14.5 (med 10.3) | 0.0–7.9 (med 3.2) | 27.9–29.3 (med 28.7) | 29.4–33.8 (med 32.2) | 7.0–43.0 (med 39.6) | 1.0–4.3 (med 2.4) | 0 | 0 |
| paragraph/1900000/css/txt/wrap-on/minimap-off | at | 2 | 3.0–12.7 (med 9.7) | 0.0–5.2 (med 2.9) | 30.2–31.5 (med 30.8) | 29.6–31.9 (med 30.8) | 34.5–41.6 (med 38.1) | 1.7–3.7 (med 2.7) | 0 | 0 |
| paragraph/1900000/css/txt/wrap-on/minimap-off | plain | 6 | 2.5–14.8 (med 10.0) | 0.0–6.5 (med 2.8) | 27.9–29.2 (med 28.7) | 31.3–33.8 (med 32.0) | 4.9–43.6 (med 40.3) | 1.0–5.1 (med 3.9) | 0 | 0 |
| paragraph/1900000/css/txt/wrap-on/minimap-off | probe-im | 2 | 4.5–13.4 (med 10.2) | 0.0–6.1 (med 3.5) | 28.0–28.2 (med 28.1) | 32.5–33.7 (med 33.1) | 42.1–46.6 (med 44.3) | 2.8–3.2 (med 3.0) | 0 | 0 |
| paragraph/250000/css/txt/wrap-on/minimap-off | plain | 6 | 2.4–13.7 (med 9.6) | 0.0–6.5 (med 3.0) | 27.8–30.1 (med 28.9) | 29.9–35.0 (med 32.9) | 37.0–44.0 (med 41.7) | 2.5–4.1 (med 2.9) | 0 | 0 |
| reload/250000 | plain | 6 | 2.5–14.0 (med 9.7) | 0.0–6.5 (med 3.0) | 28.2–29.6 (med 29.0) | 30.4–33.8 (med 32.6) | 35.0–41.2 (med 39.8) | 1.5–3.1 (med 2.2) | 0 | 0 |
| session/restore-3 | plain | 6 | 2.3–12.8 (med 9.8) | 0.0–0.0 (med 0.0) | 0.0–0.0 (med 0.0) | 0.0–0.0 (med 0.0) | 9.2–37.5 (med 21.2) | 1.3–5.6 (med 2.8) | 0 | 0 |

Every Page Down inside the Line Inspector is presented within two 120 Hz frames
(at most 14.8 ms from injection) and its dispatch stays within `B_interact`
(at most 7.9 ms); the open and close transitions (25–35 ms) and Escape's
input-to-frame (up to 47 ms, the close transition) are the recorded
`AdwDialog` exception. Copy Whole Line, now copying the paragraph on a worker
and served to `wl-paste`, stays within 5.6 ms.

C2's cases, `main` → review-fix build, median / max of each repetition's worst
dispatch (ms): open within 0.6 ms of `main` everywhere (1.9M wrap on 14.2/14.4
→ 14.3/15.0; CR-only 17.5/18.0 → 17.0/17.7; U+2029 16.9/17.0 → 16.8/17.8, the
declared hidden-tail envelope), restore of three 16.9/17.5 → 15.5/16.2; reload
of 250,000 open 8.7/8.8 → 9.9/13.4 (one repetition), within `B_settle`. Every
interaction phase stays within `B_interact`: with the focus fix, paging,
Ctrl+Home and typing on a protected document now reach the view (viewport
scroll and the rate-limited notice), so CR-only and U+2029 paging is 6.1–7.3 ms
(`main` 1.0, where nothing scrolled) and typing 1.7–6.8 ms (`main` 1.0–2.8). In
two U+2029 repetitions the open's hidden-tail insert (the declared envelope,
19.1 ms) fell after the phase boundary and is charged to `background`. AT and
probe IM on 1.9M wrap on: no phase over budget outside the dialog transition.
The lane's mutter log prints
`meta_window_set_stack_position_no_sync: assertion 'window->stack_position >= 0'`
once per `copy-line` phase on the branch (never on `main`, which has no such
phase): it is the compositor's reaction to `wl-paste`'s own short-lived
surface, at the phase's start, not to LushText.

**Note editor install (M3) and gutter (S4), measured** in an optimized widget
build (`CARGO_PROFILE_{DEV,TEST}_OPT_LEVEL=3`, one exclusive hold):
a document note of 40,000 short lines and one 60,000-character paragraph
installs in 4 turns, the longest 3.5–3.6 ms (two runs), where it used to be
one dispatch (`test_a_long_line_note_installs_folded_in_bounded_turns_and_copies_from_its_text`).
On a document of thirty 40,000-character wrapped lines, an edit-growth entry
is one 9.1–9.4 ms dispatch with nothing after it and leaves the gutter's width
alone; its entry undo is a 1.9–2.1 ms dispatch followed by a 139 ms one, which
`perf` (`cycles:u`, run `pv-inspector/perf2`) puts in Pango's layout of the
restored 40,000-character paragraph (`default_break` 22 %): the toolkit floor
of an editable paragraph below `L_PROTECT_EDIT`, C2's recorded exception, not a
whole-document relayout. For comparison, a gutter width change of the same
document (what showing the marker column at entry did) is a 230–241 ms
dispatch: every paragraph laid out again, new ledger axiom **A75**
(`make gtk-axioms`: the last, offscreen paragraph of a wrapping view lands
exactly where a fresh view at the new width puts it).

**Deferred: task 5.1a** (lane cases for Save As of a protected document and a
live word-wrap toggle; **done in C3**, task 7.6, `protected/1900000/save-as-wrap`). Both need a measurement-build hook (the file chooser,
a runtime settings write) the lane does not have; Save As writes the source on
the worker with no GTK snapshot, pinned by C2's widget tests. Owner: C3
(`sliced-protected-install`), whose verdict re-measures the protected install
and its re-layouts.

## C3 `sliced-protected-install` (2026-11-02)

Branch `sliced-protected-install` on `main` 8cd1441a (C2, C2b, C5, C6 merged).
**Adopted, both halves** (design D1): C2's verdict had already measured the
one-turn hidden-tail insert over `B_SETTLE_US` (23–28 ms at 1.9M characters,
0.76 s at 100 MB, 3.5 s per frame after a 32 MiB recovery), and the clear of a
protected tail was one paragraph-sized turn too.

### What was built

- `model/protected_install.rs`: the GTK-free planner (visible slices, one
  head per folded paragraph, hidden-tail slices of at most
  `PROTECTED_TAIL_SLICE_BYTES` ending on a scalar boundary and never inside a
  CRLF), the clear plan (`clear_ranges_for_turn`), and
  `ProtectedInstallLimits::for_settle_budget`. `for_settle_budget(B_SETTLE_US,
  26 µs/KiB, 100 %)` is 315,076 bytes, above `MAX_SLICE_BYTES`, so both slice
  constants are the 256 KiB cap, a power of two.
- `ui/protected_fold.rs::FoldedInstall` executes it for load, buffer
  replacement, the local-history preview and note bodies: a tail that needs
  several slices goes in one slice per turn, each followed by a left-gravity
  boundary mark (A76: no segment merge copies a slice); the paragraph's
  **delimiter goes in before its tail** and its **head after it**, at a
  left-gravity mark at the paragraph start (A76), so no slice re-lays out the
  head (A77) and the growing paragraph is never the buffer's last line (A79).
  A tail that fits one slice goes in after its head in the same turn,
  unmarked, and several share a turn within the slice budget.
- `BoundaryLedger` (one per editor) keeps the marks and range sizes, never
  text; a clear deletes from each paragraph's start forwards, as many ranges
  as fit `PROTECTED_CLEAR_SLICE_BYTES` per turn (A76: a backwards clear's
  per-range cost grows with the text before the range).
- **The stand-in terminator.** A body whose last paragraph is folded and has
  no delimiter gets a `\n` under an invisible, non-editable tag
  (`lushtext-protected-install-terminator`), kept after the install.
  `document_end_iter` stops before it, and every buffer snapshot (save, draft,
  local history, preview), the properties panel's statistics and the widget
  tests read through it.
- Protected install turns are paced at `PACED_SLICE_PRIORITY` through
  `input_quiet::paced_idle`, inside `edit_lock::with_own_edit`; C2's
  one-tail-insert-per-iteration admission now applies per slice.
- Evidence: `LoadEvidence::protected` and `BufferReplacementEvidence::protected`
  (`ProtectedSliceEvidence`), and the editor's memory estimate charges 128
  bytes per live boundary.
- Lane: the `protected` group (Save As of a protected document through the
  measurement build's `app.probe-save-as`, a live word-wrap toggle, zoom and
  font change: C2b task 5.1a, carried here as task 7.6), the
  `recovery/protected-32mib` case (C2's D13 shape: two recovered single-paragraph
  drafts of 32 MiB, no trailing delimiter), and the install cases' small first
  tab, mid-install tab switches and scrolls, and reload (clear plus
  re-install).

### What the implementation found

- **A79, a GTK defect (toolkit floor, upstream report drafted).**
  `gtk_text_view_get_iter_at_location` for a point below the layout takes the
  last line and asks `gtk_text_iter_set_visible_line_index` for the line's full
  byte count; at the end iterator the loop subtracts one byte per iteration,
  so it walks the last line's hidden bytes one at a time (probe A79: 43.9 /
  88.0 / 175.1 ms at 1 / 2 / 4 MiB undelimited, 4 µs delimited; about 42 ns
  per byte). GtkSourceView's gutter and below-text layer ask for that point on
  every drawn frame while the visible area extends below the text. The first
  lane trials (tail first, delimiter after it) froze for 316–364 ms per frame
  at 1.9M characters and 182 s in total at 100 MB (perf, `perf-p19`,
  `perf-i100`), and `main` pays 3.5 s per frame after the 32 MiB recovery, a
  pre-existing blocker. The delimiter-first order and the stand-in terminator
  keep every protected paragraph off the last line. Failing first on `main`
  (`c3-ff` worktree): one insert of 2,990,000 bytes, one clear of 3,000,861
  characters, and a below-the-layout lookup of 69.7 ms for a 1M-character
  undelimited paragraph. Upstream draft:
  `.claude/handoffs/c3/upstream-gtk-set-visible-line-index.md`.
- A dense document took one turn per folded paragraph (405 turns for 801
  paragraphs, two marks each) until one-slice tails went in unmarked and
  batched (widget test `..._dense_small_tails_are_batched`).
- Backwards clearing grew per range (A76); the clear is forwards.
- A tail slice's insert grows with the paragraph's segment count (A77: bounded
  by the visible text and the segments): 1.84 ms for the first tenth of the
  slices to 2.18 ms for the last at 100 MB (382 slices) and 1.86 → 2.77 ms at
  250 MB (954 slices), and 1.90 → 3.74 ms (4.1 ms at most) at the 500 MB open limit (1,908 slices): linear in the slice count, within `B_SETTLE_US` up to the largest document LushText opens.

### Verdict (interleaved, 2026-11-02)

`main` 8cd1441a against the branch build (`bin/branch-final`), the branch's
driver for both, one exclusive hold per run (`scripts/measure.sh`),
alternating; load 0.6–1.0 before and after every hold (`runs.log`). Worst
dispatch per repetition, median / max (ms), and `load_request` to
`load_complete` (ms, median):

| Case | Phase | `main` | branch | budget |
|---|---|---|---|---|
| paragraph 1.9M wrap on | open | 14.5 / 14.6 | 8.9 / 9.2 | 16 |
| paragraph 1.9M wrap off | open | 14.7 / 15.1 | 8.6 / 10.0 | 16 |
| paragraph 250,000 | open | 8.7 / 9.1 | 8.9 / 9.7 | 16 |
| delimiters/cr | open | 31.3 / 31.6 | 31.0 / 32.1 | 16 ¹ |
| delimiters/u2029 | open | 31.3 / 32.0 | 31.2 / 32.8 | 16 ¹ |
| session/restore-3 | restore | 14.6 / 15.9 | 8.8 / 8.9 | 16 |
| reload/250000 | reload | 5.0 / 6.6 | 5.8 / 7.1 | 16 |
| protected 1.9M ⁵ | open | 14.5 / 14.8 | 8.8 / 8.9 | 16 |
| protected 1.9M ⁵ | save-as | — ² | 5.9 / 10.3 | 16 |
| protected 1.9M ⁵ | wrap-toggle | 3.4 / 4.2 | 2.1 / 4.4 | 16 |
| protected 1.9M ⁵ | zoom-step | 11.9 / 14.3 | 11.9 / 14.9 | 16 |
| protected 1.9M ⁵ | font-change | 13.9 / 15.4 | 15.0 / 16.6 | 16 ³ |
| recovery/protected-32mib | restore | 3,526 / 3,563 | 10.0 / 11.2 | 16 |
| recovery/protected-32mib | scroll | 1,769 / 1,780 | 1.0 / 2.1 | 8 |
| recovery/protected-32mib | tab-switch | 3,528 / 3,546 | 7.3 / 8.7 | 8 |
| install 100 MB | open (mid-install) | 756 / 758 | 8.7 / 8.8 | 16 |
| install 100 MB | reload | 5.8 / 6.2 | 3.7 / 6.3 | 16 |
| install 250 MB | open (mid-install) | 1,896 / 1,901 | 8.9 / 9.1 | 16 |
| install 250 MB | reload | 3.1 / 4.3 | 5.1 / 7.2 | 16 |
| install 500 MB ⁴ | open (mid-install) | > 180,000 (cap) | 8.7 / 8.8 | 16 |
| install 500 MB ⁴ | reload | — | 8.0 / 8.4 | 16 |
| AT on: install 100 MB | open | 894 / 898 | 10.1 / 10.1 | 16 |
| AT on: recovery 32 MiB | restore | 3,523 / 3,529 | 10.4 / 10.6 | 16 |
| AT on: paragraph 1.9M | open | 12.3 / 14.6 | 10.1 / 10.1 | 16 |

¹ Both sides: the 31 ms dispatch is `post_open_language_attach` of the
`.html` fixture (C6's recorded A67 toolkit-floor exception, the HTML
language-definition parse); `main`'s second-largest dispatch, the one-turn
tail insert (18.7 ms), is gone on the branch.
² `main` has no `app.probe-save-as`.
³ **A LushText-owned exception, not a toolkit floor** (corrected after review, 2026-11-02). The font change is the display-wide `CssProvider` restyle LushText chose for the custom monospace font (C6 recorded it as LushText's own choice, with the queued follow-up to scope the provider to the editor and preview surfaces); Perf (`perf-fc`, the 17.1 ms dispatch) shows the frame's layout as CSS revalidation (`gtk_css_node_validate` in 10 of 21 samples) and Pango measuring the head in the new font, FreeType autofitting included, with no buffer, segment or mark code in the stack; the hidden tail is never laid out (A65). Repetitions: 20 per side over two verdicts. At load 0.6–1.0 (eight per side): `main` 11.0–15.4 ms, branch 9.4–16.6 ms; at load 3.7–4.6 (twelve per side, `verdict2/*-plain{1,2}` and `*-fc{1,2}`, a game running on the desktop): `main` median 20.7, 11.1–43.7 ms, branch 19.6, 10.1–44.2 ms. The spread is continuous rather than two clusters, it widens with host load equally on both sides, and C3 adds nothing to the dispatch, so the earlier "bimodality" (branch 16.6 against `main` 15.4) is the tail of one distribution. It stays over `B_SETTLE_US` until the provider is scoped (C6's follow-up), which owns it. **CF (2026-11-05) owns it now and splits it**: the font change loads the new rules in a dispatch that changes no widget's class (whole-tree revalidation, no re-layout; the load dispatch 1.3–2.3 ms median, at most 5.7 ms) and swaps the classes after the next paint; on the dense restyle cases the font change's worst dispatch fell by 2.4 ms in median (C6's set, 6 repetitions per side) and stayed within `B_SETTLE_US` there. The protected 1.9M row was not rerun by CF.
⁴ The 500,000,000-character fixture is one byte over `REFUSE_TO_OPEN` with its
delimiter, so it was refused on both sides and the old mid-install loop ran to
the 180 s cap; the case now uses 499,999,999 characters (the largest document
LushText opens) and stops at a `workflow-failure`. Rerun interleaved (`verdict/{branch,main}-install500`): `main` inserted the tail in one 3.7 s turn and did not reach `file-open-complete` within the 180 s cap in either repetition; the branch installed it in 7.3 s (request to complete), largest dispatch 8.8 ms.

⁵ Eight repetitions per side; the other rows four (two runs of two), the
install and AT rows two.

Every inspector open is the recorded `AdwDialog` transition exception (C2b),
unchanged on both sides. AT payloads (A78): the 100 MB install's insert events
carry at most one slice (262,144 bytes; 780 events), the head goes in as one
event, and nothing carries a whole tail.

**Total time.** To fully installed (`load_request` to `load_complete`,
median): 100 MB 2,286 → 1,490 ms and 250 MB 4,803 → 3,412 ms (phase
duration), the 32 MiB recovery 11.3 s → 0.75 s, CR-only 160 → 145 ms, U+2029
161 → 147 ms; 1.9M characters 30.1 → 33.1 ms (wrap on) and 30.6 → 35.4 ms (wrap
off), three restored 1.9M documents 71.7 → 88.3 ms. **Decision:** the 3–5 ms
(one document) and 17 ms (three) are the frames GTK draws between slices
(0.4–1 ms each) and the head's one layout running in its own dispatch rather
than inside the insert; each slice measured 1.9 ms against the 6.7 ms its
cost model allows, so a turn could take two, but the model is the
conservative one C2 and A65 measured (13–17 µs/KiB on slower runs) and
doubling the slice to 512 KiB would put a slow host's turn at the budget. The
user gets a drawn, scrollable window throughout in exchange for at most
17 ms over a 70–90 ms open; budgets are not traded for throughput.

**Storage and load.** `btrfs-stall-report` over every run window: "Storage was
NOT a bottleneck" in every window but `branch-plain1`, which shows up to three
tasks in D state on btrfs commits (IO pressure under 1 %); its figures match
`branch-plain2`'s.

**Formal verification.** Shard `core-protected-install`: K7 (clear plan), K8b
(C2's fold of one paragraph), K10 (the slice derivation never panics and is
admissible) and K11 (the rounding guard, `should_panic`), 4.54 min / 7.94 GiB
locally. The whole-plan properties (progress, exact bytes, scalar and CRLF
safety, homogeneity, per-slice bounds, `last_of_tail`, the head/tail switch,
the slice count) did not fit CBMC at any domain tried and are property-tested;
see `formal-verification.md`.

**Mid-install interactions** (tab switches to the small first tab, three
wheel steps there, back to the installing tab, until the install completes).
Dispatch: the worst lateness after the first input is 5.8 / 6.3 / 6.6 ms at
100 / 250 / 500 MB, within `B_INTERACT_US`; the phase's 8.6–8.8 ms worst is
the giant tab's editor construction before the first input.
**Input-to-frame (corrected after review, 2026-11-02).** The 270–307 ms this
record first reported, and attributed to mutter, was a pairing error in the
lane: the driver injected the first round of wheels before the window was
composed, the first two never reached the application, and the driver paired
injections with probe records by arrival order, so every later wheel was
charged with the frame of the wheel two places on. Mutter stamps each event
with its own monotonic time, so the driver now pairs an injection with the
record whose stamp falls between 1 ms before the injection and 1 ms before the
next one (at most 20 ms after the injection's D-Bus call returned; the first
wheel after a pointer move was stamped 2 ms after its call returned), counts
undelivered wheels (`scroll_delivery`) and fails the lane on any
(`undelivered_scroll_cases`), and the mid-install phase starts only once the
window's composition has run (`window_construction.stages_pending == 0`).
Re-read with this pairing, the first verdict's 100 MB mid-install wheels lost
exactly the first two of the first round and presented in 11.3 ms median.
Rerun (`verdict2/*-install`, load 3.1–4.3 with a game on the desktop): every
wheel delivered (0 of 24 and 0 of 66 lost), input-to-frame 12.2 / 16.3 / 17.3
ms (median / p95 / max) at 100 MB and 12.5 / 16.4 / 17.5 ms at 250 MB, compositor
stamp minus injection −1 to +1 ms; `main` meanwhile stalls 800 ms and 1.9 s in
those phases. The p95 is at the two-frame bound and the maximum 0.8 ms over it,
inside C6's documented double-buffered presentation floor of the headless
lane (16.7–19.3 ms), which is reported, not gated.

### Review fixes (PR #77 review, 2026-11-02)

- **Draft capture during a replacement (data safety, predates C3).** An editor whose draft was behind its buffer could be snapshotted by an autosave tick while a whole-buffer replacement (a local-history restore, a draft install, a load installation) was rewriting it: the buffer then held part of the old text and part of the new, and with a sliced protected install a tail without its head. The failing-first widget test wrote a 1.83 MB partial body over the 1.96 MB pre-restore draft. `LushtextEditorPage::draft_capture_blocked()` now makes the tab ineligible (the draft policy's `buffer_untrusted` term, in both the candidate and the captured-snapshot checks), local-history periodic capture waits on it too, and the draft stays as it was until the terminal. Not the edit lock: a save holds that over a whole buffer. Session save records only a cursor position (clamped on restore) and automation no text, so neither had the hole.
- **Ctrl+Home / Ctrl+End went to the first / last tab (pre-existing on `main`).** With #76's delivery gate the restored-session typing phase failed on both sides: AdwTabView's capture-phase Ctrl+Home, Ctrl+End and their Shift variants (first / last tab, move the tab) run whenever two tabs are open, ahead of the text view, and the switch moves focus into the other page. LushText removes those four AdwTabView shortcuts; Ctrl+Page Up / Down and Alt+digits still switch tabs.
- **Fails closed.** A sliced install whose paragraph-start mark vanished no longer appends the head after its tail (`FoldedInstallBroken`, `debug_assert!`): the load cancels as incomplete, a replacement cancels and hands its body back, the preview shows an error, a note keeps its source. A load or replacement turn whose editor is gone while a fold plan is set aborts instead of inserting hidden tails unfolded.
- **The local-history preview's clear is sliced.** One `set_text("")` emptied a snapshot of up to 50 MB in one dispatch; the preview now clears in bounded turns that follow the boundary ledger and then ordinary paragraph slices, and an install waits for it. A 6 MB protected preview cleared in 24 turns, the longest 84 µs (debug widget build; one `set_text("")` of an unfolded buffer of the same text took 2.6 ms there).
- **The stand-in terminator is out of every reader.** A widget test now autosaves a recovered undelimited folded draft through both snapshot paths and compares the bytes on disk with the original (the shared `editor_text` helper re-implements `document_end_iter`, so it could not catch a leak; the test fails with `end_iter`). The print snapshot, bookmark and restyle line counts, the memory estimate and local-history availability exclude it; GtkTextView's accessible text excludes it by construction (`gtk_text_view_accessible_text_get_contents` reads with `include_hidden = FALSE`, gtktextview.c 4.22.5).
- **Coverage wording.** The planner's whole-plan properties are tested, not proved; the property tests gained K5's visible half and K8's `head_chars + tail_chars`, and the record, the planner's rustdoc and the spec now say so (`formal-verification.md`).
- **A77's flatness check had no noise floor.** A sample run beside a release build read 5 → 24 µs for a cost that does not grow and failed; A76/A77 now count medians under 100 µs as flat (`NOISE_FLOOR_US`).

### Rerun after review (`verdict2`, 2026-11-02)

`main` cdfff201 against the merged branch (`bin/branch-review`), the branch's
driver for both (with the event-stamp pairing and the scroll-delivery gate),
one exclusive hold per run, alternating. **Not a reference-precondition
verdict:** a game ran on the desktop throughout (load 2.8–6.3, recorded per
run in `.claude/handoffs/c3/runs.log`); `btrfs-stall-report`: "Storage was NOT
a bottleneck" in every window. Every branch run passed the lane's delivery
gates (no misdirected typing, no withheld frame, no undelivered wheel); `main`'s
plain runs failed the typing gate on `session/restore-3` (Ctrl+Home, fixed here).
Worst dispatch per repetition, median / max (ms):

| Case | Phase | `main` | branch | budget |
|---|---|---|---|---|
| paragraph 1.9M wrap on | open | 12.6 / 67.8 | 9.4 / 10.1 | 16 |
| session/restore-3 | restore | 17.8 / 25.0 | 10.2 / 14.6 | 16 |
| protected 1.9M (12 reps) | open / save-as / font | 11.3 / —, 20.7 (max 43.7) | 10.8, 8.5 (max 21.6), 19.6 (max 44.2) | 16 ³ |
| recovery/protected-32mib | restore / tab switch | 3,835 / 3,793 | 11.8 / 8.8 | 16 / 8 |
| install 100 MB | mid-install, after the first input | 797–800 | 7.7–7.8 | 8 |
| install 250 MB | mid-install, after the first input | 1,929–1,966 | 5.2–7.9 | 8 |
| install 500 MB | mid-install, after the first input | > 180,000 (cap) | 11.1 / 21.6 ⁶ | 8 |
| AT on: install 100 MB / 32 MiB recovery | open / restore | 983 / 1,853 | 13.3 / 11.5 | 16 |

Mid-install input-to-frame (median / p95 / max ms, wheels lost): 100 MB
12.2 / 16.3 / 17.3, 0 of 24; 250 MB 12.5 / 16.4 / 17.5, 0 of 66; 500 MB 11.4 /
16.1 / 28.8, 0 of 186; AT on, 100 MB 12.3 / 17.4 / 22.0, 0 of 30.

⁶ At load 4.1–4.7. The 500 MB tail slices took up to 11.6 and 14.7 ms here
(3.7–4.1 ms in the load-0.6–1.0 verdict above, where the same phase's worst was
8.7–8.8 ms), and the 21.6 ms dispatch has no LushText span or frame inside it
(the main loop ran none of LushText's code: preemption by the desktop load).
The phase's raw maximum also includes the window's first realize
(`composition_realize`, 10–17 ms, C4's recorded A46 cost), which the
mid-install phase now spans because it waits for composition.

## C5 `explore-editing-long-paragraphs` (2026-10-31)

**Posture: implemented; verdict 2026-10-31 below.** Under the hybrid policy
(paragraphs up to `L_protect_edit` = 50,000 stay editable, C2; what misses
B_interact there is a recorded toolkit-floor exception), C5 lowers the per-keystroke cost of typing
in a long paragraph. Its design records the profile and every candidate
measured; in short, a keystroke in a 50,000-character prose paragraph cost:

- **one whole-paragraph Pango layout**, built inside the key press by GTK's
  commit handler (`gtk_text_view_commit_text` → `set_virtual_cursor_pos` →
  `create_display`) and reused by validation and paint: 16.5 ms wrapped,
  7.6 ms unwrapped. The floor of a stock `GtkTextView`;
- **a second, unwrapped layout of the same paragraph, 7.5 ms, from a retired
  `GtkSourceMap` kept alive** by the minimap's reflow freeze
  (`RenderHoldOverlay` held its live child strongly), in every editor with the
  minimap off, the default. A pre-existing defect (A52);
- **on GNOME's input path, about five whole-paragraph word-boundary passes**
  (2.6 ms each): GTK's Wayland text-input module asks for the surrounding text
  on every committed character, and the view's answer walks the paragraph from
  its start (A54). Measured with the lane's `wayland` variant (GTK's Wayland
  module against the headless compositor; a measurement-only variant, because
  the maintained lane sets the input method explicitly, S7B-6);
- under 1 ms of LushText's own work.

The change: the retired map is released (`RenderHoldOverlay` refers to its
child weakly; `MinimapEvidence::retired_maps_alive`), the input method gets a
bounded surrounding-text window (1,024 characters each side, within the
paragraph; the view's handler is blocked by its data), a paragraph longer than
`L_wrap_edit` = 8,192 characters is shown unwrapped by a no-wrap tag that GTK
reads from its first character (A53; hysteresis `REWRAP_MARGIN` = 1,024; the
tag covers the whole paragraph so the state survives an edit before its first
character), and the keystroke sweep is a maintained lane group with per-key
worst and summed stalls.

### Merged after C2 (2026-11-01)

C5 landed after C2 (`long-line-protected-view`), which made `protected_view`
hold above `L_protect_edit` = 50,000 (density no longer decides it). The two
compose without either standing down: the wrap gate tags every paragraph
longer than `L_wrap_edit` = 8,192, including a protected paragraph, so a
folded paragraph's visible head (at most `head_cap` = 10,000 characters, below
A28's overflow length) is shown unwrapped, like any long paragraph; its hidden
tail is never laid out (A65) whatever its wrap mode, the fold plan does not
depend on the wrap mode, and the gate's tag changes happen only inside
`insert-text`/`delete-range` emissions, never between the edit entry's
capture dispatches (A66). The keystroke group is C2's maintained group with
C5's short-paragraph position and short lengths added.

### Verdict (2026-10-31)

Reference lane, release responsiveness builds, prose keystroke fixtures (one
paragraph of the given length between 30 short lines), wrap preference on,
ten keys at each position 150 ms apart, three repetitions: the change's build
(before the whole-paragraph tag fix below)
against a baseline that behaves as `main` 10cad94a. Each run held the
exclusive measurement lock; load averages inside the lock 0.8–1.3 (one run
3.3), no other heavy process. Worst = median over keys of the worst stall in
each key's window; ranges are over start, middle and end; "short" types in a
40-character paragraph right after the long one (milliseconds):

| Length | Plain worst, before → after | Plain short | GNOME path worst, before → after | GNOME path short |
|---|---|---|---|---|
| 5,000 (wraps) | 5.0–5.3 → 5.0–5.7 | 0.7 → 0.0 | 6.3–7.0 → 5.3–5.4 | 2.5 → 1.4 |
| 8,000 | 5.7–6.8 → 1.1–4.1 | 1.1 → 1.7 | 8.2–9.1 → 0.5–4.3 | 3.8 → 1.6 |
| 10,000 | 6.9–7.3 → 0.0–3.7 | 0.5 → 2.3 | 9.2–10.4 → 1.3–4.6 | 3.8 → 1.7 |
| 12,000 | 7.6–7.9 → 1.3–5.1 | 1.3 → 2.0 | 10.3–11.1 → 1.3–4.9 | 5.1 → 1.9 |
| 20,000 | 8.6–10.6 → 2.3–7.2 | 1.9 → 2.1 | 14.6–15.4 → 2.5–6.6 | 6.0 → 1.8 |
| 50,000 | 20.7–21.2 → 8.0–11.1 | 2.4 → 1.9 | 32.4–36.7 → 7.7–10.5 | 9.5 → 2.1 |
| 100,000 | 43.7–44.6 → 16.6–19.6 | 4.5 → 2.0 | 68.6–78.0 → 15.8–18.9 | 16.7 → 2.2 |

(The verdict build's threshold was 8,000, so the 8,000 fixture crossed it at
the first key and was measured unwrapped; see the power-of-two note below.) The widget suite then found that a tag on the first
character alone forgot the hysteresis state when that character was deleted
or text was typed before it; the tag now covers the whole paragraph. A
confirmation run of the fixed build (plain, three repetitions, load 0.8) gave
the same figures: 8,000 at 1.1–4.1 ms (p90 at most 5.6, the crossing key
included), 50,000 at 8.0–10.4 ms. The p90 of the worst stall after the change stays under
B_interact up to 12,000 on both paths (at most 7.3 ms plain, 6.9 ms GNOME
path) and is 7.5–9.2 ms at 20,000.

- **B_interact (8 ms) is met** at the median for every position up to 20,000
  characters on both input paths, where `main` met it only up to about 12,000
  (plain) and 5,000 (GNOME path). On the GNOME path the change is worth 2–4×
  from 8,000 up, and typing in a short paragraph next to a long one, which
  `main` slowed with the long paragraph (9.5 ms at 50,000, because the
  surrounding text crossed into it), no longer depends on it.
- **50,000**: 7.7–8.1 ms at the middle and end, 10.5–11.1 ms at the start
  (p90 12.6–14.3): at the budget, not under it. **100,000**: 16–20 ms. Both are
  the recorded exception below, **accepted by the maintainer** ("editable up
  to ~50k", the hybrid decision of 2026-10-30). Since C2 merged (2026-11-01),
  a paragraph above `L_protect_edit` = 50,000 opens in Protected View and takes
  no keys, so the 100,000 row describes a length that is no longer editable;
  the lane's `keystroke` group now sweeps up to 49,950 (C2's edge, leaving room
  for the bursts).
- **`L_wrap_edit` = 8,192.** With bounded surrounding text a wrapped
  keystroke at 10,000 cost 6.9–7.5 ms (p90 8.5–8.8) on the GNOME path (clean
  run `wlb`): the cap breaks near 10,000, and the power-of-two rule
  (`.agents/rules/rust.md`) rounds a cap down, to 8,192. The verdict build
  used 8,000. On the rounded build, wrapped typing at 8,000 costs
  6.2–6.8 ms median on both input paths, p90 7.7–8.3 ms (three repetitions,
  load 0.5–1.7): inside the budget at the median, at its edge in the tail.
  Wrapped 5,000 costs 5.0–5.7 ms. The surrounding-text window became 1,024
  characters (a coverage window, rounded up) and its selection cap 8,192.
- **Summed stall.** Typing at the right edge of an unwrapped paragraph (middle
  and end positions) adds about 2 ms per frame for the ~150 ms after each key:
  GtkTextView scrolls horizontally with `gtk_adjustment_animate_to_value`, and
  every animation frame runs `update_im_spot_location` → caret position and
  paints the caret, two Pango caret queries that walk the line's runs up to the
  caret (about 1 ms each at the end of a 50,000-character line; profiled with
  backtraces, run `endprof`). No single dispatch is over B_interact from it,
  but the summed stall per key at the end of 50,000 / 100,000 rises from
  27 / 60 ms to 32 / 73 ms on the plain path (on the GNOME path it still falls,
  39 / 86 → 35 / 66 ms). Wrapped paragraphs do not scroll horizontally.

### The toolkit-floor exception (C5)

**Accepted maintainer exception.** The maintainer decided on 2026-10-30
(C2's hybrid, "editable up to ~50k") that keystrokes in editable paragraphs
up to `L_protect_edit` = 50,000 may miss B_interact as a recorded
toolkit-floor exception; C5's remaining 50,000 figures (7.7–11.1 ms) are
accepted under it. Above 50,000 the document is in Protected View (C2).

What remains over B_interact in an editable paragraph is GTK's:

1. **The commit-handler layout.** Every keystroke lays the edited paragraph out
   once, whole, inside the key press. Unwrapped, that crosses 8 ms at about
   50,000 characters (start position first) and reaches 16–20 ms at 100,000.
   No public API bounds it.
2. **Caret queries on one long line.** `pango_layout_get_cursor_pos` and
   `get_caret_pos` cost in proportion to the caret's offset in an unwrapped
   line, and GtkTextView asks on every frame of its horizontal scroll
   animation (IM spot location and caret paint): ~2 ms per frame at the end of
   50,000 characters, ~4 ms at 100,000.

Upstream reports to file (GTK): (1) the commit handler's synchronous
whole-paragraph layout (`set_virtual_cursor_pos` builds the line display in
the key press); (2) `gtk_text_view_value_changed` updating the IM spot
location on every frame of an animated scroll; (3) the text view's
`retrieve-surrounding` answer walking the whole paragraph's word boundaries
(LushText now bounds it itself, A54). Triaged 2026-11-04: (3) was filed as
[GNOME/gtk#8529](https://gitlab.gnome.org/GNOME/gtk/-/work_items/8529);
(1) is GtkTextView design (followed in GNOME/gtk#5764, not filed); (2) is not
triaged yet. Status of every upstream report: [`docs/upstream-reports.md`](../upstream-reports.md).

**Re-open triggers:** a GTK or Pango release that makes the commit handler's
layout incremental or bounded, or that changes A52–A54 (the axiom lane
fails); the maintainer requiring wrapped 50,000-character paragraphs within
budget (then display-only soft segmentation, design D4, is next); or editing
above `L_protect` being requested (deferred: in-buffer segmentation, a custom
virtualized widget, windowed edit-through). With C2 merged that bound is
`L_protect_edit` (50,000).

**Contaminated runs.** Until 2026-10-31 03:00 a busy-waiting helper of this
change's own scripts ran beside the measurement lock; the design's profile and
the first candidate pairs date from then and are comparative only. Every figure
in this section comes from runs after it was removed.

## SC `spawn-completion-priority` (implementation record, 2026-11-02)

OpenSpec change `openspec/changes/spawn-completion-priority/`, queued from
C2's `eviction/reselect` finding (see C2's record).

**Attribution first, and a corrected premise.** The attribution ran on `main`
cdfff201 with C6 merged: `eviction/reselect`, plain, `perf record -e cycles:u
--call-graph dwarf --user-callchains -F 999`, launched as a child under
`scripts/measure.sh`, load 0.9–2.9. Artifacts are in
`.claude/handoffs/lush-priority/perf-main-evict/`.

- **69.5 %** of the application's cycles went to
  `incremental_validate_callback`, that is Pango and HarfBuzz laying out every
  line of the loaded documents, hidden tabs included.
- Loads ran three at a time: each charges `8 × 8 MiB + 1 MiB` = 65 MiB of the
  256 MiB transient budget.
- Each next load's installation began **26 ms** after the previous permit's
  release, worker read included. So the completions are **not** what holds this
  case back since C6 merged. The permit is held through an installation that C6
  paces below the validation idle, and hidden tabs validate fully. That cause is
  **EV** (see "Sequence and gates"), next after C3.

The completion starvation itself is real (A80): a `G_PRIORITY_DEFAULT_IDLE`
source waits for the whole backlog. SC fixes that class:

- **Delivery.** `gtk-lush-tasks` delivers completions from one source at
  `completion_priority()` (`GDK_PRIORITY_REDRAW + 4`), one per dispatch.
- **LushText hand-offs.** The load and save admission drains and permit
  releases run at the same priority.
- **Lane case.** `save/during-validation` saves a scratch file while two 8 MiB
  `short-code` documents (below C6's density bound, so installed unpaced) are
  validating.

**Found while verifying (fixed in SC):**

- `composition::after_content_revealed` asked `gtk_widget_is_visible`, which is
  false before the window is shown. The opened-document focus handoff was
  therefore lost whenever the startup gate released an activation open before
  stage P, and the keyboard stayed on the status-bar toggle. The
  `window_composition` focus tests failed in about half of the runs with the
  earlier completions. They now pass 5/5.
- A restore-priority widget test relied on an idle gap.
- After merging C3, a composition stage could share its dispatch with the
  first session-restore turn: the session load now completes before stage P,
  and GLib runs every ready source of the top priority in one iteration (A81).
  The result was 16.5–26 ms in 4 of 9 `session/restore-3-small` runs. The
  composition stages now run one step above `G_PRIORITY_DEFAULT_IDLE`. The
  language catalog's one-off build, which shared stage W's dispatch on `main`
  and a turn's dispatch after that, now runs at `G_PRIORITY_DEFAULT_IDLE - 2`.
- A74's growth check failed once in `make gtk-axioms` under load, because the
  small and large windows were measured one after the other. The probe now
  alternates them over three rounds.

**Verdict (interleaved, `main` cdfff201 against the branch).** Release
measurement builds, one run per exclusive hold through `scripts/measure.sh`,
recorded in `.claude/handoffs/lush-priority/runs.log`, with the load as it
was: 0.8–2.3 in round 1 and 4–7 in round 2 (other agents' builds; the
maintainer rule is to never wait for an idle host).

Set A covered `baseline/small`, `total-size/*` (16,000, 1 MB, 5 MB), `dense/*`
(120, 500, 710), `markdown/preview`, `editorconfig/tab-width`,
`session/restore-3-small` and `recovery/small-drafts`, 3 + 2 repetitions per
side, merged side-labelled for the C6 verdict. Set B covered
`eviction/reselect`, `save/during-validation`, `session/restore-3` and
`paragraph/1900000` wrap on and off, 3 + 2 repetitions. The comparison tables
are `V-A-compare.md` and `V-B-compare.md`.

- **`save/during-validation`.** Save latency, median / max over 5 runs, went
  from **3,865 / 4,704 ms** to **4.1 / 4.5 ms**. The phase's worst dispatch is
  19.8 / 31.5 ms on `main` and 20.4 / 21.0 ms on the branch. That worst
  dispatch is the same on both sides: a 19 ms dispatch about 200 ms into the
  phase, right after the two 8 MiB documents publish. No LushText span lies
  inside it, and it changes the view's `upper` twice: GTK laying out a just
  installed document. It is EV and C6 territory, not a completion.
- **`eviction/reselect`.** It hits its 20 s cap on both sides, unchanged, as
  attributed (EV).
- **`session/restore-3`, `paragraph/1900000`, C6's set.** Every phase's median
  stall is within one reference frame of `main`.
- **The C6 verdict.** The absolute-rule items are the known C6 toolkit floors,
  on both sides: the language attach at 31–36 ms (A67), `dense/120` zoom steps
  and the `dense/710` font change (A72). The no-regression rule flagged four
  activation-to-laid-out medians in that merge. Re-measured:
  - `recovery/small-drafts` is bimodal on **both** sides, depending on whether
    the restored-draft alert's slide animation falls inside the settle window.
    The alert is revealed while the window is mapped, at about 150 ms on both
    sides. An instrumented build, 5 repetitions per side, gave `main` 45–344 ms
    (168, 162, 45, 344, 45) and the branch 44–330 ms (47, 155, 330, 150, 44).
  - Focused interleaves, 2 × 5 repetitions per side under load 4–6:
    `dense/500/wrap-off` median 81.5 → 84 ms, `dense/500/wrap-on`
    193 → 194 ms, `session/restore-3-small` 67 → 72 ms. All are within one
    reference frame.
  - `total-size/1000000/txt`: 810, 808, 800, 877 and 992 ms on `main`; 817,
    809, 823, 855 and 904 ms on the branch.
- **Input-to-frame.** No phase's median regresses by more than one reference
  frame. One exception was re-measured: `paragraph/1900000` wrap-off scroll,
  median 25.7 ms on `main` against 39.6 ms on the branch, both with a 59 ms
  maximum. Focused scroll-only interleave (`RESPONSIVENESS_PHASES=scroll`, 2 × 5 repetitions per side, load 5–9; the scroll then follows the open, so its input-to-frame includes the post-open settle): `main` median 252 ms (174–284), the branch 237 ms (182–257). There is no regression, and the full-sequence difference was noise.
- **After merging C3** (`main` ebdd6aa1, with the composition and catalog
  priorities in place, 2 × 3 repetitions per side, load 3.6–7.4):
  - `session/restore-3-small` restore median goes from 13.1 to 11.1 ms;
  - `recovery/small-drafts` restore goes from 11.2 to 10.9 ms;
  - `recovery/protected-32mib` tab-switch goes from 21.4 to 9.2 ms;
  - the `baseline/small` profiles stay within one reference frame, except
    steady-populated background (9.9 → 14.5 ms).

  The worst dispatches left on the branch are the first frame after
  composition (window measure and frame layout, 12–28 ms; 13–20 ms on
  `main`) and one restore turn of two page activations under load (24 ms;
  19 ms on `main`). Each is a single source, and neither is a completion or a
  coalesced dispatch.
- **The visual-geometry failure: resolved (see "Review fixes" below).** One
  full `make visual-geometry-smoke` under load failed
  `workspace-tree-reveal--desktop--force-light--beyond-cap`
  `revealed-row-at-rest`: the section header moved from y 150 to y −14,435
  after `workspace-path-revealed`. The independent review reproduced it and
  attributed it to a pre-existing race between the reveal's settle check and
  the slice bin's forwarding idle, not to SC.
- **Two lane-gate failures, identical on both sides, not caused by SC.**
  - The `session/restore-3` type-start misdirected-typing gate. It is
    AdwTabView's capture-phase Ctrl+Home/End, fixed in C3's PR #77.
  - Keys refused during an install under load (`total-size/*` type-during-settle,
    round 2 only). This predates C6 and the coordinator queued it as its own
    change after C3.

### Review fixes (2026-11-03)

An independent review of PR #79 (evidence in
`.claude/handoffs/lush-priority/review-vg/`) found two defects. Both are fixed
in this change, failing first.

- **The completion source had no bound.** The first version claimed that the
  worker cap kept the source to "at most eight dispatches in a row". It does
  not. A FIFO deeper than the cap, or a completion that spawns its successor,
  refills the queue as fast as it drains. The reviewer's 64-task chain
  (`starve-experiment.rs`) starved every priority from 125 to 200 for the
  whole chain: validation, C6's paced slices, composition stages, restore
  turns, the `ViewportSliceBin` idles. The source now **ages** (design D6):
  - After eight consecutive completions, or 16 ms of continuous occupancy, it
    detaches and comes back after a `G_PRIORITY_DEFAULT_IDLE` hop.
  - When a source in between keeps the loop from reaching 200, it comes back
    after a 16 ms deadline at 124 instead. A validation backlog does that
    (A80), and a hop at 200 alone would wait for the whole backlog.
  - Two unit tests built from the experiment failed on the unaged source, with
    0 validation batches and 0 default-idle dispatches during the chain. They
    pass now, with at most eight completions between two lower-priority
    dispatches.
  - The reviewer's combined form (an always-ready 125 **and** a 200 idle during
    one chain) cannot show progress at 200 under any scheduler, because GLib
    dispatches only the top ready priority. On `main` the chain itself would
    never finish. The design records this.
- **The reveal could settle one layout early (pre-existing).**
  `ViewportSliceBin::outer_request_pending()` turned false when the forwarding
  idle moved the outer scroller, before the bin re-sliced from that value. The
  bin now keeps the request pending until its next `size_allocate` consumes
  the move.
  - Proof: an adoption-lab test that polls right after the idle failed first,
    2 of 2. The Kani slice loop has two new harnesses in
    `widgets-slice-loop-requests`: settle evidence is current whenever nothing
    is pending, plus a `should_panic` pin of the pre-fix accessor. Every
    `gtk-lush-widgets` shard was rerun locally, at 5.2–11.1 min and at most
    1.92 GiB each.
  - A "confirm in a later frame" guard in the reveal was tried and dropped. For
    an already-visible row nothing changes, so the clock produces no frame. The
    guard ran out the wall budget (`unsettled`) in 3 `workspace-tree-reveal-at-rest`
    cases in each of the first two loop runs, and both runs were discarded.
  - **Visual-geometry loop: 10 of 10 full lanes pass on the branch**, 0
    failures. Five ran unloaded (load 1.9–5.6) and five beside 10 CPU spinners
    on the lane's 16 CPUs (load 12.5–16.6). The table is
    `.claude/handoffs/lush-priority/vg-review/table.tsv`.
- **Hand-off dispatches are now measured, not assumed.** The measurement build
  records every completion as a `task_completion` span (the
  `completion-observer` feature) and every admission drain and permit release
  as a `task_handoff` span. Drains and releases at 124 co-dispatch with the
  completion source (A81), so the report also takes the watchdog's worst
  lateness over any iteration overlapping either: the combined dispatch.

**Re-verdict (interleaved, `main` ebdd6aa1 against the reviewed branch,
2026-11-03).** Release measurement builds, one run per exclusive hold through
`scripts/measure.sh`, 3 + 2 repetitions per side, with the load as it was:
8.9→3.6, 3.6→1.6, 1.6→0.9 and 0.9→1.4 over the four runs. Cases:
`baseline/small` (all five profiles), `session/restore-3`,
`session/restore-3-small`, `recovery/small-drafts`, `save/during-validation`
and `total-size/1000000` (css and txt). Artifacts: `RV-*`; the comparison
table is `RV-compare.md`.

- **Every hand-off dispatch is within `B_SETTLE_US`.** Over 57 runs: 2,897
  completions with a worst of **4.8 ms**, 924 hand-offs with a worst of
  0.1 ms, and a worst combined dispatch of **4.2 ms**.
- **`save/during-validation`.** Save latency is 2.4–4.7 ms on the branch and
  3.82–3.87 s on `main`, as before the aging.
- **No regression beyond one reference frame.** Every phase's median stall and
  input-to-frame maximum is within 8.3 ms of `main`. The C6 no-regression rule
  reports none. Its absolute-rule failures are the language-attach floor
  (A67, 31 ms on `total-size/1000000`), on both sides.
- The largest median shift is `baseline/small/steady-populated`
  type-during-settle, at 4.9 ms on `main` and 12.2 ms on the branch (maxima
  16.6 and 20.8 ms). Its input-to-frame maximum improves, from 19.4 to
  18.1 ms. No completion or combined dispatch in it exceeds 4.8 ms, so the
  stall is not a hand-off.

## EV `defer-background-tab-layout` (implementation record, 2026-11-03)

OpenSpec change `openspec/changes/defer-background-tab-layout/`. Artifacts in
`.claude/handoffs/ev/`.

**Probes first.** A82: a `GtkSourceView` on an `AdwTabView` page that was never
selected is neither mapped nor realized, yet validates everything loaded into
its buffer while hidden (149 iterations for 16,384 lines), because its scrolled
window creates its layout; so selecting fewer tabs cannot help. A83:
`gtk_text_view_set_buffer` to an empty buffer stops that at once and leaves the
document buffer untouched; a reattach re-lays out from nothing and comes back
at the buffer start, and a scroll queued right after it lands at the wrong line
(100 instead of 1,024) unless re-asserted on each `upper` change. A81 (new,
general): GLib runs every ready source of the top priority in one iteration,
so N validating views make an iteration N batches long. The A69 citations for
co-dispatch (SC, held typing, this record, four code comments) now cite A81.

**What was built.** Each editor view holds a placeholder from construction
until its tab is first shown (or selected in a window still composing), and
again once seen laying out while hidden (`upper` changes while unmapped);
`buffer()` is always the document buffer. A detached view keeps its top line
and is held there after a reattach until laid out or the user gives input.
Paced installs and replacements into background tabs wait at
`G_PRIORITY_DEFAULT_IDLE + 1`, below the visible tab's; after its worker read a
load's permit keeps only its decoded body and frees its reading slot.

**Found while verifying (fixed, failing-first):** a widget test edited through
the view's buffer; the "focus handoff before the alert announcement" order was
incidental (a protected document's alert could be announced first; the
selected editor's announcement now waits while the opened document's focus
handoff is pending: 2 of 5 failures under ten spinners without, 0 of 5 with);
Protected View's primary-selection bookkeeping across `set_buffer` (a
`Gtk-CRITICAL`); and a first verdict's design (background slices at the
completion priority) starved the visible view's layout
(`recovery/protected-32mib` scroll input-to-frame 109–145 ms in 5 of 5 against
`main`'s 18–40 ms) and was replaced.

**First verdict (V2, 2026-11-03), and what it got wrong.** V2 measured
`bin/branch-8`, a build from before the last changes of the first round, not
the committed code (31aeb90a). Its `install/100000000` `mid-install` row
(51–66 ms in 5 of 5 branch repetitions, a 35–52 ms stretch with no LushText
span right after the activation's second open) was recorded as "not
reproduced in 9 further interleaved branch runs"; that was false: `I-branch8`
rep0 (54.1 ms) and `I-branchfinal` rep0 (32.3 ms) show the same stretch, so it
appeared in 6 of 12 repetitions. V2's table is kept (`V2-compare.md`) as a
measurement of `branch-8` only.

**The 35 ms stretch (attributed, fixed).** In `branch-8` the activation turn
ended, after its second `open_document`, with `sync_tab_bar_visibility`, the
focus handoff and `gtk_window_present`, none of them spanned. Spans added
since show the first two under 0.1 ms; on Wayland `gtk_window_present` of a
mapped window blocks the GTK thread in a dispatch loop until the compositor
answers an xdg-activation token request (`gdk_wayland_toplevel_focus`,
`gdktoplevel-wayland.c` 2385–2393), so whatever delays Mutter lands in our
dispatch; `main` ran the same call but its dispatch ended 0.3–0.5 ms after the
second open in every V2 repetition. perf could not catch it: 16 repetitions
under `perf record -e cycles:u --call-graph dwarf` (the final candidate idle,
`branch-8` idle and beside eight CPU spinners; `perf-gap*`, `R-gap1`) never
reproduced it, so the attribution rests on the span bracketing and the GTK
source, not on samples. The turn now presents only a window that is not
active (a window already raised needs no raise). V4: `mid-install` 9.5/9.9 ms
(median/max) against `main`'s 9.1/14.0, no span-less stretch in any of 5
repetitions.

**Review fixes (review of 31aeb90a).** No view is detached while an
assistive technology listens, or past 2^18 lines; the primary selection
survives a detach and Protected View's choice made while detached is applied
by the attach; bookmarks, print, held typing and the search bar's scroll use
the document buffer; the kept position inside a wrapped paragraph is the
display line at the top; the retained load charge covers the residency not
yet installed; background slices age into the paced priority after 256 ms; a
dense document's reattach is revealed outward in `V_SLICE` ranges (at most
2^14 lines, see below). Design D7 and tasks 6.x of the change. `set_buffer`
costs (PyGObject probe, `scratchpad m2`, the lane's AT setup): with a
listener about 2 ms per MiB for either direction (8 MiB 9–19 ms, 32 MiB
69–82 ms, 100 MiB 180–220 ms); without one the detach walks the lines (2.6–4.2
ms for 204,601 lines, 25–28 ms for 2,557,503, 0.5–1.1 ms for 100 MiB of
4,000-character lines) and the attach is under 0.2 ms at any size.

**Verdict (V4, interleaved, 2026-11-03 19:34–20:14).** `main` f79e7a5d with
the probe's `adjustment_value` mark backported (`main-mark-backport.diff`, so
the wheel rule applies to both sides) against `bin/branch-134d1f08` built from
134d1f08, 3 + 2 repetitions per side, run without waiting for an idle host:
1-minute load 0.8–4.1 over the runs (`runs.log`). Table `V4-compare.md`, AT
variant `AT4-compare.md`. The final commit changes only which dense documents
take the paced reattach (2^14 lines); its two affected cases were rerun on a
build of it (`R5-evict-branch`, 3 repetitions, load 2.5–6.1) against V4's
`main` rows.

- **`eviction/reselect`**: `main` hits the 20 s cap in every repetition; the
  branch completes every one. Open 14.9–16.2 ms worst dispatch (the
  activation turn of four opens), `reselect` 4.8–6.8 ms, `switch-hidden`
  8.2–8.3 ms (R5). V4's 78–80 ms there came from the paced reattach of a
  204,601-line document: hidden lines add no height, so the first 2000 px
  batch validated every hidden line (about 0.4 µs each); hence the 2^14-line
  limit.
- **`eviction/switch-hidden-dense`** (new; an 11 MiB document of
  2,000-character lines never shown): `switch-hidden` 10.1–10.2 ms in V4 and
  11.2–16.9 ms in R5, against `main`'s 17.0–19.3; the worst dispatch is the
  newly shown tab's first frame. Before the paced reattach (V3, c6ae9355) it
  was 27–29 ms in 3 of 5.
- **`eviction/switch-hidden-giant`** (new; a 1.9M-character paragraph never
  shown): `switch-hidden` 4.0/9.3 against `main`'s 9.5/12.3.
- **The first switch to the never-shown 32 MiB protected tab**
  (`recovery/protected-32mib` `tab-switch`), 21.9/23.7 ms in 2 of 5 V2
  repetitions (1 of 5 on `main`): the worst dispatch was that tab's first frame
  (`post_open_first_editor_frame` 21–25 ms, layout of its visible folded
  part). In V3 and V4 the branch's medians are 5.5–5.9 ms (max 7.4) and
  `main`'s 5.1–5.3 (max 8.6), not over budget in any repetition; recorded, not
  reproduced.
- **`recovery/protected-32mib` scroll** input-to-frame max 25.9 ms against
  `main`'s 54.0 (V4); the wheel rule now reports a wheel that moved no view as
  `no-visible-change` instead of charging it a later frame, on both sides.
- **No regression beyond one reference frame** in any other phase's median
  stall or input-to-frame maximum (V4), with two rows read: the
  `switch-hidden-dense` `background` median 1.0 → 10.1 ms is the dense
  document's own paced install running there behind the small selected tab
  (on `main` it ran inside `open`, 28.5/30.0 ms against the branch's 9.5/10.0),
  and `session/restore-3` `inspect` input-to-frame max 26.5 → 39.8 ms is one
  repetition of a C6 floor phase (V3: `main` 38.0, the branch 35.6). The C6 floors
  (`protected/*` `inspect`, `font-change`, `zoom-step`; `session/restore-3`
  `inspect`) are over budget on both sides alike.
- **`at` variant**: with a listener no view is detached, so hidden tabs lay
  out as on `main`: `eviction/reselect` hits the 20 s cap on both sides (the
  CPU M2 accepts); `switch-hidden-dense` 17.6/19.6 against 18.0/18.3 and
  `switch-hidden-giant` 9.2/10.0 against 8.8/9.0. No swap dispatch is sent to
  the listener.

**Review round 2 (review of 8ac53b8e, 2026-11-04).** The second review
found the AT mirror reading `EventListenerDeregistered` as `(ssas)`; it is
`(ss)`, so a listener never left and, after any assistive technology had run
once, no view was detached for the rest of the session (fails safe, but
silently switches the change off). Fixed with a pure, unit-tested parser
(failing-first), and the mirror now reads as listening until the registry
answers and for good on failure, rebuilding its (sender, event) set from the
answer. Detached editors attach one per dispatch when a listener appears. A
paced reattach hid paragraphs from the in-tab search: Replace All right after
switching to a dense tab replaced 17 of 400 matches (failing-first); the
search context now reveals first and no paced reveal starts while one is
attached. That test found a pre-existing crash: sourceview5 0.11's
`replace_all` binding asserts a zero count carries an error, so Replace All
that matched nothing panicked; the call is made directly.

**Recorded exception (2026-11-04): a large document detached before an
assistive technology started.** It sends its whole text once, when its tab is
shown, instead of in 256 KiB install slices as on `main`. Measured with the
PyGObject probe under the lane's AT setup and a listener recording payload
sizes: 32 MiB, 71–75 ms in one dispatch, one `TextChanged` of 33,554,400
characters; 100 MiB, 182–186 ms, one of 104,857,582; 160 MiB, nothing sent
(over D-Bus's 128 MiB message limit; the a11y bus itself allows 1 GB). Reason:
re-installing the document in slices through the buffer-replacement workflow
would rewrite the buffer (undo, modified state, marks, caret) to change only
what the AT is told. Because an unanswered mirror reads as listening, only an
assistive technology started mid-session reaches it.

**The detach with a minimap (N5).** New lane cases `eviction/detach` and
`eviction/detach-minimap` (one 8 MiB document of 204,601 short lines shown,
then hidden and re-laid out by a zoom step): the `view_detach` span is
5.3–5.9 ms without and 8.6–12.7 ms with the minimap bound, which walks the
lines again (load 2.9–9). The 2^18 cap is kept: in the same phase `main`, which
never detaches, re-lays out the hidden view and its map in 175–189 ms, against
the branch's 11.5–14.1 ms. A 2^17 cap for minimap views (c4aba7a4) was
removed.

**Verdict V5 (2026-11-04 02:15–02:39, the affected cases).** Interleaved, 3 + 2
repetitions per side, `main` f79e7a5d with the probe mark backported against
`bin/branch-c4aba7a4`, without waiting for an idle host (load 1.8–4.8; a Java
process and a search indexer were running), plus the `at` variant 2 + 2.
`V5-compare.md`, `AT5-compare.md`. The final commit only removes the 2^17
minimap cap, which never took effect in these runs (the minimap case detached
in every repetition), so V5 measures its behavior.

- `eviction/reselect`: `main` hits the 20 s cap; the branch completes,
  `reselect` 5.7/6.5 ms (median/max), `switch-hidden` 6.2/6.8.
- `switch-hidden-dense` 10.2/15.0 against `main`'s 19.1/22.2;
  `switch-hidden-giant` 7.5/9.0 against 9.7/11.6.
- `detach` phase: 84.7/87.0 against 82.2/90.4 without the minimap (the zoom
  step's own re-layout of the shown tab, on both sides); 11.5/14.1 against
  175.5/176.8 with it.
- `at` variant: no view is detached (no `view_detach` span of a laid-out
  view); `eviction/reselect` hits the cap on both sides; the other cases match
  `main` within a reference frame.

**Review round 3 (review of 6929f019, 2026-11-04).** The print request now
records the buffer of the compositor it builds in the evidence its runner sees
(the M4 test fails first against a wrong compositor buffer: `Some(0)` against
`Some(23)`). Pacing is refused only while the search query is non-empty, and
the query becoming non-empty reveals (an empty query finds nothing; widget
test, failing first against the previous rule). 2^18 with a minimap is a
recorded power-of-two exception beside the constant, now measured at exactly
2^18 lines (PyGObject probe, load 1.2–1.4): 10.4–10.6 ms with the minimap,
6.0–6.2 ms without. A test's simulated registry is sticky; a closed
accessibility-bus connection returns the mirror to unknown; `GTK_A11Y=accesskit`
reads as AT-SPI.

**The `steady-populated` shift SC left (attributed).** The worst dispatch at
the start of `type-during-settle` is the populated window's first editor frame
on both sides, 11.4–12.8 ms unloaded, within `B_SETTLE_US`; the loaded
outliers (17.7, 20.9 ms; V4 max 17.1 on `main`, 11.0 on the branch) are over
budget on both sides. SC delivers the load's completion earlier, so readiness,
where the phase starts, now comes before that frame (4 of 5 repetitions,
against 1 of 5 on `main`). perf on `main` (`perf-populated/`, the 16.8 ms
frame): `gtk_widget_query_size_for_orientation` 22 % (label measures through
Pango, FreeType autofit glyph loading 20 %), CSS node validation 21 %, cairo
fills 12 % (the headless cairo renderer): the first frame of a window showing
300 sidebar rows and the properties panel. No new or longer dispatch.

## C6b `chunked-highlight-attach` (implementation record, 2026-11-04)

**Status:** implemented on branch `chunked-highlight-attach` from `main`
`0592acf1` (EV merged and archived); merged as PR #81 (`main` `afdcc6a8`) and
archived 2026-11-04. C6c (`bound-context-flip-keystrokes`) is next. After the
re-review the mechanism ships **dormant**: no language has a chunked cap
(see "Re-review round" below). Design and decisions:
`openspec/changes/archive/2026-11-04-chunked-highlight-attach/design.md`.

**What was built.** A highlighted document above its language's
attach-after-install threshold (C6's measured gate, now
`SyntaxLanguage::attach_after_install_threshold`) installs with its language
attached: the language comes off before any old text is cleared, the first
paragraph-aligned chunk goes in, the deferred attach stage attaches the
language to that one chunk, and each later chunk of at most
`HIGHLIGHT_CHUNK_BYTES` (8 KiB) goes in only once the context engine has
reported the buffer analysed to its end (A84), or after one
`G_PRIORITY_LOW + 1` wait behind its worker. Load, reload, eviction reload,
draft recovery, local-history restore and undo, and save-formatting
mirror-back share it (`LushtextEditorPage::decide_highlight_chunk`; pure half
`model/highlight_chunking.rs`, Kani-proved in `core-post-open-settle`). A
sliced clear-only replacement (memory eviction) and a cancelled chunked
install remove the language before clearing. A background tab's chunked
install does not wait for the tab to be shown.

**Decisions (coordinator, 2026-11-04; the maintainer may override):**

- **D1, gate at `S_SYNTAX` (1 MiB), not at the open limit.** The chunked
  install is bounded per dispatch at any size but proceeds at the engine's
  analysis speed (A84, `--chunked-install`, 1 MiB, load 4.0–4.8): markdown
  0.61–0.65 s, rust 0.70 s, c and xml 0.82–0.84 s, python3 1.13–1.16 s
  (typescript 2.3 s, js 1.7 s, css and sh 1.3 s before D10 removed them).
  The document is incomplete meanwhile, so `S_SYNTAX` bounds the wait.
- **D2, the first chunk goes in before the attach**, so the first frame shows
  text as on `main` (the plan attached to the empty buffer; A84 pins an
  attach to one chunk the same way).
- **D10, five languages, not ten** (first revision; **superseded by the review round below**: two languages, 512 KiB; **then by the re-review**: none). The first verdict round measured keys of
  31–39 ms for js, typescript, css, sh and yaml at 1 MiB on the lane's new
  blank-line-free fixtures (perf: `idle_worker` and `first_update_callback`,
  `perf-sh-typing`); reproduced with the A84 sample's `--typing-limit` on the
  lane's own fixtures; a blank line every four lines removes it (sh 1 MiB:
  1.2–4.8 ms against 40–55 ms per key). Blank-line-free typing limits: markdown,
  rust, c, python3 and xml pass at 1 MiB (worst key 2.2, 5.9, 4.6, 3.2,
  7.3 ms); css and sh fail at 8 KiB, yaml at 16 KiB, js and typescript at
  4 KiB (ledger A84). The chunked set is the five that pass.

**Finding for the maintainer (queued as C6c, not changed here).** C6's own gates for js
(18 KiB), typescript (10.5 KiB), css (37 KiB), sh (39 KiB) and yaml (144 KiB)
are above those blank-line-free typing limits: on `main` a keystroke in such a
document can cost 9–41 ms. Enforcing `B_INTERACT_US` would remove highlighting
from almost every JavaScript and TypeScript file, so it is recorded for a
decision. **Queued as C6c** (coordinator, 2026-11-04): gate on the longest
blank-line-free run, with a carried bound in the edit-time watch, plus a
runtime downgrade when an edit needs a low-priority worker dispatch.

### Verdict (interleaved, 2026-11-04)

`main` `0592acf1` against the branch (`branch-wip2`), release
`responsiveness-probes` builds, plain variant, 3 repetitions per side
(main 2 + 1, branch 2 + 1, interleaved main, branch, main, branch; an
earlier branch round with the ten languages was discarded after D10), each
run through `scripts/measure.sh`; 1-minute load 3.0–5.3 before and after each
run (`.claude/handoffs/c6b/runs.log`, `measure.log`). The A67 language parse
is the C6 toolkit-floor exception and is reported separately ("attach");
"worst" excludes it.

| Case | Worst dispatch, median / max (main → branch, us) | Attach parse max (main / branch, us) | Slowest key max (main / branch, us) | Activation to laid out, median (main / branch, ms) |
|---|---|---|---|---|
| highlight/md/1048576 | 11016 / 13341 → 9099 / 12662 | 37599 / 33522 | 1520 / 2494 | 778 / 992 |
| highlight/rs/1048576 | 7639 / 8396 → 8170 / 8323 | 2368 / 2211 | 1851 / 2525 | 699 / 1121 |
| highlight/c/1048576 | 7970 / 8673 → 8695 / 11220 | 2839 / 4234 | 1322 / 1730 | 701 / 1171 |
| highlight/py/1048576 | 7936 / 10596 → 8680 / 11932 | 6308 / 5946 | 1839 / 4042 | 619 / 1293 |
| highlight/xml/1048576 | 9094 / 10485 → 8495 / 8879 | 2256 / 3131 | 1637 / 3184 | 712 / 1781 |
| highlight/{html,json,js,ts,css,sh,yaml} (gated both sides) | 7341–10340 / 8817–11155 → 7270–9566 / 7964–12087 | unchanged | 1556–5223 (branch), 1934–3526 (main) | within ±30 ms |
| total-size/{16000,1000000,5000000} and /txt | 6882–10336 → 7354–11430; max 13920 → 13834 | unchanged | ≤ 2930 both | within ±100 ms |

On the branch the five chunked languages are highlighted at 1 MiB (on `main`
they are not), with every post-open dispatch within `B_SETTLE_US` (worst
12.7 ms) and every key within `B_INTERACT_US` (worst 4.0 ms). Activation to
laid out is 0.2–1.1 s later: the engine's analysis, which `main` does not do;
reported, not gated (design D9), and bounded by D1.

The construction profiles (`baseline/small`, five profiles, not chunked:
plain text) are in the rerun below; the first set was taken while the host
slowed down part-way (branch round 2: the window template, which C6b does not
touch, took 10.2–12.3 ms against 8.7–9.3 ms on `main`).

#### Construction profiles, rerun (interleaved, 2026-11-04)

`baseline/small`, five profiles, main, branch, main, branch, 2 repetitions
each (4 per side), load 2.2–4.1 (`B-*` runs):

| Profile | First painted frame, median (main / branch, ms) | Content fill-in, median / max (main / branch, ms) | Largest composition dispatch, max (main / branch, us) | Worst post-open dispatch, max (main / branch, us) |
|---|---|---|---|---|
| fresh keyfile | 34.4 / 39.8 | 18.4 / 19.3 — 20.7 / 31.3 | 10550 / 11507 | 9718 / 15802 |
| fresh dconf | 37.6 / 35.5 | 20.3 / 21.9 — 19.7 / 20.0 | 11494 / 9647 | 11019 / 9647 |
| steady dconf | 37.1 / 35.7 | 20.1 / 24.3 — 19.3 / 20.4 | 11251 / 11540 | 11251 / 11540 |
| steady keyfile | 35.8 / 35.7 | 19.1 / 21.9 — 18.7 / 21.9 | 10830 / 10896 | 10830 / 10896 |
| steady populated | 36.3 / 36.2 | 24.7 / 26.3 — 24.9 / 26.3 | 14657 / 11629 | 14657 / 13119 |

No difference beyond the run-to-run spread (these documents are plain text and
take no new path); every dispatch within `B_SETTLE_US`, every fill-in within
the accepted 40 ms. **Met.** No new exception to `B_INTERACT_US` or
`B_SETTLE_US` is recorded; the A67 parse remains C6's toolkit-floor
exception.

### Independent review round (2026-11-04): caps measured, fixes, rerun

The independent review of PR #81 asked to fix first. Its findings and what
was done:

- **M1, flipping keys.** The typing limits used only keys that never change
  the syntax context of the rest of the document. A84's new `--flip-limit`
  types one at the document's start after the chunked install, then its
  closer (rust `"`, python3 `"""`, markdown a code fence, c `/*`, xml
  `<!--`), every dispatch within `B_SETTLE_US`, on both fixture families,
  64 KiB to 1 MiB, through `scripts/measure.sh` (load 3.5–4.4): markdown and c
  pass at 1 MiB (worst 2.9 and 3.3 ms, echo ≤ 6.1 ms); rust passes at 128 KiB
  (fails at 256 KiB: 16.6–23.3 ms); python3 fails at 64 KiB (18.5–18.7 ms);
  xml at 256 KiB (37.2–38.4 ms, blank-line-free).
- **S6, install time.** Target ~0.5 s: markdown 0.61–0.65 s per MiB and c
  0.82 s per MiB give 512 KiB each.
- **Caps** (`document_scale::chunked_threshold`, the smaller power of two):
  **markdown 512 KiB, c 512 KiB**; rust (128 KiB), xml (128 KiB) and python3
  (none) do not clear their C6 gates and keep them. The first revision's
  "five languages up to 1 MiB" is withdrawn.
- **S1/S2:** an identity change during a chunked install keeps the stage
  armed the install's way (derived from ownership; cleared when the stage
  runs or is withdrawn); `test_a_rename_during_a_background_chunked_install_*`
  fails without it. **S3:** a replacement takes the language off at its first
  clear turn. **S6:** an install's attach no longer waits for input-quiet
  (typed keys are held until the install's terminal; at 512 KiB about
  0.3–0.4 s); this landed after the rerun's binary and changes no measured
  phase (the lane types only after `file-open-complete`). **S7**, N1–N4.
- **S4:** the D9 exemption is encoded in `c6_verdict` (`exempt_regressions`
  for `attrs.chunked` cases); the first revision's branch report was **"not
  met"** (`baseline/small/steady-populated@plain#1 background:
  post_open_first_editor_frame 18021 us > B_SETTLE_US`), attributed to a host
  slowdown during that run (the window template, which C6b does not touch,
  took 10.2–12.3 ms against 8.7–9.3 ms on `main`) and met on the `B-*` rerun.
- **S5:** evidence counters `chunks_on_coverage`/`chunks_after_wait`;
  failing-first by mutating the mechanisms (coverage predicate forced to
  `false` and to `true`, no language removal at install begin and at abort;
  all four caught), plus tests for the stall fallback, eviction reload and
  save-formatting mirror-back. **Kani liveness:** the install finishes within
  a step bound even when coverage never arrives or the attach leaves no
  language (0.44 s).
- **Incident:** the reviewer's `a84 --help` opened the sample's window on the
  live desktop. Every sample now prints its usage and exits on `--help`/`-h`
  and refuses unknown arguments before GTK starts (`make check-gtk-axioms`
  checks it; build.md).

**Rerun** (interleaved, `main` 0592acf1 against `branch-r1`, 3 + 3 plain and
1 + 1 `at`, load 0.9–3.2, `R-*` and `RAT-*` runs; highlight group at 512 KiB):

| Case | Worst dispatch, median / max (main → branch, us) | Slowest key max (main / branch, us) | Activation to laid out, median (main / branch, ms) |
|---|---|---|---|
| highlight/md/524288 (chunked) | 7185 / 8229 → 13552 / 14360 | 2053 / 1630 | 363 / 402 |
| highlight/c/524288 (chunked) | 7622 / 8198 → 8224 / 8546 | 2286 / 4362 | 514 / 693 |
| highlight, the ten gated languages | ≤ 15406 → ≤ 14367 | ≤ 3184 / ≤ 3999 | ±1 |
| `at` variant, the highlight group | ≤ 15125 → ≤ 13744 | ≤ 2588 / ≤ 3266 | md 354 / 411, c 509 / 695 |
| baseline/small, five profiles | ≤ 11690 → ≤ 12754 | ≤ 1708 / ≤ 1591 | ±3 |

First painted frame 34.6–36.7 ms (main) against 33.9–36.6 ms (branch); fill-in
at most 23.3 against 27.8 ms (populated). The combined side-labelled report is
**"not met"** on two lines, both attributed: `highlight/md/524288` gains a
12.6–14.4 ms "unattributed" dispatch ~14 ms after the settings flush at
~500 ms — GIO's keyfile-backend monitor dispatch after a settings write, C6's
recorded floor, which `main` has too (12.0 ms at 512 ms in the same case) but
outside its settle window because it settles ~40 ms earlier; and
`total-size/5000000/txt` laid out 13 ms later than `main` (2,143 against
2,130 ms; plain text, a path C6b does not change: run spread). Every dispatch
is within `B_SETTLE_US` and every key within `B_INTERACT_US`; no new exception.

### Re-review round (2026-11-04): worst-case caps, the input-quiet carve-out

The re-review of PR #81 asked to fix first:

- **F1, worst-case flipping keys.** The flip measurement above ran on
  fixtures that close every construct soon after it opens (c `/* … */` on
  every line, markdown fenced blocks), so an opener typed at the start
  resynchronises at the next closer. The generator's new `--flavor no-closers`
  (c `//` comments only; markdown indented code and no fence; self-tested to
  hold no `*/`, fence or `-->`) is the worst case. `--flip-limit` now probes
  the start and the middle, reports flipping keys (opener and closer, within
  `B_SETTLE_US`) and the ordinary keys typed after each (within
  `B_INTERACT_US`) apart, and probes markdown's `<!--`. Through
  `scripts/measure.sh` on a quiet host (load 3.3–4.4; the run waited for
  another user's jobs to finish), two repetitions:

  | Language, size | Flip worst (us) | Ordinary worst (us) | Verdict |
  |---|---|---|---|
  | c 64 KiB | 11678 | 3017 | pass |
  | c 84 KiB (C6 gate) | 11507 | 3005 | pass |
  | c 128 / 256 / 512 KiB | 20152 / 33827 / 33805 (closer, start) | 3655 / 7907 / 10162 | fail |
  | markdown 128 KiB | 9244 (`<!--`) | 6253 | pass |
  | markdown 256 KiB | 11743 | 11858 (inside `<!--`, start) | fail |
  | markdown 320 KiB (C6 gate) | 16145 | 11503 | fail |
  | markdown 512 KiB | 35139 (`<!--`); fence 7861 | 30954 | fail |

  C's largest passing power of two is 64 KiB and markdown's 128 KiB, each
  below its C6 gate (84 and 320 KiB). **No shipped language has a chunked
  cap** (`document_scale::chunked_threshold` returns `None` for every
  language); every language keeps its C6 gate, and the README's figures are
  `main`'s again. The mechanism stays dormant for C6c, whose runtime
  downgrade would bound the flips that cap every language here. Every build
  runs the shipped table; the `const` scale functions gained `_with`
  variants taking Markdown's cap, the editor passes the run-time
  `markdown_chunked_cap()` (always `None` unless a widget test holds
  `override_markdown_chunked_cap_for_test`, `test-utils`), and the Kani
  harnesses (`core-post-open-settle`, `core-document-scale`) choose `None` or
  `MODEL_MARKDOWN_CAP` (512 KiB) symbolically, proving both.
  `test_a_shipped_markdown_document_above_its_gate_is_reduced_and_not_chunked`
  pins the shipped 320–512 KiB behaviour (fails with the cap forced on). The
  lane's highlight group is
  now all gated. **Pre-existing, added to C6c:** markdown's own C6 gate
  fails the ordinary-key criterion inside an open HTML comment
  (11.3–11.5 ms at 320 KiB on `main`'s gate).
- **F2, input-quiet.** Only the editor the keys go to (its view mapped and
  holding the keyboard focus of the active window, `has_focus`, not merely
  its window's focus child, `is_focus`: the input clock is process-wide)
  skips input-quiet, because its typed keys are held until the
  install's terminal; every other chunked install (a background tab, an
  unfocused or hidden editor) waits for input-quiet or the deadline.
  `test_a_background_chunked_install_attaches_only_after_input_quiet` (keys
  on the window every 4 ms from the open until 600 ms after the install's
  request: no language meanwhile) fails with the skip widened to every
  install; `test_the_focused_editors_chunked_install_attaches_while_keys_arrive`
  (no deadline run) fails with the skip removed, and
  `test_a_focus_child_in_an_inactive_window_waits_for_input_quiet` (the
  focus child of window A while keys arrive in active window B) fails with
  `is_focus`. A presented test window is never active in the headless
  harness: the tests activate it with a real press through the compositor's
  RemoteDesktop interface (`common::activate_with_real_press`, as the A60
  and A64 controls do). The lane does not measure
  the foreground case: it types only after `file-open-complete`, after any
  install's terminal, and with no shipped language chunked a lane case could
  not reach it; the two tests pin both halves of the carve-out.
- **S-a.** `c6_verdict` compares a stage only over dispatches starting by
  the baseline's median settle plus one frame (self-tested `late` and
  `early`), so the keyfile monitor dispatch `main` also has, outside its
  earlier settle, no longer counts. The 5 MB plain-text case was rerun
  interleaved twice: `Q-*` (main, branch, main, branch, with the highlight
  group, load 2.3–5.8) and `T-*` (ABBA, `total-size` only, load 1.2–3.6),
  8 runs per side. Activation to laid out, medians: main 2,162 ms, branch
  2,220 ms; install time (first to last slice) 2,135 against 2,181 ms
  (+2.1%; Mann-Whitney U 47.5 of 64, p ≈ 0.07). **Unresolved.** What the
  data shows: the slice count, each slice's duration (1.42–1.51 ms on both
  sides) and the presentation interval (8.33 ms) are the same; the install
  takes more frames (median 260 against 254) because fewer frames fit two
  slices (13 against 22–25 gaps under 7 ms in the quietest pair); `T-main2`
  was slower than every `T-branch` run. No branch code runs in a plain-text
  install except a few constant-time `Option` checks per slice, so no cause in the change
  was found. The highlight group matches `main` on every case (activation to
  laid out within ±13 ms, inside each case's run-to-run range; every dispatch
  ≤ 9.3 ms, every key ≤ 4.2 ms).
- **S-b, S-c, S3, nit.** The install flag is cleared on the attach request's
  no-path, Plain and settled early returns; whether a chunked install owns
  the buffer is an explicit fact the load and the replacement set and clear;
  a cancelled replacement re-arms the chunked order only if its mutation had
  started (`test_a_replacement_that_ends_before_its_first_turn_keeps_the_language`
  fails without it); the Kani `LIVE_DEADLINE` documentation states the
  carve-out.
- **Tests.** C6b adds 17 widget tests: 16 in `post_open_settle.rs` and 1 in
  `window.rs` (the earlier count of seven was wrong).

## C6c `bound-context-flip-keystrokes` (implementation record, 2026-11-04)

**Status:** implemented on branch `bound-context-flip-keystrokes` from `main`
`0417ecac`; merged as PR #82 (`main` `e31c7cb1`) and archived 2026-11-05.
The scoped font `CssProvider` follow-up (CF in the table above) is next.
Design and decisions:
`openspec/changes/archive/2026-11-05-bound-context-flip-keystrokes/design.md`.

**What was built.** Every edit of a highlighted document is re-analysed by
the editor itself, before GtkSourceView's first update: the edit
after-handlers arm one idle at `G_PRIORITY_HIGH_IDLE - 1`
(`ui/editor_page/reanalysis_pacing.rs`), whose steps call
`ensure_highlight` on an empty range at a bound sized towards
`B_INTERACT_US / 4` (`model/reanalysis_pacing.rs`, Kani-proved in
`core-document-scale`). A step whose report ends short of its bound merged;
the first update and the worker then find nothing (ledger **A85**). A pacing
session that spends `REANALYSIS_ALLOWANCE_US` (`B_SETTLE_US`) without merging
removes syntax highlighting for the session with the run-time reason
`ReductionReason::EditReanalysis`, before any 30 ms worker slice, announced
through the reduced-features notice. Bracket matching stays. A reopen or
external reload highlights the document again.

**The planned run gate was measured and replaced (design D0, D3).** Before
building it, the A85 sample's `--edit-cost` typed into real files from the
reference machine's checkouts (below their C6 gates, copied to scratch, never
committed; load 0.5–1.1): a 16.3 KB js file re-analysed 14.5 K characters per
key across blank lines (its longest blank-line-free run is 9.6 K), a 4.2 KB
TypeScript file needed a worker slice (about 2.5 µs per character), real shell
scripts with 28 K-character runs cost 3.3–3.9 ms per key (the lane's fixture:
40–55 ms), and a 79 K-character yaml run merged within 1.3 K. A static gate at
the fixture limits (2–8 KiB) would have removed highlighting from 5–46 % of the
real js, ts, sh and yaml files on the machine (2,910 / 108 / 12,572 / 19,052
files scanned). The cost is how far the engine's re-analysis runs, which only
the engine reveals; the pacing reads it as it goes.

**Measured with the A85 sample (presented editor-like view, C6b's paced
install, `scripts/measure.sh`, load 0.4–1.1, 2026-11-04;
`.claude/handoffs/c6c/`).** Worst dispatch from the key until quiet, plain
(`main`'s behaviour) against paced (allowance 16 ms):

| Case | Plain worst | Paced worst | Highlighting after the paced keys |
|---|---|---|---|
| Ordinary key, lane blank-line-free fixtures at the C6 gates: js 18 KiB, ts 10.5 KiB, css 37 KiB, sh 39 KiB, yaml 144 KiB | 12.6–20.4 ms (a worker slice each) | 3.1–5.5 ms (steps) | removed at the first key at the start (re-analysis > 16 ms) |
| Ordinary key, md 320 KiB, rs 184 KiB, c 84 KiB, py 48 KiB, xml 132 KiB | ≤ 2.1 ms engine (≤ 7.4 ms frame) | ≤ 1.8 ms engine | kept |
| Real files (js 16–18 KB, ts 4 KB, css 23 KB, sh 14–32 KB, yaml 9–147 KB), 48 keys each | up to 10.9 ms, two with a worker slice | ≤ 5.2 ms | kept, except one 17.6 KB js file (a key at its top re-analysed > 16 ms) |
| Flip and closer, worst case (`--flavor no-closers`): c 64 / 84 / 128 / 512 KiB | 11.2 / 14.4 / 23.8 / 33.8 ms | 2.9 / 5.3 / 6.2 / 6.0 ms | removed at the closer (c 64–128 KiB) or the opener (512 KiB) at the start; kept in the middle of 64–84 KiB |
| Flip, worst case: markdown `<!--` 128 / 256 / 320 / 512 KiB | 11.3 / 15.2 / 21.3 / 34.6 ms | 6.3 / 7.1 / 6.6 / 4.9 ms | kept at 128 KiB; removed at 256 KiB and above |
| Flip, both families: rust `"` 184 KiB, python3 `"""` 48 KiB, xml `<!--` 132 KiB | 26.7–27.3 / 11.0–11.2 / 20.2–27.0 ms | 4.5–4.6 / 6.3–6.4 / 6.2–6.7 ms | removed |
| Ordinary keys typed after each flip | up to 33.6 ms (markdown `<!--` 512 KiB) | ≤ 6.9 ms | — |

Every paced dispatch stays within `B_INTERACT_US`; every flip within
`B_SETTLE_US` by a wide margin. The price is the frame: the steps run before
the first update, which outranks the frame clock, so a key whose re-analysis
spends the allowance is echoed 19.9–23.4 ms after its insert (plain:
14.5–19.9 ms), against the 16.7 ms two-frame figure. **Recorded exception**
(input-to-frame is reported, not gated, C6): the keys that spend the allowance
are echoed up to about 23 ms, once per session (the downgrade follows), or on
each key that re-analyses 8–16 ms. The 10 ms allowance (the engine's own
first-update slice) kept every echo at `main`'s (14.1–19.2 ms) but removed
highlighting after flips that cost 11–14 ms on `main` and from a 4 KB real
TypeScript file. **Decision (coordinator, 2026-11-05, user experience
first): keep `B_SETTLE_US`**; the echo exception above stays recorded (design,
"Decisions on the open questions").

**Exceptions and residuals.** (1) The echo above. (2) TypeScript files above
about 6 KB and some JavaScript files above about 16 KB lose highlighting when
an edit near their start re-analyses more than 16 ms; on `main` such a key costs
a 10 ms first update plus a worker slice. (3) The A85 fixture's frame (cairo
renderer, full-screen highlighted text) costs 6–9 ms on its own in some cases,
on both sides; it is the sample's view, not the pacing (the lane verdict below
is the application's).

**Chunked caps (D4): stay dormant.** With pacing, flips at 512 KiB stay within
`B_INTERACT_US` per dispatch (markdown 4.9–6.5 ms, c 5.9–7.2 ms), so C6b's
per-dispatch reason for withholding the caps is gone; but a 512 KiB document
would open highlighted and lose it at the first flip near the top, and the
chunked install would ship for the first time. **Decision (coordinator,
2026-11-05): the caps stay dormant**: a document that opens highlighted and
loses highlighting at the first flip near its top is a worse surprise than
C6's gate; revisit only with a design that avoids the downgrade at that size.

**Test-seam ceiling.** 169 → 170 for
`model::reanalysis_pacing::override_reanalysis_allowance_for_test`, accepted
by the coordinator (2026-11-05) with the reason recorded in the workflow
matrix.

**Review (2026-11-05).** An independent review found the A85 probe did not
check two claims its ledger row and spec scenario make: one
`highlight-updated` per step and no tagging. Checking them showed the second
was inaccurate: every step applies the `gtksourceview:context-classes:*` tags
over the range it analysed (as the first update would), and no style tag. The
probe now counts reports per step and style-tag changes during the steps,
against a control in which `ensure_highlight` over the whole document tags it
(439 changes), and the ledger, design and code comments say "no style tag".

### Verdict (interleaved lane, 2026-11-05)

`main` `0417ecac` against the branch (`branch-1`), release
`responsiveness-probes` builds, the lane's new `reanalysis` group (28 cases:
each of the twelve measured languages at its C6 gate on the blank-line-free
`short-language` fixture, plus c and markdown on `--flavor no-closers`, each
also as a `/flip` case whose type-start and type-middle bursts type the
language's opener, seven letters, its closer and three letters: `/*`…`*/`,
`<!--`…`-->`, `"`…`"`, `"""`…`"""`), plain variant, main, branch, main,
branch, one repetition each (two per side), every run through
`scripts/measure.sh` after the quiet gate (load 0.96–1.26 before, 1.00–1.17
after; `.claude/handoffs/c6c/runs.log`, `V-*`). Worst main-loop stall of the
three typing phases (type-start, type-middle, type-end: navigation plus ten or
seventeen keys) and the worst key echo:

| Case | Worst typing stall, main → branch (us) | Worst key echo, main → branch (us) |
|---|---|---|
| reanalysis/c/86016 | 7745 → 8676 | 21340 → 20287 |
| reanalysis/c/86016/flip | 9107 → 7722 | 28783 → 27997 |
| reanalysis/c/86016/no-closers | 8171 → 7206 | 22575 → 26263 |
| reanalysis/c/86016/no-closers/flip | 12061 → 7334 | 21717 → 20765 |
| reanalysis/css/37888 | 15109 → 8464 | 26059 → 22330 |
| reanalysis/css/37888/flip | 15040 → 6120 | 29635 → 27460 |
| reanalysis/html/16384 | 6224 → 5003 | 17380 → 23135 |
| reanalysis/html/16384/flip | 7451 → 9490 | 25534 → 26449 |
| reanalysis/js/18432 | 15425 → 6320 | 25899 → 24021 |
| reanalysis/js/18432/flip | 14310 → 6215 | 26401 → 20430 |
| reanalysis/json/20480 | 16428 → 7499 | 29511 → 22079 |
| reanalysis/json/20480/flip | 16342 → 8586 | 28161 → 24194 |
| reanalysis/md/327680 | 6330 → 7660 | 17036 → 20018 |
| reanalysis/md/327680/flip | 5971 → 6640 | 20948 → 23598 |
| reanalysis/md/327680/no-closers | 8396 → 7086 | 17766 → 17113 |
| reanalysis/md/327680/no-closers/flip | 6037 → 6925 | 23667 → 20454 |
| reanalysis/py/49152 | 6119 → 6883 | 22199 → 31101 |
| reanalysis/py/49152/flip | 11285 → 4601 | 26795 → 27674 |
| reanalysis/rs/188416 | 7209 → 7083 | 22868 → 19479 |
| reanalysis/rs/188416/flip | 32291 → 6381 | 26212 → 29375 |
| reanalysis/sh/39936 | 14418 → 8621 | 23496 → 24902 |
| reanalysis/sh/39936/flip | 19665 → 8798 | 26540 → 20819 |
| reanalysis/ts/10752 | 13054 → 5045 | 27149 → 21150 |
| reanalysis/ts/10752/flip | 12556 → 5269 | 28577 → 21956 |
| reanalysis/xml/135168 | 6067 → 7667 | 20826 → 17706 |
| reanalysis/xml/135168/flip | 11077 → 9409 | 28400 → 26006 |
| reanalysis/yaml/147456 | 19005 → 4875 | 21994 → 20306 |
| reanalysis/yaml/147456/flip | 18658 → 4844 | 25334 → 27301 |

**Every dispatch that re-analysed highlighting stays within `B_INTERACT_US`
on the branch**: the largest dispatch ending in a `highlight-updated` mark in
any typing phase is 5.5 ms (main: 32.3 ms, rust's `"`; 9.7–16.4 ms on every
ordinary key of the blank-line-free js, ts, css, sh, yaml and json documents).
The branch's typing stalls above 8 ms (8.5–9.5 ms in html, json, xml, sh, css
and c) are all frame layout and paint (the frame after `Ctrl+Home` or a key
on a full screen of highlighted text, cairo renderer), the same frames `main`
has (8.0–13.1 ms there); `scripts/attr.py` in the handoff attributes each.
Key echo is within the run-to-run spread on both sides (main 17.0–29.6 ms,
branch 17.1–31.1 ms; the lane's double-buffered presentation floor is about
two frames). No case failed on either side; every case's
`post_open_language_attach` (the A67 parse) exceeds `B_SETTLE_US` on both
sides, C6's recorded toolkit-floor exception, unchanged.


## CF `scope-font-zoom-restyle` (implementation record, 2026-11-05)

**Status:** implemented on branch `scope-font-zoom-restyle` from `main`
`1d483b24`; merged as PR #83 (`main` `256cd35a`) and archived 2026-11-05.
`power-of-two-thresholds` (PT in the table above) followed; see "PT" below. Design and decisions:
`openspec/changes/archive/2026-11-05-scope-font-zoom-restyle/design.md`. Measurement artifacts:
`.claude/handoffs/scope-font-zoom-restyle/` (`runs.log`, `compare-*.md`).

**What was built.** `ui/font_zoom.rs` (cross-cutting, `WFR-FONT-ZOOM`) owns two
display `GtkCssProvider`s at user priority and a font generation. Each
generation's rules (`ui/font_zoom/policy.rs`, GTK-free, unit-tested to equal
`apply_font_css`'s declarations at every level) carry every zoom on the menu's
grid and the mod-10 lattice of the current level, keyed by
`lushtext-font-<g>-zoom-<z>`. A zoom step swaps that class on every registered
widget inside `restyle_everywhere` and loads nothing; a base-font change (or a
level on a new lattice, such as 55 % set from outside the UI) loads `g + 1`
into the provider the widgets do not use and swaps after the next painted
frame, a second change before the swap superseding it. Registrations: every
editor surface view, and every `.monospace` widget through
`follow_editor_font_monospace` (search-result labels, the file peek, the
minimap, Markdown code blocks and image fallbacks, the notes and
local-history previews, the Line Inspector through its Blueprint
`monospace: true` and a mapped Rust registration). `make
check-font-zoom-registration` forbids `.monospace` elsewhere; a widget-test
walk fails on any unregistered monospace widget in the populated surfaces. A
pending swap blocks `document-settled`, and the measurement build's settle
sentinel waits for it, with the load and the swap recorded as the post-open
spans `post_open_font_load` and `post_open_font_swap`.

**Axiom.** A86 (probe and sample): a display provider reload revalidates every
widget even with unchanged text (8.5 → 25.0 ms frame from 0 to 1,500
unrelated rows); an unmatched load changes no font or layout extent; a
per-widget class swap applies the new size and does not grow (12.5 →
13.0 ms). A 72-level rule set does not slow an unrelated redraw or hover
change (task 3.4, within noise).

**Verdict (interleaved, 2026-11-05).** `main` `1d483b24` against the branch,
alternating `main`, branch, `main`, branch, plain variant, 3 repetitions per
run, host 1-minute load 0.5–3.7 before and after every run (`runs.log`),
`btrfs-stall-report`: storage not a bottleneck in every window. Figure: the
median over repetitions of each restyle phase's worst main-loop stall (the
lateness watchdog, µs); the lane's "interaction" dispatch is not used, because
with a provider reload or a class swap the cost lands in the next frame, not
in the action's dispatch, and the driver-computed settle could end before
that frame on `main` (0-dispatch entries).

C6's dense restyle set (dense 120, 500, 710, wrap on and off; 6 repetitions
per side):

| Case | zoom step main / branch | font change main / branch | wrap toggle main / branch |
|---|---|---|---|
| dense/120/wrap-off | 12,482 / 11,971 | 8,401 / 7,251 | 2,895 / 3,833 |
| dense/120/wrap-on | 13,324 / 8,768 | 9,097 / 7,013 | 2,949 / 2,175 |
| dense/500/wrap-off | 19,587 / 16,013 | 11,834 / 6,352 | 6,509 / 7,392 |
| dense/500/wrap-on | 8,922 / 9,275 | 11,004 / 9,745 | 2,606 / 2,128 |
| dense/710/wrap-off | 17,908 / 16,055 | 12,279 / 6,521 | 8,052 / 8,638 |
| dense/710/wrap-on | 7,933 / 7,237 | 14,213 / 11,518 | 4,989 / 7,668 |
| **median of per-case differences** | **−1,274** | **−2,389** | +734 (unchanged mechanism: noise) |

The whole dense grid (120, 500, 2,000, 10,000, 50,000; 12 repetitions per
side): zoom step −1,482 µs, font change −2,647 µs, wrap toggle −11 µs in
median. The populated restyle cases (dense/2000 wrap-on, sidebar shown, 12
repetitions per side):

| Case | zoom step main / branch | font change main / branch |
|---|---|---|
| sidebar over 300 entries | 19,798 / 17,885 | 18,940 / 15,496 |
| sidebar over an empty folder (control) | 20,912 / 18,478 | 17,995 / 15,441 |
| search panel listing results | 25,163 / 17,911 | 24,400 / 15,700 |

No open, background, click or typing phase regressed beyond one reference
frame. The font change's load dispatch (`post_open_font_load`) was 1.3–2.3 ms
in median, at most 5.7 ms, and the swap's re-layout followed in later frames.

**Adoption (D5) and the deviation.** Met: the populated zoom step is within
1 ms of its control (−593 µs on the branch; −1,114 µs on `main`, so the lane's
sidebar, a virtualized list, does not show the growth A86 pins); the font
change's dispatches are each at most `main`'s single dispatch and within
`B_SETTLE_US` on C6's set; nothing regresses beyond a reference frame. **Not
met:** the dense zoom step dropped by 1.3–1.5 ms in median, short of D5's
2 ms. The 32 % revalidation share C6 measured is about 10 % on these windows,
where the visible text's re-layout at the new size dominates. Decision
(coordinator, autonomous, user experience first; the maintainer may
override in review): **adopt**, because every restyle phase improves or holds,
the font change improves by 2.4–2.6 ms in median and by 8.7 ms with search
results listed, the zoom step with search results by 7.3 ms, and the zoom
cost no longer grows with unrelated widgets (A86); abandoning would give all
of that up for a threshold set before the share was measured.

**Residuals over budget, recorded.** The zoom step stays over `B_INTERACT_US`
on every dense row but dense/710 wrap-on (7.2 ms; the others 8.8–16.1 ms) and over `B_SETTLE_US` on dense/500 and
dense/710 wrap-off (16.0–16.1 ms) and on the populated cases (17.9–18.5 ms):
the visible paragraphs' re-layout at the new size, C6's text-re-layout floor.
With search results listed, the result labels' own re-layout adds to it
(D7). The font change's whole-tree revalidation (step 1 of D4) stayed within
`B_SETTLE_US` on every dense restyle case, so no A86 exception was needed;
the populated font change's worst stall is 15.4–15.7 ms.

**Found, pre-existing, identical on both sides (not this change's).** On
dense/10000 and dense/50000 (160 lines of 10,000 and 50,000 characters,
editable), a restyle re-lays out whole paragraphs in one dispatch (zoom step
about 100 ms and 470–485 ms, font change 70 ms and 320–330 ms on both sides),
and typing stalls about 1.2 s per burst, with the lane reporting misdirected
typing and withheld typing frames for those cases (`main` 12 / 18 and the
branch 12 / 18 in `V-*3`, `V-*4`). Left to the coordinator: it belongs to the
long-paragraph programme (C5, Protected View's `L_PROTECT_EDIT`), not to the
font provider.

**Independent review (2026-11-05).** The evidence was re-read against the
claims (interleaved, host load 0.5–3.7, `btrfs-stall-report` clean) and holds.
Decisions (design.md, "Review decisions"): (1) adoption despite the 2 ms miss
is **kept**, user experience first, the residuals above staying recorded; (2)
the dense/10000 and dense/50000 stalls are queued as the next programme change,
`DL bound-dense-long-line-editing` (sequence table), because they fail the
full-tier lane on `main` and the programme cannot close with them; (3) review
fix: `make check-font-zoom-registration` also forbids a builder's
`.monospace(true)` and `set_property("monospace", true)`, both of which set
`GtkTextView:monospace` and escaped the check (self-test cases failing first).

## PT `power-of-two-thresholds` (implementation record, 2026-11-05)

**Status:** implemented on branch `power-of-two-thresholds` from `main`
`fbf0f875`; amended 2026-11-05 by the maintainer's presentation-cap decision
(see "Amendment" at the end of this section); merged as PR #84 (`main`
`d59a1256`) and archived 2026-11-06. `bound-lists-by-budget-not-count` (item 1
of the queue below) followed: merged as PR #87 (`main` `1af354a1`) and archived
2026-11-07; `bound-notes-browser-by-budget` followed: merged as PR #88 (`main` `4860fa2d`) and archived 2026-11-07 (see "BN"); `drill-down-palette-categories` followed: merged as PR #89 (`main` `4b567cdf`) and archived 2026-11-07 (see "DP"); `bin-minimap-markers-per-row` followed: merged as PR #90 (`main` `9b8f9102`) and archived 2026-11-09 (see "BM"). Design and decisions:
`openspec/changes/archive/2026-11-06-power-of-two-thresholds/design.md` (D1-D11, "Spec review
(stage 2)", "Implementation decisions (stage 3)"). Measurement artifacts:
`.claude/handoffs/power-of-two-thresholds/` (`runs.log` with the load before
and after every run, `measure.log`, `open/`, `search/`, `m1-main/`,
`m1-branch/`, `spec-stage-notes.md`).

**What was built.**

- **The gate** (D10). `make check-thresholds`, in `check-policy`, runs
  `scripts/check-threshold-kinds.py` (self-tested on a fixture tree) over all
  460 integer constants of the production crates against
  `docs/thresholds.md`. Rows are keyed by file and module-qualified name, with
  `cfg` variants, and multi-line declarations are evaluated. Every in-scope
  constant is a power of two, derived, an external fact, a measurement record
  or a recorded exception, and carries a matching rustdoc `Kind:` line.
- **Tiers** (D3). The minimap and save-only local history above 8 MiB, undo
  off and local history unavailable above 64 MiB, the open refusal above
  512 MiB (task 3); the buffer-snapshot and Replace All per-file caps 8 MiB.
  Every message that names a tier prints it through
  `model::size_units::iec_size` ("64 MiB"; any other size with one decimal).
  `byte_tier_reference` is the independent table the tier unit tests and the
  Kani harness `scale_byte_tiers_match_the_tier_reference` compare with.
- **Caps down** (D4): `N_VISIBLE` 262,144, `N_HEAD` 8,192, the thirteen
  10,000-entry caps 8,192 (the search and sidebar truncation texts read
  "8,192+"), the bookmark excerpt scan 16,384 lines, the minimap marker caps
  1,024 and its wrapped-layout line budget 4,096, palette categories 32, the
  notes browser 256, the CSS attach gate 32 KiB.
- **Coverage windows up** (D5): `R_BATCH` and `R_FOLD` 256, so `N_DENSE` is
  218; construction stages 64; frame-phase frames 16,384; the manifest-repair
  entry charge 128; the reveal settle budget 64 frames.
- **Retention and retries up** (D6), so nothing stored is pruned: local
  history 64 per file and 256 in all, recents 256 (read bound 4 MiB: a full
  store of 4,096-byte paths is about 2.03 MiB, and the former 1 MiB bound
  already reset a full 200-entry store), search history 32, the set-aside
  bound 128 bodies, retries 4 / 4 / 8 / 1,024.
- **Text caps** (task 4.6): announcements 256 (128 would cut the
  229-character longest reduced-features notice, pinned by a unit test),
  the notice 256, the editor identity 128 (stage 4: 64 cut 7.96 % of corpus
  file names, so D2 decides it; 128 cuts 0.0020 %). The minimap's long-line
  marker threshold stays 120, an external fact (the 120-column convention).
- **Measured items**: the open tier (task 3), `L_PROTECT`/`N_INSPECT`
  (task 5.1), `V_SLICE` (task 5.2), the direct-install threshold (task 5.3)
  and the search per-turn caps (task 5.4), each in its own commit; figures
  below.

**Corpus counts** (task 4.5; a throwaway counter with `pulldown-cmark`
0.13.5 and the app's parser options over `~/Workspace` without `.git`,
`.claude` worktree copies and `target` build output, the crates.io registry
and, for paragraphs, `/usr/include`; `taskset -c 8-23`, I/O only). Every
share is above the 0.05 % materiality floor, so all stay recorded exceptions:

| Cap | Today → power of two below | Files that lose the feature (share of those that have it) |
|---|---|---|
| `L_MINIMAP` | 22,000 → 16,384 | 5,556 of 765,618 (0.73 %) |
| `L_INVISIBLES` | 50,000 → 32,768 | 650 of 767,315 (0.085 %) |
| `L_SEARCH_HIGHLIGHT` | 10,000 → 8,192 | 2,366 of 757,002 (0.31 %) |
| `MAX_MARKDOWN_EVENTS` | 50,000 → 32,768 | 700 of 404,113 Markdown files ≤ 4 MiB (0.17 %) |
| the two table-cell caps | 1,000 → 512 | 1,256 of 404,439 (0.31 %) |

**Open tier** (task 3; `open-peak.py`: each file opened headless through
`scripts/measure.sh` inside its own 12G `with-memory-cap` scope, scope
`memory.peak` read until the tab is loaded and the app idle; three
repetitions per side, interleaved; load 0.94–2.20):

| Fixture | `main` at 500,000,000 B (MiB) | branch at 512 MiB (MiB) | median ratio |
|---|---|---|---|
| short-line text | 2,349 / 2,153 / 2,152 | 2,393 / 2,290 / 2,290 | 1.064 |
| UTF-16LE | 2,130 / 2,067 / 2,067 | 2,266 / 2,217 / 2,218 | 1.073 |

Both are within the tier ratio, 1.074, plus the 5 % run spread; no swap, no
OOM kill. Load to laid out: text 166–168 s against 177 s; the largest
dispatch 80–82 ms on `main` against 59–60 ms (text, the C2 conversions).

**Lane** (tasks 5.1–5.3; `main` `fbf0f875` against the candidate build, one
run each, interleaved, `dense/{120,500,512,710}` in both wrap modes,
`baseline/small` and short-line `total-size` at 512, 600 and 704 KiB as
`.txt` and `.html`; plain, AT and probe-IM; three repetitions; load 0.9–1.2;
`compare-m1.md` in the handoff directory has every case):

| Case | worst validation until settle, max (main / branch, ms) | worst post-open dispatch, max (main / branch, ms) | activation to laid out, median (main / branch, ms) |
|---|---|---|---|
| dense/500/wrap-on | 9.4 / 10.5 | 9.4 / 10.8 | 87.5 / 85.1 |
| dense/512/wrap-off | 12.3 / 6.2 | 13.1 / 8.6 | 71.9 / 52.1 |
| dense/512/wrap-on | 11.2 / 9.7 | 11.2 / 9.7 | 86.9 / 85.7 |
| dense/710/wrap-off | 9.6 / 8.6 | 9.6 / 8.6 | 78.8 / 76.3 |
| dense/710/wrap-on | 12.3 / 11.5 | 12.3 / 12.6 | 201.5 / 101.6 |
| total-size/614400/txt | 5.7 / 4.6 | 14.4 / 13.3 | 444.6 / 450.8 |
| total-size/720896/txt | 5.8 / 5.4 | 11.4 / 12.9 | 481.0 / 481.7 |

- **`L_PROTECT` / `N_INSPECT` 512** (D7): dense/512 meets what dense/500 meets
  on the same build (every post-open dispatch at most 9.7 ms against 10.8,
  validation at most 9.7 against 10.5), so both are 512; the Line Inspector
  window grows by 12 characters. Neither `main` nor the branch meets the
  literal "validation within half of `B_SETTLE_US`" on dense/500 wrap-on
  (9.4 and 10.5 ms), which is how C6 accepted it; the criterion applied is
  dense/500's own figures.
- **`V_SLICE` 16,384** (D8): every post-open dispatch at most 12.6 ms and
  every validation dispatch at most 11.5 ms (`main`, at 11,360: 13.1 and
  12.3 ms), activation to laid out no later on any dense case. 8,192 was not
  needed. C6's accepted miss (dense wrap-on 13–15 ms later than `main`) is
  closed.
- **Direct-install threshold 512 KiB** (D8): activation to laid out within
  6.2 ms of `main` at every size (one reference frame is 8.3 ms). The
  31 ms `.html` post-open dispatch at every size is `post_open_language_attach`
  on both sides, C6's recorded A67 parse.

**Search per-turn caps 256** (task 5.4; the widget fixture
`search_panel::test_search_turns_dispatch_and_retire_at_most_256`, failing
first at 250, on the optimized widget build — opt-level 2, no debug
assertions — three repetitions through `scripts/measure.sh` at load
0.59–0.78): a full turn of 256 `Match` events 11.6–13.5 ms median and
14.9 ms at most; a full retirement turn of 256 references 16–23 µs median,
86 µs at most. The debug build at 250 already took up to 15.8 ms per turn.

**Kani** (task 6): every shard whose harnesses, or the production code they call, read a
changed constant was rerun with `make kani KANI_SHARD=<shard>
KANI_MEASURE=…`, one at a time inside the memory cap, on the final values
(2026-11-05, local, load 0.7–5.7 because other agents ran meanwhile; local
timing does not change the CI budgets in `scripts/kani-shards.py`). All 73
harnesses verified:

| Shard | Why it reran | Harnesses | Local wall time / peak RSS | CI budget |
|---|---|---|---|---|
| `core-document-scale` | the tier reference, the tiers, `L_PROTECT`, `R_BATCH` | 14 | 6.2 min / 2.6 GiB | 8.2 min / 2.7 GiB |
| `core-paragraph-profile` | `ProfileConfig::PRODUCTION` (`L_PROTECT`, `N_VISIBLE`, `R_BATCH`) | 4 | 10.0 min / 8.8 GiB | 14.7 min / 8.3 GiB |
| `core-profile-watch-step` | the watch's `R_BATCH` window | 1 | 13.7 min / 10.1 GiB | 19.4 min / 10.4 GiB |
| `core-profile-watch-reads` | the `2·R_batch + 2` read bound | 1 | 13.9 min / 10.1 GiB | 18.4 min / 10.4 GiB |
| `core-profile-watch-unsound` | the same window | 1 | 14.1 min / 10.1 GiB | 19.5 min / 10.4 GiB |
| `core-protected-fold` | `N_HEAD`, `R_FOLD`, `N_DENSE` | 6 | 6.8 min / 1.6 GiB | 6.9 min / 1.5 GiB |
| `core-protected-fold-shape` | the same | 3 | 12.2 min / 1.5 GiB | 13.0 min / 1.6 GiB |
| `core-protected-fold-mapping` | the same | 5 | 5.2 min / 1.6 GiB | 6.0 min / 1.6 GiB |
| `core-protected-view` | `N_INSPECT ≤ min(HEAD_CAP, L_PROTECT)` | 6 | 2.3 min / 3.3 GiB | 1.1 min / 3.4 GiB |
| `core-protected-view-navigation` | the production `N_INSPECT` width | 1 | 0.2 min / 1.5 GiB | 13.3 min / 1.5 GiB |
| `core-post-open-settle` | `V_SLICE`, the direct-install threshold | 15 | 9.8 min / 1.8 GiB | 11.0 min / 2.0 GiB |
| `core-paragraph-boundary` | `MAX_DENSITY_WINDOW` and `R_BATCH` | 3 | 10.3 min / 3.7 GiB | 21.9 min / 3.8 GiB |
| `core-journal-and-write` | `SOFT_BOUND_BODIES` (the harness now pins 128) | 13 | 5.9 min / 2.5 GiB | 12.0 min / 2.6 GiB |

The navigation harness fell from 13.3 CI minutes to 13 local seconds: with
`N_INSPECT` 512 the window arithmetic is a shift and a mask, which is the
rule's own argument for powers of two. `core-protected-install` was not rerun:
its harnesses read no changed constant. `core-paragraph-profile` peaked at
8.8 GiB locally against its 8.3 GiB runner budget, below the 12 GiB margin;
the next dispatched Kani run re-measures every budget. `make
check-kani-shards` passes.

**Proof strength** (task 6.3): `scripts/proof-strength.py` ran the module's
Kani harnesses (the Kani arm) against the seven byte-tier mutants of
`DocumentScale::for_profile_in_with` (the undo comparison and both
local-history comparisons, `>`/`<=` replaced by `==`, `<`, `>=` or `>`);
`scale_byte_tiers_match_the_tier_reference` kills all seven, in 5–6 s each,
as the parity harness did against the retired table. The tests arm over all
235 mutants of the module was not rerun: the tier tests are unit tests whose
values the change fixed, and the arm's cost (one build per mutant) bought
nothing the Kani arm did not show.

**Verification lane** (task 7.3; quick tier, `main` `fbf0f875` against the
final branch, interleaved, plain, one repetition, load 0.8–2.3): the
keystroke group, the dense group, `baseline/small` and `total-size` at
8 MiB and 8 MiB + 1 pass on both sides with matching figures (every
keystroke within a millisecond or two of `main`; `total-size` laid out in
5.59–5.66 s on both). Two observations, both attributed:

- The `.html` keystroke cases show one ~31 ms dispatch in the open or
  background phase in some repetitions, on both sides: in a three-repetition
  rerun 3 of 9 runs on `main` (load 1.0–1.2) and 3 of 9 on the branch (load
  5.1–5.3, so not a verdict) carry it. It is the HTML language attach's parse
  (C6's recorded A67 exception), which also gives the 31 ms `.html`
  `total-size` dispatch above.
- The 64 MiB `total-size` edges exceed the quick tier's phase cap on both
  sides (a 64 MiB open takes longer than the quick tier allows);
  the full tier's 20 s open-phase cap is shorter too (rerun `t64-*`, both
  sides), so the lane times neither. The 10–20 s "worst" it prints for them
  is the cap's length, not a dispatch: the open-tier runs of task 3 measured
  every dispatch of a 500 MB–512 MiB open with the same probe, at most
  82 ms. The 64 MiB edges themselves are pinned by the unit tests at 64 MiB
  and 64 MiB + 1 and by the Kani tier-reference harness. Extending the lane's
  `total-size` cap to such sizes is a lane follow-up, not part of PT.

**Recorded exceptions** (D9, each with its measurement in its rustdoc and in
`docs/thresholds.md`): `L_SYNTAX`; eleven per-language attach gates (HTML
46 KiB and JSON 24 KiB included), JSON's typing gate and `S_SYNTAX_OTHER`;
`L_MINIMAP`, `L_INVISIBLES`, `L_SEARCH_HIGHLIGHT`; `L_PROTECT_EDIT`;
`V_BATCH`; `MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES`;
`MAX_PREVIEW_IMAGE_SOURCE_PIXELS`; `MAX_MARKDOWN_EVENTS` and the two
table-cell caps.

**Displayed sizes: done (D14).** The properties panel, the file peek and the
local-history list printed "MB"/"KB" for a size they *display* (not a tier);
all three now print through `model::size_units::iec_size` ("10 MiB",
"1.5 KiB"), task 9.7.

**Amendment (maintainer decision, 2026-11-05; spec stage).** The maintainer
reviewed PR #84 and decided that LushText must be extremely performant **and**
handle high loads and large workspaces with no visible loss. The first
version's "caps rounded down" (design D4) shipped losses against `main`:
palette categories 50 → 32, the notes browser 500 → 256, Replace All skipping
files of 8–10 MiB, every user-visible 10,000 cap → 8,192 (workspace search,
Replace Preview, Protected View find, the sidebar directory cap, palette and
notes-browser note entries, the set-aside, format-upgrade and journal scans),
minimap markers capped at 1,024 (2,000), the minimap and per-edit local
history lost for files of 8–10 MB, Protected View heads 1,808 characters
shorter, dense heads 218 characters instead of 250, CSS files of 32–37 KiB
unhighlighted.

- **New kind** (`.agents/rules/rust.md`, design D12): a **presentation cap**
  is a cap on what the user sees or gets (results, rows, notes, markers,
  entries listed, files a bulk action processes, characters announced, the
  size or length at which a feature turns off). It rounds **up** to the power
  of two at or above its pre-PT value when the bound it protects (memory,
  per-turn GTK time against `B_interact`/`B_settle`, worker memory) still
  holds by measurement; otherwise it keeps its pre-PT value as a recorded
  exception with the measurement. When in doubt, a cap is a presentation cap.
  Internal resource caps keep rounding down. `text cap` is folded in.
- **Targets** (design D12's table has every constant, its pre-PT, PR #84 and
  new value, the bound and its measurement): palette 64; notes browser 512;
  Replace All per file 16 MiB (memory still bounded by the unchanged 64 MiB
  aggregate undo payload); every user-visible 10,000 cap 16,384; minimap
  markers 2,048 and the wrapped-layout line budget 8,192; bookmark excerpt
  scan 32,768 lines; recent-load diagnostics 32; the reduced-features notice
  512 characters; the minimap and save-only local-history tier 16 MiB (else
  10,000,000 bytes); `N_HEAD` 10,000 and the CSS gate 37 KiB as exceptions
  (C2's head sweep and A69 show the power of two above breaking the bound);
  `V_BATCH` 65,536 (else 64,000) so `N_DENSE` ≥ 250; conditionally, the
  Markdown event cap 65,536, table cells 1,024, `L_MINIMAP` 32,768,
  `L_INVISIBLES` 65,536, `L_SEARCH_HIGHLIGHT` 16,384, the palette index
  131,072 and the image cap 2^26 pixels.
- **Measured** (task 9.5, 2026-11-05, interleaved with `main` `fbf0f875`,
  each run started at 1-minute load below 4; design "Implementation of the
  amendment" has the table, `runs.log` every figure): rounded up and within
  their bounds — palette 64 (settle 6.4–7.9 ms ≤ `B_interact`), workspace
  search, Replace Preview and Protected View find 16,384, the notes browser
  512 rows and 16,384 open-editor, source and sidecar entries, palette notes
  16,384, the set-aside, preflight and journal scans 16,384, bookmark
  excerpts 32,768 lines, minimap markers 2,048 and the wrapped-layout line
  budget 8,192, Replace All 16 MiB per file (peak growth ratio 1.57; the
  64 MiB aggregate unchanged; files within `main`'s 10 MiB are replaced in a
  first pass, so a larger file never takes the undo budget of a file `main`
  replaced, stage-4 review), save-only local history 16 MiB, `V_BATCH`
  65,536 (`N_DENSE` 256), the Markdown event cap 65,536 and table cells
  1,024, `L_SEARCH_HIGHLIGHT` 16,384, the palette
  index 131,072 files and directories, the image cap 2^26 pixels. Kept as
  recorded exceptions by measurement: `MAX_DIR_ENTRIES` 10,000 (a capped
  expansion's terminal dispatch 41–43 ms at 16,384 against 25–30 ms on
  `main`), `S_MINIMAP` 10,000,000 bytes (a 16 MiB open with the minimap
  37–38 ms against 23–27 ms, and a settle past the lane's cap), `L_MINIMAP`
  22,000 (background 13.4 ms worse at 32,768). `L_INVISIBLES` keeps 50,000
  because every document past it is in Protected View, which draws no
  invisibles (C2 D10), now a compile-time assertion. `N_HEAD` 10,000 and CSS
  37 KiB are exceptions cited from C2's sweep and A69. Failing-first tests
  assert no loss against `main` for every cap (task 9.3). `main` already
  misses `B_settle` at its own values on the first-results reveal of a
  search (20–23 ms), the notes browser rebuild (32–38 ms) and a capped
  directory expansion (25–30 ms); D13 item 1 covers the ones a cap scales.
- **Displayed sizes** in IEC units (D14), done: the properties panel, the file
  peek and the local-history rows print through `model::size_units::iec_size`.

**Queued after PT merges** (design D13; maintainer, 2026-11-05), each a
separate change, in this order:

1. `bound-lists-by-budget-not-count` (after PT's archive, 2026-11-06;
   merged as PR #87, `main` `1af354a1`, and archived 2026-11-07): the palette's "N more…" row with a
   category drill-down mode; an incremental, virtualized notes browser
   bounded by retained bytes, with "showing X of Y"; minimap markers binned
   per pixel row with no count cap; workspace search streamed without a
   count cap, bounded by retained bytes; sidebar directories past 10,000
   entries loaded incrementally, without the truncation placeholder.
   **Split at its spec stage (2026-11-06; design D1, branch
   `bound-lists-by-budget-not-count`).** This change is the workspace slice:
   - workspace search without a match count, bounded by a 256 MiB
     retained-byte budget, which is the power of two above `main`'s
     worst-case retention at 16,384 matches; past it the panel states the
     shown and found counts;
   - sidebar directories without an entry count, loaded in time-sliced
     batches behind a loading row, with a budget row only past
     `SIDEBAR_DIRECTORY_RETAINED_BYTES`;
   - one Kani-proved `model/list_budget.rs`.
   It also discharges `main`'s first-results reveal (22–26 ms), the Replace
   Preview entry (27.8–29.2 ms, PT M3, found while reading PT's evidence) and
   the 10,000-entry expansion terminal (25–30 ms). **Implemented 2026-11-06**
   (stage 3, branch `bound-lists-by-budget-not-count`), merged as PR #87
   (`main` `1af354a1`) and archived 2026-11-07: see "BL" below. Queued as their own changes, in this order, right
   after it:
   - `bound-notes-browser-by-budget` (after BL's archive, 2026-11-07;
     implemented 2026-11-07, merged as PR #88 (`main` `4860fa2d`) and
     archived 2026-11-07, `openspec/changes/archive/2026-11-07-bound-notes-browser-by-budget/`): an
     incremental, virtualized notes browser bounded by retained bytes, with
     "showing X of Y". The probe answered yes, `AdwSidebar` builds every row
     (A92), so a `GtkListView` with section headers over a lazy row model
     replaced it; the source lost its row and sidecar counts and skips a
     refused row instead of stopping; the open-editor snapshot is sliced. See
     "BN" below;
   - `drill-down-palette-categories` (after BN's archive, 2026-11-07;
     merged as PR #89, `main` `4b567cdf`, and archived 2026-11-07): the "N more…" row per category and the category
     drill-down mode. **Proposal written 2026-11-07** (branch
     `drill-down-palette-categories`,
     `openspec/changes/archive/2026-11-07-drill-down-palette-categories/`), judged worth doing
     with refinements: the category view is a second `GtkListView` over a
     lazy model of ranked source ordinals (BN's pattern), ranked by the
     grouped view's own selector so its first 64 rows are the grouped rows;
     an 8 MiB retained-byte budget (`list_budget::rows_within`, Kani B5)
     holds the whole 131,072-file index; exact counts, with a
     name-prefiltered empty-query count pass (1.5–2.1 ms against 7.4 ms
     for the full-path test at 131,072 files, spec microbenchmark); Escape
     leaves one level through the dismissal ladder; Page Up/Down; the
     palette's note source loses its counts (`SkipAndCount`); GTK axioms
     A96 (natural-height measure flat in items) and A97 (`stop-search` and
     Backspace on an empty entry). **Implemented 2026-11-07** (stage 3,
     branch `drill-down-palette-categories`; merged as PR #89 and
     archived 2026-11-07): see "DP" below;
   - `bin-minimap-markers-per-row` (after DP's archive, 2026-11-07;
     **implemented 2026-11-07**, merged as PR #90 (`main` `9b8f9102`) and
     archived 2026-11-09; see "BM"): markers
     binned per pixel row. It must
     first attribute PT's M7 worst dispatches (118–140 ms and 741–823 ms,
     which include a whole-buffer `set_text` and relayout) before claiming
     any minimap blocker. **Proposal written 2026-11-07** (branch
     `bin-minimap-markers-per-row`,
     `openspec/changes/archive/2026-11-09-bin-minimap-markers-per-row/`), judged worth doing,
     with refinements.

     **The M7 attribution claims no minimap blocker** (`main` `9b8ba5ae`,
     minimap on and off interleaved, load 0.3–1.3):
     - the 118–140 ms came from PT's test, which never destroyed its windows;
       with the windows destroyed the dispatch is 9.6 ms with the minimap and
       3.3 ms without it;
     - the 741–823 ms is the editor's wrapped layout of 8,192-character lines
       installed with `set_text`: 646–694 ms with the minimap and 645–789 ms
       without it (owner: DL);
     - in the modified phase, the minimap's share is 8–11 ms of 115–124 ms
       (owner: the toolkit relayout after 2,100 inserts made outside the main
       loop).

     **The cost the change removes** is the per-run projection on every
     draw (about 5 ms at `main`'s caps, linear in the marker count), plus the
     match walk.

     **The refinements:**
     - bin per strip row **group** with one probe per group;
     - modified lines in a property-less `GtkTextTag`: per-line marks cost
       768–784 ms at 131,072, the tag about 1.1 µs per edit; saving swaps in
       a fresh tag and removes the old one in slices;
     - search markers only from a completed scan, because an unscanned
       `forward` scans synchronously (68–81 ms);
     - the strip redraws on map scroll;
     - Kani over the integer walker;
     - GTK axioms AX1–AX5.

     The binning prototype at 131,072 markers of each kind and 713 rows
     costs at most about 6 ms with a probe per row;
   - `adopt-gnome-51` (queued 2026-11-09, maintainer decision, after
     BM's archive; merged as PR #91, `main` `14649be0`, and archived
     2026-11-11, `openspec/changes/archive/2026-11-11-adopt-gnome-51/`): move the Flatpak runtime 50 -> 51 (GTK 4.24, GtkSourceView
     5.22, Pango 1.58) with the test environment on the same toolkit (toolbox
     `hypatia-f45`); like every GTK version change it starts with
     `make gtk-axioms`. Spec written 2026-11-09
     (`openspec/changes/archive/2026-11-11-adopt-gnome-51/`): on GTK 4.24.1 axioms A7 and A20
     (GTK !9778) and A32 (GtkSourceView 8d679ccd) changed verdict; the
     widget suite passes but logs three new toolkit warning classes, and
     the quick tier finds typed text drawn late (`baseline/small` p50
     5.3 -> 10.0 ms, 33 withheld keys); all are in scope. The source floor
     stays GNOME 50. Ordered before `lighten-notes-browser-rows`
     (coordinator decision, 2026-11-09): every later measurement, and the
     upstream re-tests below, should run on the toolkit users will get, and
     E1–E3 are fixed per-dialog costs that do not grow;
   - `retest-upstream-fixes-on-nightly` (**next**, after `adopt-gnome-51`'s
     archive, 2026-11-11): the GNOME nightly re-test of the upstream fixes
     recorded in `docs/upstream-reports.md` (A79, A54, A28, A52, A29, and
     A102 before its GtkSourceView report is drafted);
   - `lighten-notes-browser-rows` (after `retest-upstream-fixes-on-nightly`; queued 2026-11-07 from BN's stage 4
     review, coordinator decision, user experience first): first measure
     whether the about 205 realized rows (A1) or each row's widgets cause
     BN's recorded fixed costs E1–E3 (dialog first frames 47–76 ms, the
     frame after a row-changing query 22–23 ms, close 26–34 ms), then remove
     the cost that dominates, keeping separate accessible title and subtitle,
     or keep E1–E3 as measured exceptions with that attribution recorded.
     Queued after `bin-minimap-markers-per-row` because E1–E3 are fixed
     per-dialog costs that no longer grow with the notes, while the palette
     and minimap still scale with the document or workspace.
   Also recorded: a process-wide retained-byte ledger for many expanded
   huge sidebar directories (the budget, like the count before it, is per
   directory). From BN (besides `lighten-notes-browser-rows`, queued above):
   the local-history browser's `AdwSidebar` (bounded by
   retention at 64 snapshots per file, so no blocker; a candidate if
   retention grows), and the palette's note counts
   (`MAX_PALETTE_NOTE_ENTRIES`, its sidecar count and its stop-at-first-refusal
   rule) belong to `drill-down-palette-categories`.
2. `stream-replace-all`: chunked replace with an on-disk undo journal, so the
   per-file limit reaches the open tier with constant memory; skipped files
   listed with their reason and an "Open and replace in editor" action.
3. `scale-file-index`: an interned path arena and a byte budget instead of
   the 131,072-file count, targeting at least 1,000,000 files, with a
   Criterion benchmark and a measurement on a real monorepo.
4. Profile and slice the 59–60 ms largest-file open dispatch (the open-tier
   runs above).
5. Then DL `bound-dense-long-line-editing`. (`avx2-health-scan-kernel`, once
   queued here, resumed and merged first: PR #85, `main` `504642a9`, archived
   2026-11-06.)

**Next release notes (AppStream, prepared, not inserted).** "Size limits are
now round numbers, and none of them went down: per-edit local history applies
up to 16 MiB, undo and local history up to 64 MiB, files up to 512 MiB open,
and Replace All rewrites files up to 16 MiB. Workspace search keeps 16,384
results, the command palette 64 per category, the notes browser every note
and bookmark (it says "Showing X of Y" if memory runs short), and every
minimap marker (`bin-minimap-markers-per-row` removed the 2,048 cap). Sizes
are shown in MiB and KiB everywhere. Local
history keeps more snapshots (64 per file, 256 in all), recent documents keep
256 entries and search history 32, so nothing you have is trimmed."

## BL `bound-lists-by-budget-not-count` (implementation record, 2026-11-06)

**Status:** implemented on branch `bound-lists-by-budget-not-count` from
`main` `469e3f84`; merged as PR #87 (`main` `1af354a1`) and archived
2026-11-07. `bound-notes-browser-by-budget` followed: merged as PR #88 (`main` `4860fa2d`) and archived 2026-11-07 (see "BN"). Design and decisions:
`openspec/changes/archive/2026-11-07-bound-lists-by-budget-not-count/design.md` (D1–D9, "Spec
review", "Implementation decisions"); evidence in
`.claude/handoffs/bound-lists-by-budget-not-count/`.

- Workspace search has no match-count cap: a 256 MiB retained-byte budget
  (`SEARCH_RESULTS_RETAINED_BYTES`, the power of two above `main`'s worst
  case at 16,384 matches), admission serialized with the send, counting past
  it; 40,000 ordinary matches are all shown. The sidebar has no entry-count
  cap: a 512 MiB per-directory budget (above both `main`'s 10,000-row worst
  case and 131,072 typical rows), one scan pass when it fits, time-sliced
  batches behind a loading row, a budget row only past it.
- `model/list_budget.rs`, Kani shard `core-list-budget` (pull-request gated;
  CI runs 42322273437 (2.76 min) and 42331071390 (2.53 min), 1.63 GiB each,
  recorded at the larger, 2.8 min / 1.7 GiB, at the archive, 2026-11-07).
  Run 42331071390 also took 16.57 min for `core-geometry-policies`, over the
  15-minute pull-request margin; that shard was re-split into
  `core-geometry-policies` and `core-minimap-fit`, both still gated
  `pull-request` (docs/next/formal-verification.md, "List budget and geometry
  policy shard re-split").
- Verdicts (optimized widget build, presented, load below 4, interleaved with
  `main`; `runs.log`) are tabulated in the design's "Verdicts" table; every
  dispatch of the three former blockers is now within `B_SETTLE_US`, with no
  toolkit-floor exception.
- **Stage 4 review (2026-11-06).** Collapsing a sidebar directory was over
  `B_SETTLE_US` on `main` too (17.9–24.5 ms at its 10,004-row cap). The
  section now holds each expanded directory's child store, so GTK's collapse
  only unlinks it, and retires the listing in time-sliced turns at
  `G_PRIORITY_LOW`: collapse 15.1–15.3 / 16.0–16.8 / 23.3–23.7 ms at 10,000 /
  16,385 / 131,072 entries (before: 18.0–18.6 / 21.0–21.3 / 55.8–61.4).
  Past about 16,000 entries what remains is GTK's rb-tree teardown (new
  axiom **A91**) plus the realized-row teardown (A1): a **recorded
  toolkit-floor exception**. The review also fixed the rustdoc gate
  (`probes/a89.rs`), a Sonar finding, and two failures `main`'s CI already
  had: RUSTSEC-2026-0433 (`pulldown-cmark` 0.13.6) and the Focus Mode zoom
  test's unsettled width. Decisions on every open item: the design's "Stage
  4 review".
- **Follow-ups recorded:** a process-wide retained-byte ledger across many
  expanded huge sidebar directories (the budget, like the count before it, is
  per directory); sharing the sidebar's cached path copies as `Arc<Path>`
  (the charge counts eight copies); not scanning the discarded
  `is_expandable()` probe store at all (`main` scans it in full, then the
  expansion scans again); `slice-large-directory-collapse` (stage 4 review):
  an owned collapse path that empties a large directory's store in slices
  before GTK's collapse, removing A91's share of the exception, and the
  attribution of the rare 14–18 ms re-expansion outlier at 131,072 entries.

## BN `bound-notes-browser-by-budget` (implementation record, 2026-11-07)

**Status:** implemented on branch `bound-notes-browser-by-budget` from
`main` `e4ec2624` (stage 3, commits `a778212c` and `cc687cc8`), reviewed in
stage 4 on 2026-11-07, merged as PR #88 (`main` `4860fa2d`) and archived
2026-11-07. `drill-down-palette-categories` followed: merged as PR #89 (`main` `4b567cdf`) and archived 2026-11-07 (see "DP"); `bin-minimap-markers-per-row` followed: merged as PR #90 (`main` `9b8f9102`) and archived 2026-11-09 (see "BM"). Design and decisions:
`openspec/changes/archive/2026-11-07-bound-notes-browser-by-budget/design.md` (D1–D10, "Spec
review (stage 2)", "Implementation decisions (stage 3)", "Stage 4
review"). Evidence: `.claude/handoffs/bound-notes-browser-by-budget/`.

- Browse Notes and Browse Bookmarks list every row the source retains. They
  use a virtualized `GtkListView` with section headers over a lazy row model
  (`ui/window/notes/row_model.rs`), which replaced `AdwSidebar`. A92 found
  that `AdwSidebar` builds one widget per item. The 512-row cap is gone. The
  source has no row or sidecar count: a byte refusal skips the row and
  counts it (`ByteRefusal::SkipAndCount`), and the palette keeps its
  stop-at-first-refusal rule (`Stop`). The open-editor capture runs in turns
  under the unchanged 4 MiB `RetainedBudget`, with no count.
- "Showing X of Y notes and bookmarks" is shown only past a byte budget,
  with one polite announcement per source generation. Since the stage 4
  review its counts are grouped in thousands, through the shared
  `model/list_budget.rs::grouped_thousands`, like the workspace tree's and
  workspace search's notices.
- Verdicts (optimized widget build, interleaved with `main`, load below
  2.5): every dispatch costs the same at 512, 16,384 and 90,687 rows. Query
  publish is 2–3 ms (`main`: 78 ms to 17 s), the keystroke at most 0.4 ms,
  and the worst capture turn 2.3 ms. Peak RSS at 90,687 rows is 168–246 MiB
  (`main`: 4.8 GiB). **Recorded toolkit-floor exceptions**, enforced at twice
  their measured worst:
  - **E1**, the dialog's first frames: 47–76 ms.
  - **E2**, the frame after a query that changes the rows on screen:
    22–23 ms.
  - **E3**, closing the dialog: 26–34 ms.
  
  All three come from the about 205 rows GTK realizes for any list (A1,
  A90).
- **Stage 4 review.** It fixed a double count in "of Y": an in-scope editor
  the capture reached after its byte cut was counted both live and from its
  sidecar. It also grouped the notice's counts, made `item()` answer by the
  reported length during a shrinking publish, and fixed a misplaced doc
  comment. `core-list-budget` was rerun (6 of 6). It decided to keep E1–E3,
  the list's 18 px lower start (a `:first-child` header rule would change
  row heights while the list recycles), and the `WordChar` metadata wrap.
- **Follow-ups recorded:**
  - `lighten-notes-browser-rows`, queued right after
    `bin-minimap-markers-per-row` and, since 2026-11-09, after
    `adopt-gnome-51` (see the PT queue). It first measures
    whether the realized-row count or the row's widgets dominate E1–E3,
    then removes that cost or keeps E1–E3 as measured exceptions, and keeps
    separate accessible title and subtitle.
  - The local-history browser's `AdwSidebar`. It is bounded by retention
    (64 snapshots per file), so it is not a blocker, but it is a candidate
    if retention grows.
  - The palette's note counts (`MAX_PALETTE_NOTE_ENTRIES`, its sidecar
    count and `Stop`), for `drill-down-palette-categories`.

## DP `drill-down-palette-categories` (implementation record, 2026-11-07)

**Status:** implemented on branch `drill-down-palette-categories` from
`main` `ed37423a` (stage 3, 2026-11-07), merged as PR #89 (`main`
`4b567cdf`) and archived 2026-11-07; `bin-minimap-markers-per-row` followed: merged as PR #90 (`main` `9b8f9102`) and archived 2026-11-09 (see "BM").
Design and decisions:
`openspec/changes/archive/2026-11-07-drill-down-palette-categories/design.md` (D1–D12,
"Spec review (stage 2)", "Implementation decisions (stage 3)", "Measured
verdicts"). Evidence: `.claude/handoffs/drill-down-palette-categories/`.

- Every command palette group cut by the 64-row cap ends with a "N more…" row
  stating the exact hidden count (the empty query counts its rest without
  retaining or scoring, by length when nothing is excluded). Enter on it, a
  double-click or `win.set-command-palette-category` shows the category view:
  a second `GtkListView` over the lazy `PaletteCategoryModel` of ranked
  source ordinals, ranked by the grouped view's own selector so its first 64
  rows are the grouped rows, bounded by an 8 MiB budget that holds the whole
  131,072-file index (`rows_within`, Kani B5). Escape leaves one level (the
  entry latches the window's Escape; the ladder gained a
  `CommandPaletteCategory` rung), Backspace leaves only after a release on the
  empty query (A97), Page Up/Page Down move 16 rows.
- The palette's note source has no row or sidecar count and skips and counts
  a byte refusal, and its open-editor capture is the notes browser's sliced,
  byte-bounded one (`OpenEditorCaptureOwner`), so 131,072 sidecar bookmarks
  and 20,000 editor bookmarks are all searchable.
- Verdicts (optimized widget build, interleaved with `main`, load below 4):
  every category-view dispatch is flat at 512, 16,384 and 131,072 rows;
  Enter 0.07–0.19 ms, first publish 6.8–8.2 ms, a query's publish 1.0–3.9 ms
  with its frame under 0.7 ms (the realized rows are retargeted, not
  rebuilt), Page Down and the far select-and-scroll at 131,072 under
  `B_INTERACT_US` (2.2–6.9 ms worst), Escape 0.3–1.3 ms after moving the
  model's clear to an idle (it was 4.7–11.2 ms inside Escape). The grouped
  publish matches `main` by the same harness. The empty-query count costs
  2.7 ms (32 tabs) and 4.4 ms (256 tabs) on the worker at 131,072 files. The
  note source at 131,072 sidecar bookmarks builds in 269–347 ms with a
  158–254 MiB peak against `main`'s 34–50 ms and 90–114 MiB for the 16,384
  rows it kept. **Recorded toolkit-floor exception X1**: the frame after the
  first category publish, 13.2–16.6 ms (at most 0.6 ms over `B_SETTLE_US`),
  the about 205 realized rows; bounded at twice its worst by a widget test.
- **Review (stage 4, 2026-11-07):** three defects fixed failing-first: the
  category action left the shown view before refusing an id of another mode
  (R1); a same-query republish (an index or note-source change) sent the
  category selection back to row 0 (R2, now kept, clamped); and grouped rows'
  categories were inferred from header labels (R3, `Header` now carries its
  category). Open decisions accepted: the note-source worker cost, the
  empty-query count (index-kept count to `scale-file-index`), the deferred
  clear (to `lighten-notes-browser-rows`); the `core-list-budget` CI budget
  was re-measured by PR #89's Kani run `42372071326` (1.83 min, 1.63 GiB,
  inside the 2.8 min / 1.7 GiB record, which stands). Record: design.md
  "Review (stage 4)".
- **Accepted items, carried by later changes (archive, 2026-11-07):** the
  empty-query count's remaining per-file cost (one canonical-path read per
  file, 2.7–4.4 ms on the worker at 131,072 files) goes to
  `scale-file-index`, whose index can keep the canonical-path count that
  removes it; `lighten-notes-browser-rows` also covers the palette's
  category rows (the deferred clear's 9.5–11.5 ms row-unbinding dispatch and
  X1 share BN's E1–E3 cause, the about 205 realized rows, A1/A90), so its
  attribution and fix are measured on both lists.
- **Follow-ups recorded:** the GTK Lush lazy-model extraction candidate (two
  consumers now, `docs/next/gtk-lush.md`); the empty-query count's remaining
  per-file cost is one canonical-path read, which a canonical-path count kept
  by the index would remove (for `scale-file-index`); the deferred clear's
  9.5–11.5 ms dispatch (row unbinding) is within `B_SETTLE_US` and shares
  BN's E1–E3 cause, so `lighten-notes-browser-rows` should cover the palette's
  rows too.

## BM `bin-minimap-markers-per-row` (implementation record, 2026-11-07)

**Status:** implemented on branch `bin-minimap-markers-per-row` (stage 3,
2026-11-07), reviewed in stage 4 on 2026-11-09, merged as PR #90 (`main`
`9b8f9102`) and archived 2026-11-09. `adopt-gnome-51` followed (merged as
PR #91, archived 2026-11-11). Design and
decisions: `openspec/changes/archive/2026-11-09-bin-minimap-markers-per-row/design.md`
("Implementation record" and "Measured verdicts" hold every figure, the
handoff's `runs.log` every run).

- **What shipped.** The marker strip bins per pixel row (row groups from
  `line_at_y`, A99; each kind's index asked at most once per group plus once,
  Kani `marker_rows_*` in the new `core-minimap-binning` shard), so the three
  2,048 caps are gone and every marker the map shows is drawn. Search markers
  come from a completed scan (A100), and until then from a bounded private
  copy of the band's text (`forward_async` was dropped: a query change leaves
  a pending request unanswered and GIO logs a CRITICAL, A102). Modified lines
  are one property-less tag per generation (A98), swapped on every clean
  transition and retired in 4,096-toggle low-priority slices. The strip
  redraws on map scroll (A101).
- **M7 attribution (D1).** No minimap blocker: PT's 118–140 ms came from
  leaked test windows, the 741–823 ms is the editor's wrapped layout (owner:
  DL), and the modified phase is the toolkit's validation of edits made
  outside the main loop (GtkTextView/GtkSourceMap); the minimap's share of it
  is now nil (8–11 ms on `main`).
- **Verdicts.** First in-band search marker 150–153 ms on every document
  (`main` 261–348 ms); the minimap's own work at 131,072 markers of each kind
  at most 0.11 ms per draw, 6.4 ms per refresh, 4.3 ms per retirement slice;
  `main`'s per-draw projection at its caps was 16.8–22.5 ms. Lane
  (`minimap-markers/*`, full tier, interleaved with `main`, 2026-11-09): at
  2,048 markers find 5.2–5.8 ms (`main` 9.9–10.7) and Page Down 1.9–2.8 ms
  (`main` 5.3–5.8); at 131,072 every phase within noise of `main`, which
  draws a 64th of the markers, and the open within `B_SETTLE_US` (`main` up
  to 17.8 ms).
- **Pre-existing defects fixed.** `gtk_lush_settle::Debounce` kept every
  superseded timeout source (131,072 edits in one dispatch: `Too many GWeakRef
  registered`, and `main` stalled for over 27 minutes); `main`'s modified
  source marks collapsed onto line 0 on `set_text` (wrapped phase 612–799 ms
  with the minimap off, 81 ms now).
- **Exceptions.** Scroll frames over `B_SETTLE_US` with the minimap on are the
  `GtkSourceMap`'s own repaint (on `main` too; the branch stays within `main`
  + one reference frame); the A1 search phase is the map repainting its search
  highlights; edits outside the main loop and Replace All are toolkit costs.
  Lane at 131,072 markers: typing 9.7–12.4 ms is the editor's own (8.9–13.5
  ms with the minimap off), find 12.0–12.5 ms carries about 8 ms with the
  minimap shown on both sides (3.6–4.6 ms off), equal to `main`. The load
  over 131,072 modified runs (up to 146.9 ms) is the in-tab search left
  active over it: the same load over a clean buffer with that search active
  is 39.6–95.1 ms on the branch and 40.1–123.5 ms on `main`.
- **Stage 4 review (2026-11-09).** One finding, fixed failing-first: D5's
  kept search bins were not implemented, so a strip draw during an edit's
  rescan dropped the search lane on bands longer than
  `MINIMAP_INTERIM_SEARCH_CHARS` (widget test
  `test_minimap_search_lane_is_kept_while_an_edit_rescans`, red on the stage-3
  tip). `origin/main` had not moved; no merge.
- **Decided (design "Stage 4 review and decisions").** The 131,072-marker
  find dispatch is not split here (equal to `main`; the minimap's own work is
  bounded by the widget budget test); a probe of the `GtkSourceMap` repaint on
  scroll is a **deferred follow-up** (next free axiom id), which also carries
  that split; `core-minimap-binning` stays `scheduled` until a dispatched CI
  run measures it within the pull-request margins; A102 is **recommended** for
  an upstream GtkSourceView report through `upstream-gtk-reports`, not filed
  (the maintainer's call).
- **Follow-ups, decided and open after the archive (2026-11-09).**
  - Promote `core-minimap-binning` from `scheduled` to `pull-request` only
    after a dispatched CI run measures it within the pull-request margins
    (`make check-kani-shards`); until then it runs weekly.
  - Probe the `GtkSourceMap` repaint on scroll (next free axiom id); it owns
    the scroll-frame exception above and, if the probe shows a split is
    possible, the 131,072-marker find dispatch.
  - A102's upstream GtkSourceView report stays pending: it is drafted only
    after the GNOME nightly re-test (queued after `adopt-gnome-51`) shows the
    superseded `forward_async` request still never returns.

## Axioms each change cites

| Change | Axioms |
|---|---|
| C1 `document-scale-policy` | A27 (tag changes do not dirty the buffer), A29 (context engine cut-off), A31 (2000 px validation batches, the source of R_batch), A32 (source-map detach CRITICAL); added by C1: A42 (paragraph delimiters and history-dependent buffer lines), A43 (`chars_in_line` cost, verified by source), A44 (same-emission language downgrade), A45 (unparent stops a pending tick) |
| C2 `long-line-protected-view` | A23, A24, A25, A26, A28, A30, A33, A34, A35; from C1, A42 (paragraph delimiters), A43 (`chars_in_line` cost), A44 (same-emission language downgrade), A45 (unparent stops a pending tick); added by C2 (its P1–P8): A59 (a capture-phase claim keeps presses from the view's gestures), A60 (non-editable: no input-method focus-in or key filtering), A61 (a stopped `move-cursor` holds under caret browsing), A62 (AT-SPI text and caret writes on a folded paragraph; lane-pinned like A35), A63 (one drop target, no drag source; removal), A64 (stopped key-binding signals and a captured Shift+F10 guard; action enablement does not), A65 (a hidden tail is never laid out; 13–17 ms per MiB insert), A66 (chunk-read cost by iterator positioning; the persistent byte-positioned iterator is flat) |
| C3 `sliced-protected-install` | A24, A35, A36, A53, A65; added by C3: A76 (a left-gravity mark keeps slices from merging; forward deletion cost), A77 (a hidden append re-lays out only visible text), A78 (AT payloads bounded by the run, extends A35), A79 (a point below the layout walks the last line's hidden bytes; GTK defect) |
| C4 `bound-window-construction-dispatch` | A38 (keyfile write per `set_*` on the calling thread), A39 (`delay`/`apply` coalescing), A40 (no backend lock; a second backend stays stale), A41 (`changed` timing) |
| C4c `shorten-window-content-fill-in` | A51 (a realize frame runs on the unmapped window, is never presented, and paces the first visible frame one `FRAME_INTERVAL` later), A46–A50 unchanged |
| C4b `first-frame-window-shell` | A46 (one-time first realize; small in a minimal window), A47 (`present` measures nothing; invisible subtrees skipped), A48 (a reveal is measured in the next frame), A49 (concealed vertical revealers measure a visible child), A50 (breakpoint before map vs the snapshot swap), A21 (`window-removed` teardown) |
| C5 `explore-editing-long-paragraphs` | A23–A28, A30, A32, A37, A42, A43, A45; added by C5: A52 (an unparented, alive text view keeps laying out its buffer), A53 (a paragraph wraps by its first character's tags), A54 (the view answers surrounding text from the paragraph start; its handler matched by signal and data; the Wayland module asks on every commit) |
| SC `spawn-completion-priority` | A31, A70, A81 (equal-priority co-dispatch; cited as A69, the context-engine row, until EV added A81); added by SC: A80 (a validating text view, on screen or on a hidden stack page, holds every lower-priority idle back until its whole buffer is laid out; a source at `GDK_PRIORITY_REDRAW + 4` runs before the first incremental batch) |
| EV `defer-background-tab-layout` | A31, A70, A80; added by EV: A81 (every ready source of the top priority runs in one iteration, so N validating views make an iteration N batches long), A82 (a never-selected `AdwTabView` page's source view validates its buffer while hidden: the scroller creates its layout), A83 (an empty buffer stops it at once and leaves the document untouched; a reattach re-lays out from nothing, back at the start; a scroll queued right after it lands elsewhere, re-scrolling on each `upper` change keeps it) |
| C6b `chunked-highlight-attach` | A27, A67, A68, A69, A70, A80, A83; added by C6b: A84 (the largest `highlight-updated` end since an insert or attach reaches the end of the last line's text once the context engine has analysed the buffer; a `G_PRIORITY_LOW + 1` idle always finds it there) |
| C6c `bound-context-flip-keystrokes` | A27, A69, A84; added by C6c: A85 (an edit's first update runs right after an idle queued at its priority before the edit and before a `G_PRIORITY_LOW - 1` idle; `ensure_highlight` on an empty range from a `G_PRIORITY_HIGH_IDLE - 1` idle re-analyses in steps whose report says whether it merged; removing the language between steps removes the first update and worker) |
| BL `bound-lists-by-budget-not-count` | A1 (realized-row cap), A10 (mapped is not drawn), A22 (synchronous expansion; the `is_expandable()` probe store); added by BL: A87 (a presented list view pays per `items-changed`, not per item; an unrealized one does not walk its items), A88 (an end-of-store batch splice under a tree list grows sub-linearly), A89 (a tail `set_expanded` grows sub-linearly in its position), A90 (revealing a list costs its realized rows), A91 (a row's collapse removes its children in one call whose cost grows with them; stage 4 review) |
| BN `bound-notes-browser-by-budget` | A1 (realized-row cap), A10 (mapped is not drawn), A87 (per `items-changed`, not per item); added by BN: A92 (`AdwSidebar` builds one row widget per item, off-screen ones included), A93 (one replacing `items-changed` makes the list's item manager, and a selection holding an item, ask for every added item; remove-then-add asks only around the realized rows; fresh row objects are accepted), A94 (section headers are not focus stops; the header's role is `row-header`), A95 (select-and-scroll to a far row realizes only around it; its time grows with the rows) |
| DP `drill-down-palette-categories` | A1 (realized-row cap), A87, A90 (revealing a list costs its realized rows), A93 (remove-then-add, and BN's retarget publish), A95 (far select-and-scroll); added by DP: A96 (a natural-height lazy list measures, allocates and publishes around its realized rows: 206 / 205 `get_item` calls at 512 and 131,072 items), A97 (`stop-search` on Escape with or without text, text kept; a capture controller sees Backspace before the entry, on an empty entry, and its release; a held key arrives as presses without releases) |
| BM `bin-minimap-markers-per-row` | A33 (`visible-only`), A42 (paragraph lines), A45 (map retirement), A1-style band-only projection unchanged; added by BM: A98 (a property-less tag is bookkeeping only; text inserted strictly inside a tagged range carries it), A99 (`line_at_y` on a map is monotone, skips hidden lines, flat cost), A100 (`forward` is 3 µs inside a completed scan, 74.5 ms before it; `occurrences-count` reads -1 after an edit and notifies on rescan), A101 (an overlay sibling of the map is not redrawn when the map scrolls), A102 (`forward_async` never blocks a dispatch, but a pending request superseded by a query change never returns and GIO logs a CRITICAL) |
| CF `scope-font-zoom-restyle` | A47 (hidden widgets take a class without a restyle until shown), A72 (why the font stays CSS, not a tag); added by CF: A86 (a display provider reload revalidates every widget even with unchanged text and grows with the window; an unmatched load changes no font; a per-widget class swap applies the new size and does not grow with unrelated widgets) |
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
| `Gtk-WARNING: Trying to snapshot GtkGizmo … without a current allocation`, the trough of the editor's horizontal or vertical scrollbar. **Fixed in C0** (pre-existing-blockers rule), so it is no longer owned by C1. Root cause, confirmed against GTK 4.22.5 and GtkSourceView 5.20.0 (ledger **A37**): `GtkTextView` validates invalidated lines from an idle at `GTK_PRIORITY_RESIZE - 2`, but a change made inside a frame — the key press of the smallest reproduction (one 500-character line, wrap off, Ctrl+Home and typing) is handled in the frame clock's flush-events phase — or just before a frame that input started (`gdkframeclockidle.c` runs it from `flush_idle` at `GDK_PRIORITY_EVENTS + 1`; find in dense and 1 MB documents, where GtkSourceView's search scan changes tags from an idle) leaves that idle pending at paint time. The view is not re-allocated in that frame's layout, so `gtk_text_view_paint` flushes the validation inside the snapshot (`gtk_text_view_flush_first_validate` → `gtk_text_view_update_adjustments`): a widened line or a queued scroll changes the scroll adjustments, `GtkRange` queues its trough's allocation, and `GtkScrolledWindow` snapshots the scrollbar after its child. Lines that fit the viewport never change `upper`, which is why they never warned. The `mark-set` handlers that run inside GtkSourceView's snapshot were ruled out: disabling LushText's Notes handler left the warning in place. Fix: `ui/editor_page/frame_validation.rs` queues the view's allocation on every edit and caret move, and before the paint (at `before-paint` or after the frame clock's layout handlers) after a tag change, so `gtk_text_view_size_allocate` flushes the validation in layout. Evidence: widget tests `editor_page::test_typing_in_a_long_line_keeps_adjustment_changes_out_of_the_snapshot` (20 adjustment changes inside the paint before the fix, 0 after) and `editor_page::test_tag_changes_in_frames_keep_adjustment_changes_out_of_the_snapshot` (8 without the tag handling, 0 with it), probe A37, and the lane: the minimal case and dense 500/2000/50000, total-size 1/5 MB and paragraph 500/15000/22000 (all phases) show no warning, nor do four more repetitions each of dense 500 and total-size 1 MB (before the fix dense 500 warned on find in 3 of 5 runs). The lane classifier now treats this warning as a **regression that fails the run** (`FIXED_DEFECTS` in `scripts/responsiveness-smoke-driver.py`), not as an owned finding. Residual: a size-changing tag that GtkSourceView applies inside its own snapshot (highlighting the visible region) is still validated in that paint; the editor validates it in the next frame's layout. **Observed once on 2026-10-13** (find in a 15,000-character wrapped paragraph beside heavy CPU load, `rr-20261013T210157Z-toolbx-1753909`) and not reproduced in 94 further runs; **explained and fixed 2026-10-14**: a second route, the overscroll idle's bottom-margin change (a default-style property: invalidates every line, queues nothing) just before a frame GTK started from its synthesized post-relayout motion, which redrew the view; `frame_validation.rs` now also queues the allocation on those properties' `notify` (see "Idle re-confirmation (2026-10-13)"). **Recurred 2026-10-30** in the C2 keystroke sweep (2 of 2 wrap-off cases at 500,000 characters, `coarse-500000` CSS rep 0 and prose rep 1), both in `place-middle` in the frame the Escape key press started (probe-correlated): a third route. Closing find disposes the `GtkSourceSearchContext`, whose dispose removes its match tag with `gtk_text_tag_table_remove` (`gtksourcesearchcontext.c`); that invalidates the tagged ranges and emits only the table's `tag-removed`, never `remove-tag`, and with search highlighting gated off by the document scale no `tag-changed` precedes it. On a paragraph past A28's 32-bit overflow the match tag changes the line's measured width (widget reproduction: `upper` 953 with the tag, 205,765 without), so the revalidation in the paint moved the horizontal adjustment. Fix: `frame_validation.rs` also queues the view's allocation on `tag-removed`, at once like an edit (deferring it to the frame's layout handlers, as tag changes are, left 4 of the 8). Evidence: `editor_page::test_tag_table_removals_in_frames_keep_adjustment_changes_out_of_the_snapshot` (8 adjustment changes inside the paint before, 0 after). A full search-bar reproduction at 500,000 characters did not fail headless before the fix, because the headless revealer collapses at once and allocates the view in the same frame; the lane's sliding revealer does not | fixed in C0; residual fixed; C2 recurrence fixed |



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
- Upstream reports (filed, commented on, or followed since 2026-11-04: see
  [`docs/upstream-reports.md`](../upstream-reports.md)): the `GtkSourceMap` `update_id` that survives
  `set_view(NULL)` (A32; fixed upstream in 5.22.0, a 5.20 backport request is
  drafted), Pango's int32 width overflow (A28), log attrs over
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

**Coordinator note at landing (2026-11-03).** In the re-verdict, `baseline/small/steady-populated` `type-during-settle` shows a worst-stall median of 4.9 ms on main against 12.2 ms on the branch, and a max of 16.6 ms against 20.8 ms. Both sides exceed B_settle in that phase; input-to-frame improved (19.4 → 18.1 ms), and no completion or hand-off exceeded 4.8 ms. The shift is **not attributed**. It is handed to the EV change (`defer-background-tab-layout`), which works on exactly the populated profile, to be attributed with perf and fixed or recorded. The design's A69 citation for equal-priority co-dispatch points at the context-engine row; EV should cite or add a general equal-priority co-dispatch axiom.

## AG `adopt-gnome-51` (implementation record, 2026-11-10; PR #91)

**Status:** merged as PR #91 (rebase merge, `main` `14649be0`) and archived
2026-11-11. `retest-upstream-fixes-on-nightly` is next. Design and decisions:
`openspec/changes/archive/2026-11-11-adopt-gnome-51/design.md`. After the
merge, `builder-diagnostics-runtime.yml` was dispatched on `main` `14649be0`
(run 43107260604) to publish the `gnome-51-debug` runtime image; it was still
in progress at the archive, so its outcome is not recorded here.

The Cominotti Flatpak moves to `org.gnome.Platform//51` (GTK 4.24.1,
GtkSourceView 5.22.0, Pango 1.58.2, Libadwaita 1.10.0) and the Rust pin to
1.98.0 (the SDK's `rust-stable//26.08`); CI tests on `fedora:45`, with a
`GTK Axioms (GNOME 50 floor)` job on `fedora:44`. GNOME 50 stays the source
floor. The bump found, and this change fixed at the cause:

- **L1, "typed text drawn late"** (100 withheld typing phases on `main`): not
  late text. GTK 4.24's reworked frame clock aligns a frame's frame time to its
  refresh grid (ledger **A105**: −8.2 to +8.1 ms from the frame's
  `before-paint`), and the lane driver matched keys to frames by frame time,
  charging a key to the next key's frame. The probe now records when each
  frame began (`began_us`) and the driver attributes by it (`frame_start_us`,
  with a self-test); `baseline/small` has 0 withheld phases on the branch.
  The `main` binary has no `began_us`, so its lane keeps the artifact.
- **W1** (1,786 slider warnings): GtkSourceView 5.22 leaves a resized map
  slider unallocated (ledger **A103**, an upstream defect, drafted in
  `docs/upstream-reports.md`); the minimap's one-pixel slider correction class
  now applies only below 5.22.
- **W2** (830 measure warnings): `GtkListView` is height-for-width on GTK 4.24
  (ledger **A104**, `!9906`); `ViewportSliceBin` measures its child's width for
  no height.
- **W3**: the excluded-names entry row was a typed `[row]` template child that
  replaced the `AdwExpanderRow`'s header and list on every Libadwaita
  (1.10 warns); it is now an untyped child, appended through `add_row`.
- The `FLAKY` minimap-marker test waited on an empty search.
- **V1** (8 `minimap-sidebar-mid` visual-geometry cases red on `main` and the
  branch): GtkSourceView 5.22 places the map's slider from the views' content
  heights, margins included (ledger **A106**), and the minimap copied the
  editor's end-of-document overscroll margin 1:1 onto its map, drawing the
  slider below the visible text. From 5.22 the margin is scaled by the map's
  text scale; a constant residual of about 4 px remains (the map's own 5 px top
  inset).

**L2 was pre-existing, and is fixed here:** `paragraph/10000/*` `type-start`
and `type-middle` stalled 1.2–1.3 s per key on GTK 4.22 too
(`power-of-two-thresholds/am14-main`). The first reading charged it to C2's
editable-paragraph floor; stack sampling put it in the five **Right** presses
before the typing instead: GTK's visual cursor step lists every cursor position
of the display line through Pango, quadratic in its length (ledger **A107**:
76 ms at 2,500 characters, 1,207 ms at 10,000), and the C5 wrap gate shows the
10,001-character paragraph unwrapped. The editor now answers a visual step with
a logical one, which lands on the same place on left-to-right text, when the
paragraph is longer than `L_LOGICAL_CURSOR_STEP` (256) and holds no
right-to-left text (`model/cursor_step.rs`, `ui/editor_page/cursor_steps.rs`).
Lane `paragraph/10000/*`: 0 misdirected; type-start 1.20–1.32 s → 7.4–13.1 ms,
type-middle 1.19–1.25 s → 1.4–6.9 ms. Each remaining caret step or keystroke
costs one whole-paragraph layout (6–10 ms at 10,000 characters), the toolkit
floor already recorded for editable paragraphs of 3,500–50,000 characters (C2,
C5). A Pango report is drafted (`docs/upstream-reports.md`). **L3**
(`baseline/empty` failed `no-editor-point` on every toolkit) was a driver
defect: the case opens no document; its editor interactions are now recorded
unavailable, and the lane fails on any failed case (`failed_cases`), which it
did not before.

Axioms A7, A20 and A32 are version-qualified, as are the new A103–A106
(catalogue `version_bound`,
`asserted_branch` in the observation, both ranges required by
`make check-gtk-axioms`).

**Verdict (interleaved quick tier, every case, GTK 4.24.1, 2026-11-11;
design "Measured verdict").** The branch passes the lane in both runs (0
withheld, 0 misdirected, 0 failed cases); `main` fails both (97 / 93 withheld,
36 misdirected). Baseline profiles: `B_settle` 11.2 / 14.0 ms (`main` 11.5 /
642.3), input-to-frame 62.8 / 61.7 ms (`main` 374 / 379), `B_interact`
indeterminate at 6.2 / 6.7 ms, scroll p95 8.55–8.57 ms on both sides. 188
figures go from over a limit to within it. **Recorded exceptions:**

- **E1** `paragraph/10000/*` `find` 8.1–10.5 ms (`main` 5.8–7.9): one
  whole-paragraph layout, the C2/C5 floor, which `main`'s 1.2 s visual arrow
  steps had already paid before the find. Measured in the stage-4 review: with
  only the find phase (no typing before it), interleaved, the branch's find is
  no slower than `main`'s (median 4.1 / 4.4 against 4.8 / 4.6 ms, maximum 8.7 /
  7.7 against 14.2 / 8.0 ms); accepted as a recorded exception.
- **E2** seven single-sample figures at the budget edge, within one reference
  frame of `main` (dense scroll and typing, hidden-tab switch, re-analysis
  scroll, one 26 ms detach-minimap open).
- **E3** 99 input-to-frame figures of 16.7–20.0 ms over the limit on the
  branch only: the lane's double-buffered floor; `main` reads lower only
  through the L1 frame-time artifact.
- **E4** 15 input-to-frame maxima more than a frame worse than `main`, all
  single samples in classes already over on `main` (minimap/invisibles
  scroll, `AdwDialog` inspect, Protected View keys, one session Page Down).
- **E5** the A67 language attach landing in a different phase.
- **E6** everything else (179 dispatch, 254 input-to-frame figures) is over on
  `main` too, within a frame of it, and owned by C2/C5, C2b, C6, C6c, DL, EV
  or SC.

**Stage-4 review (2026-11-11).** One should-fix, fixed failing-first: the
logical arrow step was also taken in a long paragraph with no strong character
(dashes, digits) shown by a right-to-left view, where GTK lays it out right to
left and Right moved the caret backwards; the step now also requires Pango's
base direction to be left to right (A107 restated). Decisions: E1 accepted with
its measured attribution; `GTK Axioms (GNOME 50 floor)` should become a
required check (maintainer applies the setting); the A103 and A107 upstream
reports should be filed (maintainer approval per report).

The `main`@1.98.1 against `main`@1.98.0 pair is context only (maintainer
decision, 2026-11-11): its one usable pair shows a median per-phase delta of
+0.08 ms and no attributable compiler effect. GTK 4.22 → 4.24 on `main`'s
dispatch figures is within noise (medians ±1 ms; context table in the design).

