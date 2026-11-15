## Context

The command palette's file index (`services/palette/index.rs`, 1,642 lines)
is built on a worker by `FileIndex::rebuild_cancellable_with_hint`, installed
behind a disposal reservation of `MAX_FILE_INDEX_RETAINED_BYTES` (64 MiB) by
`ui/window/palette_shell.rs`, and mutated by the palette's serialized
incremental worker (`ui/command_palette/index_execution.rs`), which clones the
installed `FileIndex` and applies a batch of at most `MAX_PENDING_INDEX_UPDATES`
(1,024) adds, removes and renames under a `FileIndexMutationLedger`. Queries
score `IndexedFile::name` with nucleo on the worker and return
`ScoredResult { item: SearchResultItem::File(&IndexedFile), score,
source_ordinal }`; the category view keeps `u32` source ordinals and asks
`index.files().get(ordinal)` for the rows GTK requests
(`ui/command_palette/category_model.rs`).

Today each file is an `IndexedFile { path: PathBuf, identity:
PaletteFileIdentity /* Canonical(PathBuf) | Unavailable */, name: String,
workspace_folder: Arc<PathBuf> }`: three heap allocations of about 80 + 80 + 20
bytes plus an 88-byte shell. Construction canonicalizes every file
(`indexed_file_from_path` → `fs_metadata::canonical_path`, one `realpath` per
file) and inserts a clone of each canonical path into `canonical_files:
HashSet<PathBuf>` to deduplicate overlapping workspace folders.

Programme item 3 (`docs/next/long-line-documents.md`): "an interned path arena
and a byte budget instead of the 131,072-file count, targeting at least
1,000,000 files, with a Criterion benchmark and a measurement on a real
monorepo". DP's archive left one cost here: the empty query's count reads one
canonical path per file (2.7–4.4 ms on the worker at 131,072), "which a
canonical-path count kept by the index removes".

## Measured baseline

Stage 1, 2026-11-14, worktree at `main` `2c91ccf2`, a throwaway test (kept as
`.claude/handoffs/scale-file-index/stage1-scratch_index_baseline.rs.txt`, not
committed) driving the real `FileIndex::rebuild_cancellable_with_hint` over a
generated tree in tmpfs: 131,072 empty files in 8,223 directories (16 files per
directory, Chromium-like directory names such as
`third_party/blink/renderer/core/layout/module_00012`, names such as
`layout_block_flow_7.cc`), workspace folder path 32 bytes. Logs:
`stage1-baseline.log` (debug) and `stage1-baseline-release.log` (release).

