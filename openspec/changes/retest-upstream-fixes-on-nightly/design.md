## Context

`docs/upstream-reports.md` records five upstream items whose maintainers
answered with a fix or a request to re-test. It also records two drafted,
unfiled reports. A third candidate (A102) waits on this re-test. The ledger
pins every behaviour with a probe in `gtk-lush-axioms`, plus one sample each.
`make gtk-axioms-runtimes` (`scripts/run-gtk-axioms.sh runtimes`) builds the
samples inside each installed GNOME SDK and runs every `--check` against that
SDK's toolkit. It is a local-only lane, and the tool this re-test is built on.

The ids, corrected:

| Issue | Axiom | Our workaround |
|---|---|---|
| gtk#8528 | A79 | Protected View's stand-in `"\n"` terminator (`ui::protected_fold::document_end_iter`) |
| gtk#8529 | A54 (the cache cost: A26) | the bounded surrounding-text answer (`ui/editor_page/surrounding_text.rs`) |
| gtksourceview#432 | **A67** (recorded as A52 in `docs/upstream-reports.md` and in the task text; A52 is the unparented-view axiom) | C6 D2's toolkit-floor exception: the first attach runs at `G_PRIORITY_LOW` after input-quiet |
| pango#582 | A28 | wrap-off paragraphs past the overflow length are unrenderable (C1) |
| context-engine guard | A29 | syntax off before the engine sees a giant paragraph (C1) |
| drafts | A102, A103, A107 | `bin-minimap-markers-per-row` D5; `adopt-gnome-51` W1; `cursor_steps.rs` |

## Measured baseline (spec stage, `origin/main` `d3c0bca7`, 2026-11-11)

Host: Fedora 45 toolbox. Evidence:
`.claude/handoffs/retest-upstream-fixes-on-nightly/baseline-runtimes.out` and
`baseline-measure.log`. One `scripts/measure.sh` run of
`scripts/run-gtk-axioms.sh runtimes org.gnome.Sdk//master org.gnome.Sdk//51`
with slow samples included. The load was 1.64 before and 1.03 after, and the
run took 1,030 s, A29's stopped run included. This is **one run, not
interleaved**. These numbers decide the scope only, not a verdict.

### B1. What the nightly carries

The installed `org.gnome.Sdk//master` is commit `64e223c7…`, built
2026-11-11 05:29:50 UTC ("49-branchpoint-1458-g286a1d4c1"). A newer build
exists on `gnome-nightly`: `469c3f9d…`, 16:43:22 UTC. Its `/usr/manifest.json`
lists these sources:

| Module | Source commit | `pkg-config` |
|---|---|---|
| gtk | `4.25.0-90-g1c64c1de` (2026-11-10) | 4.25.1 |
| pango | `1.58.2-51-g6bd640fd` (2026-11-09) | 1.59.0 |
| gtksourceview | `5.22.0-64-g9f23f713` (2026-11-06) | 5.23.0 |
| libadwaita | `1.10.0-37-g39fff666` | 1.11.alpha |
| glib | `2.90.1-185-g107dde15` | 2.91.0 |

Containment was checked with GitLab `repository/merge_base` (the fix is
contained when the merge base equals the fix):

