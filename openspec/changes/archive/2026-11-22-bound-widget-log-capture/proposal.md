## Why

`scripts/run-widget-tests.sh` writes the whole, unfiltered output of the widget harness into a plain `mktemp` file in the caller's `/tmp`. It then reads every warning line of that file into one shell variable. Nothing bounds either step, and an abnormal exit leaves the file behind. On 2026-11-15 a test that flooded a `Gtk-CRITICAL` left four such files of 3.85–3.91 GiB in `/tmp`. The coordinator removed them on 2026-11-18 at 20:11, which freed about 15.5 GiB of tmpfs.

### Measured baseline

The evidence is the `slice-large-directory-collapse` worktree's runs and their logs in `.claude/handoffs/slice-large-directory-collapse/impl/`. Read-only tails are in `~/Documents/cst-review/program-2026-11-17/lushtext-tmp-logs/`.

- **The writer is confirmed.**
  - `tmp.U8l7j91nfu` was 4,137,407,761 bytes, written at 16:26 on 2026-11-15.
  - `new-tests-r2.log` is 4,137,395,570 bytes, written at 16:26:54, with the same tail. It is the sanitized stream of the same run (`scripts/run-widget-tests.sh --scope-label nt2`).
  - That log ends in `run-widget-tests.sh: line 191: … Killed ( … run_with_widget_log … )`, then `widget-memory: peak 4096 MiB, oom_kill 4` inside a `MemoryMax=4G MemorySwapMax=0` scope.
  - `tmp.4wmLt5B8RQ` (17:39:07) pairs the same way with `final-tree-r1.log` (17:39:08). That log ends with heavy.sh's child `Killed` and `flood lines: 36539101`.
  - `tmp.sn5oJHrsL3` has no cargo trailer, so its run was killed from outside before the harness finished.
- **The capture caused the out-of-memory kill.** `/tmp` is tmpfs. tmpfs pages are charged to the memory cgroup of the process that writes them. So the 3.9 GiB capture alone filled the 4 GiB scope, and the kernel killed four processes in it. Which ones is not recorded; that mutter was among them is inferred from the later tests' `GTK4 init failed` cascade.
- **The scan made it worse.** After the harness finished, the scan copied every warning line, nearly the whole file, into a bash variable. The kernel then killed the subshell that held both the variable and the file's name.
- **Nobody could clean up.**
  - The file's path was added to `OWNED_TEMP_PATHS` inside the `( … )` subshell of `run_headless`.
  - Bash resets traps in a subshell, so the subshell had no trap of its own, and the parent's `EXIT` trap never saw the path.
  - Even a plain SIGTERM to that subshell leaves the file. A scratch reproduction on bash 5.3.15 shows the parent trap with 0 registered paths and the capture left on disk.

### The same pattern elsewhere

- `scripts/run-gtk-axioms.sh` (`run_logged`/`scan_warnings`) tees whole probe and sample output into a plain `mktemp -d` directory and slurps its warnings the same way.
- `scripts/run-mutants.sh` tees all of cargo-mutants' output into two plain `mktemp` files and has no trap at all. It only needs to know whether one phrase appeared.
- Bounded leftovers that no sweep can reclaim:
  - `scripts/heavy.sh`'s `heavy-report.XXXXXX`, which carries no PID;
  - `scripts/with-memory-cap.sh`'s envelope pid file;
  - its self-test markers and directories. Six were left in `/tmp` from 2026-11-17.
- Bounded temp files with no trap, removed on normal paths only: `scripts/ci-image-widget-test.sh`'s `setup_log`, `scripts/run-snap-smoke.sh`'s `LOG` (outside its `WORK_DIR` trap) and `scripts/blueprint-templates.sh`'s two `output_file`s. The visual and accessibility smoke session logs and the ci-image run logs are artifacts under their lanes' output directories, not temp files.

`testing.md` says a leak is a defect, and `preexisting-blockers.md` makes it part of this work stream.

## What Changes

- **Bounded stream scan.** A new shared shell library and helper, `scripts/bounded-log-scan.sh`, adds a scan stage that sits beside the live stream. It classifies lines with the runners' existing `grep -E` / `grep -Ev` invocations and regex variables, verbatim. What it writes is bounded:
  - an exact count of unexpected lines;
  - at most 64 KiB of them for the report;
  - for runners that need them, the lines the runner parses later, capped at 1 MiB, with overflow failing closed.

  The stage reads its input to EOF whatever the cap, so the harness never sees a broken pipe or stalls. The runner waits for the stage before it reads the verdict, and a missing summary fails the run.
