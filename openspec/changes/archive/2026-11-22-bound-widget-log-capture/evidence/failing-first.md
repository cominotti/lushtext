# Failing-first evidence

Host: sophia toolbox, bash 5.3.15, GNU coreutils 9.11, GNU grep 3.12, GNU Awk
5.4.1, `/tmp` on tmpfs. Branch `bound-widget-log-capture`.

## 1.1 Widget runner capture cases on main's runner

The test-only entry point (`run-widget-tests.sh --self-test -- <command>`,
which sets `WIDGET_HARNESS_COMMAND` and calls `run_headless`) and the three
capture cases were added to the runner while `run_with_widget_log` still ran
inside `run_headless`'s `( … )` subshell and still teed into a plain `mktemp`
file. Run on 2026-11-18 at about 21:20 (`./scripts/run-widget-tests.sh
--self-test`, exit 1, 8.7 s):

```
Error: capture case (a): a capture over 64 KiB existed during the flood: /tmp/tmp.HFoNKtrDmq/base/tmp.7SIxktbghp
Error: capture case (a): the report does not count the 1048576 unexpected lines: …
Error: capture case (b): after kill -9 the base holds tmp.e4W9o0AB2D
Error: capture case (b): after the orphan ended the base holds tmp.e4W9o0AB2D
Error: capture case (c): after SIGTERM to the subshell the base holds tmp.fGpcEB17ve
```

- (a) fails on the size of main's `tmp.*` capture (and on the missing count,
  which main never printed);
- (b) fails on the `tmp.*` leftover while the orphaned subshell streams;
- (c) fails on the `tmp.*` leftover the killed subshell could not remove.

The cases were then made parallel and race-proof (each in its own directory,
the runner started as its own process group by `bls__start_group`, which
fails the case unless the group is its own, so no kill case can pass without
its kill). The final case code was re-run against a scratch copy of the
runner whose `run_with_widget_log` and `run_headless` were main's (the
`( … )` subshell, `tee` into `mktemp`, the `$(grep …)` slurp), at about
21:45 (exit 1, 6.1 s):

```
Error: capture case (b): after kill -9 the base holds tmp.IpmCg5kq4d
Error: capture case (c): after SIGTERM to the subshell the base holds tmp.tgWOMhuHT4
Error: capture case (b): after the orphan ended the base holds tmp.IpmCg5kq4d
Error: capture case (a): a capture over 64 KiB existed during the flood: …/a/base/tmp.rCogeDm6yV
Error: capture case (a): the report does not count the 1048576 unexpected lines: …
```

With the bounded stage (group 2) the same cases pass.

## 3.3 and 3.4 on the unchanged helpers

The new cases were added to the self-tests first and run on the unchanged
`with-temp-scope.sh`, `heavy.sh` and `with-memory-cap.sh` (2026-11-19 about
01:00, each through `scripts/heavy.sh --mem none`):

- `with-temp-scope.sh --self-test`, exit 1 (the live-PID and plain `tmp.*`
  guards passed, as they do on main):

  ```
  Error: the sweep kept the dead owner's lt-scan-4000000000-Ab3xYz.
  Error: the sweep kept the dead owner's lt-heavy-4000000000-Ab3xYz.
  Error: the sweep kept the dead owner's lt-envelope-4000000000-Ab3xYz.
  ```

- `heavy.sh --self-test`, exit 1:

  ```
  heavy self-test: the admission report entry was 'heavy-report.XegqU2' (status 0), expected one lt-heavy-4035261-XXXXXX directory
  ```

- `with-memory-cap.sh --self-test`, exit 1 (one case failed):

  ```
  self-test: the envelope pid directory was '', expected ${TMPDIR:-/tmp}/lt-envelope-4036162-XXXXXX
  ```

  On main the pid file was a plain `with-memory-cap-envelope.XXXXXX` file
  and `envelope_pid_dir` did not exist.

## NUL bytes (found in the final review, 2026-11-19)

With GNU grep 3.12 (the host's `/usr/bin/grep` and the CI widget image), a
stream of `{"axiom":"A1"}`, `noise \0 bytes`, `{"axiom":"A2"}`,
`x CRITICAL y`:

- main's widget scan (`grep -E … file | grep -Ev …`): **0** warning lines,
  only `grep: …: binary file matches` on stderr, so one NUL anywhere hid every
  warning (pre-existing, fail-open);
- main's axiom count (`grep -cE` on the file): 3, correct;
- the stage before the fix: unexpected 0, kept **0** (a regression for the
  axiom counts and the mutants phrase).

The new `bls__case_binary` self-test case failed first on the stage
(`binary: a NUL byte changed the scan: complete=1 count=0 kept=0 report=''`)
and passes once every stage grep reads the stream as text (`-a`): one
unexpected line, two kept. The stage self-test passes in the CI widget image
and on bare fedora:44 and fedora:45, the base images of the
`gtk-axioms-floor` and `gtk-axioms` jobs, which ship GNU awk 5.3.2 and 5.4.1.

The live job-log filter (`emit_sanitized_widget_stream`, main's
`grep --line-buffered -Ev`) had the same flaw: `--self-test -- <cmd>` with a
harness printing `line one`, a NUL line and a line after it showed only
`grep: (standard input): binary file matches`, even the line before the NUL
(pipeline status 0, the verdict unaffected). The new classification check
failed first (`a NUL byte cut the live stream to 'grep: (standard input):
binary file matches'`) and passes with `-a`.

## Review: heavy.sh with an unusable TMPDIR (2026-11-19)

`heavy_report_dir` runs in a command substitution, where bash clears
errexit, so a failed `mktemp -d` left `report_dir` empty and the report path
`/report`. On main the plain `report=$(mktemp …)` assignment stopped heavy.sh
at once. The new `heavy.sh --self-test` case (`TMPDIR=<missing base>
heavy.sh --mem none -- sh -c 'echo ran'`: nonzero, command not run, no
`/report` in the output) failed first on the branch:

```
heavy self-test: an unusable TMPDIR gave status 1 and: mktemp: failed to create directory via template ‘…/no-such-base/lt-heavy-452475-XXXXXX’: No such file or directory
…/scripts/heavy.sh: line 739: /report: Permission denied
…/scripts/heavy.sh: line 848: /report: Permission denied
```

and passes once each step of `heavy_report_dir` returns its failure
(`heavy self-test: ok …`). The command never ran in either tree.

## Review: the scan stage with an unusable TMPDIR (2026-11-19)

`bls__case_no_tmpdir` (`TMPDIR=<missing base> bls_start …` must return
nonzero with no stage or descriptor, and the verdict must fail closed naming
the reason) failed first against the unchanged `bls_start`/`bls_finish`
(the new case run over a copy of the old functions):

```
bounded-log-scan self-test: no TMPDIR: bls_start gave status 0, BLS_IN='11' BLS_PID='599572' BLS_R_SUMMARY=''
head: error reading 'standard input': Is a directory
bounded-log-scan self-test: no TMPDIR: complete=0 incomplete='the warning scan wrote no summary'
```

The old code returned 0 and started a stage after its `exec` tried
`/report` and the other outputs, which only the missing permission refused.
With the fix, the case passes (`bounded-log-scan self-test passed`). A widget
run with `TMPDIR=/nonexistent-lt-review scripts/run-widget-tests.sh
--self-test -- echo RAN` now prints `Error: the scan could not create its
outputs in /nonexistent-lt-review; not running the widget tests.`, exits 1
and never runs the command. Before the fix it printed `/report: Permission
denied` and `BLS__W_REPORT: unbound variable`.