| Fix | Commit | In nightly | In a tag |
|---|---|---|---|
| gtk !10651 "Reduce chances GtkTextView will cause long stalls" (`790f6a8d` validation time limit, `b390d1e5` tag-toggle search limit) | merge `7c578d79` | yes | `4.25.0` |
| gtk !10646 "update changed stamp for word-boundary cache" | merge `cb73e884` | yes | `4.25.0` |
| gtk !10228 "textiter: stop visible line index scan at line boundary" | — | **open, not merged** | — |
| pango `f60f461` "layout: avoid repeated work when wrapping large layouts" | `f60f461a` | yes | none |
| gtksourceview !465 "prevalidate syntax files at compile time" | landed rebased as `4f966029`…`a6af0874` (the MR's `sha` `3e34734f` is **not** an ancestor) | yes | none (master gained 5.24 version macros on 2026-11-06) |
| gtksourceview `aa6a36a3` (A29 guard) and !463 `1899b981` | — | yes | none; the `gtksourceview-5-22` branch has only `78832d75` and `6959237d` since 2026-11-01 |

So none of the fixes is in GNOME 51 (GTK 4.24.1, Pango 1.58.2,
GtkSourceView 5.22.0) or in GNOME 50. Pango's `f60f461` message is about
**wrapping** cost ("keep track of the unconsumed width"). It does not touch
the `int` x-position arithmetic behind #582, and B2 agrees.

### B2. Target samples (nightly vs GNOME 51 SDK, one run)

| Axiom | Nightly (GTK 4.25.1, Pango 1.59.0, GSV 5.23.0) | GNOME 51 (GTK 4.24.1, Pango 1.58.2, GSV 5.22.0) |
|---|---|---|
| A79 lookup below the layout, hidden 1/2/4 MiB | holds: 82.5 / 158.9 / 320.5 ms; delimited 11 / 11 / 12 µs | holds: 78.3 / 152.0 / 296.8 ms; delimited 5 / 3 / 8 µs |
| A28 x-position overflow | holds: advance 8,220 units, overflow at 261,252 characters (x 2,147,483,220 → −2,147,475,856) | holds (ledger, host: 238,319 characters with the host font's 9,011-unit advance) |
| A26 word end on a 300,000-character paragraph (first / repeat / after an edit elsewhere) | holds: 13.2 ms / 0 / 13.1 ms | holds: 14.9 ms / 0 / 14.7 ms |
| A54 surrounding text | holds: view answers 20,000 of 20,000 characters; later handler 0 calls | holds: same |
| A67 first attach html / markdown / json / rust | holds: **12.7 / 12.8 / 8.5** / 1.1 ms | holds: 32.4 / 33.1 / 21.2 / 2.9 ms |
| A29 context-engine cut-off | **no verdict**: the sample ran over 600 s at 100% CPU and was stopped (exit 143) | holds: cut-off at 65,536 characters (2.0005 s), both CRITICALs |
| A102 superseded `forward_async` | holds: callback never ran, 1 CRITICAL; far walk 98.9 ms, worst dispatch 66 µs | holds: same; 99.6 ms, 60 µs |
| A103 map slider | holds: slider resize 1 warning, margin class 1 | holds: same |
| A107 visual step | holds: short 76.9 ms, long 1,208 ms, logical 2.4 ms | holds: 77.5 / 1,207 / 2.5 ms |

The maintainer measured A67 at 19/16/11 ms on his machine. On this host the
nightly is **under `B_SETTLE_US` (16 ms) for all three** languages, and still
over `B_INTERACT_US` (8 ms) for html and markdown. A79's per-lookup cost is
about 1.8× the ledger's host figures (44/88/175 ms) in both SDKs. That is the
same sample built in the SDK, so it is the SDK build, not a toolkit change,
and the comparison has to be within a run, which is why the rounds are
interleaved (D6).

### B3. Pre-existing lane blockers found

| Id | What | Where |
|---|---|---|
| V1 | A29's fixture is bounded only by length (cap 16,777,216 characters). On nightly the doubling did not reach the cut-off and ran unbounded in wall time | nightly |
| V2 | A23 fails: `paragraph_height` 6,627 against `last_char_bottom` 6,628 (line height 16, 427 display lines, so the line positions are fractional). `.agents/rules/build.md` records this as "renders A23 with the SDK's fonts; read those verdicts from the host" | GNOME 51 **and** nightly SDKs |
| V3 | A59, A60, A61, A64 and A97 report `fixture-invalid`, because the SDK sandbox runs without a session bus and so has no RemoteDesktop. Recorded as a known caveat | both SDKs |
| V4 | `runtimes` runs every sample; there is no way to run only the affected ones for interleaved rounds | lane |

Because of V2 and V3, `make gtk-axioms-runtimes` has never exited 0 on any
SDK. A lane that always fails cannot tell a new failure from an old one, so
these fall under the pre-existing-blockers rule.

### B4. Nightly-only verdict changes (not targets)

A4, A70, A74, A83, A85 and A86 hold on GNOME 51 and fail on nightly. A sample
of what changed:

- A4: `scroll_to` now writes the adjustment when it is called.
- A70: back-to-back inserts are no longer validated by one dispatch.
- A85: GtkSourceView's first update is now 0–39 µs, with no
  `highlight-updated` after the language is removed. On GNOME 51 the flip's
  first update was 14 ms.

These fit GtkSourceView's 2026-11-06 work (`highlight: limit work before
painting`, `deadline`, `tagplan`) and GTK's frame-clock/text-view changes.
None of it is attributed yet.

## Goals / Non-Goals

**Goals:**

- Answer the maintainers' questions with interleaved, versioned measurements
  on the newest nightly, and with the upstream-facing reproducers they can
  run themselves.
- Make `make gtk-axioms-runtimes` give a readable, per-SDK verdict that
  passes on the shipped SDK, and run a subset on demand.
- State each workaround's retirement condition as a version floor plus a
  measurement.
- Draft new upstream reports only for defects that still reproduce on the
  nightly.

**Non-Goals:**

- Posting anything upstream. Filing and commenting are the maintainer's
  decision, one item at a time.
- Restating any axiom for nightly behaviour. Nightly is not a supported
  toolkit, and the ledger rule restates only after attribution and review
  (B4 goes to the next GNOME adoption).
- Building GTK with the still-open !10228. The facts sheet says the A79
  sample would answer it; the maintainer decides whether to ask.
- Production code changes, unless D7 triggers.

## Decisions

### D1. Facts sheets, not drafted replies

The task asks for "short, human-sounding replies" for gtk#8528 and
pango#582. We do not draft them. Instead each folder gets
`retest-facts.md`, which lists:

- the measured numbers;
- the exact versions and source commits;
- the commands that produced them;
- the reproducer output files;
- the open questions the maintainer may want to answer.

The maintainer writes the reply from that.

Rationale: the `upstream-gtk-reports` skill makes this a hard rule:
"Replies to maintainer feedback are the user's to write: do not draft them
… at most list the facts the user may want to check". GTK's `CONTRIBUTING.md`
(fetched 2026-11-11) says "Do NOT feed the review feedback to an LLM/GenAI
tool" and "Reject comments and feedback that appear to be LLM/GenAI output".
Asking for "human-sounding" text would also disguise AI help, which the
skill's lessons file records as the failure that drew a maintainer's
reproach on gtk#8529. Christian Hergert maintains all three projects, so the
same rule is applied to pango#582, gtksourceview#432 and gtk#8529 (whose
maintainer pointed at !10646 without asking for a re-test; its facts sheet
carries D4's numbers in case he asks). Alternative
considered: draft replies with a disclosure line. Rejected, because it
contradicts both the skill's hard rule and GTK's review rule, and the
facts carry the value either way.

### D2. Update the nightly first, and pin what was measured

The first task updates `org.gnome.Sdk//master`; `--user` is approved.
`rust-stable//26.08` is already installed. The task records:

- the build commit, date and subject;
- the module commits from `/usr/manifest.json`;
- `merge_base` containment for each fix, recomputed against the new
  build's commits.

Every result names that build. The token in `~/Tokens/gitlab.txt` is read
only inside commands and never printed. If the newer build no longer contains
a fix (a revert), that is recorded and the fix is treated as absent.

### D3. Fix the runtimes lane at its causes, failing first

- **V4 (`--only`).** `runtimes [--only A28,A79,…] [<sdk-ref>…]` checks only
  the named samples. An unknown id fails the run, as `sample` already does.
  The self-test covers id parsing and the count check.
- **V3 (RemoteDesktop).** The runtimes mode passes
  `GTK_LUSH_AXIOMS_SANDBOX=1` into the sandbox. A sample whose fixture needs
  RemoteDesktop checks that flag **before** connecting, prints
  `skipped (needs RemoteDesktop): <id>` and exits 0. `count_sample_results`
  counts the line like the slow skip. The host lane never sets the flag, so a
  host compositor that lacks RemoteDesktop still fails as `fixture-invalid`.
  Rejected alternative: give the sandbox a session bus. That brings back the
  `xdg-dbus-proxy` broken-pipe noise the sandbox flags were added to remove.
- **V2 (A23).** Attribute first: the final pixel row with the SDK's font,
  which line height and which rounding. Then make the check structural:
  - the last character's rectangle lies inside the paragraph's last display
    line band, `[height − line_height, height]`, allowing at most the one
    pixel of rounding that fractional line positions produce;
  - and its display line index equals the last one.

  The ledger row is reviewed before the probe is edited (ledger rule). A unit
  test of the predicate over the recorded 6,627/6,628 pair fails first.
- **V1 (A29).** Next to the length cap, the fixture gets a fixed wall
  budget of 60 s for its attempts. It does not **start** an attempt that its
  recorded attempts predict would end past the budget: the prediction is the
  cumulative time so far plus the last attempt's duration times the larger
  of 4 and the last growth ratio (analysis of this markup grows about
  quadratically, so doubling the line costs about 4×; the GNOME 51 SDK
  recorded 0.11 → 0.42 → 1.66 → 6.57 s, ratios 3.7–4.0). **Amended at implementation (2026-11-11):** the prediction carries a
  5/4 margin. On the nightly (build `469c3f9d`) the last growth steepened
  from 3.96× to 4.95× (10.45 s → 51.7 s), so the unmargined rule admitted
  the 131,072-character attempt and the doubling ended at 65.7 s, past the
  budget the spec requires a run to end within. With the margin the nightly
  stops after 14.0 s; GNOME 51's prediction before its last attempt is
  8.8 s + 5 × 6.57 s ≈ 41.7 s, still inside 60 s. The nightly sequence is a
  failing-first test case. It then reports
  `FixtureInvalid` with every attempt recorded, as the spec already requires
  for the length cap. Runner speed still cannot flip a verdict to
  `Violated`. A pure test of the stop rule over recorded attempt sequences
  fails first, and it includes GNOME 51's recorded sequence, which must
  **continue** to its cut-off.

  Rejected during spec review: stopping once one call passes twice the
  2000 ms cut-off (4000 ms) without the CRITICAL. The baseline refutes it:
  in the GNOME 51 SDK the 32,768-character call ran **6.57 s with no
  cut-off**, and the 65,536-character call then cut off at 2.0005 s, while
  on the host the 32,768-character call cut off at 2.0006 s. So the probe's
  call duration is not what the engine's 2000 ms timer measures; task 2.8
  attributes what that timer covers before the probe is edited. That rule would have made
  A29 `FixtureInvalid` on the shipped toolkit, and in the pull-request
  `GTK Axioms` job, where A29 runs because it is not marked slow. What nightly actually does to the cut-off
  is then measured (D6). If the cut-off no longer fires there, that is a B4
  item for the next adoption, and A29's floor statement is unchanged.

### D4. A54 records the cost !10646 changes

The A54 axiom is about the answer's extent, and the nightly answer is still
the whole paragraph. So the bound stays whatever the cost does. GTK !10646
changes the **cost** of answering after an edit (the word-boundary cache's
stamp). The probe gains recorded-only values: the time of one
`retrieve-surrounding` emission on a 50,000-character paragraph, cold, and
right after a one-character insert, as the median of 5. Durations never
decide the verdict (spec). These are the numbers gtk#8529's facts sheet
needs, and the re-measure condition of C5's keystroke exception.

A26 already measures the cache !10646 touches: "a character change anywhere
invalidates the cached attrs" (`chars_changed_stamp`), with
`word_end_after_edit_elsewhere_us` on a 300,000-character paragraph as a
recorded value. Added during spec review, A26 joins the re-test inventory
(D6, the ledger notes and gtk#8529's facts sheet). The baseline shows the
recomputation still happens on nightly: 13.2 ms first call, 13.1 ms after
an edit elsewhere (GNOME 51: 14.9 / 14.7 ms). Its statement is about the
cache's invalidation, so if a round shows the after-edit cost dropping to
the repeat cost (0 µs), that is a B4 preview, not a restatement.

### D5. New upstream drafts only where the nightly still reproduces

For each of A102 (GtkSourceView), A103 (GtkSourceView, an existing draft)
and A107 (Pango):

- confirm on the newest nightly with interleaved rounds, then rerun the
  standalone C or PyGObject reproducer inside the nightly SDK;
- search the tracker (issues and MRs, several phrasings, open and closed);
- read the touched file's current source and recent commits on the default
  branch;
- fetch the project's issue template and CONTRIBUTING AI policy.

Drafts follow the skill's folder layout (`report.md` with the title line,
the reproducer, `run.sh`, `output.txt`) and end with the disclosure line in
the maintainer's voice. They use no LushText vocabulary. The existing A103
`draft.md` is refreshed into that layout. A candidate that no longer
reproduces, or is already reported or fixed, is dropped and recorded in
"Followed, not filed". The baseline shows all three reproduce. The skill's
Workflow is **not** run, because this stage forbids sub-agents. Its steps
1–4 are done inline, sequentially, and its review step is a second pass
re-reading against the fetched template.

### D6. Measurement protocol

Rounds:

- Affected samples only (`--only`): A79, A28, A26, A54, A67, A29, A102,
  A103, A107.
- Four rounds at most. Each round runs nightly then GNOME 51, and the order
  alternates between rounds (N,51 / 51,N / …).
- Each round goes through `scripts/measure.sh` with the load before and
  after recorded, and starts only at a 1-minute load below 4.
- Medians and ranges across rounds are reported.

Structural verdicts must agree across rounds; a disagreement is a flake
under the pre-existing-blockers rule. The upstream reproducers (`repro.c`,
`repro.py`) run inside each SDK under a private `mutter --headless`, on CPUs
8–23.

### D7. Production code only if GNOME 51 makes a workaround wrong

None of the fixes is in GNOME 51 (B1), so no workaround's premise changes on
the shipped toolkit. If a GNOME 51 run disagrees with a workaround's premise
anyway, the consumer test is made to fail first, then the code is fixed in
this change. The PR then stops being docs-only.

### D8. No Kani shard reruns

No pure policy, budget constant, or proved module changes. The probe and
script edits sit outside every harness's call graph. If D7 triggers and
touches a proved module, its shard reruns under `scripts/heavy.sh`, one CBMC
at a time.

### D9. Records and retirement conditions

Each workaround's "When resolved" cell names the first release that carries
the fix as a floor condition. A row that has no such release names the
commit and says so. The intended content, to be confirmed by D6:

- **A79:** unchanged on a nightly with !10651. The stand-in terminator stays
  until a GTK release containing !10228, or an equivalent, is the floor **and**
  the A79 probe's last-line lookup is flat.
- **A54:** the answer is still the whole paragraph, so the bound stays on
  every version. Once GTK ≥ 4.26 (the first release with !10646) is the
  floor, re-measure C5's keystroke exception with D4's numbers.
- **A67:** once GtkSourceView ≥ 5.24, the first release with !465, is the
  shipped runtime **and** the floor, re-measure the first attach. If html,
  markdown and json are ≤ `B_SETTLE_US` on the lane, lift C6 D2's
  toolkit-floor exception. The attach stays at `G_PRIORITY_LOW` after
  input-quiet, because it still exceeds `B_INTERACT_US`.
- **A28:** `f60f461` does not fix the overflow. The workaround stays.
- **A29:** the guard is in no release and not on `gtksourceview-5-22`. The
  workaround is unrelated to the guard and stays.

The ledger rows get a nightly "Verified against" note, labelled as a preview
build, never as a supported toolkit (the spec delta). B4 items go to the
programme record under the next GNOME adoption. `docs/upstream-reports.md`
"Last checked" moves to the run date. The `A52` mislabel is corrected to
`A67`.

The ledger's "Pinned by" cells for A67–A73 all name the probe of the axiom
fifteen ids lower (`probe_a52` … `probe_a58`), found during spec review; the
catalogue registers `probe_a67` … `probe_a73`, which `check-gtk-axioms.py`
already enforces for the catalogue but not for the ledger cell. The checker
gains that rule (a `gtk_lush_axioms::probe_aNN` in a row's "Pinned by" must
be the row's own probe), with a self-test case; run against the current
ledger it fails on exactly A67–A73 first, then the seven cells are
corrected.

## Risks / Trade-offs

- **[Nightly moves daily]:** every number names its build commit, and the
  facts sheets say which build.
- **[Sandbox skip hides a real failure]:** the flag is set only by the
  runtimes mode. The host lane and CI never set it, and they keep running
  A59–A61, A64 and A97 for real.
- **[A23 tolerance masks a real change]:** the one-pixel rounding rule is
  derived from the attributed fractional line height. The display-line-index
  check stays exact.
- **[A29 wall bound and runner speed]:** the bound only ever yields
  `FixtureInvalid`, never `Violated`, so a slow runner cannot flip the
  verdict. GNOME 51's attempts take about 10.8 s in total, and the
  prediction before its last attempt is 8.8 s + 5 × 6.57 s ≈ 41.7 s (with
  the implementation's 5/4 margin), inside 60 s; the GNOME 51 and 50 SDKs and
  the Fedora 45 host all reached the cut-off at 65,536 characters in
  10.5–10.8 s on 2026-11-11. A slower runner is expected to reach the cut-off at a shorter line,
  so its sum stays about the same; task 2.9 records A29's attempts on the
  host and both SDKs to confirm the margin, because CI's `GTK Axioms` job
  runs A29. A runner whose prediction exceeds 60 s before the
  cut-off reports `FixtureInvalid`, never `Violated`.

## Migration Plan

Docs, probes and script only. Nothing ships to users. Rollback is a revert.

## Implementation decisions (2026-11-11)

- **I1, A29 margin** (D3 V1 amended above): a 5/4 margin on the predicted
  growth, so a run ends within the budget the spec states; measured on the
  nightly's own sequence.
- **I2, A107 dropped as a new report:** Pango's tracker already has
  GNOME/pango#763 (open since 2023, the same `pango_layout_line_get_cursors`
  → `pango_layout_get_cursor_pos` chain in its backtrace), and a maintainer
  answered there and in #762 that PangoLayout is not meant for arbitrarily
  long paragraphs. Our numbers add nothing the issue asks for, so no comment
  either; recorded under "Followed, not filed".
- **I3, a second mislabel:** the `docs/upstream-reports.md` "Followed" row on
  GtkSourceView's fixed idle slices cited A54; the slice axiom is A69 (the
  same fifteen-id offset as the ledger's A67–A73 cells). Corrected; on the
  nightly, which has !463, A69 holds.
- **I4, gtk#8529's reproducer on GTK 4.24.1:** the filed `repro.c` stops at
  its own setup on GTK 4.24.1 (its input method is not created before it
  measures; also on the Fedora 45 host), and runs on the nightly. It is not
  edited, because it is attached upstream; A54's recorded costs give the 4.24.1
  numbers, and the facts sheet says how the reproducer would wait instead.
- **I5, A29 on the nightly:** `FixtureInvalid` in every round, consistently
  (not a flake): analysis is faster and never reaches the 2000 ms deadline
  (still in `analyze_line` on master) within the budget. A preview item for
  the next adoption, like B4; A29's statement is unchanged.
- **I6, no production code, no Kani shard (D7, D8):** every GNOME 51 run agrees
  with each workaround's premise (A79 linear, A54 whole paragraph, A67 over
  `B_INTERACT_US`, A28 overflow, A29 cut-off, A102, A103, A107), so nothing in
  `lushtext-core` changes and no proved module is touched.

## Open Questions

None blocking. Whether to ask for !10228 to be tested is the maintainer's
call, and the gtk#8528 facts sheet lists it.
