## Context

Three runners scan the output of the processes they run for toolkit warnings, or for one phrase:

- `scripts/run-widget-tests.sh` (`run_with_widget_log`, `check_for_unexpected_widget_warnings`);
- `scripts/run-gtk-axioms.sh` (`run_logged`, `scan_warnings`, `count_sample_results`, `runtime_summary`);
- `scripts/run-mutants.sh` (`run_cargo_mutants` and the mirror-slot runner).

Each one tees the whole stream into a plain `mktemp` path in the caller's temporary directory, then greps the file after the command ends. Two of them read the matching lines into a shell variable.

`scripts/with-temp-scope.sh` gives the command under test a private `TMPDIR` and fails with `TEMP-LEAK:` on leftovers. It is the *command* the runners wrap, so their captures live outside it, in shared `/tmp`.

### Measured baseline (2026-11-15, `slice-large-directory-collapse`)

| Capture | Size | Paired log | How the run ended |
| --- | --- | --- | --- |
| `/tmp/tmp.U8l7j91nfu` (16:26) | 4,137,407,761 B | `impl/new-tests-r2.log`, 4,137,395,570 B, 16:26:54 | `line 191: … Killed ( … run_with_widget_log … )`, `widget-memory: peak 4096 MiB, oom_kill 4` |
| `/tmp/tmp.Sm4QofwXsj` (17:26) | 4,144,072,471 B | not kept | cargo trailer present, so killed after the harness ended |
| `/tmp/tmp.sn5oJHrsL3` (17:29) | 4,146,853,637 B | not kept | no cargo trailer: killed from outside mid-run |
| `/tmp/tmp.4wmLt5B8RQ` (17:39) | 4,202,015,116 B | `impl/final-tree-r1.log`, 17:39:08 | `agent-locks.sh: … Killed env … "$@"`, `flood lines: 36539101` |

All four ran in a `MemoryMax=4G MemorySwapMax=0 OOMPolicy=continue` scope. `/tmp` is tmpfs (47 GiB), and tmpfs pages are charged to the writer's memory cgroup. The writer was `tee`, inside the scope.

So the capture itself consumed the scope:

- the kernel killed four processes. The victims are not recorded; mutter being one of them is inferred from the later tests' `GTK4 init failed` cascade;
- the scan's `unexpected="$(grep …)"` then copied about 36 M matching lines into bash memory;
- the subshell holding the capture's name was killed.

The path was registered in `OWNED_TEMP_PATHS` *inside* `run_headless`'s `( … )` subshell, which has no traps (bash resets them in subshells), so the parent's `EXIT` trap could not remove it. A scratch reproduction confirms this: bash 5.3.15, SIGTERM to the subshell, parent trap sees 0 paths, capture left on disk.

The coordinator removed the four files on 2026-11-18 at 20:11, freeing about 15.5 GiB of tmpfs. That tmpfs had been invisible to admission's MemAvailable reserve until then.

Other entries in `/tmp` at 20:10 on 2026-11-18:

- `heavy-report.XXXXXX`: six files. Two were from the minute of the listing and probably live; one was from 2026-11-17.
- `with-memory-cap-marker.*` and `with-memory-cap-envelope-test.*`: six entries from with-memory-cap self-tests on 2026-11-17.
- About 3,800 `tmp.*` files holding another project's agent-docs listings. These are not LushText's.

## Goals / Non-Goals

**Goals:**

- No capture of a command's output grows with that output.
- A scan's verdict is exactly today's: same regexes, same `grep -E`/`grep -Ev` semantics, same locale.
- No exit path leaves a named scan entry in shared `/tmp`: normal, Ctrl-C, SIGTERM, OOM kill, `kill -9`. The one exception is a `kill -9` inside the stage's few-command open window, which leaves a PID-bearing directory that the sweep reclaims (D2).
- What a runner must leave on disk carries the owning PID and is reclaimed by the existing sweep.
- Self-tests prove the bound, the drain, the wait, the linger case and the abnormal exits. They run in the hook and in CI.

**Non-Goals:** fixing the CRITICAL flood (`slice-large-directory-collapse`), stopping a run early on a flood, bounding the live sanitized stream, and removing other projects' `/tmp` files. The proposal gives the reasons.

