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
  `PALETTE_CATEGORY_CANDIDATE_BYTES` they cite stays, because
  `SearchResultItem::File` carries a `u32` ordinal (design D1, D8). If the
  layout assertion still moves, rerun `make kani KANI_SHARD=core-list-budget`
  once, after the code is final, and update its budget and threshold row.
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
- [ ] 1.3 Re-probe ledger axiom A95 at 2^18 and 2^20 rows, as a **separate**
      slow measurement (a `slow` probe entry, or a size list the runner only
      takes when slow probes are included). Leave the existing fast 16,384 /
      131,072 pair in the pull-request `GTK Axioms` lane unchanged. Record the
      far select-and-scroll time and `get_item` count per size in the ledger
      row, "Verified against" the pinned GTK and libadwaita. Run
      `make gtk-axioms` (slow probes included) and `make check-gtk-axioms`.
      This sets the cap in either direction before task 4.4 (design D8): if
      2^18 rows fit `B_INTERACT_US`, revisit the cap upward; if the
      application figure at 131,072 (DP: 2.2–6.9 ms; the D10 verdict gates)
      does not fit, lower it, never below 2^16. Record the decision.
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
      most 96 B/file, and `retained_index_bytes` at most 60 B/file (fails on
      `main`: 557 and 363 B/file). 96 leaves a quarter of headroom when the
      per-file figure carries to 2^20, where 128 B/file is the cap itself;
      task 4.1 asserts the 2^20 traversal directly.
