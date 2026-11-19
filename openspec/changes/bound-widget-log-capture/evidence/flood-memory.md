# Flood memory evidence (task 1.2, design D8)

Command, run from the worktree with `TMPDIR` a private directory under `/tmp`
that the task removed afterwards:

```
scripts/heavy.sh --mem 256M -- env TMPDIR=<private base> \
  ./scripts/run-widget-tests.sh --self-test -- <ctl>/flood 128 \
  2>&1 >/dev/null | tail -c 3000
```

- **Cap:** `--mem 256M`, so heavy.sh's own `with-memory-cap.sh` scope is the
  cap (`MemoryMax=256M MemorySwapMax=0 OOMPolicy=continue`; heavy.sh passes no
  `--address-space`).
- **Flood:** 128 chunks of 2^16 lines = 2^23 (8,388,608) `Gtk-CRITICAL` lines
  of 85 bytes, about 713 MB, through the test-only entry point
  (`--self-test -- <command>`), so through `run_headless` and
  `run_with_widget_log` exactly as a real run.
- **How the counts were read:** the fake harness writes its cgroup path; an
  observer outside the scope polled the scope's `memory.events` every 20 ms
  and the base's capture size, and read `oom_kill` of
  `user-lushtext-heavy.slice` (hierarchical) before and after. On main the
  runner itself is killed, so its own `report_memory_scope` line never prints.

## Main (runner teeing into a plain `mktemp` file), 2026-11-18 21:15–21:17

Run 1 (1.2 s, pipeline status 137):

- scope `memory.peak` 256,147,456 B (the cap);
- the capture grew to 249,036,800 B in the base; OOM killed the live
  stream's `awk` and then the runner (`Killed env … "$@"`);
- the capture was gone afterwards: the orphaned `( … )` subshell survived,
  its `$(grep …)` slurp was killed, so `unexpected` came back **empty**, the
  warning check passed, and the subshell removed the file. An OOM during the
  scan therefore also hid the warnings.

Run 2 (same command, pipeline status 137):

- the scope's last `memory.events` read: `max 15024`, `oom 880`,
  `oom_kill 1` before it vanished; the slice's `oom_kill` went from 26 to 32,
  so **6 processes were OOM-killed** in that window;
- the leftover capture `tmp.6bERPqfP1W` was **262,414,336 B** (the whole cap,
  charged to the scope as tmpfs pages), left in the base after the run;
- the leftover and the base were then removed by the task.

## Branch (the bounded stage), 2026-11-18 about 22:05

Same command and flood (128 chunks, 2^23 lines, about 713 MB), stdout kept
(filtered to the report lines) so the runner's own `report_memory_scope` line
shows. Load before 3.66, after 4.49 (a build-time figure, not a verdict).

```
with-memory-cap: MemoryMax=256M MemorySwapMax=0 OOMPolicy=continue in user-lushtext-heavy.slice (48G) for: env TMPDIR=… ./scripts/run-widget-tests.sh --self-test -- …/flood 128
... and 8387837 more unexpected lines (8388608 in total)
Error: unexpected warning output during widget tests (8388608 unexpected lines).
widget-memory: peak 12 MiB, oom_kill 0
```

- exit 1 (the unexpected lines), in 12.5 s, the flood itself 12.3 s;
- **peak 12 MiB against the 256 MiB cap, `oom_kill 0`** (the slice's
  `oom_kill` stayed at 32);
- the exact count, 8,388,608, and a 64 KiB excerpt;
- the private base held no entry during the run (an observer polled it every
  20 ms in a first branch run) and none afterwards.