- **Nothing named on any exit path.** The stage writes to files that are unlinked as soon as they are opened and read back through `/proc/self/fd`. They are created inside a PID-bearing `lt-scan-<pid>-XXXXXX` directory that is removed right after the opens, so a normal exit, Ctrl-C, SIGTERM, an OOM kill or a `kill -9` of the runner leaves no scan entry in any temporary directory; a `kill -9` inside that open window leaves at most one such directory, which the stale sweep reclaims. An orphaned pipeline still streaming after a `kill -9` stays within the cap. The command under test gets none of the stage's descriptors, so a daemon it leaves running cannot hold the verdict. The runner tees into the stage with `tee -p`, so a stage that dies fails the verdict closed without taking the harness or the live stream down.
- **`run-widget-tests.sh`** uses the stage instead of `tee "$log_file"` plus `check_for_unexpected_widget_warnings`' slurp. `run_with_widget_log` runs in the top-level shell, and the headless environment moves into the `( … )` subshell that runs the command, which creates and registers nothing. Any path the runner still owns is registered in the shell that owns the trap. The classification self-test keeps every existing case and gains the bound, drain, wait, linger and abnormal-exit cases.
- **`run-gtk-axioms.sh`** uses the same stage. Its consumers (result count, slow-skip count, the runtimes summary) read the kept result, skip and observation lines instead of a full log, and its plain `mktemp -d` log directory goes away.
- **`run-mutants.sh`** detects the "no relevant mutants" phrase through the stage in keep-only mode (no warning classification), with no capture and no unowned file.
- **PID-bearing, reclaimable runner temp entries.** `heavy.sh`'s report and `with-memory-cap.sh`'s envelope pid file move into PID-bearing directories whose prefixes `scripts/with-temp-scope.sh`'s stale sweep reclaims, together with the stage's `lt-scan-`. The with-memory-cap self-test keeps all its scratch, markers included, under one root removed by its trap.
- **The bounded no-trap temp files** of the ci-image, snap-smoke and Blueprint scripts are registered in a trap or moved under the lane's trapped directory.
- **Local proof.** `make check-temp-scope` (in `make check-policy`) runs `scripts/run-widget-tests.sh --self-test` in place of its direct `with-temp-scope.sh --self-test`. That self-test already runs the temp scope's and now also runs `scripts/bounded-log-scan.sh --self-test`, so each runs once, in the commit hook as well as in CI's `ci.yml` step. The same recipe shellchecks both scripts.
- **Docs.** `testing.md` "Temporary Files" (bounded captures; trap registration across subshells), `build.md` (the widget and axiom warning scans), and a dated entry in `docs/memorials/2026-10-20-test-data-incidents.md`.

## Non-goals

- **The flood itself.** The `gtk_widget_get_parent` CRITICAL flood is a test defect owned by `slice-large-directory-collapse`. This change makes it fail cheaply. It does not fix it.
- **Stopping a run early on a flood.** Killing the harness after N unexpected lines would lose every later test's result, and any unexpected line fails the run anyway.
- **Bounding the sanitized stream.** The live stream to the caller's terminal, CI job log or `target/widget-parallel/<shard>.log` (written by `scripts/widget-shards.py` and overwritten each run) stays complete. It is the run's own output on disk or in the job log, it is not charged to the test scope as tmpfs is, and truncating it would hide how far a run got.
- **Removing other leftovers.** About 3,800 small `/tmp/tmp.*` files hold another project's content, and the maintainer owns removing them. The plain `tmp.*` name is never swept, because nothing proves it is ours.

## User-visible impact

No change for the application or its users. For contributors and agents:

- A flooding widget or axiom run now ends with a bounded report: the exact count of unexpected lines and the first 64 KiB of them. It no longer ends in an OOM kill and a multi-GiB tmpfs file.
- Nothing is lost compared with main. The verdict is the same for every input main could scan; only the printed excerpt of a failing run is capped. The two new failures are named: a scan that did not finish (the stage died), and kept lines over 1 MiB for the axiom runner.

## Capabilities

### New Capabilities

- `test-lane-log-capture`: how test and verification runners scan the output of the processes they run.
  - **Purpose:** "Test and verification runners scan the output of the processes they run without holding that output: every capture is bounded, leaves nothing named on any exit path, and keeps the warning verdict exact."

### Modified Capabilities

None. The temp-scope behaviour this change extends, the stale sweep's prefixes, has no main-spec requirement. Its new obligations are ADDED to `test-lane-log-capture`.

## Impact

- **Code:**
  - `scripts/bounded-log-scan.sh` (new);
  - `scripts/run-widget-tests.sh`, `scripts/run-gtk-axioms.sh`, `scripts/run-mutants.sh`;
  - `scripts/heavy.sh` (report path), `scripts/with-memory-cap.sh` (envelope pid file and self-test scratch), `scripts/with-temp-scope.sh` (sweep prefixes);
  - `scripts/ci-image-widget-test.sh`, `scripts/run-snap-smoke.sh`, `scripts/blueprint-templates.sh` (bounded temp files);
  - `Makefile` (the `check-temp-scope` recipe only).
- **No Rust, GTK, persisted format, action or automation change.** Kani: none (no proven module touched). Timing: none (no timed path touched). GTK axioms: none (no toolkit behaviour relied on).
- **Overlap:** `adopt-faster-change-flow` also edits `Makefile` and `.agents/rules/build.md`. Its `Makefile` hunks (`.PHONY`, `check-policy`, the `check-agent-helpers` comment, shellcheck list and self-test lines, a new recipe, the help line) do not include `check-temp-scope`, which is why this change lints its scripts there. The files are still shared, so the coordinator orders the two in the ledger (`parallel-agent-work.md`).
