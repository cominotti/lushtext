Rules for every task:

- Headless only: never `make run`, never the live desktop, never the real
  `~/.local/share/lushtext` or `~/.config`.
- CPUs 8–23 (`taskset -c 8-23`); every heavy non-measurement command through
  `scripts/heavy.sh [--mem <cap>] -- <cmd>`, one command per call; every timing
  through `scripts/measure.sh` (valid only with a recorded 1-minute load below
  4, else labelled non-verdict and retried), `main` and the branch interleaved.
- Fixtures are synthetic and generated into the run's temp scope; never copy a
  user file into a fixture.
- Evidence goes in `.claude/handoffs/scale-file-index/`.
- Failing first means the test is committed or recorded red, with the failure
  named below, before the code that turns it green.
- Kani: none (no proven module touched). `model/list_budget.rs` and its
  `core-list-budget` harnesses are not edited, and the 36-byte
  `PALETTE_CATEGORY_CANDIDATE_BYTES` they cite stays (design D8). If task 4.4
  changes that charge, rerun `make kani KANI_SHARD=core-list-budget` once,
  after the code is final, and update its budget.
- Timing: the affected cases are design "Timing": Criterion
  `file_index_rebuild`, `file_index_search`, `file_index_incremental`,
  `palette_categories` (2^17 on both builds, 2^20 on the branch), and the
  responsiveness lane group `workspace-index` (D10).

## 1. Baseline and toolkit facts (before any fix)

- [ ] 1.1 Fixture generator (design D9): `scripts/generate-workspace-index-fixture.py`
      (seeded, deterministic, `--files`, `--out`, a `--self-test` wired into
      `make check-responsiveness-lane`), plus a bench helper building the same
      shape through `services::filesystem::fixture`. Shape: depth and
      files-per-directory and name-length distributions from the design's
      table, one symlinked directory and one symlinked file. Sizes 2^17 and
      2^20.
- [ ] 1.2 Verdict baseline on `main` at 2^17 (2^20 cannot be indexed on
      `main`): build wall time and `peak_build_bytes`, `retained_index_bytes`,
      clone and drop of the index, one query (64 rows; 0 and 32 open tabs),
      the empty-query count with 32 and 256 open tabs, one incremental batch of
      1, 64 and 1,024 adds, a directory rename of 10,000 descendants. Through
      `scripts/measure.sh`, four interleaved rounds; record min–max in the
      design's "Measured baseline" beside the stage-1 non-verdict figures.
- [ ] 1.3 Re-probe ledger axiom A95 at 2^18 and 2^20 rows (extend
      `gtk_lush_axioms::probe_a95` and its sample; mark it `slow` if the 2^20
      case makes it so, with the ledger row saying where it runs). Record the
      far select-and-scroll time and `get_item` count per size in the ledger's
      row ("Verified against" the pinned GTK and libadwaita). Run
      `make gtk-axioms` and `make check-gtk-axioms`. If the growth flattens
      enough that 2^18 rows fit `B_INTERACT_US`, revise D8's cap before task 4.4
      and record it.
- [ ] 1.4 Commit (signed, conventional) and push: generator, bench helper,
      baseline record, A95 re-probe.

## 2. Failing-first tests (red before section 3)

- [ ] 2.1 Unit, `services/palette`: a 2^20-file in-memory index built through
      the public constructor reports no truncation and
      `retained_byte_weight() ≤ 64 MiB` for the D9 shape (fails on `main`:
      `FileLimit` at 131,072, and about 363 B/file).
- [ ] 2.2 Unit, build over a temp tree with no symlinks: a new
      `FileIndexBuildMetrics::canonical_resolutions` field (no global test
      counter) equals the number of workspace folders plus symlink entries
      (fails on `main`: the field does not exist; `main` resolves once per file
      and directory). Trees with a symlinked file reached **before** and
      **after** its target, and a symlinked directory, keep `main`'s winners,
      ordinals and identities (design D2).
- [ ] 2.3 Unit, build ledger: `peak_build_bytes` of the 2^17 temp tree is at
      most 128 B/file (fails on `main`: 557 B/file).
