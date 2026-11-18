# Failing-first evidence (task 1.2, 1.3)

Script version under test: the branch at `eda53775` (the scripts identical to
`main` `c16d6fdc`) plus the new self-test cases, which run only with
`LUSHTEXT_ADMISSION_WINDOW_RED=1`. Host: kernel 7.2.15, systemd 262.1, bash
5.3.15. Run 2026-11-17, one-minute load 4.4.

Commands (run directly; the self-tests use private lock directories, private
per-run slices and a private admission lock):

```
LUSHTEXT_ADMISSION_WINDOW_RED=1 ./scripts/with-memory-cap.sh --self-test
LUSHTEXT_REFERENCE_CPUS=<one past the last CPU> LUSHTEXT_ADMISSION_WINDOW_RED=1 ./scripts/heavy.sh --self-test
```

## with-memory-cap.sh --self-test (exit 1, 12 cases failed)

```
self-test: --admission-verdict 3145728 104857600 6291456 1048576 2097152  gave status 2, expected 0
self-test: --admission-verdict 3145728 104857600 6291456 1048576 4194304  gave status 2, expected 1
self-test: --admission-verdict 3145728 2097152 6291456 1048576 2097152  gave status 2, expected 1
self-test: --admission-verdict 3145728 104857600 6291456 1048576 2097152 4194304 gave status 2, expected 1
self-test: --admission-verdict 3145728 104857600 10485760 1048576 2097152 4194304 gave status 2, expected 0
self-test: --admission-verdict 3145728 41943040 50331648 16777216 2097152 37748736 gave status 2, expected 0
self-test: --admission-verdict 0 0 2097152 - 2097152  gave status 2, expected 0
self-test: --admission-verdict 0 0 2097152 - 2097152 2097152 gave status 2, expected 1
self-test: --admission-verdict 1048576 0 2097152 - 2097152  gave status 2, expected 1
self-test: a check-once admission behind a busy admission lock gave status 0 after 70 ms (report '', marker created), expected 75 within 4 s, a 'misfit: ' line and no marker
self-test: a 2G request was not held behind an older 4G waiter in a 6G slice with 3G admitted (status 0, expected 124)
self-test: the 4G waiter was not admitted within 10 s of the scope ahead of it ending
with-memory-cap self-test: 12 case(s) failed
```

- The verdict fixtures: `--admission-verdict` is unknown on `main`, so it
  falls through to the usage error (2).
- The 2 s bound: `main` ignores `LUSHTEXT_ADMISSION_ONCE` and
  `LUSHTEXT_ADMISSION_LOCK`. It admitted through the real lock and ran the
  command. The case never holds the real lock.
- Fair order: first-fit admits the 2G request beside the waiting 4G one. The
  4G waiter then misses the 10 s window because `main` ignores
  `LUSHTEXT_ADMISSION_POLL_SECONDS` and sleeps 15 s between checks. The NOWAIT
  guard (a 2G NOWAIT request is admitted) passes on `main`, as intended.

An earlier run of the same cases failed for the wrong reason: the legacy
accounting case's 2G sleeper was still counted, so the slice was fuller than
the cases assumed. The cases now stop the earlier sleepers first.

## heavy.sh --self-test (exit 1, 5 admission cases failed)

```
heavy self-test: a misfit found after LOCK kept LOCK held (exclusive try status 1, marker absent)
heavy self-test: a command's own exit 75 under heavy.sh --envelope 2G gave status 2 after 0 run(s), expected 75 after 1
heavy self-test: behind a busy admission lock heavy.sh kept LOCK (exclusive try status 0) or started the command (marker created)
heavy self-test: heavy.sh --mem none -- with-memory-cap.sh 2G gave status 0 (exclusive try status 0, stderr 'with-memory-cap: waiting for memory: 2 GiB of 3 GiB admitted to user-lushtext-heavy-selftest-h101718.slice, 39 GiB available, need 2 GiB plus a 1 GiB reserve '), expected 2 naming --envelope within 3 s
heavy self-test: an inner 2G call in a 2G envelope ran in '' (stderr 'usage: …/scripts/heavy.sh [--mem <cap|none>] -- <command> [args...] …'), expected a lushtext-cap scope under user-lushtext-heavy-selftest-h101718-env*.slice
heavy self-test: 5 admission case(s) failed
```

- The window after the probe: the `systemctl` shim makes the probe and the
  NOWAIT re-probe see the private slice empty, and the admission see it with
  its 2G sleeper (budget 3G, reserve 1G). The blocking `admit` then held
  `LOCK` shared, and a measurement's `INTENT`-then-`LOCK` could not be taken
  in 4 s.
- Exit 75 is the command's: under `--mem 2G` it passes on `main`, as a
  guard. Under `--envelope 2G` it fails with a usage error.
- A busy admission lock: `main` used the real admission lock and started the
  command while the private one was held.
- `--mem none` inner path: the inner admission waited for memory under
  `LOCK` held shared ("waiting for memory: 2 GiB of 3 GiB admitted"), and
  `heavy.sh` had not exited after 3 s. (The message read "status 0" for a
  process still running. The final cases print "still running" instead.)
- The envelope: `--envelope` is a usage error on `main`.

## Guarded and unguarded runs (task 1.3)

- With `LUSHTEXT_ADMISSION_WINDOW_RED=1`, exactly the cases above fail on the
  current scripts.
- Without it, `make check-agent-helpers` passes, and so does the commit hook.
  See the commit of these cases, made through the main checkout's
  `scripts/heavy.sh --mem none -- git commit`.