- [ ] 2.4 Unit, mutations: adding one file to a 2^20 index reports, through a
      new mutation-ledger evidence field, at most a logarithmic number of
      path comparisons (fails on `main`: one per file) and is admitted within
      64 MiB (fails if growth doubles a shrunk vector, design D3); a directory
      rename over 10,000 descendants reports one canonical resolution (fails on
      `main`: 10,000). Also the D5 cases, each against today's observable
      result: a rename of a workspace folder root and a rename to a path
      outside every folder (display strings equal `main`'s), a rename into a
      new directory, two adds of one path in one batch (the second refused),
      and a ledger-refused directory rename (its subtree dropped). Plus
      reclamation: on a 2^17 index, remove and re-add 2^17 files in batches
      of 1,024. Every add is admitted, and `retained_byte_weight` returns to
      within one growth step of the start (fails if dead bytes are never
      compacted). And near the 64 MiB cap, with dead bytes below a quarter of
      the weight, an add that fits the live size is still admitted (design
      D5's second compaction trigger).
- [ ] 2.5 Unit, open tabs: the empty query's rest count at 2^20 with 256 open
      tabs equals the exact count and its selector metrics report no walk over
      the rest (fails on `main`: `RestCount::WalkWith` visits every file).
- [ ] 2.6 Property (`tests/properties/palette.rs`, `property-tests`), design
      D6: the arena and the retained per-file reference agree on generated
      trees, overlapping folders, symlinks (both traversal orders), a non-UTF-8
      name (two names sharing one lossy string), and mutation sequences, each
      applied to the temp tree first. It checks the file list, order,
      identity, query results and empty-query counts; `FileLimit` /
      `DirectoryRetentionLimit` under scaled count limits; and, under scaled
      byte limits, the arena list as a prefix of the reference's untruncated
      list. Red until the arena exists, then green.
- [ ] 2.7 Unit, `ui/command_palette/policy.rs`: `category_row_limit()` equals
      `MAX_CATEGORY_VIEW_ROWS` (131,072) when `MAX_INDEXED_FILES` is 2^20, and
      the two floor-retention `const` assertions derive from the row cap (fails
      on `main`: the constant does not exist and the floor follows
      `MAX_INDEXED_FILES`).
- [ ] 2.8 Widget (`command_palette_category.rs`): a 2^20-file index, a query
      matching more than 131,072 files, Enter on the workspace More row. The
      view lists 131,072 rows and says "Showing 131,072 of N results". The End
      dispatch and the far select-and-scroll are each bounded at
      2 × `B_INTERACT_US`, the widget-harness convention DP's tests use; the
      verdict at ≤ `B_INTERACT_US` is lane case D10 (design D8). Also a note
      category over the cap is cut the same way (the recorded loss). Fails on
      `main` (the index stops at 131,072). The existing tests at the old
      ceiling switch from `MAX_INDEXED_FILES` to `MAX_CATEGORY_VIEW_ROWS`, so
      they keep their 131,072 fixtures and now prove "no visible loss at the
      row cap"; rename `test_palette_category_no_visible_loss_at_the_index_ceiling`
      to match. The retirement tests in `command_palette.rs` (`retire-full`)
      move to `LARGE_INDEX_RETIREMENT_FILES` (design D3), and
      `classify_index_retirement`'s unit tests change from "exactly at the
      cap" to "at least the threshold". Build the 2^20 fixture through the
      streaming constructor (task 3.6), never a `Vec` of 2^20 owned paths.
      Then `make check-widget-shards`: if the `surfaces` shard's measured time
      grows past its margin, move the test or split the shard as
      `scripts/widget-shards.py` prescribes.
- [ ] 2.9 Commit and push the red tests. The hook runs all-targets Clippy, so
      no test may name a symbol that does not exist. For each test, choose
      and record in the handoff one of two forms:
      - **Assertion-red:** add the minimal compiling stub first, with the
        `main` behaviour, and the test fails on its assertion. This applies to
        `canonical_resolutions` and the mutation evidence field (counting
        today's work), `MAX_CATEGORY_VIEW_ROWS` and
        `LARGE_INDEX_RETIREMENT_FILES` (defined equal to today's values), and
        a stubbed streaming constructor.
      - **Recorded-red:** for the property test 2.6, which needs the arena's
        API. Its red run is recorded against a local branch, and it is
        committed in 3.8 with the arena.

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
      from owned path triples. Add a `test-utils` streaming constructor
      (`FileIndex::from_paths(impl IntoIterator<…>)`), so 2^20-file fixtures
      in unit, widget and bench code never hold 2^20 owned paths at once.
      `SearchResultItem::File(FileOrdinal)` consumers build the view with
      `index.file(ordinal)` (design D1); 3.6 makes no visual change.
- [ ] 3.7 Limits (D3): `MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES` 2^20,
      doc comments restated (the count is the latency bound, the ledgers the
      memory bound); `classify_index_retirement` re-keyed on
      `LARGE_INDEX_RETIREMENT_FILES`. In `docs/thresholds.md`: the moved
      constants' rows with the new path column, and new rows (with `Kind:`
      lines in code) for `LARGE_INDEX_RETIREMENT_FILES`,
      `INDEX_INCREMENTAL_GROWTH_DIVISOR` and `MAX_QUERY_CHUNKS` (design D3);
      then `make check-thresholds`.
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
      - `palette_pipeline_hardening` gains a 2^20 group
        (`palette_pipeline_hardening_1048576`, rapid queries over the maximum
        index, the MODIFIED performance scenario).
      - `file_index_rebuild` at 2^20 over the D9 tree on disk asserts no
        truncation, `peak_build_bytes` ≤ 128 MiB and `retained_index_bytes`
        ≤ 64 MiB, and prints both. That is the real-traversal proof of the
        1,048,576-file spec scenario.
      - `make performance-smoke` keeps its filters on the 2^17 and existing
        groups; the 2^20 groups run in `make bench` only, so the smoke stays
        lightweight. Update `scripts/run-performance-smoke.sh`'s labels if
        IDs change (task 3.7a).
- [ ] 4.2 Verdict measurement of D7: the worker query p50 at 2^20 (release,
      four interleaved rounds against the branch's own single-pass build if
      4.3 is needed). Pass bar: ≤ 50 ms. Also measure the category view's
      single-pass ranking at 2^20 (Enter on the More row of a query matching
      more than 131,072 files). If it is over 50 ms, record it in the design
      as the measured exception D7 names.
- [ ] 4.3 Only if 4.2 misses the bar: chunked scoring for grouped queries over
      `min(MAX_QUERY_CHUNKS, available_parallelism())` scoped threads (no
      split below 2^17 files), per-chunk matcher and heap of at most the
      group's limit, the shared cancellation, merged by score then ordinal.
      The category ranking stays one pass (design D7). D6's property test
      covers equality with the single pass. Re-measure, and record both in
      the design.
- [ ] 4.4 `MAX_CATEGORY_VIEW_ROWS` (131,072, or the value 1.3 justified, never
      below 2^16) in `ui/command_palette/policy.rs`; the floor retention
      derived from it; `category_row_limit()` returns the smaller of the cap
      and the budget's rows; the category model's cut reports "Showing X of Y
      results". Its `docs/thresholds.md` row, and the
      `PALETTE_CATEGORY_FLOOR_RETENTION` / `PALETTE_CATEGORY_RETAINED_BYTES`
      rows re-worded to "the ranking peak of the row cap". If 1.3 lowered the
      cap, the spec delta's 131,072 figures (the category requirement, its
      "A category larger than the row cap" scenario, the 2.8 test and
      design D8) move with it in the same commit. Test 2.8 green.
- [ ] 4.5 Commit and push.

## 5. Lane, verdicts and smoke

- [ ] 5.1 Responsiveness lane group `workspace-index` (design D10) in
      `scripts/responsiveness-smoke-driver.py`: cases at 2^17 and 2^20 from the
      1.1 generator, the palette open, a typed query's publish, the empty
      query, Enter on the More row, Page Down and End in the category view.
      The 2^20 cases run in the `full` tier only, with a size-scaled
      `command-palette-index` timeout. `make check-responsiveness-lane`
      self-tests the new cases, and `docs/runbooks/responsiveness-lane.md`
      documents the group (design D10).
- [ ] 5.2 Verdict run of the lane cases and the Criterion groups, `main`
      against the branch at 2^17 (four interleaved rounds; the `main` side runs
      this branch's driver against a `main` build) and the branch at 2^20.
      Gate on "End in the category view" at 131,072 rows ≤ `B_INTERACT_US`
      (design D8). Pass bars: design "Timing". Record figures, loads and
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
      implemented with its verdict figures and follow-ups. Record the
      synthetic-tree deviation from "a measurement on a real monorepo" and
      the note-category loss above 131,072 rows, the real-tree measurement as
      a follow-up, and close the DP "accepted items" entry for the
      empty-query count.
- [ ] 6.5 GTK axiom ledger A95 row (1.3); `.agents/skills/gtk-testing`
      references if a testing pitfall appeared.
- [ ] 6.6 Commit and push.
