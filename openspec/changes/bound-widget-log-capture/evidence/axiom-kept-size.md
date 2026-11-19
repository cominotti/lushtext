# GTK axiom kept-output size (task 4.2, design D6)

`scripts/heavy.sh --envelope 16G -- make gtk-axioms` at `4b92aed6` plus the
then-uncommitted summary-line edit committed as `d65b413f`
(2026-11-19 01:33–01:45, shipped toolkit GTK 4.24.1 / Libadwaita 1.10.0,
local default `GTK_LUSH_AXIOMS_INCLUDE_SLOW=1`):

```
GTK axioms: probe binary passed; 109 sample checks held (0 slow skipped by design; 109 kept lines, 61591 of the 1048576-byte cap).
heavy: envelope …: peak 3882 MiB, oom_kill 0
```

- Kept output: **61,591 bytes, 5.9 % of the 1 MiB cap**, below the quarter
  (262,144 bytes) at which design D6 says to revisit the cap; the cap stays.
- The 109 kept lines are exactly the 109 sample observations (`{"axiom"…}`
  lines in the live stream: 109), so the result count matched the samples
  run, and the probe binary's own run passed its scan.
- No whole-output log was written (the runner has no `mktemp` left).
- Load before 10.56, after 7.01 (one-minute), a lane run, not a verdict.
