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
| B3 | build wall time | 1,738–1,861 ms | 911–956 ms |
| B4 | `FileIndex::clone` (the incremental worker's base copy) | 36.6–52.0 ms | 26.6–30.1 ms |
| B5 | drop of that clone | 22.0–26.0 ms | 15.8–16.5 ms |
| B6 | `search("lbf", 64)` | 11.6–15.7 ms | 6.5–7.7 ms |

The release ranges span all three runs of the log (run 0 gave 956.2, 30.1, 16.5
and 6.5 ms; stage 2 widened the ranges, which first quoted runs 1–2 only).
B3–B6 are **non-verdict**: the 1-minute load was 10–20 (other agents
building), and tmpfs flatters `realpath` against btrfs. B1 and B2 are exact.
Truncation at this size: none; 64 MiB / 363 B ≈ 184,000 files is where this
shape would hit `RetainedByteLimit`, and 128 MiB / 557 B ≈ 241,000 where it
would hit `BuildByteLimit`, both long before 2^20.

**Stage 3 baseline on `main` (task 1.2), 2026-11-14,** `main` `72c039ef`
with a visibility-only patch (`stage3-main-visibility.patch`) so a release
test harness (`stage3-sfi_harness.rs.txt`) can drive the per-file index at
the D9 shape: the 2^17 generated tree on btrfs (`scripts/generate-workspace-
index-fixture.py --files 131072`), each case timed 5 times, four rounds
through `scripts/measure.sh` (`runs.log`, `stage3-baseline-main.log`).
Rounds r1 and r2 started at a 1-minute load of 3.96 and 3.84 (verdict);
r3 and r4 started at 6.53 and 6.73 and are **non-verdict** (another agent's
lint at 370 % CPU and a game client on the host). The non-verdict rounds lie
inside the verdict ranges except where noted, so the ranges below are r1–r2:

| M | Figure (D9 shape, 2^17 files requested) | `main`, release |
| --- | --- | --- |
| S1 | files indexed | 119,157: truncated by `RetainedByteLimit` (the D9 shape's longer paths) |
| S2 | `retained_index_bytes` | 67,108,718 (563 B/file) |
| S3 | `peak_build_bytes` | 99,137,141 (831 B/file) |
| S4 | build wall time (r2; r1 lost its line to a harness grep) | 1,819–1,858 ms (r3–r4: 1,755–1,870) |
| S5 | clone / drop | 34.2–52.8 / 24.6–34.8 ms |
| S6 | one query, 64 rows, 0 / 32 open tabs | 9.1–20.1 / 35.0–51.4 ms |
| S7 | empty query, 32 / 256 open tabs | 7.0–11.8 / 13.0–16.7 ms |
| S8 | one incremental batch of 1 / 64 / 1,024 adds (clone excluded) | 30.5–43.3 ms / 1.98–2.23 s / 31.7–32.2 s |
| S9 | directory rename of 9,872 descendants | 288–335 ms |

S8 is the D5 motivation measured: an add costs a linear scan per file, so a
1,024-file batch (a branch switch) holds the worker for half a minute.

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
    dirs:      Vec<DirRecord>,         // { display: DisplayRule, canonical: CanonicalRule }
    files:     Vec<FileRecord>,        // { dir: u32, name: Span, flags: u8 } — 12 bytes
    names:     String,                 // every directory and file name, UTF-8 (lossy, as `IndexedFile::new` is today)
    raw_names: Vec<(u32, Box<OsStr>)>, // exact bytes for the rare non-UTF-8 name, keyed by record
    canonical_keys: Vec<u64> + Vec<u32>, // sorted canonical-path hashes and their file ordinals (D4)
    symlink_identities: Vec<(u32, Box<Path>)>, // canonical path of a symlinked file (D2)
    visibility,
}
```

`DisplayRule` is `Child { parent: u32 (a directory or a folder root), name:
Span }` for every directory the build reaches, or `Detached(u32)` (an index
into an owned display-path table, charged to the ledger), used only when a
rename gives a directory a display path that is not a child of an indexed
directory (D5). Files never store a folder. Their folder is the one their
directory chain started from, so a detached subtree keeps its original folder,
and `relative_display` falls back to the full path, as `IndexedFile` does on
`main` when the path is not under its folder.

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

**Search results carry the ordinal, not the view.** `SearchResultItem::File`
carries a `FileOrdinal(u32)`, and the consumer, which already holds the
`FileIndex` it searched (grouped rows, the category model, benches), builds the
view with `index.file(ordinal)`. Stage 2 checked the alternative with rustc
1.98.0 on x86_64 (enum layout does not change in the pinned 1.98.1 patch
release; source and output in the handoff,
`stage2-search-result-layout-check.rs.txt`): an enum whose `File` variant holds the 16-byte view is
24 bytes (four dataful variants leave no niche for the tag), which makes
`RetainedCandidate` 40 bytes and `PALETTE_CATEGORY_CANDIDATE_BYTES` 44 instead
of 36. With a `u32` payload the enum stays 16 bytes, so the existing layout
assertion, the 36-byte charge, the `core-list-budget` Kani harnesses and the
threshold rows stay as they are.

Non-UTF-8 names: `raw_names` holds the exact bytes of every directory **and**
file name that is not valid UTF-8, keyed by record kind and id. Path
reconstruction uses the exact bytes, so such a file opens at its real path.
Every name comparison the build and the mutations make (D2, D5) also uses the
exact bytes, never the lossy name, because two different non-UTF-8 names can
share one lossy string.

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

**One intended divergence from `main`, in races only.** On `main`, a file
that disappears between `readdir` and its `realpath` is still indexed, with
`PaletteFileIdentity::Unavailable(NotFound)`, and counted in
`identity_failures`. With derivation it is indexed with its derived canonical
path. Either way the index lists a file that is gone, and opening it fails the
same way. No open tab can hold it, so the exclusion cannot differ. On a static
tree both give the same identity, which is what D6 compares.
`identity_failures` then counts only symlink resolutions that fail.

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

New named constants, each with a `Kind:` line and a `docs/thresholds.md` row
(task 3.7): `LARGE_INDEX_RETIREMENT_FILES` (test evidence, below);
`INDEX_INCREMENTAL_GROWTH_DIVISOR = 16` (the growth step above, a power of
two); `MAX_QUERY_CHUNKS = 4` (D7); `MAX_CATEGORY_VIEW_ROWS` (D8). The
threshold rows of the moved constants change their path column to the new
module (`services/palette/index/…`).

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
  worker), the file by binary search of its canonical key. A component with
  no directory record yet (a folder created after the build) gets one,
  charged to the mutation ledger. The added file's canonical path is resolved
  with one `realpath`, as today. If it equals the derived path (directory
  canonical plus name), the record is ordinary; otherwise it is a symlink and
  its identity goes to `symlink_identities`. New records are appended
  (today's order). The key table is merged once, after the batch, so adds
  inside one batch also check a batch-local set of the keys added so far;
  that keeps `main`'s sequential rule that a second add of the same path or
  identity in one batch is refused.
- **Remove:** a directory subtree's ids are marked, then one `retain` pass
  over the 12-byte records (O(n) integer work, today O(n) path comparisons);
  key table compacted in the same pass.
- **Rename:** a directory rename rewrites one `DirRecord` (parent, name,
  canonical rule) and leaves its descendants' records untouched (today it
  rebuilds and re-canonicalizes every descendant). A file rename rewrites one
  record. Both re-key the affected canonical hashes: O(descendants) hashing of
  reconstructed paths, with no filesystem call beyond the one `realpath` of
  the renamed entry. A rename onto an existing identity is deduplicated
  exactly as today (`deduplicate_files` keeps the first ordinal). The cases
  `main` handles implicitly, each made explicit:
  - *New parent not interned:* its missing directory records are created as
    for an add, charged first.
  - *Old path is a workspace folder root:* its folder record keeps its
    display path, and its directory record becomes `DisplayRule::Detached`
    with the new path (D1). Its canonical rule becomes `Resolved` with the
    new path's `realpath`. On `main` the files keep their
    `workspace_folder`, and `relative_display` falls back to the full path
    because the new path is not under the folder. Reconstruction gives the
    same strings.
  - *New path outside every workspace folder:* the same representation, so
    descendants reconstruct `new_path/suffix` and display it in full, as
    `main` does. The existing ledger test renames a folder to a 256 KiB
    `/renamed/…` prefix this way, and the detached path is charged before
    the rewrite.
  - *Ledger refusal:* `main` drops each descendant whose replacement does not
    fit (path cleared, then `retain`). The arena does the same for the
    subtree whose new record does not fit: its records are removed in the
    batch's `retain` pass, and the ledger is released.
  - *Identity of renamed descendants:* `main` re-resolves each descendant,
    so a path missing on disk gets `Unavailable`. The arena derives it from
    the renamed directory's new canonical path. This is intended, as in D2
    (a rename reaches the worker after the rename succeeded on disk, so the
    descendants exist).
- **Reclaiming dead storage:** removes and renames leave unreferenced bytes
  in `names` and directory records nothing references. Because capacity is
  charged, a long session could otherwise refuse adds on a small live index.
  A batch therefore compacts the arena, with one pass that rewrites `names`,
  `dirs`, the record spans and `raw_names` and then shrinks to length. Two
  triggers start it:
  - when dead bytes exceed a quarter of the charged weight, which is the
    same 3/4 rule `should_compact_after_removal` applies today;
  - at once, whenever a growth charge would be refused while dead bytes
    exist. The refused add is retried after the pass, so an index near the
    64 MiB cap does not refuse an add while it holds reclaimable bytes below
    that quarter. The pass is O(n) on the
  worker. It holds the compacted vectors, at the live length, beside the old
  ones only for the pass, which is the same transient copy `shrink_to_fit`
  makes on `main`. The ledger then records the compacted weight, so the
  `retained_byte_weight` agreement holds.

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
generated queries, and equal empty-query counts with generated open-tab sets.
The reference keeps its own owned row type (`ReferenceIndexedFile`, today's
fields), because `IndexedFile` becomes the view.

- **Mutations are applied on disk first.** Each generated create, remove and
  rename is first performed on the temp tree, then fed to both
  implementations, as the application feeds the worker after a sidebar
  operation succeeded. On a mutation the disk never saw, `main` re-resolves
  the path to `Unavailable` and the arena derives it (D2, D5), which is the
  recorded divergence and not a parity failure.
- **Truncation parity is by count only.** Under scaled file and directory
  limits both report the same `FileLimit` / `DirectoryRetentionLimit` at the
  same file. The byte ledgers cannot agree, because the arena charges far
  fewer bytes. Under scaled byte limits, the property instead asserts that
  the arena's list is a prefix of the reference's **untruncated** list in
  the same order (both stop at their first refusal), that it reports a byte
  reason, and that its weights respect its own caps.

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
faster than B6's per-file `String`s. The split is skipped only if that
verdict is within the bar, and the result is recorded in this design.

Split mechanics and bounds:

- The chunk count is `min(MAX_QUERY_CHUNKS = 4, available_parallelism())`,
  and a source below 2^17 files is never split. Each chunk owns its own
  nucleo `Matcher` and scratch buffer, because `FuzzyQuery` reuses one per
  query.
- The chunks are `std::thread::scope` threads joined before the palette
  worker returns. They run **beside** the `gtk_lush_tasks` slot the worker
  holds, not in new slots: at most three extra CPU-only threads, living one
  query and touching no GTK object. That is a recorded exception to "all
  background work goes through `spawn_blocking_then`", with the same shape
  as `services/editor_io/text_scans/health_avx2.rs`.
- **Retention:** each chunk keeps at most the group's 64 rows, so a split
  grouped query holds at most 256 candidates before the merge (the MODIFIED
  "retains only bounded top results" requirement).
- **The category view's ranking is not split.** Four heaps of up to 131,072
  candidates would hold about 18 MiB against its 8 MiB budget. It stays one
  pass, which task 4.2 measures at 2^20. If it is over the 50 ms bar, the
  figure and the reason are recorded here as a measured exception: entering
  a category, or typing inside one, over a 2^20-file index then takes that
  long on the worker, and the GTK thread is not affected.

The GTK-thread publish cost does not depend on the index size (at most 64
rows per group).

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
the `u32` ordinal (D1) and stays 16 bytes, pinned by the existing layout
assertion, so `PALETTE_CATEGORY_CANDIDATE_BYTES` stays 36. With 36-byte rows
the row cap (131,072, 4.5 MiB) binds before the 8 MiB budget (233,016 rows),
so in production the budget no longer cuts a category. It stays the memory
bound, and the "budget cuts a category" scenario keeps its test-override
proof.

**What the cap is measured against.** The A95 probe (a bare `GtkListView` of
sectioned rows) gives 10.4 ms at 131,071 rows, above `B_INTERACT_US`. DP's
application measurement at 131,072 rows gave 2.2–6.9 ms worst, inside it.
The **gating** figure is the application's: the far select-and-scroll at
131,072 rows of a 2^20-file index, in the optimized widget build of the
task 2.8 test, verdict runs through `scripts/measure.sh`, ≤ `B_INTERACT_US`
(revised in stage 4, see "Review (stage 4)": the lane case "End in the
category view" that first held this gate is not an input the palette
accepts). The same test in the debug widget lane bounds the dispatch at
2 × `B_INTERACT_US`, the widget-harness convention, as a regression guard. If
that verdict misses the bar, task 1.3/4.4 lowers the cap to the largest power
of two whose verdict fits, never below 65,536 (2^16). Below that, the cap
stays at 2^16 and the figure is recorded as a measured exception.

**The cap applies to every category, and that is the one loss.**
`category_row_limit()` is shared by all categories. On `main` it is the
budget's 233,016 rows, so a note or bookmark category of 131,073–233,016
matches is listed whole there. It is now cut at 131,072 and says "Showing
131,072 of N results". Decision: keep one cap for every category, user
experience first. The far select-and-scroll cost is a property of the list,
not of the row kind (A95): at 233,016 rows it extrapolates to about 18 ms in
the probe, over `B_INTERACT_US`, and on `main` that cost is already paid
today. The rows stay reachable by narrowing the query. The loss is stated in
the proposal's "User-visible impact".

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

This deviates from the programme item's "a measurement on a real monorepo".
No public monorepo checkout is on disk, and cloning one is not in this
change's scope, so the synthetic tree is calibrated on the three real trees
above. The deviation is recorded in the programme record (task 6.4), and a
real-tree measurement remains a follow-up.

### D10. Responsiveness lane cases

*Deferred in stage 4 (see "Review (stage 4)", item 3): no `workspace-index`
lane group is built by this change; the text below is the plan a follow-up
change takes up together with a palette row-selection action.*

A new lane group `workspace-index` (headless, the existing driver) opens a
workspace of 2^17 and 2^20 files, waits for `command-palette-index`, then
measures the palette's open, a typed query's publish, the empty query, Enter on
the workspace "More" row, Page Down and End in the category view. Pass bar:
every GTK dispatch ≤ `B_INTERACT_US` for input and ≤ `B_SETTLE_US` for publish,
as on `main` at 2^17; the worker query per D7.

Tiers and comparison:

- The 2^17 cases run in both tiers. The 2^20 cases run in the `full` tier
  only, because generating and indexing a million files takes minutes. The
  `command-palette-index` wait has a timeout scaled to the size.
- `main` has no `workspace-index` group, so the `main` side of the
  interleaved 2^17 comparison runs **this branch's** driver against a `main`
  build, as earlier programme changes did.
- `docs/runbooks/responsiveness-lane.md` documents the group, and the
  driver's self-test covers the new cases.

## Ledger axioms relied on

- A1 (realized-row cap), A90, A93, A96: grouped and category lists cost what
  they realize, not what the model holds; unchanged by this change at 2^17
  rows.
- A95: the far select-and-scroll grows with rows; the basis of D8. Re-probed
  at 2^18 and 2^20 (task 1.3). Those two sizes run as a separate
  measurement, marked `slow`. The probe's existing 16,384 / 131,072 pair stays
  fast and stays in the pull-request `GTK Axioms` lane, because the notes
  browser's and the category view's designs rely on it. No new axiom unless
  the re-probe shows a behaviour A95 does not state.

## Timing

Timed paths: the index build (worker), the incremental mutation batch
(worker), the query and the empty-query count (worker), and the palette's GTK
dispatches at 2^17 and 2^20. Protocol (`verification-runs.md`): every timing
through `scripts/measure.sh`, a run valid only with a recorded 1-minute load
below 4, otherwise labelled non-verdict and retried; `main` and the branch
interleaved, four rounds on the affected cases (Criterion: `file_index_rebuild`,
`file_index_search`, `file_index_incremental`, `palette_categories` at 2^17 on
both, 2^20 on the branch only since `main` cannot index it; the D8 widget
rounds at 2^20; the D10 lane cases are deferred, stage 4). Pass bars: D7 (≤ 50 ms grouped query at 2^20; the category ranking
measured and, if over, recorded as D7's exception), D8 (the far select-and-scroll
≤ `B_INTERACT_US`), and no
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

## Review (stage 2)

Independent review on 2026-11-14 against `main` `2c91ccf2`. The approach
holds: the arena, derived identity, the key table, 2^20 limits under
unchanged ledgers, and a category row cap. These are the issues found and
how each was decided (user experience first):

1. **The search-result layout was wrong.** A 16-byte view in
   `SearchResultItem::File` makes the enum 24 bytes and the candidate charge
   44, which a `rustc` layout check showed. Decision (D1): carry the `u32`
   ordinal, keeping 16 bytes, the 36-byte charge, the `core-list-budget` Kani
   harnesses and "Kani: none".
2. **The MODIFIED requirement bodies dropped clauses** that archiving would
   have deleted from the main spec: the category view's rank definition, "no
   per-row text", the list-budget policy; the byte ledger's 2× relation; the
   traversal's direct metrics; the note-source clauses. Decision: each body
   rebuilt from the baseline text with only this change's edits. No scenario
   was renamed.
3. **The split's retention contradicted "retains only bounded top results",**
   and four category heaps would exceed the 8 MiB budget. Decision (D7): the
   requirement now admits ≤ 4 chunk heaps for grouped queries, the category
   ranking stays one pass, and its 2^20 figure is measured and, if over
   50 ms, recorded as an exception. The split's chunk count follows
   `available_parallelism()`, and its scoped threads are a recorded
   exception to the `spawn_blocking_then` rule.
4. **The row cap cuts note categories that `main` lists whole** (131,073–
   233,016 rows), so "loss: none" was false. Decision (D8): one cap for
   every category, the loss stated in the proposal.
5. **The `B_INTERACT_US` basis of the cap was ambiguous** (probe 10.4 ms,
   application 2.2–6.9 ms), and task 2.8 named two bounds. Decision (D8): the
   lane verdict at 131,072 rows gates; the widget test guards at 2 ×; the cap
   can be lowered, never below 2^16.
6. **D5 was incomplete:** in-batch duplicates, new directories, a renamed
   folder root, a rename outside every folder (which an existing test
   performs), ledger refusal, and dead arena bytes after removes and renames
   (which would refuse adds on a small live index). Decision: each case
   specified, plus compaction at the existing 3/4 rule.
7. **Parity was overclaimed.** Byte truncation cannot agree between
   representations, and the rename re-resolution and race identities
   diverge. Decision (D2, D6): count-limit parity, a prefix property under
   byte limits, mutations applied on disk first, and the divergences
   recorded.
8. **The 2^20 construction peak had no asserting task.** Task 2.3's 128 B/file
   at 2^17 left zero headroom at 2^20. Decision: task 2.3 at ≤ 96 B/file, and
   task 4.1 asserts no truncation, peak ≤ 128 MiB and installed ≤ 64 MiB on a
   real 2^20 traversal.
9. **Failing-first versus the commit hook:** tests naming symbols that do not
   exist yet would fail all-targets Clippy. Decision (task 2.9): each test
   lands assertion-red against a minimal stub that compiles, or recorded-red
   in the handoff, and says which.
10. **Smaller fixes:** the baseline ranges now include run 0. A95's
    2^18/2^20 re-probe is a separate slow measurement, so the PR lane keeps
    the fast pair. The rapid-query scenario at 2^20 gets a bench, and the
    lane's 2^20 cases are full-tier and use the branch driver for `main`. The
    widget shard budget is re-checked. Non-UTF-8 comparisons use exact bytes.
    The synthetic-tree deviation from "a real monorepo" is recorded. The new
    constants get threshold rows.


## Implementation decisions (stage 3, 2026-11-15)

Decided during implementation, user experience first; each one is checked by
the test named with it.

1. **One `u64` key per file** (D4 planned 12 B/file): the key is the
   canonical-path hash's top 32 bits joined to the file ordinal, so the
   table is one sorted `Vec<u64>` (8 B/file) and a lookup is a range of
   equal top halves, each confirmed by an exact comparison against the
   reconstructed canonical path (`arena.rs`, `key`/`key_range`; forced
   collisions in `index/tests.rs`).
2. **`SearchResultItem::File(FileOrdinal)`** as the stage-2 review settled;
   the owned per-file record is `OwnedIndexedFile` (`index/owned.rs`,
   `test-utils`), the input of `FileIndex::from` and `from_paths` and the
   output of `owned_files()`, so tests compare plain values with the
   oracle's.
3. **The per-file index is the oracle, `ReferenceFileIndex`**
   (`index/reference.rs`, `test-utils`): `main`'s `index.rs` moved whole,
   renamed, with no behaviour change. Its fixture constructor with scaled
   limits keeps the name `rebuild_cancellable_with_limits` on both, so the
   `*_for_test` ceiling of `make check-workflow-boundaries` is unchanged.
4. **Three `main` behaviours the property test found**, which the arena
   first got wrong and now matches:
   - a symlink in a directory finds a target admitted earlier in the same
     directory (the directory's file range is open during its scan);
   - a file renamed out of a nested workspace folder keeps the folder that
     still contains it (`a_file_moved_out_of_a_renamed_nested_folder_keeps_its_folder`);
   - a path inside several folders resolves to the deepest folder whose
     root is alive (`roots_containing`), and a folder whose root directory
     is removed is retired at `finish_mutation_batch`
     (`remove_path_prunes_workspace_folders`).
5. **Every mutation batch ends with `finish_mutation_batch`**, which drops
   dead records from `len()`, merges the key table and compacts; the palette
   worker calls it after each batch (`index_execution.rs`).
6. **The reclamation test's bound is four thirds of the start plus one
   growth step**, not "within one growth step": dead bytes are compacted
   once they exceed a quarter of the weight (D5), so between compactions the
   weight legitimately reaches `start + start/3`; the test also asserts it
   returns within one growth step of the start at least once
   (`index_scale_tests.rs`).
7. **The 2^20 bench groups run only under `LUSHTEXT_BENCH_SCALE=1`**, which
   `make bench` sets: their setup builds the index and writes the D9 tree in
   the group function, before Criterion's filter, so without the gate each of
   `make performance-smoke`'s filtered runs would pay it.
8. **The category cap stays 131,072** (task 1.3, D8). With slow probes, A95
   on GTK 4.24.1 is flat: 1.6–2.3 ms at 262,144 and 1.8–2.3 ms at 1,048,576
   rows, 212 `get_item` calls at every size, so on the shipped GNOME 51
   runtime 2^18 rows would fit `B_INTERACT_US`. The cap is not raised,
   because (a) on the GNOME 50 source floor (GTK 4.22.5) the probe grew,
   10.4 ms at 131,072 (2026-11-07, one run; not re-run on 4.22 here) and about 18 ms extrapolated to the budget's
   233,016 rows; (b) above 233,016 the retained-byte budget binds anyway, so
   a raise could only list 131,073–233,016 rows, and the floor retention
   derived from the cap (`PALETTE_CATEGORY_FLOOR_RETENTION`, 36 B a row)
   would then exceed the 8 MiB budget it must fit. Follow-up: when the source
   floor moves to GNOME 51, set the cap to the budget's rows, which also
   removes the note-category loss.
9. **The note-category loss is proved by a unit test**, not a second widget
   test: `a_note_category_above_the_row_cap_is_cut_and_counted`
   (`ui/command_palette/policy.rs`) ranks 131,073 bookmark rows through
   `category_search` with `category_row_limit()` and asserts 131,072 listed,
   131,073 matching and the cut reported. The widget path that renders the
   cut is the same for every category and is covered by the workspace test.
10. **No red commit** (task 2.9): the red runs were recorded against a local
    build of the tests on `main`'s code (`stage3-red-record.txt`,
    `stage3-red-run.log`) and the tests were committed with the arena.
    The first commit, the A95 probe (`f73b843b`), also carried the staged
    `index.rs` → `index/reference.rs` rename, so that one commit does not
    build on its own; the next (`de04a565`) does.
11. **Chunked scoring (task 4.3) is not built**, so `MAX_QUERY_CHUNKS` and its
    thresholds row (named by task 3.7) do not exist. The single-pass worker
    query at 2^20 measured 31.0–34.5 ms (one query, 0 and 32 open tabs, five
    runs each, `stage3-branch-w20-nonverdict.log`) on a loaded host through
    `scripts/heavy.sh`: a non-verdict figure, but a third under D7's 50 ms
    bar, so the split it would pay for is not needed. The verdict rounds are
    queued (item 13).
12. **The lane group `workspace-index` (task 5.1) is not built**, and D8's
    gate needs a revision before it is: the palette has no `End` binding (its
    key controller handles Tab, Page Up/Down, Up/Down and Backspace; `End` in
    the focused entry moves the text cursor), so "End in the category view"
    is not an input a lane, or a user, can give. The far jump the cap bounds
    is a select-and-scroll to a far row (a selection restore, or the end of
    a long Page Down run), which the optimized widget test measures:
    1.9–2.9 ms to the last and 2.1–2.5 ms back to the first of 131,072 rows
    of a 2^20-file index (three non-verdict runs). Open decision for the
    review: either D8 gates on that widget figure (DP's method) or a
    later change adds a palette row-selection action and the lane group.
13. **Timing verdicts are queued, not run here.** The shared-machine
    coordinator stopped the interleaved rounds before they ran (their
    `measure.sh` held `INTENT` behind another change's long heavy round) and
    queued them in `.claude/handoffs/MEASURE-QUEUE.md`
    (`stage3-interleave.sh`: `main`/branch at 2^17 twice, the branch at
    2^20, then the D7 Criterion groups and the D8 widget runs). Until they
    run, the figures below are the stage-1/stage-3 baseline on `main` and
    non-verdict branch figures.

14. **Two bench-lane defects fixed on the way** (pre-existing-blockers rule):
    the `near_policy_long_paths` setup asserted the per-file index's byte
    cost (it now asserts the arena interns the directory chain, see the
    bench's doc), and `make performance-smoke` named a widget proof that
    `bound-notes-browser-by-budget` had renamed
    (`test_notes_browser_caps_large_result_sets_with_refine_notice`, now
    `test_notes_browser_lists_every_match_without_a_refine_notice`), so the
    smoke failed on `main` too. The 2^20 `add_file` case frees one file
    (untimed) before each add, so it times an admitted add whatever the
    synthetic index's size.

### Branch figures, non-verdict (2026-11-15)

Release harness (`stage3-sfi_harness.rs.txt` with
`stage3-sfi_variant-branch.rs.txt`, a visibility-only copy of `de04a565`),
D9 tree on btrfs, five runs per case, through `scripts/heavy.sh` on a host
at a 1-minute load of 5–15 (other agents' builds): not verdicts.

| M | Figure | `main`, 2^17 requested (r1–r2) | branch, 2^17 (one run) | branch, 2^20 |
| --- | --- | --- | --- | --- |
| S1 | files indexed | 119,157 (`RetainedByteLimit`) | 131,072, no truncation | 1,048,576, no truncation |
| S2 | `retained_index_bytes` | 563 B/file | 41 B/file | 44,002,118 (41 B/file) |
| S3 | `peak_build_bytes` | 831 B/file | 58 B/file | 61,620,198 (58 B/file) |
| S4 | build wall time | 1,819–1,858 ms | 421 ms | 3,606–3,766 ms |
| S5 | clone / drop | 34.2–52.8 / 24.6–34.8 ms | 1.1 / 0.001 ms | 3.5–4.1 / 0.001 ms |
| S6 | one query, 0 / 32 open tabs | 9.1–20.1 / 35.0–51.4 ms | 4.3 / 4.2 ms | 31.7–34.5 / 31.0–31.2 ms |
| S7 | empty query, 32 / 256 open tabs | 7.0–11.8 / 13.0–16.7 ms | 0.05 / 0.46 ms | 0.04–0.15 / 0.30–0.81 ms |
| S8 | batch of 1 / 64 / 1,024 adds | 30.5 ms / 2.0 s / 31.7 s | 3.8 / 4.3 / 15.5 ms | 5.5–9.5 / 25.8–27.3 / 354–369 ms, all refused at the 2^20 cap |
| S9 | directory rename, 9,872 descendants | 288–335 ms | 7.4 ms | 41–54 ms |

The 2^20 adds are refused because the index is full. A refused add costs
about 0.35 ms each in a 1,024-add batch, worker-side and far under `main`'s
admitted cost; the cause was the O(n) live count, fixed in stage 4 ("Review
(stage 4)", item 2).

## Review (stage 4, 2026-11-15)

Independent review of the implementation (`c7d4d424`) against the design,
the rules and `main`. The arena, the derived identity with `main`'s
first-path-wins symlink rule, the key table, the 2^20 limits under the
unchanged ledgers, the growth by `max(capacity / 16, need)` with
compaction, the 131,072-row category cap with its "Showing X of Y" notice
(announced and reported through `category_limited`), the reference-model
property test, and the mutation worker's ticket, replay, overflow rebuild
and readiness all hold. D7's chunked query was not built (stage 3,
decision 11), so the branch spawns no thread outside `spawn_blocking_then`;
the recorded exception has nothing to cover. Findings and decisions:

1. **Must fix: a byte-truncated build published an empty index.**
   `Traversal::finish` built the key table and the per-directory hashers
   after the traversal, charging the keys (8 B a file, 16 for a symlink) to
   the installed ledger and the hashers (two cloned SipHash states, about
   160 B a directory) to the build ledger, neither reserved while files were
   admitted. When either did not fit, the finish discarded every file. That
   is reachable at production limits: a sparse tree of 2^20 files with about
   2 files per directory (the lushtext checkout's shape) has about 500,000
   directories, so the hashers alone take about 80 MiB beside a 50 MiB
   arena, over 128 MiB, and the palette would have indexed nothing where
   `main` indexes a truncated prefix. Every build that stopped on a byte
   limit hit it too. Fixed:
   - `PathHasher` folds a SipHash over the path's components and is one
     `u64`, so the memo costs 32 B a directory; hashing by components also
     agrees with `Path` equality, which confirms every hit.
   - The traversal charges each file's keys to the installed ledger when it
     admits the file, and the visit set's scratch (40 B a directory, more
     than the hashers' 32, a `const` assertion) is carried over to pay for
     the hashers. The finish therefore needs nothing the traversal did not
     hold, and its failure branch is unreachable (a `debug_assert!`).
   - Test: `a_byte_truncated_build_publishes_its_prefix_under_every_budget`
     sweeps the build and installed budgets over a sparse tree: a larger
     budget never publishes fewer files, and under every build budget the
     arena publishes at least what the per-file reference publishes. Red
     runs: `.claude/handoffs/scale-file-index/stage4-review-red.txt`.
2. **Fixed: a refused add on a full index counted every record.**
   `add_resolved` called `live_len()`, an O(n) count, for each add once the
   index held 2^20 records: the 0.35 ms a refused add cost (the stage-3
   follow-up). The batch now keeps the count, counted once, advanced by its
   adds and forgotten by a kill.
3. **D8's gate and D10's lane group (task 5.1).** The palette has no `End`
   binding and the automation surface no row-selection action
   (`docs/automation-reference.md`: `win.set-command-palette-category`
   enters a category, nothing selects a row), so "End in the category view"
   is neither an input a user gives nor one the lane can drive, and "Page
   Down" over 131,072 rows is 8,192 dispatches of the 16-row step whose
   cost A95 says does not grow with the list. The dispatch the cap bounds is
   the far select-and-scroll, which the task 2.8 widget test drives through
   the category view's own selection path. Decision: D8 gates on that
   test's optimized build, three verdict runs through `measure.sh` (queued
   with the coordinator, `sfi-branch-d8-widget-r1..r3`), ≤ `B_INTERACT_US`;
   D7 gates on the queued Criterion groups (`sfi-branch-d7-criterion`). The
   `workspace-index` lane group is deferred to a follow-up change that adds
   a palette row-selection action (with its automation documentation), and
   is recorded in the programme record. No spec text names the lane group,
   so the spec delta is unchanged.
4. **Recommendations, not fixed here.** The mutation batch's transient
   scratch is not charged to the mutation ledger (the `(parent, name)` map,
   the hashers memo, the merged key tables), as `main`'s transient clone was
   not; with item 1's 32-B hashers the worst case at 2^20 directories is
   about 100 MiB on the worker for one batch, and a batch is a few
   app-initiated operations. A directory removal walks every directory once
   (`subtree`), O(directories) per removal, against `main`'s O(files).
5. **Verdict binaries.** The queued harness, Criterion and widget binaries
   were built from the stage-3 code. Item 1 changes the build's finish and
   the key hashing, not the query D7 times nor the selection D8 times; the
   2^17 build and mutation rounds were rebuilt from this review's commit
   before the queue ran (see the queue entry).


## Measured verdicts (2026-11-15)

The coordinator ran the queued campaign (`MEASURE-QUEUE.md`, entry
"scale-file-index — stage 3 interleaved rounds") in a quiet window. Every
round went through `scripts/measure.sh` (`runs.log` has each round's load and
top processes before and after) and was pinned with `taskset -c 8-23`.
The CPU set the kernel granted was not recorded per round. A round is a
verdict only if it completed and its recorded 1-minute load at the start was
below 4. All nine rounds below meet that.
The first `sfi-main-r5` attempt (02:28, load 4.79, exit 143, stopped by the
convoy breaker) and every figure in "Measured baseline" and "Branch figures,
non-verdict" outside the r1–r2 baseline rounds stay non-verdict.
Binaries:

- the release harness from `main` `72c039ef` and from the review fix
  `1bc819b6` (visibility-only copies);
- the Criterion and optimized widget (`opt-level 2`) binaries, rebuilt at
  `1bc819b6` at about 13:58 as a heavy job, not measured
  (`rebuild-bench.json`, `rebuild-widget.json`).

Logs: `stage3-interleaved.log`, `stage3-d7-criterion.log`,
`stage3-d8-widget.log`.

**Harness rounds at 2^17, D9 tree on btrfs** (medians of 5 runs per case;
order `main` r5 13:31:58 → branch r1 13:35:51 → `main` r6 13:37:37 → branch
r2 13:55:35; start loads 3.43, 3.91, 3.60, 3.07; end loads 3.25, 3.91, 3.30,
2.99). `main` indexes 119,157 files (`RetainedByteLimit`), the branch all
131,072.

| Case | `main` r5 / r6 | branch r1 / r2 | Bar (no slower than `main` beyond its spread) |
| --- | --- | --- | --- |
| build | 1,759 / 1,796 ms | 412 / 409 ms | pass |
| `retained_index_bytes`, B/file | 563 | 41 | pass (task 2.3: ≤ 60) |
| `peak_build_bytes`, B/file | 831 | 60 | pass (task 2.3: ≤ 96) |
| clone / drop | 32.5–34.1 / 24.5–25.1 ms | 0.19–0.24 / 0.000 ms | pass |
| query, 0 open tabs | 8.9 / 9.1 ms | 3.88 / 3.83 ms | pass |
| query, 32 open tabs | 34.0 / 34.6 ms | 4.00 / 4.01 ms | pass |
| empty query, 32 / 256 open tabs | 5.8–7.4 / 10.7–13.3 ms | 0.042 / 0.25 ms | pass |
| batch of 1 add (clone excluded) | 30.0 / 30.5 ms | 2.06 / 2.08 ms | pass |
| batch of 64 adds | 1,895 / 1,910 ms | 2.71 / 2.67 ms | pass |
| batch of 1,024 adds | 31.29 / 31.11 s | 15.8 / 14.4 ms | pass |
| directory rename, 9,872 descendants | 293 / 283 ms | 5.08 / 4.82 ms | pass |

**Harness round at 2^20, branch only** (`sfi-branch-w20`, 13:56:30–13:57:06,
start load 2.81, end 3.15): 1,048,576 files, no truncation;
`retained_index_bytes` 44,002,118 (41 B/file, under 64 MiB) and
`peak_build_bytes` 63,134,538 (60 B/file, under 128 MiB); build 3,496 ms
(3,417–3,709); clone 3.66 ms, drop 0.001 ms; query 31.4 ms with 0 open tabs
and 31.8 ms with 32; empty query 0.048 ms with 32 open tabs and 0.36 ms with
256; a directory rename of 9,872 descendants 31.6 ms. Batches of 1, 64 and
1,024 adds are all refused at the 2^20 cap, at 6.1, 6.5 and 19.4 ms: the
stage-4 fix (item 2) took the 1,024-add batch from 354–369 ms to 19.4 ms.

**D7, Criterion at 2^20, branch** (`sfi-branch-d7-criterion`,
14:05:38–14:06:54, start load 2.18, end 2.98; 10 samples each, Criterion's
median and interval). The single pass is within the bar, so task 4.3 (the
chunked split) is **not applicable**, and the branch spawns no scoped
threads.

| Case | Median [interval] | Bar | Verdict |
| --- | --- | --- | --- |
| `query_match` (64 rows) | 42.9 ms [41.7–45.3] | ≤ 50 ms | pass |
| `query_open32` | 43.3 ms [42.3–45.0] | ≤ 50 ms | pass |
| `no_match` | 15.3 ms [15.1–15.8] | none | recorded |
| `empty_query` | 0.99 ms [0.97–1.02] | none | recorded |
| `empty_query_open32` | 14.5 µs [14.1–15.0] | none | recorded |
| `category_ranking_row_cap` | **50.75 ms [49.36–52.45]** | measured, 50 ms if not an exception | **over; the measured exception** |

The `query_match` figure is higher than the harness's 31 ms because the two
measure different things. Criterion's synthetic index matches the query
`file_42` against far more names (`file_N.ext`) than the D9 tree does. The
grouped-query bar holds on both.

**The category ranking's measured exception (D7).** Ranking the workspace
category whole takes 50.75 ms on the worker. The case is the 131,072-row
cap, over the bench's 1,047,534-file synthetic index, for the query `file`.
Every `file_N.ext` name matches that query, so far more than 131,072 match.
That is not a pass, and its interval straddles the bar. It is recorded as
the exception D7 names. What the user sees:

- After Enter on a More row, the category view publishes about 51 ms later,
  and the activation is not debounced.
- Typing inside the category at that size puts the publish about 1 ms past
  the 200 ms the grouped view's scenario allows (150 ms debounce plus about
  51 ms).
- No GTK dispatch is affected: the D8 rounds below put the category view's
  dispatches at 2.0–3.8 ms.

Below 2^20 files the time falls with the number of files scored. A query
with fewer matches also does less selection work.

The ranking is already a bounded heap, not a full sort. It costs more than
the grouped query because of the 131,072 heap insertions and the final sort
of 131,072 rows. The two cases score different queries (`file` and
`file_42`), so their difference, about 8 ms, only estimates the selection
share; that share was not measured on its own. The scoring pass is the rest,
the same pass the grouped query pays. The partial-selection alternatives
were weighed and not taken:

- `select_nth_unstable` over every match would retain about 1,000,000
  32-byte candidates (32 MiB) against the 8 MiB ranking budget.
- A 2 × cap selection buffer would need about 9 MiB at the row cap, over the
  8 MiB budget. The second floor-retention `const` assertion
  (`PALETTE_CATEGORY_RETAINED_BYTES / 2 < PALETTE_CATEGORY_FLOOR_RETENTION`)
  says exactly that two cap-sized retentions do not fit.

Neither stays inside this change's memory bound. They could recover at most
the estimated 8 ms selection share, and the scoring pass that dominates
would remain.
Splitting the category's scoring was already rejected (four heaps of 131,072
candidates, about 18 MiB). The follow-up is recorded in
`docs/next/long-line-documents.md`: a chunked category ranking whose chunks
share one bound, and a breakdown of scoring against selection measured
first.

**D8, optimized widget build, branch** (task 2.8's
`test_palette_category_of_a_million_files_is_cut_at_the_row_cap`, the far
select-and-scroll at the 131,072-row cap of a 2^20-file index; three runs
14:07:26, 14:08:00, 14:08:34, start loads 2.73, 3.49, 3.16, end loads 2.67,
3.45, 3.15).

| Dispatch | r1 | r2 | r3 | Bar | Verdict |
| --- | --- | --- | --- | --- | --- |
| select and scroll to the last row | 2,220 µs | 2,985 µs | 2,020 µs | ≤ `B_INTERACT_US` (8,000 µs) | pass |
| back to the first row | 2,354 µs | 3,788 µs | 2,264 µs | ≤ `B_INTERACT_US` | pass |

The row cap stays 131,072. Each run logs one
`Gtk-CRITICAL … Unable to register the application … org.a11y.atspi.Registry: unit failed`.
It comes from the headless session's missing systemd user bus
(`org.freedesktop.systemd1` fails to activate under the private
`dbus-daemon`). It is not this change's warning: the pending change
`quiet-headless-session-services` (PR #110) removes it.

**Storage** (`btrfs-stall-report --no-below`), one report per window:

- 13:31–13:41: "Storage was NOT a bottleneck: IO pressure 0.7%, at most 1
  task(s) waiting on storage, longest btrfs commit 313 ms (written 3,499
  MiB)"; 0 episodes.
- 13:55–13:58: "Storage was NOT a bottleneck: IO pressure 0.9%, at most 0
  task(s) waiting on storage, longest btrfs commit 250 ms (written 2,078
  MiB)"; 0 episodes.
- 14:05–14:09: "Storage was NOT a bottleneck: IO pressure 0.8%, at most 1
  task(s) waiting on storage, longest btrfs commit 253 ms (written 1,220
  MiB)"; 0 episodes.

No alert or `!` event was recorded in any window.

**Result.** The verdicts pass:

- D7's grouped query at 2^20;
- D8's far select-and-scroll at the row cap;
- every 2^17 case against `main`;
- the 2^20 byte ledgers.

The category ranking at 2^20 is the one measured exception: 50.75 ms
[49.36–52.45] on the worker, recorded above with its follow-up. Task 4.3 is
not applicable.
