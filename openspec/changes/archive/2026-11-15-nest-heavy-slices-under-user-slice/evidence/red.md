# Red runs (failing-first evidence)

Every case below was added before the behaviour it tests, and run against the
unchanged implementation. Host: kernel 7.2.15-300.fc45, systemd 262.1, inner
`user.slice` cpuset 0,2-6,8-23 (CPU 1 and 7 outside it), caller affinity 8-23
(`taskset -c 8-23`), 2026-11-15.

## 1. `with-memory-cap.sh --self-test`, new cases, implementation of main (task 1.2)

State: the new self-test (per-run private slice derived from the real slice
name, so on main's naming it is `lushtext-heavy-selftest-<pid>.slice`, which
main's guard accepts) over main's wrapper body. Command:
`taskset -c 8-23 ./scripts/with-memory-cap.sh --self-test`. Exit 1, 16 cases
failed:

```
self-test: LUSHTEXT_HEAVY_SLICE=lushtext-heavy-selftest.slice gave status 0 ('with-memory-cap: MemoryMax=64M MemorySwapMax=0 OOMPolicy=continue in lushtext-heavy-selftest.slice (48G) for: true'), expected 2 naming user-lushtext-heavy
self-test: --placement-verdict '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-1.scope' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '8-23' '8-23' '8-23' gave status 2, expected 0
self-test: --placement-verdict '/u/user.slice/libpod-1.scope/container' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '8-23' '8-23' '8-23' gave status 2, expected 78
self-test: --placement-verdict '/u/lushtext.slice/lushtext-heavy.slice/run-1.scope' '/u/user.slice' '/u/lushtext.slice/lushtext-heavy.slice' '8-23' '8-23' '8-23' gave status 2, expected 78
self-test: --placement-verdict '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-1.scope' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '0-23' '8-23' '8-23' gave status 2, expected 78
self-test: --placement-verdict '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-1.scope' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '0-23' '' '0-23' gave status 2, expected 0
self-test: --placement-verdict '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice/run-1.scope' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '8-11,16,20-23' '8-23' '8-23' gave status 2, expected 0
self-test: --placement-verdict '' '/u/user.slice' '/u/user.slice/user-lushtext.slice/user-lushtext-heavy.slice' '8-23' '8-23' '8-23' gave status 2, expected 78
self-test: a CPU refusal did not name both lists: 'with-memory-cap: invalid cap '--placement-verdict' (expected e.g. 16G, 12288M, or none)'
self-test: a failed read did not name the read: 'with-memory-cap: invalid cap '--placement-verdict' (expected e.g. 16G, 12288M, or none)'
self-test: placement_check is not defined (no re-read of a CPU disagreement)
self-test: --print-slice printed '', expected 'lushtext-heavy-selftest-2833045.slice'
self-test: scope path '/user.slice/user-1000.slice/user@1000.service/lushtext.slice/lushtext-heavy.slice/lushtext-heavy-selftest.slice/lushtext-heavy-selftest-2833045.slice/run-p2833146-i32180369.scope' is not under user.slice ('/user.slice/user-1000.slice/user@1000.service/user.slice') and the heavy slice ('/user.slice/user-1000.slice/user@1000.service/lushtext.slice/lushtext-heavy.slice/lushtext-heavy-selftest.slice/lushtext-heavy-selftest-2833045.slice')
self-test: a caller on every online CPU got '0-23' in the scope, expected a subset of user.slice's 0,2-6,8-23
self-test: a caller pinned to CPU 1 got '1|accepted|' in the scope, expected user.slice's 0,2-6,8-23 and a refused taskset -c 1
self-test: --exec-in-scope outside lushtext-heavy-selftest-2833045.slice gave status 2 ('with-memory-cap: invalid cap '--exec-in-scope' (expected e.g. 16G, 12288M, or none)'), expected 78 naming '/user.slice/user-1000.slice/user@1000.service/lushtext.slice/lushtext-heavy.slice/lushtext-heavy-selftest.slice/lushtext-heavy-selftest-2833045.slice' and no marker
with-memory-cap self-test: 16 case(s) failed
```

Not red, by design:

- **No CPU property** passed on main too (`CPUWeight=[not set]`, empty
  `AllowedCPUs=` on the scope, `lushtext-heavy-selftest-<pid>.slice` and
  `lushtext.slice`): it guards a non-change.
- **Isolation** passed on main: the private slice already isolates the run.
  It would be red only if the self-test used the shared slice.
- **Legacy accounting** and **legacy left alone** were skipped: on main's
  naming the legacy name of the private slice is the private slice itself.
  Their red run is the intermediate state of task 2.4, below.
- The leftover check (the `--self-test` parent, after the child returns)
  found no `*selftest-<child pid>.slice.d` directory: the `EXIT` trap's stop
  and revert removed it.

## 2. `heavy.sh --self-test`, "capped scope" case (task 1.3)

The case now picks a CPU from the intersection of the caller's affinity and
the inner `user.slice`'s effective cpuset, expects the slice's whole set when
they share none, and takes the slice from `with-memory-cap.sh --print-slice`
instead of the literal `/lushtext-heavy.slice/`. Against main's wrapper
(`taskset -c 8-23 ./scripts/heavy.sh --self-test`), exit 1:

```
usage: …/scripts/with-memory-cap.sh [--address-space] <cap|none> <command> [args...]
heavy self-test: with-memory-cap.sh --print-slice failed
```

## 3. Legacy accounting: renamed, no loop yet (task 2.4)

State: the rename, the guard, `--print-slice` and the placement check in place;
`admitted_kib` still counted the current slice alone. Same command, exit 1:

```
self-test: a 2G scope in the legacy lushtext-heavy-selftest-2855573.slice was not counted (status 0, expected 75)
with-memory-cap self-test: 1 case(s) failed
```

After wrapping the unchanged per-scope body in the loop over the current and
legacy slices, the same command passed (exit 0, 3.3 s wall):

```
with-memory-cap self-test: ok (scope sets MemoryMax, no swap and OOMPolicy=continue inside user-lushtext-heavy-selftest-2858141.slice, under the user manager's user.slice, with no CPU property; CPUs: a caller on every online CPU narrowed, one pinned to CPU 1 given 0,2-6,8-23; the placement check refuses a process outside its slice (78) and re-reads a CPU disagreement; old slice names refused; nested calls run in place; --address-space fails an allocation cleanly; Ctrl-C reaches the command; arguments with $ pass unexpanded; --wait-admissible waits without reserving; NOWAIT admission fails fast, even behind a waiter; a shared self-test slice's scope not counted; legacy: a legacy scope counted, the legacy slice untouched; the real slice's budget untouched)
```

## 4. `measure.sh --self-test`, reference CPU set cases, implementation of main (task 3.1)

State: the self-test exports its own affinity as `LUSHTEXT_REFERENCE_CPUS`
and adds the reference cases (CPUs computed at run time: `c`=8, the
unavailable CPU = 1, the first online CPU outside the toolbox cpuset) over
main's `measure.sh`. `taskset -c 8-23 ./scripts/measure.sh --self-test`,
exit 1:

```
measure self-test: --print-reference-cpus with no override printed 'usage: …/scripts/measure.sh [--label L] [--log FILE] [--lock-timeout S] -- <command> [args...]
       …/scripts/measure.sh --self-test', expected 8-23
measure self-test: a reference of CPU 8 gave the command '8-23 | ', expected '8 8|override '
measure self-test: the records lack cpus=8 reference=8 source=override: 2026-11-15T01:55:33-03:00 before label=ref  loadavg=24.52 24.24 18.06 2026-11-15T01:55:34-03:00 after label=ref status=0 seconds=0.004 loadavg=24.52 24.24 18.06 
measure self-test: reference '1' gave status 75 (expected 78, command not run): measure: LOCK still held after 1 s; released INTENT and gave up. Holders of <tmp>/responsiveness-measure.lock:
    3519313       00:01 sleep 30
measure self-test: the refusal of '1' does not name both sets: measure: LOCK still held after 1 s; released INTENT and gave up. Holders of <tmp>/responsiveness-measure.lock:
    3519313       00:01 sleep 30
measure self-test: reference '8,1' gave status 75 (expected 78, command not run): measure: LOCK still held after 1 s; released INTENT and gave up. Holders of <tmp>/responsiveness-measure.lock:
    3519313       00:02 sleep 30
measure self-test: the refusal of '8,1' does not name both sets: measure: LOCK still held after 1 s; released INTENT and gave up. Holders of <tmp>/responsiveness-measure.lock:
    3519313       00:02 sleep 30
measure self-test: reference 'abc' gave status 75 (expected 2, command not run): measure: LOCK still held after 1 s; released INTENT and gave up. Holders of <tmp>/responsiveness-measure.lock:
    3519313       00:03 sleep 30
measure self-test: a CPU set changed during the command gave status 0, expected 78
measure self-test: the after record lacks status=0 and cpus_changed=8: 2026-11-15T01:55:39-03:00 before label=change  loadavg=23.04 23.94 17.99 2026-11-15T01:55:40-03:00 after label=change status=0 seconds=0.004 loadavg=23.04 23.94 17.99 
measure self-test: a CPU set changed during the lock wait gave status 0 (expected 78, command not run)
measure self-test: 11 reference CPU case(s) failed
exit=1
```

Not red, by design: the existing cases (the export is a no-op on main), the
order case (`heavy.sh --mem none -- measure.sh` with an unavailable reference
exits 2 on main too: a guard that the new check stays behind the
inside-heavy refusal) and the flip case (main reads nothing to flip).

## 5. `heavy.sh --self-test` with an unusable inherited reference (task 3.1)

State: `measure.sh` with the reference check (task 3.3), `heavy.sh`'s
self-test not yet exporting its own affinity. The Makefile line hands it one
past the last possible CPU (`LUSHTEXT_REFERENCE_CPUS=24` here), as a
game-time hook or a host without 8-23 would. Exit 78, from the nested-heavy
case's `measure.sh -- true`:

```
measure: reference CPU set 24 is not available (this cgroup allows 0,2-6,8-23); not measuring elsewhere
measure: a CST game profile narrows the toolbox cpuset while a game runs: wait for it to end, or measure on another set explicitly with LUSHTEXT_REFERENCE_CPUS (recorded as an override, not comparable with reference runs)
```

## 6. The lane driver's CPU-set cases (task 3.2)

`/usr/bin/python3 scripts/responsiveness-smoke-driver.py self-test`, with the
new cases (`lane_affinity` unset/equal/mismatch/over the cap, the
`cpu_set_fields` record, the three report labels and the mixed one, and four
`merge_runs` merges) over main's driver, exit 1 at the first case:

```
    expect(lane_affinity(range(24), None, 16) == "0-15", "unset: the first 16 CPUs of the inherited affinity")
           ^^^^^^^^^^^^^
NameError: name 'lane_affinity' is not defined
```

With `lane_affinity`, `cpu_set_fields` and `affinity_label` in place but
`merge_runs` still copying the first part's fields, exit 1 in the merge
cases (a merge of a part written before the fields has no `reference_run`):

```
    expect(old_unpinned["affinity"] == "8-23" and old_unpinned["reference_run"] is False, f"a part without the fields equals an unpinned part ({old_unpinned})")
                                                  ~~~~~~~~~~~~^^^^^^^^^^^^^^^^^
KeyError: 'reference_run'
```

With the merge rule: `responsiveness-smoke-driver self-test passed.`
