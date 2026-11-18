Kani: none (no proven module touched). Timing: none (no timed path touched). GTK axioms: none (no toolkit behaviour relied on, so no ledger entry or sample). Smokes: none (no file under the UI prefixes).

Before the first heavy command: take an implementation slot (`parallel-agent-work.md`) and read `$H/bound-widget-log-capture/COORDINATOR-NOTE.txt`. This change edits `Makefile` (the `check-temp-scope` recipe only), so start implementation only once the ledger says `adopt-faster-change-flow`, which also edits `Makefile`, has merged or the coordinator has ordered the two. Then merge `origin/main`.

## 1. Failing-first cases against main's runner

- [ ] 1.1 Add a test-only entry point to `scripts/run-widget-tests.sh` (a `--self-test` sub-case, not a user flag) that runs `run_with_widget_log` over a given fake command. Add three cases, all with `TMPDIR` set to a private base that the case removes:
  - (a) a flood of 2^20 `Gtk-CRITICAL` lines, asserting no capture over 64 KiB exists while the flood runs;
  - (b) `kill -9` of the runner mid-flood, asserting the base gains no entry;
  - (c) SIGTERM to the `run_headless` subshell mid-run, asserting the base gains no entry.
  Run them on the unchanged runner and record in `openspec/changes/bound-widget-log-capture/evidence/failing-first.md` that each fails: (a) on the `tmp.*` capture's size, (b) and (c) on the `tmp.*` leftover. Keep the commit with the cases failing and the fix in the same task group, so no red commit lands.
- [ ] 1.2 Record the memory evidence (design D8) in `evidence/flood-memory.md`. Run a synthetic flood of about 2^23 lines through `run_with_widget_log` as `scripts/heavy.sh --mem 256M -- <entry point>`, so heavy.sh's own scope is the 256M cap; read `memory.peak` and `memory.events` through the runner's `report_memory_scope`. Set `TMPDIR` to a private directory under `/tmp` that the task removes. On main, record the `oom_kill` count and the leftover's size before removing it. Rerun the same command after group 2 and record the peak and `oom_kill 0`. Never leave main's leftover in shared `/tmp`.

## 2. The bounded scan stage (design D1–D3, D7)

- [ ] 2.1 Write `scripts/bounded-log-scan.sh`: `bls_start`, `bls_finish`, `bls_unexpected_count`, `bls_unexpected_report`, `bls_kept_path`, and `--self-test`. It must provide:
  - the verbatim `grep -E`/`grep -Ev` classification;
  - an `LC_ALL=C awk` sink with the 64 KiB report cap and an exact count, draining to EOF;
  - the optional keep branch, capped at 1 MiB, with overflow recorded;
  - a summary line written at EOF;
  - outputs unlinked at open and read back through `/proc/self/fd`;
  - a top-level `exec {fd}> >(…)` stage with `$!` recorded and awaited;
  - every stage descriptor closed for the command under test.
  The file carries an SPDX header and a usage comment.
- [ ] 2.2 Write its self-test with every D7 case:
  - classification samples;
  - the flood bound at 2^20 lines (unexpected and benign);
  - the drain with the exit status propagated;
  - a slow stage awaited;
  - a killed stage failing closed;
  - a `setsid` linger under 1 s;
  - `kill -9` and SIGTERM of a runner mid-flood, asserting the private base gains no entry during and after the orphan and each output stays within its cap (`stat -L /proc/<pid>/fd/<n>`), then killing the orphan group.
  Measure its wall time (budget under 5 s) and record it in `evidence/self-test-time.md`.
- [ ] 2.3 Switch `run-widget-tests.sh` to the stage. Remove `log_file`, `OWNED_TEMP_PATHS+=` inside the subshell, and the `$(grep …)` slurp. `check_for_unexpected_widget_warnings` reads the stage's count and report. `self_test_warning_classification` runs its cases through the stage and calls `bounded-log-scan.sh --self-test`. Cases 1.1 (a)–(c) must now pass.
- [ ] 2.4 Run `./scripts/run-widget-tests.sh --self-test`, `./scripts/bounded-log-scan.sh --self-test`, `shellcheck -x` on both, and `make check-temp-scope`. Commit through `scripts/heavy.sh --mem none -- git commit -F <file>` and push.

## 3. The other runners (design D5, D6)

