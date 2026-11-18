# Lane case 4.4: `scripts/heavy.sh --envelope 24G -- make mutants-smoke`

Command, launched with the worktree's `scripts/heavy.sh`, detached
(`setsid`), from the branch at `10fa0162` plus the uncommitted group 4 edits
(the Makefile preflights, the envelope peak line and the sub-slice stop):

```
scripts/heavy.sh --envelope 24G -- make mutants-smoke MUTANTS_MEMORY_MAX=24G \
  MUTANTS_LOCAL_JOBS=1 MUTANTS_RE=chunked_threshold_with
```

The focused scope is the one `reduce-build-disk-writes` lane case 3.6 used
(6 mutants in the default smoke file, `model/document_scale.rs`).

| | |
| --- | --- |
| Start, load | 2026-11-17T23:02:58-03:00, 3.85 |
| End, load | 2026-11-17T23:05:40-03:00, 13.80 |
| Exit status | 0 |
| Envelope | `with-memory-cap: envelope MemoryMax=24G MemorySwapMax=0 in user-lushtext-heavy-env1439791t1794967379101392301.slice (reserved in user-lushtext-heavy.slice, 48G, by lushtext-cap-1439791-1794967379101392301.scope)` |
| Recipe preflight | `./scripts/with-memory-cap.sh --preflight "24G"` passed inside the envelope |
| Inner scope | `with-memory-cap: MemoryMax=24G MemorySwapMax=0 OOMPolicy=continue in user-lushtext-heavy-env1439791t…slice (24G) for: ./scripts/run-mutants.sh smoke`: admitted against the envelope, not the host slice, so no wait under `LOCK` |
| Mutants | slot-1, baseline 14 s build + 48 s test, `6 mutants tested in 3m: 6 caught`; 0 caught by the address-space limit |
| Envelope peak | `heavy: envelope user-lushtext-heavy-env1439791t1794967379101392301.slice: peak 9460 MiB, oom_kill 0` |

## Sub-slice lifecycle seen on the real slice

- Before the run, `systemctl --user list-units --all 'user-lushtext-heavy-env*'`
  listed the two empty sub-slices the `make test` runs of lane case 4.3 had
  left (`env862173t…`, `env1181072t…`). Fifteen seconds after this
  envelope's admission only its own sub-slice was listed: the admission's
  sweep stopped them (placeholder gone, `cgroup.events` `populated 0`).
- After the run the listing was empty: the placeholder stopped its own
  sub-slice once its sentinel was stopped and nothing was left in it.

## Size

The 24G envelope used here is the task's figure for a one-slot run; the
documented default stays `--envelope <MUTANTS_MEMORY_MAX>` (36G with three
slots). One slot peaked at 9.2 GiB, so three slots at their
`MUTANTS_LOCAL_BUILD_JOBS=4` fit 36G as the existing cap assumed; no default
changes.