## Decisions

### D1. One shared scan stage, classification kept verbatim

`scripts/bounded-log-scan.sh` is a bash library (sourced) with an executable `--self-test`. It provides:

- `bls_start <warn-ERE> <benign-ERE> [<keep-ERE>] [--keep-ignore-case]`
- `bls_finish`
- the readers `bls_unexpected_count`, `bls_unexpected_report` and `bls_kept_path`.

The stage pipes the stream through `grep -E "$warn" | grep -Ev "$benign"`, the same two invocations and the same regex variables the runners use today. The result goes into one `LC_ALL=C awk` sink that does three things:

- counts every line;
- writes lines into the report output until 64 KiB (2^16) are written, cutting at a line boundary and cutting a single line longer than the cap at the cap;
- keeps reading to EOF.

An optional keep branch, `grep -E "$keep"` on the raw stream, keeps the lines a runner parses later. It writes at most 1 MiB (2^20) and an exact kept-line count, then records overflow and keeps draining. The stage fans the stream out to the keep branch itself (`tee -p` into a process substitution inside the stage) and waits for that branch before it exits. In keep-only mode (the mutation runner) there is no warning branch.

Each sink writes its own summary line (`count`, `bytes`, `overflow`, `done`) only at EOF, so the stage has exited only after every sink has summarized. The sinks write through the descriptors they inherit (`>&N`). They never reopen `/dev/fd/N` for writing, because that opens the same inode with `O_TRUNC`. A `grep` that matched nothing exits 1. That is not a stage failure: the summaries carry the verdict, never the stage's exit status.

