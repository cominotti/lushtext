Evidence directory: `.claude/handoffs/diagnose-ci-only-reflow-guard-failure/`.
Heavy commands go through `scripts/heavy.sh` (container runs as `heavy.sh --mem
none -- scripts/ci-image-widget-test.sh …`), one command per call: a shard run
is one call (`--runs 1`), and single-test batches stay under about 15 minutes
per call (design D1). No CPU pinning: the lane limits with `--cpus` (a quota),
`--cpuset` only for a diagnostic, and never `taskset`. "Starved" in this
change means a lane run at `--cpus=1`.

Commits (design "Commits and proofs"): the lane (1.x) commits alone. The trace
(2.1) is reverted before any smoke runs. Every edit under
`crates/lushtext/tests/widget/` or `crates/lushtext-core/src/ui/` (3.1–3.3,
4.2) lands in as few commits as possible, each after
`make visual-geometry-smoke`, `make accessibility-smoke` and `make visual-smoke`
on its final contents.

## 1. The CI-image lane (design D1)

- [x] 1.1 Add `scripts/ci-image-widget-test.sh`:
      - read the widget job's image and `dnf` line from
        `.github/workflows/ci.yml`;
      - take `--cpus N` / `--cpuset LIST`, `--runs N`, `--memory`, `--out
        DIR`, and `--shard NAME` or `-- <test-binary args>`;
      - bind-mount the worktree read-only and the cargo target directory
        read-only at their own paths, with labels disabled for the
        container only, never relabelling `$HOME`;
      - run the prebuilt widget binary as `<binary> --exact <names>`, which
        self-supervises into headless Mutter; `--shard` takes the names from
        `scripts/widget-shards.py list`. The summary states what the lane
        lacks against CI's `run-widget-tests.sh` path (temp scope, warning
        scan, outer retry; design D1);
      - print per-run verdicts and keep each run's log;
      - fail clearly when the image's toolkit versions differ from the host's,
        and exit 77 (unsupported host) without Docker.
- [x] 1.2 `--self-test`, without Docker: image and package extraction against
      `ci.yml` (the `widget-tests` job's image and `dnf` line, not another
      job's), argument errors (`--shard` with `--runs` above 1 refused), and
      the `--shard` list from `scripts/widget-shards.py`. Wire it into
      `make check-agent-helpers`, its self-test and its `shellcheck` list. The
      wait-for self-test's `pgrep -f` scan already covers `scripts/`.
- [x] 1.3 Reproduce B3 with the tracked lane: the guard red at `--cpus=4` and
      green unlimited, at least five runs each (the unlimited control had
      n = 1), on `origin/main` code. Record the logs.
      **Done, with a deviation:** red at `--cpus=4`, 5/5 (and 2/2 before),
      with CI's exact message; **red unlimited too, 5/5** (16 CPUs visible,
      1-min load 4.5 to 6.6), the same message and value 8. B3's single
      unlimited green does not reproduce, so the CPU count is not the
      condition (design B3, correction). Logs: `lane-1.3-cpus4/`,
      `lane-1.3-unlimited/`, binary `ci-lane-bin/widget-main` (`main` code
      at 77a45ad2).
      The lane also counts a retry-rescued pass (`FLAKY:`) and a shard run
      that selects other than the shard's tests as red, and mounts Cargo's
      `build-dir` as well as the target directory (test binaries live there).

## 2. F1: name the writer (design D2)

- [ ] 2.1 Env-gated trace, test-utils only and never committed. It logs every
      vertical `changed` and `value-changed` from page construction on, a
      backtrace at each value move, and at each settle `at_lower`,
      `is_paused` and `reflow_scrolled`.
- [ ] 2.2 Run the guard in the lane at `--cpus=4` and unlimited. Diff the
      traces and name the site that moves the value from 0 to 8. Confirm or
      refute B1's inferred link (`at_lower` false at the settle).
- [ ] 2.3 From the trace, name the property of the image that lets the move
      happen, if one does. First the font × CPU 2×2 at `--cpus=4`: the CI
      image with Adwaita Mono installed (`adwaita-mono-fonts`), and a
      container of the toolbox's own image with CI's font set
      (`fontconfig-ci/fonts.conf`); the two cells B3 has are the CI image
      with its fonts (red) and the toolbox at 16 CPUs (green). Then any other
      candidate the trace names. GTK is on cairo in every run (design B2);
      record mutter's GL backend for each run from its log (hardware or
      llvmpipe), since that is the only place the GPU difference reaches.
      Record the result in design D2.
- [ ] 2.4 If the writer is a toolkit behaviour the fix relies on, add ledger
      entry A113 with a `gtk-lush-axioms` probe and sample. Run
      `make gtk-axiom-sample AXIOM=A113 CHECK=1` and the GNOME 50 floor
      (`scripts/run-gtk-axioms.sh runtimes --only A113`). Update the
      `gtk-lush-axioms` CHANGELOG.

## 3. F1: product or fixture, then fix (design D3)

Tests: the editor-minimap scenarios "Presented at the top on a four-CPU host",
"Presented at the top in a real window" and "Reflow repair guard starts at
rest".

- [ ] 3.1 Discriminator, failing first if the product is at fault: a widget
      test that opens a 2,000-line file from disk in a `LushtextWindow` the
      way a user does (`open_document`, not session restore, not
      `set_text` into a new tab), with word wrap off and the minimap shown,
      settles, and asserts `value == lower` within 0.5. No test code writes
      an adjustment (never `put_editor_at_top_left`). Run it in the lane at
      `--cpus=4` with the image's fonts and with Adwaita Mono installed, and
      record red or green for each.
