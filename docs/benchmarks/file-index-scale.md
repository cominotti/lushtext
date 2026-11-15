# Palette File Index at Scale

`scale-file-index` (2026-11) replaced the palette's per-file index (one owned
record and canonical path per file) with an interned path arena, and raised
`MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES` to 2^20 inside the unchanged
64 MiB installed and 128 MiB construction ledgers. The design, decisions and
verdicts are in
`openspec/changes/archive/2026-11-15-scale-file-index/design.md`.

## Fixture

`scripts/generate-workspace-index-fixture.py` writes a seeded,
monorepo-shaped tree (depth, files per directory and name lengths from the
design's D9 table, one symlinked directory and one symlinked file);
`services::palette::workspace_index_fixture` builds the same shape for
benches and tests through `services::filesystem::fixture`. The generator's
`--self-test` runs in `make check-responsiveness-lane`.

## Criterion groups

| Group | Sizes | Where it runs |
| --- | --- | --- |
| `file_index_search` | up to 2^17 (query, empty query, no match, 32 open tabs) | `make bench`, `make performance-smoke` |
| `file_index_rebuild` | existing cases plus the D9 tree at 2^17 | `make bench`, `make performance-smoke` |
| `file_index_incremental` | up to 2^17 (add, remove file and directory, rename file and directory, base clone) | `make bench` |
| `palette_categories` | the category view at its 131,072-row cap | `make bench` |
| `file_index_scale` | 2^20: the queries, the clone, every mutation kind, the category ranking at the row cap | `make bench` only (`LUSHTEXT_BENCH_SCALE=1`) |
| `file_index_scale_rebuild` | 2^20: the real traversal of the D9 tree on disk, asserting no truncation, `peak_build_bytes` ≤ 128 MiB and `retained_index_bytes` ≤ 64 MiB | `make bench` only |
| `palette_pipeline_hardening_1048576` | 2^20: bounded queries and rapid latest-query replacement | `make bench` only |

The 2^20 `add_file` case frees one file (untimed) before each add, so it times
an admitted add, not a refusal at the cap. A `--test` run of the three 2^20
groups (`LUSHTEXT_BENCH_SCALE=1`) executes every case and the D9 traversal:
1,048,576 files indexed with no truncation, `peak_build_bytes` 61,620,054,
`retained_index_bytes` 44,001,974, three canonical resolutions (the folder
and the two symlinks), 17.6 s wall in all.

The three 2^20 groups build the index (and write a million files) in their
group function, before Criterion applies its filter, so they run only when
`LUSHTEXT_BENCH_SCALE=1`, which `make bench` sets. Every filtered run, such as
each of `make performance-smoke`'s, skips them and says so on stderr.

## Figures

Release harness on the D9 tree (design, "Branch figures"; non-verdict, loaded
host). Per file: 41 B installed and 58 B at the construction peak, against
563 B and 831 B on `main`, so 1,048,576 files fit in 44 MB installed and
62 MB at the peak. One query over 2^20 files took 31–35 ms on the worker
(D7's bar is 50 ms); the empty query's count with 256 open tabs is under
1 ms (it was a walk over every file); a 1,024-file add batch at 2^17 took
15.5 ms against `main`'s 31.7 s. The verdict rounds are queued on the shared
machine (design, decision 13).
