Rules for every task:

- **Headless only.** Never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`. Samples run only with `--check`,
  and reproducers only under a private `mutter --headless`.
- **CPUs, memory and helpers.**
  - Use CPUs 8–23 (`taskset -c 8-23`).
  - Heavy work (builds, `make gtk-axioms`, `make check`, `git commit`) goes
    through `scripts/heavy.sh`.
  - Timing runs go through `scripts/measure.sh`.
  - Waits go through `scripts/wait-for.sh`.
  - Never `pgrep -f`, never `find /`.
- **Measurements.** Run only at a 1-minute load below 4 (wait if it is
  higher), and record the load before and after each run. Use at most four
  interleaved rounds (nightly, GNOME 51, order alternating), on the affected
  samples only (design D6).
- **Evidence** goes in `.claude/handoffs/retest-upstream-fixes-on-nightly/`.
- **Token.** `~/Tokens/gitlab.txt` is read only inside commands and never
  printed or echoed.
- **Nothing is posted upstream.** Filing and commenting are the maintainer's
  decision, one item at a time.
- **Failing first** means the test, self-test or probe check is committed,
  or recorded red, before the change that turns it green.
- **Never relax a check** beyond what an attribution justifies. A probe is
  edited only after its ledger row is reviewed (ledger rule).

## 1. Update and pin the nightly (design D2)

- [x] 1.1 Record the installed `org.gnome.Sdk//master` (`flatpak info --user`).
  Then `flatpak update --user org.gnome.Sdk//master`, and update
  `org.gnome.Platform//master` too if installed. Record the new build
  commit, date and subject, and that `org.freedesktop.Sdk.Extension.rust-stable//26.08`
  is present.
- [x] 1.2 Copy the SDK's `/usr/manifest.json` module commits for gtk, pango,
  gtksourceview, libadwaita and glib, and the `pkg-config --modversion` of
  each, into `nightly-build.md`.
- [x] 1.3 Recompute GitLab `merge_base` containment against the new build's
  commits, and the first containing tag, for:
  - gtk !10651 (`7c578d79`), !10646 (`cb73e884`), and !10228 (expected
    still open);
  - pango `f60f461`;
  - gtksourceview !465 as landed (`4f966029`…`a6af0874`), `aa6a36a3`, and
    !463 (`1899b981`).

  Also record the `gtksourceview-5-22` branch's commits since 2026-11-01. If
  a fix is no longer contained, mark it absent and carry that into every
  later record.

## 2. Make the runtimes lane answer (design D3, failing first)

- [x] 2.1 V4: add the self-test cases for `runtimes --only`: id parsing,
  unknown-id failure, and the expected-count check. Run
  `scripts/run-gtk-axioms.sh --self-test` and record it red.
- [x] 2.2 V4: implement `--only <ids>` in `run_runtimes`, building and
  checking only the named examples. The self-test turns green.
- [x] 2.3 V3: add the self-test case: a `skipped (needs RemoteDesktop): A59`
  line counts as a result and is not a warning. Record it red.
- [x] 2.4 V3: pass `GTK_LUSH_AXIOMS_SANDBOX=1` in the runtimes mode only.
  Make the RemoteDesktop fixture helper (`crates/gtk-lush/axioms/src/probes/remote_input.rs`, used by A59, A60, A61, A64 and A97)
  print the skip line and exit 0 when the variable is set, before
  connecting. Confirm the host `make gtk-axioms` still fails those samples
  when RemoteDesktop is unavailable: run one sample with the RemoteDesktop
  name made unreachable, or record the source path showing the host never
  sets the variable.
- [x] 2.5 Print the per-SDK summary (id, verdict or skip reason, toolkit
  versions) after each SDK.
- [x] 2.6 V2 (A23): attribute the 6,627/6,628 mismatch in the GNOME 51 SDK.
  Record the font, the display line positions of the last two lines, the
  line height, and the rounding of `y + height`. Review the A23 ledger row
  and its dependents (C1/C5 designs citing A23).