| M | Figure (131,072 files) | Debug | Release |
| --- | --- | --- | --- |
| B1 | `retained_index_bytes` | 47,644,732 (363 B/file) | same (deterministic) |
| B2 | `peak_build_bytes` | 73,045,157 (557 B/file) | same |
| B3 | build wall time | 1,738–1,861 ms | 911–916 ms |
| B4 | `FileIndex::clone` (the incremental worker's base copy) | 36.6–52.0 ms | 26.6–27.5 ms |
| B5 | drop of that clone | 22.0–26.0 ms | 15.8–16.3 ms |
| B6 | `search("lbf", 64)` | 11.6–15.7 ms | 7.4–7.7 ms |

B3–B6 are **non-verdict**: the 1-minute load was 10–20 (other agents
building), and tmpfs flatters `realpath` against btrfs. B1 and B2 are exact.
Truncation at this size: none; 64 MiB / 363 B ≈ 184,000 files is where this
shape would hit `RetainedByteLimit`, and 128 MiB / 557 B ≈ 241,000 where it
would hit `BuildByteLimit`, both long before 2^20.

Arena size estimate from real path distributions (lengths only, read-only:
`git ls-files` of the two largest checkouts on disk and a walk of
`~/.cargo/registry/src`), at 24 B per file record plus canonical key, the name
bytes, and 16 B per directory record plus its name:

| Tree | Files | Dirs | Files/dir | Avg name | Arena B/file |
| --- | --- | --- | --- | --- | --- |
| lushtext checkout | 2,799 | 1,358 | 2.1 | 13.0 | 54.8 |
| invowk checkout | 4,827 | 1,161 | 4.2 | 16.7 | 47.6 |
| cargo registry sources | 25,626 | 3,500 | 7.3 | 17.8 | 45.1 |

At 55 B/file, 2^20 files need 55 MiB, inside the unchanged 64 MiB installed
budget. No public monorepo checkout is on disk (the largest is 5,131 files;
the corporate checkouts under `~/Workspace` are excluded), so the lane
measurement uses a synthetic tree (D9).

## Goals / Non-Goals

**Goals:** 2^20 files and 2^20 directories indexable within the unchanged
64 MiB / 128 MiB ledgers for monorepo-shaped trees; incremental mutation,
empty-query count and query cost that stay proportionate at 2^20; identical
results, order, deduplication and identity semantics to `main` below 131,072
files; every GTK-thread dispatch the palette runs stays within
`B_INTERACT_US`/`B_SETTLE_US` as today.

**Non-goals:** see proposal. Also no persistent index and no change to
nucleo scoring.

## Decisions

### D1. An interned path arena (`services/palette/index/arena.rs`)

```
FileIndex {
    folders:   Vec<FolderRecord>,      // workspace folder display path + canonical path (owned, few)
    dirs:      Vec<DirRecord>,         // { parent: u32 (or folder), name: Span, canonical: CanonicalRule }
    files:     Vec<FileRecord>,        // { dir: u32, name: Span, flags: u8 } — 12 bytes
    names:     String,                 // every directory and file name, UTF-8 (lossy, as `IndexedFile::new` is today)
    raw_names: Vec<(u32, Box<OsStr>)>, // exact bytes for the rare non-UTF-8 name, keyed by record
    canonical_keys: Vec<u64> + Vec<u32>, // sorted canonical-path hashes and their file ordinals (D4)
    symlink_identities: Vec<(u32, Box<Path>)>, // canonical path of a symlinked file (D2)
    visibility,
}
```

`CanonicalRule` is `Derived` (parent canonical + name: a non-symlink
directory) or `Resolved(u32)` (an index into an owned canonical-path table, for
a symlinked directory and for each workspace folder). File ordinals are
positions in `files`, contiguous, in today's traversal order, so source
ordinals, the empty query's source order and the equal-score tie-break are
unchanged. `FileIndex::file(ordinal) -> Option<IndexedFile<'_>>` returns a
borrowed view (`IndexedFile<'a> { index: &'a FileIndex, ordinal: u32 }`) with
`name() -> &str` (a slice of `names`, what nucleo scores), `path() ->
PathBuf`, `relative_display() -> String`, `identity() ->
PaletteFileIdentity` (owned, reconstructed) and `workspace_folder()`;
`files()` becomes an iterator of views.

*Rationale:* the name is the only field the hot loop reads; it becomes a
contiguous slice, which is also the cache-friendliest layout for the query
scan. Paths are needed for at most the shown rows. 12-byte records plus names
and keys measure 45–55 B/file on real trees (baseline table).

*Alternatives rejected:* (a) `Arc<str>` interning of path prefixes per file
keeps one allocation per file plus refcount traffic in every clone; (b) a
single `Vec<u8>` of full relative paths with offsets (no directory table) is
simpler but stores each directory name once per file (about 60 B/file of
relative path on these trees, B/file ≈ 85) and makes a directory rename
rewrite every descendant's bytes; (c) raising the count and the ledgers
(no arena) needs 363 MiB installed and 557 MiB at construction (B1, B2) and
would exceed the 128 MiB disposal lane the reservation crosses.

### D2. Canonical identity is derived for ordinary entries

A non-symlink entry's canonical path is its directory's canonical path joined
with its name (a name returned by `readdir` is neither `.` nor `..`), so
construction calls `realpath` once per workspace folder and once per symlink
entry, not once per file. The scanner reports whether an entry is a symlink
from the directory entry's type (`d_type`, carried by `readdir` itself),
falling back to an `lstat` only when the filesystem reports an unknown type;
`services/filesystem/sys.rs` already `statat`s every entry for its followed
kind, so common filesystems add no per-entry syscall.

Canonical uniqueness: every directory is entered at most once per build
through the canonical-directory visit set (unchanged behaviour: overlapping
workspace folders and directory aliases are visited once), so two
non-symlink files with the same canonical path cannot both be admitted. Only a
symlinked file can alias another admitted file, and the build keeps `main`'s
rule exactly — the first path traversal reaches wins, whichever kind it is —
in both orders, mid-traversal:

- The visit set maps a directory's canonical-path hash to its directory record
  and whether that directory has been scanned yet; during the build each
  directory record also holds the contiguous range of the file records its
  scan admitted (a directory's files come from one scan batch).
- **A symlinked file arrives** with canonical path `C`: if `C`'s parent is a
  scanned directory, its range is searched for `C`'s name (an exact name
  comparison over that directory's records only); a hit means the ordinary
  file was admitted first, so the symlink is skipped. Otherwise `C` is
  checked against the scratch set of symlinked files' canonical paths
  admitted so far, then admitted and added to it.
- **An ordinary file arrives** with derived canonical path `C`: if the
  symlink scratch set is non-empty, `C`'s hash (computed anyway for the key
  table, D4) is looked up in it; a hit means a symlink alias was admitted
  first, so the ordinary file is skipped, exactly as `main`'s
  `canonical_files` check skips it. With no symlinked files the lookup is
  skipped.

So ordinals, winners and identities equal `main`'s, which D6's reference model
checks with symlinks in both orders. The visit set and the symlink set key by a
64-bit hash with an exact comparison on collision (paths reconstructed from the
arena or held by the symlink entry), charged to the build ledger per entry.

*Rationale:* B3's build is dominated by the per-file `realpath`, and B2's peak
by the per-file canonical clone in `canonical_files`. The derivation is exact
under the same rule `realpath` applies to a non-symlink final component.

*Alternatives rejected:* keeping per-file `realpath` (correct but about 7 µs
per file on tmpfs, more on btrfs, and it needs the canonical set this change
removes); `std::fs::canonicalize` per directory only and per file never,
treating symlinked files as their link path (changes identity for symlinked
files, which the open-tab exclusion and the spec's "Alias path resolves to an
open file" scenario rely on).

### D3. Limits: 2^20 files, 2^20 directories, the byte ledgers stay

`MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES` become 1,048,576 (presentation
caps, powers of two, `.agents/rules/rust.md`); `MAX_FILE_INDEX_RETAINED_BYTES`
stays 64 MiB and `MAX_FILE_INDEX_BUILD_RETAINED_BYTES` 128 MiB. The ledger
(`FileIndexBuildLedger`, `FileIndexMutationLedger`) charges arena growth
(record vectors, name bytes, key tables, symlink identities, directory
records) before each push, plus the build's scratch (visit set, symlink set,
pending stack, scan batches), and keeps the O(1) `retained_byte_weight`
agreement with the installed graph (`debug_assert_eq!`).

**Capacity, not length, is charged.** `retained_byte_weight` counts each
vector's `capacity()`, as today, so the growth policy decides what fits:

- *Construction* reserves from `capacity_hint` (the previous index's file
  count and, new, its name bytes) and otherwise doubles, each growth charged
  before it happens; at the end, every arena vector is shrunk to its length
  before publication, so the installed weight is the length-based figure.
  Estimated construction peak at 2^20 files of the D9 shape: records 12 MiB +
  names up to 2× 18 MiB during the last doubling + directories about 3 MiB +
  visit set about 2 MiB + the key table built at the end (12 MiB, exact) + the
  shrink's transient copy of the largest vector, about 70–90 MiB against
  128 MiB (an estimate; task 2.3 measures it).
- *Incremental mutation* does not double a shrunk vector: `main`'s
  `next_incremental_vec_capacity` doubles, which at 2^20 files would charge
  about 30 MiB for the first added file and refuse it. The arena grows a
  vector by `max(capacity / 16, the batch's need)` (powers of two per
  `.agents/rules/rust.md`: 1/16 of a 2^20 record vector is 2^16 records,
  768 KiB), charged before the reserve.

Installed estimate at 2^20 files after shrinking: 45–55 B/file × 2^20 =
45–55 MiB (an estimate from the path-length table above; tasks 2.1 and 1.2
measure it).

**The disposal lane is unaffected in shape.** The build and the mutation
worker reserve `MAX_FILE_INDEX_RETAINED_BYTES` (64 MiB) through
`try_reserve_for_gtk`, or `try_reserve_replacement_for_gtk` crediting the
installed index's current guarded weight, and shrink the reservation to the
result's `retained_byte_weight` (`shrink_to`, `shrink_to_and_own`). Both
weights stay at most 64 MiB, as today; what changes is only that an index near
55 MiB now holds 2^20 files instead of about 150,000. The 128 MiB lane
(`DISPOSAL_RETAINED_BYTE_CAPACITY`) and its replacement headroom are not
edited, so `stream-replace-all`'s parallel work on the lane does not conflict. The four
`FileIndexTruncationReason`s are unchanged; `FileLimit` now means 2^20.

*Why keep a count at all* (the item said "a byte budget instead of the count"):
query time grows with the file count, not with bytes, and a pure byte budget
would admit about 2.8 million short-named files (64 MiB / 24 B) with about
three times the query cost of 2^20. The count is the latency bound (D7); the
bytes are the memory bound. Keeping 64 MiB keeps the index's disposal
reservation and its replacement headroom in the 128 MiB lane unchanged.

`classify_index_retirement` records a last-owned index whose length equals
`MAX_INDEXED_FILES`, test evidence that a full index is destroyed on the
worker lane (`command_palette.rs` widget tests build one at the cap). At 2^20
those tests would build a 2^20-file index three times each. It is re-keyed on
a named evidence threshold, `LARGE_INDEX_RETIREMENT_FILES = 131,072` (2^17),
recording a last-owned index of at least that length, so the tests keep their
131,072-file fixtures and keep proving the same property: a large index never
dies on the GTK thread. Kind: test evidence, not a presentation cap.

*Alternatives rejected:* 2^21 files (no tree on disk to justify it; doubles
query cost); raising the installed budget to 128 MiB (the reservation would
take the whole ordinary disposal lane during a replacement).

### D4. Canonical-key table and the open-tab count

The index keeps its files' canonical-path hashes sorted, with their ordinals
(`canonical_keys`, 12 B/file), built at the end of construction by one sort on
the worker and merged after each mutation batch. `OpenTabExclusion` resolves
each open canonical path once per query to the ordinals it matches (binary
search, exact comparison against the reconstructed canonical path), so:

- `admits(file)` is a membership test on a sorted `Vec<u32>` of at most the
  open tabs (no per-file hashing);
- the empty query's rest count is `len − |open ordinals| − shown`, exact, in
  O(tabs · log n), replacing `RestCount::WalkWith` over every file (DP's
  2.7–4.4 ms at 131,072; about 22–35 ms at 2^20 by linear growth).

*Alternative rejected:* caching the walk per (index generation, open-tab set)
still pays the full walk on every index or tab change.

### D5. Incremental mutations without per-file heap work

- **Base copy:** cloning the arena copies a handful of contiguous vectors
  (about 55 MiB at 2^20) with no per-file allocation or free; B4/B5's 27 +
  16 ms at 131,072 (about 220 + 130 ms at 2^20) become one `memcpy`-bound copy.
- **Growth:** by `max(capacity / 16, need)`, charged first (D3), never by
  doubling a shrunk vector.
- **Add:** dedup by display path and by canonical key: the directory is found
  by walking the path's components from its workspace folder through a
  batch-scoped `(parent, name) → dir` lookup built once per batch (on the
  worker), the file by binary search of its canonical key; the canonical path
  of the added file is resolved with one `realpath`, as today. New records are
  appended (today's order).
- **Remove:** a directory subtree's ids are marked, then one `retain` pass
  over the 12-byte records (O(n) integer work, today O(n) path comparisons);
  key table compacted in the same pass.
- **Rename:** a directory rename rewrites one `DirRecord` (parent, name,
  canonical rule) and leaves its descendants' records untouched (today it
  rebuilds and re-canonicalizes every descendant); a file rename rewrites one
  record; both re-key the affected canonical hashes, and a rename onto an
  existing identity is deduplicated exactly as today.

Generation arbitration (`FileIndexMutationTicket`), the replay of a rejected
batch, the queue overflow to a worker-only rebuild, and the
`index-update`/`command-palette-index` readiness are unchanged.

### D6. Behavioural parity through a reference model

The current `Vec<IndexedFile>` implementation becomes a reference module
gated `#[cfg(any(test, feature = "test-utils"))]` (the property target's
`property-tests` feature implies `test-utils`, and benches pass it), as
`search_full_sort_reference` is for ranking; `make check-filesystem-boundary`
and CI's default-feature `cargo check -p lushtext --bins` keep it out of
shipping builds. A property test
(`tests/properties/palette.rs`, `property-tests` feature) drives both with
generated trees (nested directories, overlapping workspace folders, a
symlinked directory and file, a non-UTF-8 name where the platform allows it)
and generated mutation sequences, and asserts equal file lists (display path,
relative display, name, identity, folder), equal query results and order for
generated queries, equal empty-query counts with generated open-tab sets, and
equal truncation reasons under scaled-down limits.

### D7. Query cost at 2^20

The worker query stays one linear nucleo pass over the name slices. Pass bar,
decided before measuring: the verdict-run p50 of one query (64 rows, no
exclusion; and with 32 open tabs) at 2^20 files is **≤ 50 ms** on the release
build, so results land within 200 ms of the last keystroke with the 150 ms
debounce. The stage-1 figure (B6, 7.4–7.7 ms at 131,072, non-verdict)
extrapolates linearly to about 60 ms at 2^20, over the bar, so the **expected**
path is the split, not a contingency: the query splits the files into at most
four contiguous chunks scored on scoped threads inside the same worker, each
with its own bounded top-N heap and cancellation checks, merged by score then
ordinal (deterministic, equal to the single-pass reference by D6). Task 4.2
measures the single pass first, because the contiguous name slices may be
faster than B6's per-file `String`s; the split is skipped only if that verdict
is within the bar, recorded in this design. Four threads, not more: the palette
worker already holds one of `gtk_lush_tasks`' eight slots, and four keeps the
query inside the 16-CPU affinity alongside a running build. The GTK-thread publish cost
does not depend on the index size (at most 64 rows per group).

### D8. The category view's row cap

The category view keeps its 8 MiB ranking budget
(`PALETTE_CATEGORY_RETAINED_BYTES`) and its 36-byte candidate charge, and
gains `MAX_CATEGORY_VIEW_ROWS = 131,072` (2^17). Ledger A95: a far
select-and-scroll costs time that grows with the list's rows (1.3 ms at
16,383, 10.4 ms at 131,071 in the probe; DP measured 2.2–6.9 ms worst in the
application at 131,072), so a 2^20-row category would cost about 8× that in
one GTK dispatch, far over `B_INTERACT_US`. `PALETTE_CATEGORY_FLOOR_RETENTION`
is derived from the row cap instead of `MAX_INDEXED_FILES`, and its two
`const` assertions keep holding (131,072 × 36 = 4.5 MiB ≤ 8 MiB < 9 MiB). A
category with more matches lists its best 131,072 and says "Showing X of Y
results" (the existing budget-cut message). `SearchResultItem::File` carries
the borrowed view (D1) and stays 16 bytes, pinned by the existing layout
assertion, so `PALETTE_CATEGORY_CANDIDATE_BYTES` stays 36.