- [ ] 2.4 Unit, mutations: adding one file to a 2^20 index reports, through a
      new mutation-ledger evidence field, at most a logarithmic number of
      path comparisons (fails on `main`: one per file) and is admitted within
      64 MiB (fails if growth doubles a shrunk vector, design D3); a directory
      rename over 10,000 descendants reports one canonical resolution (fails on
      `main`: 10,000).
- [ ] 2.5 Unit, open tabs: the empty query's rest count at 2^20 with 256 open
      tabs equals the exact count and its selector metrics report no walk over
      the rest (fails on `main`: `RestCount::WalkWith` visits every file).
- [ ] 2.6 Property (`tests/properties/palette.rs`, `property-tests`), design
      D6: the arena and the retained per-file reference agree on generated
      trees, overlapping folders, symlinks, a non-UTF-8 name and mutation
      sequences (file list, order, identity, query results, empty-query counts,
      truncation under scaled limits). Red until the arena exists (does not
      compile), then green.
- [ ] 2.7 Unit, `ui/command_palette/policy.rs`: `category_row_limit()` equals
      `MAX_CATEGORY_VIEW_ROWS` (131,072) when `MAX_INDEXED_FILES` is 2^20, and
      the two floor-retention `const` assertions derive from the row cap (fails
      on `main`: the constant does not exist and the floor follows
      `MAX_INDEXED_FILES`).
- [ ] 2.8 Widget (`command_palette_category.rs`): a 2^20-file index, a query
      matching more than 131,072 files, Enter on the workspace More row: the
      view lists 131,072 rows and says "Showing 131,072 of N results"; End and
      the far select-and-scroll stay within `B_INTERACT_US` (bounded at twice
      the worst measured, as DP's tests do). Fails on `main` (the index stops at
      131,072). Keep the existing 131,072 tests at 131,072 (not
      `MAX_INDEXED_FILES`), so they keep proving what they proved; the
      retirement tests in `command_palette.rs` (`retire-full`) move to
      `LARGE_INDEX_RETIREMENT_FILES` (design D3) and fail on `main` until the
      re-keyed `classify_index_retirement` exists.
- [ ] 2.9 Commit and push the red tests (the hook's Clippy must pass: mark
      the not-yet-compiling property test behind the arena's API added in 3.1
      if needed, recording where each test fails).

## 3. The arena and its identity rule

- [ ] 3.1 `services/palette/index/` split: `arena.rs` (records, names, keys,
      symlink identities, views), `traversal.rs` (the bounded build),
      `mutation.rs` (D5), `mod.rs` keeping today's public surface names.
      `model/palette.rs`: `IndexedFile<'a>` view; `SearchResultItem::File`
      carries it and stays 16 bytes (the existing layout assertion).
- [ ] 3.2 `services/file_tree.rs` and `services/filesystem`: a symlink flag on
      scanned entries from the directory entry type, `lstat` only for an
      unknown type; `make check-filesystem-boundary`.
- [ ] 3.3 Construction with derived canonical identity (D2), the hashed
      visit set with exact collision checks (a test-only hasher forces
      collisions), the canonical-key table (D4), ledger charges for every arena
      growth and scratch (D3), the four truncation reasons unchanged.
- [ ] 3.4 Incremental mutations (D5) under `FileIndexMutationLedger`; the
      worker in `ui/command_palette/index_execution.rs` keeps its ticket,
      replay and overflow rebuild.
- [ ] 3.5 `OpenTabExclusion` over the key table; `RestCount` for the
      workspace group from the open ordinals (D4).
- [ ] 3.6 Consumers: `ui/command_palette/{item,category_model,policy}.rs`,
      `ui/window/palette_shell.rs`, `services/palette/{fuzzy,category,grouped}.rs`,
      benches (`make_synthetic_index`), property and widget tests.
      `FileIndex::from(Vec<…>)` stays for benches and tests as a constructor
      from owned path triples.
- [ ] 3.7 Limits (D3): `MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES` 2^20,
      doc comments restated (the count is the latency bound, the ledgers the
      memory bound); `classify_index_retirement` re-keyed on
      `LARGE_INDEX_RETIREMENT_FILES`; `docs/thresholds.md` rows and
      `make check-thresholds`.
