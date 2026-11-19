# 2026-10-20: test data incidents

Why test isolation is a property of the test process and why every test lane
runs under `scripts/with-temp-scope.sh`. The rules are in
[`.agents/rules/testing.md`](../../.agents/rules/testing.md), "Temporary Files"
and "Test Isolation From Real App Data".

## A widget test overwrote the real session

On 2026-10-20 a widget test saved a session before calling
`ensure_gtk_init()`, which was then the only place that set
`LUSHTEXT_DATA_DIR`, and overwrote the developer's real `session.json`; the
contents were unrecoverable. Isolation is therefore established as the first
statement of the widget harness's `main`, before any test runs, and
`json_store::data_dir()` panics in test builds when it resolves into the real
home. `app::test_session_saved_before_gtk_init_lands_in_the_private_test_directory`
is the regression test.

## Temporary directories exhausted `/tmp`

A child-per-test harness multiplies a one-directory leak: one
`lushtext-test-<pid>` directory per widget child once accumulated 45,978
directories and exhausted `/tmp`'s tmpfs inode table (see
`crates/gtk-lush/proof-harness/CHANGELOG.md`). That is why the supervisor owns
one root per run and every lane fails with `TEMP-LEAK:` when anything is left
behind.

## The signal trap that raced the document portal

`with-temp-scope.sh` once exited from its Ctrl-C trap before the session's
services had released the runtime directory, which raced xdg-document-portal:
it recreated `doc/` after the removal. The wrapper now records the signal and
waits for the session to release the directory before removing the scope.

## 2026-11-15: four 4 GiB warning-scan captures in `/tmp`

`scripts/run-widget-tests.sh` teed the whole, unfiltered harness output into a
plain `mktemp` file in the caller's `/tmp`, then read every warning line of it
into one shell variable. A test that flooded a `Gtk-CRITICAL`
(`gtk_widget_get_parent`, 36,539,101 lines in one run) left four such files of
3.85-3.91 GiB in `/tmp` from runs of `slice-large-directory-collapse` on
2026-11-15:

- `/tmp` is tmpfs, and tmpfs pages are charged to the memory cgroup of the
  process that writes them. The capture alone filled the run's
  `MemoryMax=4G MemorySwapMax=0` scope, and the kernel killed four processes in
  it (`widget-memory: peak 4096 MiB, oom_kill 4`). Which ones was not
  recorded; mutter being among them is inferred from the later tests'
  `GTK4 init failed` cascade.
- The scan's `$(grep …)` slurp then copied about 36 M lines into bash memory,
  and the subshell that held the capture's name was killed. In a scratch
  reproduction of an OOM in that scan the capture was also gone afterwards,
  with no warning reported; that the slurp was killed, `unexpected` came back
  empty and the check passed and removed the file is inferred from that
  outcome, not traced, and it would mean the warnings were hidden as well.
- On main, one NUL byte anywhere in the output made GNU grep print only
  "binary file matches", so that scan reported no warning at all; the
  bounded stage reads the stream as text (`grep -a`).
- Nobody could clean up: the path was added to `OWNED_TEMP_PATHS` inside
  `run_headless`'s `( … )` subshell, and bash resets traps in a subshell, so
  the parent's `EXIT` trap never saw it. Even a plain SIGTERM to that subshell
  left the file.

The coordinator removed the four files on 2026-11-18 at 20:11, which freed
about 15.5 GiB of tmpfs that admission's MemAvailable reserve had not seen.
`bound-widget-log-capture` replaced the capture with a bounded scan stage
(`scripts/bounded-log-scan.sh`: an exact count, a 64 KiB excerpt, at most
1 MiB of kept lines, all on unlinked descriptors), moved the widget runner's
scan into the shell that owns the trap, and gave the runners' remaining named
entries PID-bearing names the stale sweep reclaims. A 2^23-line flood through
the widget runner in a 256 MiB scope then peaked at 12 MiB with no OOM kill,
where main's capture filled the scope and was OOM-killed
(the change's `evidence/flood-memory.md`).