- [x] 2.7 V2: add a pure unit test of the A23 geometry predicate over the
  recorded SDK values (and the host's equal values) that fails with the
  current exact-equality check. Then implement the structural predicate:
  the last display line index plus a bottom edge within the attributed
  rounding. The sample holds on the host, GNOME 51 and GNOME 50 SDKs.
- [x] 2.8 V1 (A29): first attribute why the GNOME 51 SDK's 32,768-character
  call ran 6.57 s without the cut-off while the host's cut off at 2.0006 s:
  read what `MAX_TIME_FOR_ONE_LINE`'s timer covers in GtkSourceView 5.22's
  `gtksourcecontextengine.c` (`update_syntax`) against what the probe's
  `time_us` covers, and review the A29 ledger row ("in one call") before
  the probe is edited. Then add a pure unit test of the stop rule over
  recorded attempt sequences that fails first:
  - a sequence whose prediction (cumulative time plus the last attempt times
    the larger of 4 and the last growth ratio) passes the 60 s budget stops
    before the next attempt, as `FixtureInvalid`;
  - GNOME 51's recorded sequence (0.112 / 0.418 / 1.661 / 6.574 s, then the
    cut-off) continues to its 65,536-character attempt;
  - the host's ledger sequence (0.125 / 0.464 / 1.84 s, then the cut-off)
    continues.

  Implement the predicted-wall-budget rule only (no single-call bound,
  design D3 V1), reporting `FixtureInvalid` with every attempt recorded. The
  A29 sample still holds on the host and the GNOME 51 SDK, with its recorded
  attempts unchanged in shape, and on nightly it ends within the budget.
- [x] 2.9 Run `make check-gtk-axioms` and `make check-agent-helpers` (shellcheck).
  Then run `make gtk-axioms` on the host (Fedora 45, GTK 4.24.1) and
  `scripts/run-gtk-axioms.sh runtimes org.gnome.Sdk//50 org.gnome.Sdk//51`.
  Both SDK summaries must have no violated or fixture-invalid sample. Record
  the added `make gtk-axioms` wall time against the pull-request job's cap,
  and A29's recorded attempts on each run (the stop rule's margin under
  60 s; CI's `GTK Axioms` job runs A29 because it is not slow).
- [x] 2.10 Ledger probe names (design D9): add the `check-gtk-axioms.py`
  self-test case "a Pinned by cell naming another axiom's probe" and the rule
  that every `gtk_lush_axioms::probe_aNN` in a row's "Pinned by" is the row's
  own probe. Record `make check-gtk-axioms` red on exactly A67–A73 (which
  name `probe_a52` … `probe_a58`), then correct the seven cells and record it
  green.

## 3. A54 records the cost !10646 changes (design D4)

- [x] 3.1 Add the recorded-only values to `probe_a54`: the median of 5
  `retrieve-surrounding` emissions on a 50,000-character paragraph, cold and
  right after a one-character insert. The verdict stays structural. Update
  the sample's printed values and the ledger row's probe text.

## 4. Interleaved re-test rounds (design D6)

- [x] 4.1 Run up to four rounds of
  `scripts/measure.sh -- scripts/run-gtk-axioms.sh runtimes --only A26,A28,A29,A54,A67,A79,A102,A103,A107 <first> <second>`,
  with the SDK order alternating (master,51 / 51,master / …). Slow samples
  are included. Keep each round's output and the measure log.
- [x] 4.2 Tabulate medians and ranges per SDK:
  - A79 lookups at 1/2/4 MiB, and the delimited lookup;
  - A28's advance and overflow index;
  - A26's first, repeat and after-edit-elsewhere word-end costs;
  - A54's cold and after-edit costs;
  - A67's first attach per language, against `B_SETTLE_US`, `B_INTERACT_US`
    and the maintainer's 19/16/11 ms;
  - A29's attempts and whether the cut-off and the second CRITICAL fire on
    nightly;
  - A102's callback and CRITICAL count;
  - A103's warnings;
  - A107's step costs.

  Any structural verdict that differs across rounds is a flake. Fix it
  before going on.
- [x] 4.3 Rerun the upstream reproducers inside the nightly SDK and the GNOME
  51 SDK under a private `mutter --headless`, saving `output-nightly.txt` and
  `output-gnome51.txt` beside each one:
  - `gtk-textview-iter-below-layout-walks-hidden-bytes/repro.c`;
  - `gtktextview-surrounding-text-whole-paragraph/repro.c`;
  - `comment-pango-582/repro.py`;
  - `gtksourceview-language-parse-blocks-main-loop/repro.c`.

  Never overwrite the folders' original `output.txt`.
- [x] 4.4 Run the nightly-wide summary once with
  `scripts/run-gtk-axioms.sh runtimes org.gnome.Sdk//master`. Record every
  sample that fails only on the nightly (expected from the baseline: A4,
  A70, A74, A83, A85, A86; A23 should now hold) with its failed check and
  the build commit, and confirm each holds on the host and the GNOME 51 and
  GNOME 50 SDK runs of 2.9. Attribute none of them.

## 5. Records (design D9; `.agents/rules/documentation.md`)