- [ ] 3.7a Name- and path-keyed gates after the `index.rs` split
      (`.agents/rules/build.md`): re-key or retire the
      `.cargo/mutants.toml` `exclude_re` entry naming
      `services/palette/index\.rs` and `truncate_to_index_limit`, and check
      `scripts/run-performance-smoke.sh`'s `file_index_*` filters and evidence
      labels against the renamed bench IDs; prove each by running the gate
      (`make mutants-list` shows the entry matching, the smoke filter selects
      its benches), and `git add -N` the new files before any diff-aware gate.
- [ ] 3.8 Tests 2.1–2.7 green; `make test-unit`, `make test-prop`,
      `make check`. Commit and push.

## 4. Query cost and the category cap

- [ ] 4.1 Criterion (`benches/benchmarks.rs`): `file_index_search` at 2^17
      and 2^20 (query, empty query, no match, 32 open tabs), `file_index_rebuild`
      on the D9 tree at 2^17 and 2^20, `file_index_incremental` at 2^17 and
      2^20 (add, remove file and directory, rename file and directory, the
      base clone), `palette_categories` at the row cap. Bench mirrors of the
      limits follow the constants.
- [ ] 4.2 Verdict measurement of D7: the worker query p50 at 2^20 (release,
      four interleaved rounds against the branch's own single-pass build if
      4.3 is needed). Pass bar: ≤ 50 ms.
- [ ] 4.3 Only if 4.2 misses the bar: chunked scoring over at most four scoped
      threads with per-chunk heaps and the shared cancellation, merged by score
      then ordinal; D6's property test covers equality with the single pass.
      Re-measure; record both in the design.
- [ ] 4.4 `MAX_CATEGORY_VIEW_ROWS` (131,072, or the value 1.3 justified) in
      `ui/command_palette/policy.rs`; the floor retention derived from it;
      the category model's cut reports "Showing X of Y results". Test 2.8 green.
- [ ] 4.5 Commit and push.

## 5. Lane, verdicts and smoke

- [ ] 5.1 Responsiveness lane group `workspace-index` (design D10) in
      `scripts/responsiveness-smoke-driver.py`: cases at 2^17 and 2^20 from the
      1.1 generator, the palette open, a typed query's publish, the empty
      query, Enter on the More row, Page Down and End in the category view;
      `make check-responsiveness-lane` self-tests the new cases.
- [ ] 5.2 Verdict run of the lane cases and the Criterion groups, `main`
      against the branch at 2^17 (four interleaved rounds) and the branch at
      2^20. Pass bars: design "Timing". Record figures, loads and
      `btrfs-stall-report` for each window in the design's "Measured verdicts".
- [ ] 5.3 Widget tests of the palette shards (`make test-widget-shard` for the
      shards holding `command_palette*.rs`), then the full `make test`;
      `make visual-geometry-smoke`, `make accessibility-smoke` and
      `make visual-smoke` (widget tests changed); `make check`; the rustdoc
      lint gate (`RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links -D
      rustdoc::private_intra_doc_links -D rustdoc::bare_urls" cargo doc
      --workspace --no-deps`, not in `make check`); and
      `cargo check -p lushtext --bins --locked` with default features (the
      reference module is `test-utils`-gated).
- [ ] 5.4 Commit and push.

## 6. Documentation

- [ ] 6.1 AGENTS.md: the "File index cap and incremental mutation" and
      "Arc workspace_folder" key design decisions rewritten for the arena (the
      stale 100,000 figures go), the module layout line for
      `services/palette/`; README.md if it states the index cap.
- [ ] 6.2 `docs/workflow-readability-matrix.md` WFR-COMMAND-PALETTE row:
      file set, re-derived measured cells (row-scoped, excluding
      `#[cfg(test)]`); `make check-workflow-boundaries`.
- [ ] 6.3 `docs/thresholds.md` (3.7) and `docs/benchmarks/` note for the new
      groups; `docs/automation-reference.md` only if a snapshot field changes
      (none planned: `command_palette.file_index_count` keeps its meaning).
- [ ] 6.4 `docs/next/long-line-documents.md`: programme item 3 marked
      implemented with its verdict figures and follow-ups; the DP "accepted
      items" entry for the empty-query count closed.
- [ ] 6.5 GTK axiom ledger A95 row (1.3); `.agents/skills/gtk-testing`
      references if a testing pitfall appeared.
- [ ] 6.6 Commit and push.
