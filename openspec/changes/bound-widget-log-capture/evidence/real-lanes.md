# Proof on real lanes (tasks 4.1 and 4.3)

## 4.1 `scripts/heavy.sh --envelope 16G -- make test`, final tree

2026-11-19 01:19–01:31, tree at `4b92aed6` (every runner change in place).
Load (one-minute) before 10.56, after 10.26: a lane run on a shared
machine, not a verdict.

- Non-widget: `2628 tests run: 2628 passed (2 slow), 13 skipped`.
- Widget shards (1724 widget tests), each through `run-widget-tests.sh` and
  so through the bounded stage:

  ```
  Shard        Status  Attempts  Selected/expected  Minutes  Peak MiB  Failed  FLAKY
  window       ok      1         432/432            6.5      436       -       -
  editor-page  ok      1         413/413            4.7      391       -       -
  surfaces     ok      1         879/879            9.6      1225      -       -
  heavy: envelope …: peak 6833 MiB, oom_kill 0
  ```

- No `TEMP-LEAK:`, `FLAKY`, `OOM-KILL:` or `SESSION-HYGIENE:` line; every
  shard's warning scan passed (the same verdict as main, which passes the
  same suite).

## 4.3 Branch flood rerun and `/tmp/tmp.*` names

- The task-1.2 branch command, rerun on the final tree (2026-11-19 01:49):
  `widget-memory: peak 13 MiB, oom_kill 0`, the exact count 8,388,608, the
  slice's `oom_kill` unchanged at 32, the private base empty, 11.2 s.
- `ls -d /tmp/tmp.* | wc -l`, by name only: 4,290 before 4.1, 4,310 after
  4.2 (22 new names, 60–88 bytes each, in bursts of four or five). In a
  three-minute window afterwards in which this stage ran nothing, the count
  went from 4,311 to 4,316: the names come from elsewhere at about the same
  rate (another project's agents; the maintainer owns them). None of this
  change's runners creates a `tmp.*` name any more (no `mktemp` capture
  remains in them, and the capture and helper cases assert that a private
  base gains no entry), and nothing was touched.