- [x] 5.1 `docs/upstream-reports.md`:
  - each re-tested row gets its result, the build commit, containment, the
    first containing tag, and a precise "When resolved" condition (floor
    version, measurement and budget, what stays);
  - the gtksourceview#432 row's `A52` becomes `A67`;
  - the A29 and A54 "Followed" rows are updated;
  - the A102 item is added under "Drafted, not filed" (or "Followed, not
    filed" if it was dropped);
  - "Last checked" moves to the run date;
  - the intro paragraph no longer says the re-test is queued.
- [x] 5.2 Axiom ledger:
  - A26, A28, A29, A54, A67, A79, A102, A103 and A107 get the nightly
    "Verified against" note, labelled as a preview build;
  - A23 and A29 rows describe the reviewed checks;
  - A54 lists the new recorded values;
  - the A67–A73 "Pinned by" cells name their own probes (task 2.10).

  Then run `make check-gtk-axioms`.
- [x] 5.3 `docs/next/long-line-documents.md`: mark
  `retest-upstream-fixes-on-nightly` done, with its results, and list the
  nightly-only verdict changes (4.4) under the next GNOME adoption. Update
  the A102 follow-up. Fix the "A52" in the queue entry to "A67".
- [x] 5.4 `.agents/rules/build.md` (GTK axioms / runtimes paragraph): replace
  the "read those verdicts from the host" caveat with the `--only` usage,
  the sandbox skip, and the summary. Update the `Makefile` comment and help
  text, and `AGENTS.md`/`README.md`, where they describe
  `make gtk-axioms-runtimes`.
- [x] 5.5 Add the probe and sample changes (A23, A29, A54, the RemoteDesktop
  skip) to `crates/gtk-lush/axioms/CHANGELOG.md`, and the README if it lists
  sample flags.

## 6. Upstream text, never posted (design D1, D5)

- [x] 6.1 Write `retest-facts.md` for gtk#8528, gtk#8529, pango#582 and
  gtksourceview#432 in their existing report folders (gtk#8529's carries
  A54's and A26's costs from 4.2, design D1/D4). Each holds the
  measured results, versions, source commits, commands, output files, and
  the open questions. For gtk#8528, note that !10228 is the open MR that
  touches the measured path, and that the sample can test it. No drafted
  reply.
- [x] 6.2 A102: verify, with fetched sources, `gtk_source_search_context_forward_async`
  and the cancellation of its task on a query change on `master`. Search
  GtkSourceView issues and MRs in several phrasings. Fetch the issue
  template and CONTRIBUTING. If it still reproduces on the newest nightly
  and is unreported, write
  `~/Documents/Agentic/Reports/GTK/gtksourceview-search-forward-async-superseded/`
  with `report.md`, a standalone C reproducer, `run.sh`, and `output.txt`
  (nightly and 5.22.0), ending with the disclosure line.
- [x] 6.3 A103: re-verify on the newest nightly and on `master`'s
  `gtk_source_map_allocate_slider`. Search the tracker again, then refresh
  `gtksourceview-map-slider-unallocated/` into the skill's layout:
  `report.md` with the title line, a standalone reproducer, `run.sh`,
  `output.txt`, and the disclosure line.
- [x] 6.4 A107: verify `pango_layout_move_cursor_visually` and
  `pango_layout_line_get_cursors` on Pango `main`. Search pango issues and
  MRs and fetch its CONTRIBUTING/template. If it still reproduces on Pango
  1.59.x and is unreported, write
  `~/Documents/Agentic/Reports/GTK/pango-move-cursor-visually-quadratic/` with
  `report.md`, a PyGObject or C reproducer, `run.sh`, `output.txt`, and the
  disclosure line.
- [x] 6.5 Do a second review pass of each draft: re-fetch the template and
  diff the headings, rerun the reproducer, recheck every claim against the
  fetched source, and check that no LushText vocabulary is left. Record
  drops with their reason in `docs/upstream-reports.md`.

## 7. Gates and PR

- [x] 7.1 If a GNOME 51 run contradicts a workaround's premise (design D7),
  write the failing consumer test first, then fix it, and rerun the affected
  Kani shard under `scripts/heavy.sh` if a proved module changes (D8).
  Otherwise record that no production code or Kani shard is affected.
- [x] 7.2 Run `make check` through `scripts/heavy.sh`. Run
  `openspec validate retest-upstream-fixes-on-nightly --strict`.
- [x] 7.3 Commit signed conventional commits, staging files by name. Push
  the branch and open the PR for the repository changes only (docs, ledger,
  probes, script). The PR body lists the measured table and links to no
  local drafts. Do not wait for CI.