- [ ] 3.2 Make the "at the top" precondition exact (`value - lower <= 0.5`)
      in `present_settled_minimap_document`, and the guard's final
      assertion exact too (today `visible_rect().y() < 10`, which accepts a
      value up to 15), keeping its burst-state and emissions message. Also do it in
      every other editor-page test whose reflow or top-anchor assertion relies
      on resting at the top: inventory them with `grep -n 'y() < 10\|at the
      top'` in `crates/lushtext/tests/widget/` and record the list. Record it red in the lane on `main` code: it fails as a
      precondition, not at the repair.
- [ ] 3.3 Fix at the writer:
      - product defect (3.1 red): fix the product. 3.1, 3.2 and the guard go
        green in the lane;
      - fixture artefact (3.1 green): give the fixture the window's settled
        contract, with the writer named in its doc comment.

      Never `set_value(lower)` in the test, never a looser precondition, never
      a longer wait without a named cause.
- [ ] 3.4 Prove it: 20 consecutive lane runs at `--cpus=4` of the guard, the
      three other D2 tests and 3.1, in calls of at most about 15 minutes.
      Five lane runs each at `--cpus=1` and `--cpus=2` of the guard (B3 was
      red there too). The whole `editor-page` shard in the lane at
      `--cpus=4`, once, green. 20 isolated toolbox runs of the guard at the
      inherited cpuset.
- [ ] 3.5 Revert the trace (2.1), then run `make visual-geometry-smoke`,
      `make accessibility-smoke` and `make visual-smoke` on the final
      contents of 3.1–3.3 before committing them: `editor_page.rs` is under
      a UI prefix whatever the fix touches (design "Commits and proofs").
      Check that the minimap cases pass if the fix touches
      `ui/editor_page/minimap/` or `overscroll.rs`.

## 4. 4.2: bookmark in preview-only mode (design D4)

- [ ] 4.1 Run the full `window` shard in the lane at `--cpus=4`, five times,
      one `heavy.sh` call per run. Record every failure's enriched message.
- [ ] 4.1b The inherited watched tests (design B5): run
      `test_minimap_native_slider_covers_the_visible_text_with_an_overscroll_margin`,
      `test_a_protected_periodic_capture_reads_the_source_not_the_buffer` and
      `test_directory_retention_stays_within_its_charge` isolated in the lane
      at `--cpus=4`, five times each. A reproduction joins the change on the
      normal path (trace, classify, fix failing first, 20/20 in the lane);
      otherwise each stays on the eight-run proof (design D5).
- [ ] 4.2 Reproduced: trace, classify, and fix failing first. Then 20/20 lane
      runs of the test and one green lane run of the full shard. Check
      whether the F1 writer explains it.
- [ ] 4.3 Not reproduced: `restore-main-ci` D4's branch. CI-log diagnosis, a
      deterministic mechanism test red before its fix, then eight branch CI
      runs of `window` without its `FLAKY` line (task 6).

## 5. Gates

- [ ] 5.1 `make check` and `make check-agent-docs` (via `heavy.sh`).
- [ ] 5.2 `make test-widget-shard WIDGET_SHARD=editor-page`, `…=window` and
      `…=surfaces` locally, with no `FLAKY` line.
- [ ] 5.3 The three smokes are current for every committed UI-prefix edit
      (3.5); rerun them only if a later edit, or the `origin/main` merge,
      changed a file under a UI prefix.
- [ ] 5.4 Kani: none, unless the fix touches a proven module (design Kani).
      Then the named shard runs once on the final code.
- [ ] 5.5 `openspec validate diagnose-ci-only-reflow-guard-failure --strict`.

## 6. CI proof (design D5; the CI-evidence exception)

- [ ] 6.0 First check that this stage's brief allows the CI-evidence
      exception (design D5, "Conflict with the stage brief"). If it does
      not, skip 6.1–6.2: record every eight-run proof still owed (shard,
      test, reason) as owed to the programme-end CI check, in tasks.md and
      `docs/next/long-line-documents.md`. If it does, check once that
      `gh run rerun` works for this account here, and whether a rerun is
      refused while an attempt is in progress (design D5's expectation). If
      reruns are not allowed, plan the runs as pushes of real commits, each
      after the previous run finished.
- [ ] 6.1 Open the PR. Read its `ci.yml` run with `timeout 540 gh run watch
      <id> --exit-status`, in repeated foreground calls.
- [ ] 6.2 Rerun until each shard has the runs D5 requires on the final code:
      whole-run reruns (`gh run rerun <run>`) while more than one shard owes
      runs, a single job (`--job <id>`) when one alone does. Record run id,
      attempt and result for each. Eight for a test still on the "not
      reproducible" branch; none are owed for a test proved in the lane,
      except directory retention, whose CI figure only the runner gives
      (design D5).
- [ ] 6.3 Any new failure in these shards joins the change
      (`preexisting-blockers.md`). It is diagnosed in the lane first.

## 7. Docs

- [ ] 7.1 gtk-testing `references/widget-testing.md`: the "CI image + CPU
      count" class (B3), the lane, and the exact at-rest precondition.
- [ ] 7.2 `.agents/rules/build.md`, or `testing.md` (lane list): the lane and
      when to use it. `.agents/rules/verification-runs.md`: the helpers that
      `make check-agent-helpers` self-tests ("all three" becomes the new
      count). `README.md`'s test-commands section, per `documentation.md`. Update the `AGENTS.md` index only if a rule changes
      materially.
- [ ] 7.3 `docs/next/long-line-documents.md`: replace the "(queued)" follow-up
      paragraph with the outcome (writer, product or fixture, proof run ids).
- [ ] 7.4 The design's "Diagnosed" record: writer, property of the image,
      decision, red and green logs.
- [ ] 7.5 If a product behaviour changed: `docs/accessibility.md` and the matrix
      rows for the first-line visibility, and `README.md` if a feature
      statement changes.