- [ ] 3.1 `run-gtk-axioms.sh`: `run_logged` uses the stage with `WARNING_REGEX`/`BENIGN_NOISE_REGEX` and the keep ERE for `SAMPLE_RESULT_REGEX`, `^skipped \(` and `^\{"axiom"`. `count_sample_results`, the slow-skip count and `runtime_summary` read the kept lines. Remove both `LOG_DIR="$(mktemp -d)"` sites, and run the existing `--self-test` cases through the stage. Add cases for 1 MiB kept overflow failing closed and for kept counts equal to a full log's on the self-test samples.
- [ ] 3.2 `run-mutants.sh`: in `run_cargo_mutants` and the slot runner, replace `log_file`/`log` plus `tee` with the stage in keep-only mode (the existing phrase ERE, `--keep-ignore-case`). Add a self-test case: the phrase is detected, it is absent from a clean stream, and no file is left in a private base.
- [ ] 3.3 `heavy.sh` and `with-memory-cap.sh` (design D5):
  - the report goes to `lt-heavy-<pid>-XXXXXX/report` and the envelope pid file to `lt-envelope-<pid>-XXXXXX/pid`, each removed by the existing traps;
  - the with-memory-cap self-test markers and envelope-test directories move under one self-test root removed by its trap.
  Add one self-test case each, asserting the name carries the PID and the entry is removed on exit. Run `make check-agent-helpers`, then run `TMPDIR=<private base> make check-agent-helpers` once and record in `evidence/helper-leftovers.md` that the base is empty afterwards.
- [ ] 3.4 `with-temp-scope.sh`: add `lt-heavy-` and `lt-envelope-` to `PID_PREFIXES`. Self-test cases: a dead-PID directory aged two hours is swept for each prefix, a live-PID one is kept, and a plain `tmp.XXXXXXXXXX` entry aged two days is kept.
- [ ] 3.5 `Makefile`: the `check-temp-scope` recipe also runs `./scripts/bounded-log-scan.sh --self-test` and `./scripts/run-widget-tests.sh --self-test`. Add `scripts/bounded-log-scan.sh` to the shellcheck list of the recipe that lints the agent helpers. Do not touch other hunks.
- [ ] 3.6 Run `make check-gtk-axioms`, `make check-agent-helpers`, `make check-temp-scope` and `make check-policy` through `scripts/heavy.sh`. Commit and push.

## 4. Proof on real lanes

- [ ] 4.1 Run `scripts/heavy.sh --envelope 16G -- make test` on the final tree. It must be green, with no `TEMP-LEAK:`, `FLAKY`, `OOM-KILL:` or `SESSION-HYGIENE:` line, and the same warning verdicts as main. Record `/proc/loadavg` before and after.
- [ ] 4.2 Run `scripts/heavy.sh --envelope 16G -- make gtk-axioms`. It must be green. Record the kept-output size against the 1 MiB cap in `evidence/axiom-kept-size.md`. If it exceeds a quarter of the cap, revisit the cap in design D6 before merging.
- [ ] 4.3 Rerun task 1.2's branch measurement, and confirm `ls -d /tmp/tmp.* | wc -l` did not grow across 4.1 and 4.2. Count by name only; touch nothing.

## 5. Documentation

- [ ] 5.1 `.agents/rules/testing.md` "Temporary Files":
  - a bullet that a runner's scan of a command's output is a bounded stream to unlinked descriptors, never a whole-output file;
  - the trap bullet amended: a `( … )` subshell has no traps, so register owned paths in the shell that holds the trap;
  - the new sweep prefixes.
- [ ] 5.2 `.agents/rules/build.md`:
  - the GTK axioms "Warning scan" paragraph (streamed, bounded, kept lines);
  - the widget runner wording where it describes the scan;
  - `make check-temp-scope` now runs both self-tests.
- [ ] 5.3 Append a dated 2026-11-15 entry to `docs/memorials/2026-10-20-test-data-incidents.md`: the four captures, tmpfs charging the scope, the subshell trap, and the 2026-11-18 cleanup. Link it from the `testing.md` bullet. Do not add a new memorial file, which would mean editing `docs/memorials/README.md`.
- [ ] 5.4 Check `README.md` and `AGENTS.md` for wording about the widget warning scan or temporary files, and update any that is now stale. Neither lists this behaviour today, so record it if neither needs a change.
- [ ] 5.5 Run `openspec validate bound-widget-log-capture --strict` and `make check-agent-docs`. Commit and push.
