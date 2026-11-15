Evidence directory: `.claude/handoffs/diagnose-ci-only-reflow-guard-failure/`.
Heavy commands go through `scripts/heavy.sh` (container runs as `heavy.sh --mem
none -- scripts/ci-image-widget-test.sh …`). No CPU pinning except through the
lane's container limit.

## 1. The CI-image lane (design D1)

- [ ] 1.1 Add `scripts/ci-image-widget-test.sh`:
      - read the widget job's image and `dnf` line from
        `.github/workflows/ci.yml`;
      - take `--cpus N` / `--cpuset LIST`, `--runs N`, `--memory`, `--out
        DIR`, and `--shard NAME` or `-- <test-binary args>`;
      - bind-mount the worktree read-only and the cargo target directory
        read-only at their own paths, with labels disabled for the
        container only, never relabelling `$HOME`;
      - run the prebuilt widget binary, which self-supervises into headless
        Mutter;
      - print per-run verdicts and keep each run's log.
- [ ] 1.2 `--self-test`, without Docker: image and package extraction against
      `ci.yml`, argument errors, and the `--shard` list from
      `scripts/widget-shards.py`. Wire it into `make check-agent-helpers`. The
      wait-for self-test's `pgrep -f` scan already covers `scripts/`.
- [ ] 1.3 Reproduce B3 with the tracked lane: the guard red at `--cpus=4` and
      green unlimited, on `origin/main` code. Record the logs.

## 2. F1: name the writer (design D2)

- [ ] 2.1 Env-gated trace, test-utils only and never committed. It logs every
      vertical `changed` and `value-changed` from page construction on, a
      backtrace at each value move, and at each settle `at_lower`,
      `is_paused` and `reflow_scrolled`.
- [ ] 2.2 Run the guard in the lane at `--cpus=4` and unlimited. Diff the
      traces and name the site that moves the value from 0 to 8. Confirm or
      refute B1's inferred link (`at_lower` false at the settle).
- [ ] 2.3 From the trace, name the property of the image that lets the move
      happen (why the toolbox at one or two CPUs did not). Record it in
      design D2.
- [ ] 2.4 If the writer is a toolkit behaviour the fix relies on, add ledger
      entry A113 with a `gtk-lush-axioms` probe and sample. Run
      `make gtk-axiom-sample AXIOM=A113 CHECK=1` and the GNOME 50 floor
      (`scripts/run-gtk-axioms.sh runtimes --only A113`). Update the
      `gtk-lush-axioms` CHANGELOG.

## 3. F1: product or fixture, then fix (design D3)

Tests: the editor-minimap scenarios "Presented at the top on a four-CPU host",
"Presented at the top in a real window" and "Reflow repair guard starts at
rest".

- [ ] 3.1 Failing first: a widget test that opens a document at its top in a
      `LushtextWindow`, settles, and asserts `value == lower` within 0.5.
      Run it in the lane at `--cpus=4` and record red or green.
- [ ] 3.2 Make the D2 fixture's "at the top" precondition exact
      (`value - lower <= 0.5`) in `present_settled_minimap_document` and in
      the guard. Record it red in the lane on `main` code: it fails as a
      precondition, not at the repair.
- [ ] 3.3 Fix at the writer:
      - product defect (3.1 red): fix the product. 3.1, 3.2 and the guard go
        green in the lane;
      - fixture artefact (3.1 green): give the fixture the window's settled
        contract, with the writer named in its doc comment.

      Never `set_value(lower)` in the test, never a looser precondition, never
      a longer wait without a named cause.
- [ ] 3.4 Prove it: 20 consecutive lane runs at `--cpus=4` of the guard, the
      three other D2 tests and 3.1. The whole `editor-page` shard in the lane
      at `--cpus=4`, once, green. 20 starved isolated toolbox runs of the
      guard.
- [ ] 3.5 Run the minimap visual-geometry cases (`make visual-geometry-smoke`)
      if the fix touches `ui/editor_page/minimap/` or `overscroll.rs`.

## 4. 4.2: bookmark in preview-only mode (design D4)

- [ ] 4.1 Run the full `window` shard in the lane at `--cpus=4`, five times.
      Record every failure's enriched message.
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
- [ ] 5.3 `make accessibility-smoke` and `make visual-smoke` only if a
      product UI path changed.
- [ ] 5.4 Kani: none, unless the fix touches a proven module (design Kani).
      Then the named shard runs once on the final code.
- [ ] 5.5 `openspec validate diagnose-ci-only-reflow-guard-failure --strict`.

## 6. CI proof (design D5; the CI-evidence exception)

- [ ] 6.1 Open the PR. Read its `ci.yml` run with `timeout 540 gh run watch
      <id> --exit-status`, in repeated foreground calls.
- [ ] 6.2 Rerun the `editor-page`, `window` and `surfaces` widget jobs one at a
      time (`gh run rerun <run> --job <id>`) until each shard has the runs D5
      requires on the final code. Record run id, attempt and result for each.
      Eight for a test proved on the "not reproducible" branch.
- [ ] 6.3 Any new failure in these shards joins the change
      (`preexisting-blockers.md`). It is diagnosed in the lane first.

## 7. Docs

- [ ] 7.1 gtk-testing `references/widget-testing.md`: the "CI image + CPU
      count" class (B3), the lane, and the exact at-rest precondition.
- [ ] 7.2 `.agents/rules/build.md`, or `testing.md` (lane list): the lane and
      when to use it. Update the `AGENTS.md` index only if a rule changes
      materially.
- [ ] 7.3 `docs/next/long-line-documents.md`: replace the "(queued)" follow-up
      paragraph with the outcome (writer, product or fixture, proof run ids).
- [ ] 7.4 The design's "Diagnosed" record: writer, property of the image,
      decision, red and green logs.
- [ ] 7.5 If a product behaviour changed: `docs/accessibility.md` and the matrix
      rows for the first-line visibility, and `README.md` if a feature
      statement changes.
