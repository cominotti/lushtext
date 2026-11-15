## Why

The command palette's workspace file index stops at 131,072 files
(`MAX_INDEXED_FILES`), so in a large monorepo `Ctrl+Shift+P` cannot find most
files at all: they are never indexed. Programme item 3 of
`docs/next/long-line-documents.md` asks for at least 1,000,000 files with an
interned path arena and a byte budget.

The count is not the only thing in the way. Each `IndexedFile` owns its
display path, its canonical path and its name as separate heap allocations, and
construction keeps a second copy of every canonical path in a deduplication set
and canonicalizes every file with its own `realpath`. Measured on `main`
(`2c91ccf2`, release profile, a 131,072-file tree of a
Chromium-like shape with 8,223 directories and a 32-byte workspace folder path;
handoff `stage1-baseline-release.log`):

| Figure at 131,072 files | Measured | Allowed at 2^20 files by today's ledgers |
| --- | --- | --- |
| Installed index (`retained_index_bytes`) | 47,644,732 B = **363 B/file** | 64 MiB / 2^20 = 64 B/file |
| Construction peak (`peak_build_bytes`) | 73,045,157 B = **557 B/file** | 128 MiB / 2^20 = 128 B/file |
| Incremental batch base clone | 26.6–27.5 ms (non-verdict) | — |
| One query (`search`, 64 rows) | 7.4–7.7 ms (non-verdict) | — |
| Build | 911–916 ms (non-verdict, tmpfs) | — |

So 1,000,000 files at today's representation would need about 363 MiB
installed and 557 MiB during construction, and every incremental mutation batch
would clone about 360 MiB on the worker. Raising the count alone would truncate
with `RetainedByteLimit` near 184,000 files of this shape. The timings are
labelled non-verdict (1-minute load 10–20 while other agents built); the byte
figures are deterministic.

The change is worth doing, refined: an arena alone does not reach the target.
Construction also needs a structural deduplication and canonical-identity rule
(no per-file `realpath`, no set of canonical paths), the incremental worker
needs operations that do not grow with every file's heap, and the category
view, whose far `scroll_to` cost grows with its row count (ledger A95), needs
its own row cap instead of "the whole index".

## What Changes

- **An interned path arena replaces `Vec<IndexedFile>`.** Directories are
  interned once (parent, name, canonical-identity rule); each file is a fixed
  record (directory, name span, identity flags) over one UTF-8 name buffer.
  Display path, relative display, name and canonical path are reconstructed on
  demand for the at most 64 grouped rows, and for the rows a category view
  shows. Estimated from three real checkouts' path distributions (lengths
  only, design "Measured baseline"), the arena needs 45–55 B/file; tasks 1.2
  and 2.1 measure it.
- **Canonical identity is derived, not resolved, for ordinary entries.** A
  non-symlink entry's canonical path is its directory's canonical path plus its
  name, so construction calls `realpath` once per workspace folder and once per
  symlink entry, and canonical uniqueness follows from the canonical-directory
  visit set the build already keeps. Symlinked files keep today's resolution
  and an explicit deduplication set.
- **The index keeps a sorted canonical-key table**, so the open-tab exclusion
  and the empty query's "N more…" count look the open tabs up in the index
  (O(tabs · log n)) instead of reading every file's canonical path (the
  2.7–4.4 ms DP left to this change, about 35 ms at 2^20).
- **Limits become 2^20.** `MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES`
  rise from 131,072 to 1,048,576 (presentation caps, powers of two). The
  installed (64 MiB) and construction (128 MiB) byte ledgers stay, now charged
  for arena growth; they are what truncate a tree of unusually long names. The
  four truncation reasons, the single-flight build coordinator, the
  incremental mutation worker, generation arbitration and the readiness
  predicates are unchanged.
- **The category view caps its rows at 131,072** (2^17, its current ceiling,
  which ledger A95 and DP's measurement put at the edge of `B_INTERACT_US` for
  a far select-and-scroll); a larger category says "Showing X of Y results", as
  the budget rule already specifies. The grouped view is unchanged (64 rows per
  category, exact "N more…" counts at any index size).
- **Benchmarks and measurements:** Criterion groups at 2^17 and 2^20 for build,
  query, empty-query count, mutations and clone; a lane measurement on a
  synthetic monorepo-shaped tree of 2^20 files in the run's temp scope (no
  public monorepo checkout is on disk; the largest has 5,131 files), interleaved
  with `main`.

## Non-goals

- No change to which files are indexed (visibility rule, `IGNORED_INDEX_DIRS`,
  `MAX_SCAN_DEPTH`), to the attention-moment refresh, or to the sidebar tree.
- No persistent or on-disk index; the index stays in memory and is rebuilt on
  scope changes.
- No change to the disposal lane's capacity (`DISPOSAL_RETAINED_BYTE_CAPACITY`,
  which `stream-replace-all` may touch in parallel): the installed budget stays
  64 MiB so the index's reservation is unchanged.
- No fuzzy-matching change (nucleo, scores, tie order).

## User-visible impact

- Workspaces of up to 1,048,576 files are fully searchable from the palette
  (today: the first 131,072 in traversal order).
- The index's memory falls about sevenfold for the same files (an estimate:
  363 B/file measured against about 50 B/file estimated), and opening the palette in a large workspace is ready sooner
  (no per-file `realpath`).
- Loss compared with `main`: none at today's sizes. A category view of a query
  matching more than 131,072 files lists its best 131,072 and says "Showing
  131,072 of N results"; on `main` such a category could not exist, because the
  index itself stopped at 131,072 files.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `command-palette-source-groups`: the file-index bounds (files, directories,
  byte ledgers), the arena representation and its identity rule, mutation and
  query cost at 2^20 files, the open-tab count through the index, and the
  category view's row cap.
- `performance-regression-coverage`: the maximum indexed-file corpus becomes
  1,048,576 files, and the arena's parity with the per-file reference is
  covered.

## Impact

- `crates/lushtext-core/src/services/palette/index.rs` (split into an arena
  module and the traversal), `services/palette/{fuzzy,category,grouped,mod}.rs`,
  `model/palette.rs` (`IndexedFile` becomes a borrowed view type),
  `services/file_tree.rs` and `services/filesystem` (a symlink flag from the
  directory entry type, no extra syscall), `ui/command_palette/{policy,item,
  category_model,index_execution}.rs`, `ui/window/palette_shell.rs`.
- Benches, unit, property and widget tests that build `IndexedFile`s or call
  `FileIndex::files()`.
- Docs: `docs/thresholds.md`, AGENTS.md (file index cap paragraph),
  `docs/workflow-readability-matrix.md` (WFR-COMMAND-PALETTE), the programme
  record, the GTK axiom ledger (A95 re-probed at 2^20).