- **The verdict.** It fails when `count > 0`, when a sink's summary is missing, or when kept output overflowed for a runner that reads the kept lines (the axiom runner). The mutation runner reads only the exact kept count, so overflow does not change its answer. The count covers all lines, so the 64 KiB report cap never changes a verdict. Only the printed excerpt is truncated, followed by `... and N more unexpected lines (M in total)`.
- **Rejected: porting the regexes to awk or Python.** awk ERE and POSIX `grep -E` differ in corner cases (bracket expressions, interval support, `\` escapes), and the smoke-warning-classification family is proven with `grep -E` (`smoke_warning_classifiers.py` self-test). One rendering, one engine.
- **Rejected: a `head -c` cap.** `head` exits at the cap. tee then gets EPIPE, and the harness either dies on SIGPIPE or stalls. The sink must drain.
- **Rejected: capping the capture and failing closed on the cap.** The count is exact for free, so failing at the cap would turn a 1-line defect into a capacity question.

### D2. Unlinked-at-open outputs, read back through `/proc/self/fd`

`bls_start` creates one directory, `mktemp -d "${TMPDIR:-/tmp}/lt-scan-$BASHPID-XXXXXX"`. It creates each output inside it and opens it twice: a write descriptor for the sink and a read descriptor for the verdict. It then removes the directory before anything else runs. From then on no exit path can leave a named entry. Only a `kill -9` inside those few commands (no trap runs) leaves one PID-bearing directory, and the stale sweep reclaims it (`lt-scan-` joins `PID_PREFIXES`, D5). A plain `mktemp` file there could never be reclaimed.

Readers reopen `/dev/fd/<read-fd>` by path, which starts a fresh file description at offset 0. So several readers (the axiom runner's three) each see the whole kept output. Bash does not mark `{var}` descriptors close-on-exec, so a child such as `python3` can open the inherited `/dev/fd/<n>`. This was checked on bash 5.3.15. After a `kill -9`, an orphaned pipeline keeps writing, within the cap, to a file with no name, and the kernel frees it when the last descriptor closes. This was verified on the host's bash 5.3.15: the read descriptor opened before the writes sees the full content after the sink ends.

- **Rejected: a run-owned, PID-bearing capture directory plus the stale sweep as the main mechanism.** The sweep reclaims a directory only an hour after its PID died. That is eventual cleanup, and a crash loop meanwhile accumulates captures, now bounded, in `/tmp`. The sweep stays only as the backstop for the open window above.
- **Rejected: putting the capture inside the private scope.** The scope is created by the wrapped command, so a capture there is the scope's own leftover (`TEMP-LEAK:`). Nesting the scope under a run directory through `TMPDIR` adds a path level. The session helper's system-bus socket is already about 80 of its 99 bytes with a seven-digit PID, so one more level breaks it.
- **Linux only:** `/proc/self/fd` is Linux-only, and so are the runners (headless Mutter, cgroups). The runtimes lane's Flatpak sandbox runs inside the host session, and the stage stays on the host side.

### D3. The stage is a top-level process substitution, awaited, and hidden from the command

`bls_start` runs `exec {bls_in}> >(stage)` in the runner's own shell and records `$!`. Process substitution inside a pipeline element sets no `$!` in the caller, as the scratch test showed (`$!: unbound variable`). The pipeline then tees into the stage with `tee -p "/dev/fd/$bls_in"`. `bls_finish` closes `bls_in`, runs `wait` on the stage's PID (its status ignored, D1), reads the summaries, and closes the read descriptors. So the verdict is read only after EOF.

`bls_start` and `bls_finish` run in the same shell, once per attempt, because `wait` only reaches that shell's own children. For the widget runner that shell is the top-level one (D4).

- **`tee -p`.** A stage that dies before EOF closes its pipe. Plain `tee` then dies of SIGPIPE, and the harness writing into `tee` dies with it, so the live stream and every later test's result are lost. With `-p` (`--output-error=warn-nopipe`), `tee` drops the dead output and keeps the live stream, and the missing summary fails the verdict closed (scratch-checked: pipeline status 0 0 0 with a stage that exits after 10 bytes).
- **Descriptors.** The command under test runs with every stage descriptor closed (`{bls_in}>&- {bls_r…}<&-`). Otherwise a daemon the command leaves behind keeps the stage's input open and holds the verdict. In the scratch test, a `setsid sleep 3` with its stdio detached held the verdict for 3 s while the descriptors were inherited, and for 3 ms once they were closed. A daemon that keeps the command's own stdout or stderr holds the pipeline itself, through `tee`, on main as on the branch. That is outside this change, and the linger case detaches the daemon's stdio for this reason.

- **Rejected: a named FIFO.** It needs a path, which reopens the leftover question. A writer that opens it before the reader exists blocks.
- **Rejected: a coprocess.** bash supports one reliably, and two runners nest stages (the axiom runtimes loop).

### D4. Ownership registered where the trap lives

Any path a runner still owns (the axiom runner's build output is not one of them) is created and registered in the top-level shell before any `( … )` subshell. A subshell never appends to `OWNED_TEMP_PATHS`.

- **The widget runner.** `run_headless` calls `run_with_widget_log` in the top-level shell (each attempt of the retry loop). The exports (`GDK_BACKEND`, the runner and monitor variables, `unset DISPLAY WAYLAND_DISPLAY`) move into a `( … )` subshell function that is the command `run_with_widget_log` runs. That subshell creates and registers nothing. The function's command is a variable that only the self-test overrides, so the failing-first cases (D8) drive the real `run_headless` structure with a fake flood.
- **The axiom runner.** `run_logged` is already called from the top-level shell's functions (`run_all`, `check_samples`).
- **The mutation runner.** Each slot's `run_slot` is a background subshell. `bls_start` and `bls_finish` both run in it, and with unlinked outputs it registers nothing.

`testing.md` gains the rule: a subshell has no traps, so register in the shell that owns the trap, or use an unlinked descriptor.

### D5. Runner temp entries that must be named carry the PID

- **`heavy.sh`'s report** becomes `${TMPDIR:-/tmp}/lt-heavy-<pid>-XXXXXX/report`. **`with-memory-cap.sh`'s envelope pid file** becomes `${TMPDIR:-/tmp}/lt-envelope-<pid>-XXXXXX/pid`. Children are handed the path, so these cannot be unlinked descriptors.
- **The sweep.** `with-temp-scope.sh`'s `PID_PREFIXES` gains `lt-heavy-`, `lt-envelope-` and the stage's `lt-scan-` (D2). The existing rules then apply: real directories owned by the user, PID gone, idle for an hour. A live run is never swept.
- **Self-test scratch.** All of with-memory-cap's self-test scratch moves under one self-test root that its trap removes. That covers the markers (`mktemp -u … touch`), the report and once-report files, the admission-lock file, the lock and envelope-test directories, and the leftover probe. A marker touched late by a background command whose root is already gone fails harmlessly instead of recreating the file.
- **Bounded no-trap temp files in lane scripts.** `run-snap-smoke.sh`'s `LOG` moves under its trapped `WORK_DIR`. `ci-image-widget-test.sh`'s `setup_log` is registered with its existing `cleanup` trap. `blueprint-templates.sh`'s two `output_file`s get an `EXIT`/`INT`/`TERM` trap, as its `tmp_dir` already has one. Their content is bounded (dnf output, 30 s of app output, compiler output), so no stage is needed.
- **Scope.** A minimal fix, as the parallel-agent rules ask for host-wide helpers: names and roots change, nothing in admission changes, and each fix gets one self-test case.

### D6. Runner-specific use

| Runner | Warn / benign | Keep | Readers after the change |
| --- | --- | --- | --- |
| `run-widget-tests.sh` | `WIDGET_WARNING_REGEX` / `BENIGN_WIDGET_NOISE_REGEX` | none | the verdict |
| `run-gtk-axioms.sh` | `WARNING_REGEX` / `BENIGN_NOISE_REGEX` | `SAMPLE_RESULT_REGEX`, `^skipped \(`, `^\{"axiom"` | `count_sample_results`, the slow-skip count and `runtime_summary` read the kept lines (`/dev/fd/<n>`). The 1 MiB kept cap is checked against the kept size of a real `make gtk-axioms` run, which is recorded. |
| `run-mutants.sh` | none (keep-only mode) | the existing no-mutants phrase ERE, `--keep-ignore-case` | "phrase seen" = exact kept count > 0, so kept overflow does not change it; no file, so no trap is needed |

The widget runner's old `self_test_warning_classification` file, a plain `mktemp`, becomes the stage's own outputs.

### D7. Self-tests and where they run

`bounded-log-scan.sh --self-test` covers:

- **Classification.** The existing widget cases are re-run through the stage: the sliced-pipeline line and the shared session-services samples pass; `Gtk-WARNING` and the AT-SPI registry critical fail.
- **The flood bound.** 2^20 unexpected lines (about 40 MB) leave report output of at most 64 KiB, an exact count of 1,048,576, and the verdict failed. 2^20 benign lines leave count 0, and the verdict passes.
- **The drain.** A producer writing well past the cap sees exit 0 with no EPIPE, and its exit status propagates.
- **The wait.** A stage slowed by a `sleep` is still awaited.
- **A missing summary.** A stage killed before EOF fails the verdict closed.
- **The linger case.** A command that leaves a `setsid` daemon with its stdio detached (`</dev/null >/dev/null 2>&1`) returns its verdict in under 1 s.
- **Abnormal exits.** The runner is killed with `kill -9`, and separately with SIGTERM, mid-flood:
  - with `TMPDIR` set to a private base, the base holds no new entry, both during the orphaned streaming and after it;
  - every stage output stays within its cap while the orphan streams (`stat -L /proc/<pid>/fd/<n>`);
  - the test then kills the orphaned process group.
- **The sweep.** It reclaims a dead-PID `lt-scan-`/`lt-heavy-`/`lt-envelope-` directory aged two hours and keeps a live one.

Budget: `bounded-log-scan.sh --self-test` finishes in under 5 s on the workstation, because it runs in the hook. `run-widget-tests.sh --self-test` takes 2.7 s on main (2026-11-18). Both are measured after the change and recorded.

`run-widget-tests.sh --self-test` (CI's `ci.yml` step, and the ci-image lane before each shard) calls `bounded-log-scan.sh --self-test`, as it already calls `with-temp-scope.sh --self-test`. So `make check-temp-scope` (in `make check-policy`) runs `run-widget-tests.sh --self-test` *in place of* its direct `with-temp-scope.sh --self-test`, and each self-test runs once in the hook. CI is not read during the programme, so the hook is the local proof. The same recipe shellchecks `bounded-log-scan.sh` and `run-widget-tests.sh`. That keeps the change out of the `check-agent-helpers` shellcheck line, which `adopt-faster-change-flow` edits.

### D8. Failing-first

Before the fix, the new widget-runner cases are run on main through a test-only entry point: `--self-test` cases that drive `run_headless`, and so `run_with_widget_log` inside main's `( … )` subshell, with a fake flood command. They must fail as follows:

- the `kill -9` case leaves a `tmp.*` entry in the private base;
- the SIGTERM-to-subshell case leaves one too;
- the flood case's capture exceeds 64 KiB.

The memory claim gets its own evidence, recorded in `evidence/flood-memory.md`. A synthetic flood larger than a small cap, run through `run_with_widget_log`. The starting point is `scripts/heavy.sh --mem 256M` and about 2^23 lines; task 1.2 fixes and records the figures. `heavy.sh` passes no `--address-space`, so the cap is `MemoryMax` alone.

- on main, OOM-kills in the scope (`oom_kill > 0`);
- on the branch, passes with a peak well below the cap and `oom_kill 0`.

`TMPDIR` is a private tmpfs directory that the task removes, so main's leftover never lands in shared `/tmp`.

## Risks / Trade-offs

- **[A slow stage throttles the harness through tee's backpressure]** grep and awk stream at hundreds of MB/s against a harness writing at most tens of MB/s, so the stage never paces the run. A 36 M-line flood moves through the stage in seconds.
- **[A runner killed with `kill -9` leaves its orphaned pipeline running]** This is unchanged from main. The difference is that the orphan now writes at most 64 KiB plus 1 MiB to unnamed files. The scoped command's own temp scope is still reclaimed by the existing sweep.
- **[`/proc/self/fd` read-back on a deleted tmpfs file]** This was verified on the host. CI's Fedora 45 container uses the same kernel interface. The self-test's read-back case fails loudly if a platform differs.
- **[Shared files with `adopt-faster-change-flow`]** `Makefile`: only the `check-temp-scope` recipe changes, and the other change does not touch that recipe. `.agents/rules/build.md`: both edit it. The coordinator orders the two in the ledger.
- **[The self-test in CI's container and the ci-image lane]** The new cases need `setsid`, `/proc` and GNU `tee -p`. Fedora's images have util-linux and coreutils. A missing tool fails the self-test loudly; it never passes it silently.

## Verification scope

- Tests: shell self-tests only, failing-first per D8. No Rust test changes.
- Kani: none (no proven module touched).
- Timing: none (no timed path touched).
- GTK axioms: none. The change relies on no toolkit behaviour, so no ledger entry is added.
- Widget lane: one real `scripts/heavy.sh --envelope 16G -- make test` on the final tree proves the runner still passes the whole suite, with the same warning verdicts and `TEMP-LEAK`-clean. One `make gtk-axioms` proves the axiom runner and records the kept size.
- Smokes: none. No file under the UI prefixes changes.

## Review decisions (2026-11-19, stage 4)

- **R1. The Replace All confirm FLAKY is routed to its own change, not fixed here.** `search_panel::test_an_apply_while_another_window_runs_is_refused` (one FLAKY at `96dcae3e`, clean at `4aa2dae4`) belongs to `stream-replace-all`'s confirm path; this change touches neither that code nor its test. The probe in `evidence/replace-confirm-flake.md` shows a real user-facing defect behind one possible cause: with the disposal lane full at Confirm, Replace All is refused for good with "try again shortly", and the preview and checked selection are discarded. User experience first means fixing it properly: keep the selection and the claimed transaction and retry the 64 MiB undo reservation on the lane's capacity wakeup, under `stream-replace-all`'s spec, with the data-safety audit and the three smokes a widget-test edit owes. That is a separate change by `parallel-agent-work.md` ("Merging": a failure seen locally blocks the merge "or gets its own change in the ledger"), which the coordinator opens. Until the ledger holds it, the FLAKY blocks this change's merge, which the maintainer's merge hold defers anyway.
- **R2. `heavy_report_dir` returns its failures** (`scripts/heavy.sh`). In a command substitution bash clears errexit, so a failed `mktemp -d` produced the report path `/report` instead of stopping heavy.sh as main's plain assignment did. Each step now returns its status, and a self-test case pins it (`evidence/failing-first.md`, "Review").