*Alternatives rejected:* raising the budget to list 2^20 rows (36 MiB of
ranking peak on the worker and a far scroll of tens of milliseconds); a fixed
row height (GtkListView has no fixed-height mode; the estimate is the
toolkit's). Task 1.3 re-probes A95 at 2^18 and 2^20; if the growth flattens,
the cap is revisited there.

### D9. Measurement on a monorepo-shaped synthetic tree

The generator (a Criterion fixture helper and a lane fixture,
`scripts/generate-workspace-index-fixture.py`, seeded, deterministic, written
into the run's temp scope, never copying user files) draws directory depth,
files per directory and name lengths from the distributions measured above
plus a published Chromium-scale shape (about 10 files per directory, depth
6–10), with one symlinked directory and one symlinked file. Sizes 2^17 and
2^20.

### D10. Responsiveness lane cases

A new lane group `workspace-index` (headless, the existing driver) opens a
workspace of 2^17 and 2^20 files, waits for `command-palette-index`, then
measures the palette's open, a typed query's publish, the empty query, Enter on
the workspace "More" row, Page Down and End in the category view. Pass bar:
every GTK dispatch ≤ `B_INTERACT_US` for input and ≤ `B_SETTLE_US` for publish,
as on `main` at 2^17; the worker query per D7.

## Ledger axioms relied on

- A1 (realized-row cap), A90, A93, A96: grouped and category lists cost what
  they realize, not what the model holds; unchanged by this change at 2^17
  rows.
- A95: the far select-and-scroll grows with rows; the basis of D8. Re-probed
  at 2^18 and 2^20 (task 1.3); no new axiom unless the re-probe shows a
  behaviour A95 does not state.

## Timing

Timed paths: the index build (worker), the incremental mutation batch
(worker), the query and the empty-query count (worker), and the palette's GTK
dispatches at 2^17 and 2^20. Protocol (`verification-runs.md`): every timing
through `scripts/measure.sh`, a run valid only with a recorded 1-minute load
below 4, otherwise labelled non-verdict and retried; `main` and the branch
interleaved, four rounds on the affected cases (Criterion: `file_index_rebuild`,
`file_index_search`, `file_index_incremental`, `palette_categories` at 2^17 on
both, 2^20 on the branch only since `main` cannot index it; lane: the D10
cases). Pass bars: D7 (≤ 50 ms query at 2^20), D10 (dispatch budgets), and no
case at 2^17 slower than `main` by more than its own min–max spread.

## Risks / Trade-offs

- **Identity derivation edge cases** (bind mounts, case-insensitive
  filesystems): `realpath` does not resolve those either, so the derived path
  equals what `main` computes; D6's reference model pins it.
- **Hash collisions in the visit set and the key table:** every hit is
  confirmed by an exact path comparison; a test forces collisions through a
  test-only hasher.
- **A tree of very long names** exceeds 64 MiB before 2^20 and truncates with
  `RetainedByteLimit`, as today's ledgers would: memory stays bounded and the
  diagnostic says why.
- **Larger API change** (`IndexedFile` becomes a view): tests, benches and two
  widget-test helpers change; behaviour is pinned by D6.

## Open Questions

None blocking. The D7 parallel split and the D8 cap are decided by the stage-3
measurements under the pass bars above.
