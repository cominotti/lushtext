# Proof on real lanes (tasks 4.1 and 4.3)

## 4.1 `scripts/heavy.sh --envelope 16G -- make test`

Three runs. The scan's `grep -a` (`a9ef5b13`, `96dcae3e`) is on the widget
verdict path: it changes nothing for text output but sees past a NUL byte,
so only the runs at `96dcae3e` and later are the final tree's. None of the
logs of the three runs contains "binary file matches", so the earlier run
was not blind on any stream.

| Tree | When | Load before / after | Non-widget | Widget shards (window, editor-page, surfaces) | FLAKY | Envelope |
| --- | --- | --- | --- | --- | --- | --- |
| `4b92aed6` | 2026-11-19 01:19–01:31 | 10.56 / 10.26 | 2628 passed | 432, 413, 879 ok, 1 attempt each | 0 | peak 6833 MiB, oom_kill 0 |
| `96dcae3e` | 02:00–02:21 | 5.69 / 7.78 | 2628 passed | 432, 413, 879 ok, 1 attempt each | **1**: `search_panel::test_an_apply_while_another_window_runs_is_refused` passed on retry | peak 1380 MiB, oom_kill 0 |
| `4aa2dae4` (code = `96dcae3e`) | 02:44–03:01 | 5.50 / 3.01 | 2628 passed | 432, 413, 879 ok, 1 attempt each | 0 | peak 1663 MiB, oom_kill 0 |

No `TEMP-LEAK:`, `OOM-KILL:` or `SESSION-HYGIENE:` line in any run. Main
was not rerun beside them; the classification equivalence is the
self-tests' (same regexes, same cases, plus the NUL cases).

The FLAKY is analysed in `replace-confirm-flake.md`: a Replace All confirm
refused for good by transient pressure in the process-wide disposal lane
(reproduced deterministically by a probe), in a test this change does not
touch, left for its own change.

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
