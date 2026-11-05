## Context

Row PT of `docs/next/long-line-documents.md` (maintainer decision of
2026-11-01) converts the thresholds that predate the rule in
`.agents/rules/rust.md` ("Thresholds are powers of two, rounded the
conservative way"). The row was queued before C6b, C6c and CF. This design
covers their constants too:

- `HIGHLIGHT_CHUNK_BYTES` 8 KiB and `MODEL_MARKDOWN_CAP` 512 KiB;
- the reanalysis pacing steps: 512, 128, 2^20 and 256 steps;
- the font-zoom `PRUNE_FLOOR` 64.

All of them already comply.

The rule defines four kinds:

- **cap**: rounded down (since D12, an *internal resource* cap only);
- **presentation cap** (D12, the maintainer's amendment of 2026-11-05): a
  cap on what the user sees or gets, rounded **up** from its pre-PT value
  when its bound still holds by measurement, else kept as an exception;
- **coverage window**: rounded up;
- **policy tier**: a power of two aligned with its neighbours, shown in IEC
  units;
- **external fact**: kept exact, cited from the ledger.

A value that cannot be a power of two without a measured, user-visible loss
is a recorded exception. The row adds:

- a re-measurement of the slice sizes;
- the user-visible tiers 8 MiB / 64 MiB / 512 MiB, the last only after the
  exclusive-load memory peak is measured;
- the coupled pair `N_INSPECT`/`L_PROTECT`.

The C6b review asked separately (N2) that `MAX_SYNTAX_PARAGRAPH_BYTES` name
its kind.

## Goals / Non-Goals

**Goals**

- Classify every in-scope constant. Convert each to a power of two the
  conservative way, or record it as an exception with its measurement.
- Keep every Kani-tied relation true and rerun the shards that read a changed
  constant.
- Make the classification a gate, so the next threshold cannot skip it.
- Put the user-visible size tiers in IEC units, both in the policy and in the
  text that names them.
- Never prune a user's stored data on upgrade.

**Non-Goals**

- Time constants (`*_MS`, `*_US`, `Duration`). The rule says time budgets
  already follow it, and C6 converted the ones it touched. The rest are out of
  this change.
- UI geometry, percentages, zoom levels, GLib priorities, multipliers,
  measured rates, protocol identifiers, structural counts, and
  test/bench/fuzz/fixture/axiom-probe data (D1).
- A measurement campaign. Measurements are limited to the ones the row names:
  - the slice sizes;
  - the memory peak for the open tier;
  - the stall basis of the coupled pair;
  - two corpus counts that decide whether a cap is an exception.
- ~~File sizes the app merely *displays*~~: brought into scope by the
  amendment (D14).

## Measured baseline

Scanned on `main` `fbf0f875` (2026-11-05, host 1-minute load 0.33–1.16 before
and after; the counts are I/O-light `find`/`awk` runs pinned with
`taskset -c 8-23`).

**Inventory.** 652 integer `const` declarations in the workspace crates,
excluding `tests/`, `kani_proofs.rs` and the time suffixes. 200 of them are
`gtk-lush-axioms` probe parameters. Of the rest, 198 are not a power of two.
D1 keeps about 95 of those in scope (the count includes the twelve per-language gates and the `cfg(test, kani)` reference tiers). The full table is at the end of this document.

**What rounding the syntax gates down would cost.** Corpus:

- the local crates.io registry (13,371 `.rs` files);
- `/usr/include` (3,702 headers);
- Python 3.14's stdlib (760 files);
- the maintainer's `~/Workspace` (77,857 `.js`, 16,414 `.ts`, 26,953 `.sh`,
  171,837 `.json`, 42,621 `.yaml` and 473,555 `.java` files).

For each language, the share of files highlighted today that would lose
highlighting:

| Gate | Today → power of two below | Files that lose highlighting (share of those highlighted today) |
|---|---|---|
| RUST | 184 → 128 KiB | 119 (0.90 %) |
| C (headers) | 84 → 64 KiB | 29 (0.80 %) |
| PYTHON | 48 → 32 KiB | 44 (6.22 %) |
| JAVASCRIPT | 18 → 16 KiB | 814 (1.19 %) |
| TYPESCRIPT | 10.5 → 8 KiB | 189 (1.23 %) |
| SHELL | 39 → 32 KiB | 1,347 (5.94 %) |
| JSON (attach) | 24 → 16 KiB | 10,883 of 148,432 (7.33 %, recount at review) |
| JSON (typing) | 20 → 16 KiB | 9,721 (6.48 %); 8,963 of 146,512 (6.12 %) at review |
| HTML (attach) | 46 → 32 KiB | 57,527 of 792,448 (7.26 %, counted at review) |
| HTML (typing) | 16 KiB | already a power of two |
| YAML | 144 → 128 KiB | 156 (0.37 %) |
| XML | 132 → 128 KiB | 231 (0.25 %) |
| MARKDOWN | 320 → 256 KiB | 592 of 613,131 (0.10 %) |
| CSS | 37 → 32 KiB | 0 of 2,353: **converts to 32 KiB** (no loss) |
| `S_SYNTAX_OTHER` (Java as a sample) | 47 → 32 KiB | 10,082 (2.17 %) |

The spec-stage scan keyed constants by bare name, so the two inline modules'
`HTML` and `JSON` collapsed: the HTML attach gate (46 KiB) was missing and the
JSON attach gate was recorded at the typing gate's 20 KiB. The review stage
recounted both (2026-11-05, load 0.86–1.35 before and after, `taskset -c
8-23`, `find` over `~/Workspace` pruning `.git`), and D10 now keys rows by
module-qualified name.

**What rounding `L_SYNTAX` down would cost.** For Markdown files, the share
whose longest line is in (1,024, 2,000] bytes, among files whose longest line
is ≤ 2,000:

- this repository's `docs/` and `openspec/`: 3.1 %;
- a 1,791-file sample of `~/Workspace`: 3.3 %;
- sixteen other `~/Workspace` batches: 0.7–21.8 %;
- the registry's Markdown: 0 %.

**What rounding `L_SEARCH_HIGHLIGHT` down would cost.** Longest line in
(8,192, 10,000]: 1 of 1,791 sampled Markdown files (0.056 %). The sample is
below D2's 2,000-file floor, so it decides nothing yet: task 4.5 recounts it,
with `L_MINIMAP` and `L_INVISIBLES`, on the full corpus.

**Palette index.** The 0.9.x AppStream release note cites "a real
87,624-file workspace". 65,536 would truncate it.

## Decisions

### D1. Scope: what is a threshold

A constant is **in scope** when it bounds work per dispatch, memory, retained
entries or retries, or when it decides a feature or tier from a document's or
data set's size, length or count. It is in scope whether it counts bytes,
characters, paragraphs, rows or entries.

**Out of scope**, each class listed in `docs/thresholds.md` so the gate can
tell the two apart:

- UI geometry in sp or px;
- percentages, permille and the zoom grid;
- GLib, GtkSourceView and mark priorities;
- exit codes, errno values and hash constants;
- time units;
- factors and multipliers;
- measured rates (`*_COST_NS_PER_KIB`);
- structural counts: enum lengths, bucket counts, the number of profile
  thresholds;
- presentation shapes, such as how many context lines an excerpt shows;
- benches, fuzzing, examples, fixtures, `gtk-lush-axioms` probe parameters
  and Kani harness bounds.

*Rationale:* the rule exists for budgets whose margin it states honestly and
whose division CBMC verifies cheaply. A 24 px margin or a 95th percentile has
neither property.

### D2. Kinds and their rounding, extended to the cases the rule leaves implicit

The rule's four kinds, plus two that the inventory needs:

- **Derived**: a quotient, product or sum of other constants (`N_DENSE`,
  `HEAD_CAP`, `MAX_SYNTAX_PARAGRAPH_BYTES`, the 72 MiB and 96 MiB composite
  budgets, `MAX_RECENT_LOAD_CANDIDATES`). It is not rounded, because rounding
  it would break the identity it encodes. Its inputs follow the rules. This is
  the C6b review's reading in N2.
- **Measurement record**: a measured value that decides nothing (`L_EDIT_STALL`).
  It is kept exact.

**Materiality.** The rule keeps an exception only for a *measured,
user-visible* loss, and the spec stage applied that unevenly (Markdown's
0.10 % kept an exception, `L_SEARCH_HIGHLIGHT`'s 0.056 % did not, CSS's 0 %
was kept anyway, and `L_MINIMAP` and `L_INVISIBLES` were not counted at all).
One criterion now decides every feature cap: rounding is a loss when at least
0.05 % (one in 2,000) of the corpus files that have the feature at today's
value lose it, in a corpus of at least 2,000 such files. Below that, or at
zero, the cap is rounded and the count recorded; a smaller sample is
recounted. *Rationale:* user experience first keeps every measured loss above
noise (Markdown's 592 files, XML's 231) while a one-file hit in a 1,791-file
sample is noise; 0.05 % is the lowest floor that a 2,000-file corpus can
resolve at all (one file).

Two cases the rule leaves implicit:

- **Retention tiers and retry budgets** round **up**. A smaller retention tier
  prunes data the user already has. A smaller retry budget turns a transient
  failure it was sized for into a reported one, which is the coverage failure
  mode.
- **Accessible-text length caps** round down. Each is checked against the
  longest string the app ships (task 4.6). A cap that would truncate a
  shipped string becomes an exception. *Superseded by D12:* they are
  presentation caps and round up; the shipped-string check stays as a floor.

*Amended by D12:* the materiality rule above decided only whether a feature
cap could be rounded **down**. Presentation caps never are, so it no longer
decides anything; its corpus counts stay as evidence.

### D3. User-visible size tiers

| Tier | Today | New | Kind |
|---|---|---|---|
| `S_MINIMAP` | 10,000,000 | 8 MiB, **16 MiB** since D12 (else 10,000,000) | policy tier, presentation cap |
| `HISTORY_SAVE_ONLY_BYTES` | 10,000,000 | 8 MiB, **16 MiB** since D12 (else 10,000,000) | policy tier, presentation cap |
| `UNDO_TOTAL_BYTES` | 50,000,000 | 64 MiB | policy tier |
| `HISTORY_UNAVAILABLE_BYTES` | 50,000,000 | 64 MiB | policy tier |
| `REFUSE_TO_OPEN` | 500,000,000 | 512 MiB if task 3 passes, else 500,000,000 (exception) | policy tier |
| `BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD` | 10,000,000 | 8 MiB | cap (it mirrors the old syntax tier) |
| `MAX_REPLACE_FILE_BYTES` | 10 MiB | 8 MiB, **16 MiB** since D12 | presentation cap (files a bulk action processes) |

*Rationale:*

- 8 and 64 MiB are the rule's tiers, and each sits next to an existing
  power-of-two neighbour: `S_SYNTAX` 1 MiB, the 64 MiB draft and Replace All
  ceilings, and the 256 MiB transient and editor budgets.
- Raising undo to 64 MiB stays inside the live-editor budget: 3 × 64 MiB =
  192 MiB < 256 MiB.
- 512 MiB raises the overweight disposal capacity
  (`REFUSE_TO_OPEN × DECODED_BODY_SOURCE_MULTIPLIER`) from 1.5 GB to 1.5 GiB
  (+7.4 %), hence the measurement.
- If it fails, 256 MiB would refuse files that open today. That is a visible
  loss, so the fallback is the exception, not the lower power of two.

**Messages.** These print the tier in IEC units:

- the open refusal (`editor_io.rs`'s "too large to edit") and its
  grew-while-opening variant ("grew beyond the supported limit"), both of
  which divide by 1,000,000 today;
- the Replace All skip messages (`content_search/replace.rs`: "larger than
  the 10 MB Replace All limit", "10 MB per-file limit", "64 MB Replace All
  limit"), which print "MB" for a MiB cap today and must follow
  `MAX_REPLACE_FILE_BYTES` to 8 MiB;
- the local-history browse limit and its unavailable message;
- the reduced-features notice;
- the editor's accessible description.

They use one formatting helper that prints whole IEC units for the tiers
("64 MiB").

**Kani.** `retired_byte_tiers` existed to prove C1 changed no tier. PT changes
the tiers on purpose. The module becomes `byte_tier_reference`, holding the
new table. The harness proves `DocumentScale::for_total_bytes` agrees with it
for every `u64`. The edges at 8 MiB, 64 MiB and 512 MiB, and each plus one,
are unit-tested.

### D4. Caps rounded down

> **Amended by D12 (2026-11-05).** Every cap below that hides, skips,
> truncates or drops something a user can observe is a presentation cap and
> rounds **up** from its pre-PT value instead; D12's table gives the new
> value of each. What remains of D4 is the internal caps (`N_VISIBLE`, the
> slice and per-turn caps).

These need no budget measurement: a smaller cap only reduces features
sooner, so every guarantee holds. The caps that turn a feature off still need
D2's corpus count of what users lose (task 4.5). The value each becomes, and what changes visibly:

- **`L_MINIMAP`** 22,000 → 16,384: no minimap for paragraphs of 16–22 k
  characters.
- **`L_INVISIBLES`** 50,000 → 32,768: no invisibles for 32–50 k.
- **`L_SEARCH_HIGHLIGHT`** 10,000 → 8,192: corpus loss ≈ 0.
- **`N_VISIBLE`** 500,000 → 262,144: no visible change. It is only counted,
  and `HEAD_CAP` is bound by `N_HEAD`.
- **`N_HEAD`** 10,000 → 8,192: Protected View heads are 1,808 characters
  shorter.
- **The 10,000-entry caps** → 8,192:
  - the scan caps: `MAX_SET_ASIDE_SCAN_ENTRIES`,
    `SIDECAR_SCAN_MAX_ENTRIES`, `LOCAL_HISTORY_SCAN_MAX_LINEAGES`,
    `REPLACE_JOURNAL_SCAN_MAX_ENTRIES`, `JOURNAL_SCAN_MAX_ENTRIES`,
    `NOTES_BROWSER_SIDECAR_SCAN_LIMIT`;
  - the result caps: `RESULT_CAP`, `FIND_RESULT_CAP`,
    `MAX_REPLACE_PREVIEW_ROWS`;
  - the entry caps: `MAX_PALETTE_NOTE_ENTRIES`,
    `NOTES_BROWSER_SOURCE_ENTRY_LIMIT`,
    `NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT`;
  - the sidebar's `MAX_DIR_ENTRIES`.

  The bench constants that mirror them follow.
- **`BOOKMARK_EXCERPT_SCAN_LINE_LIMIT`** 20,000 → 16,384.
- **The minimap marker caps** (`SEARCH_MATCH`, `MODIFIED_LINE_MARK`,
  `LONG_LINE_MARK`) 2,000 → 1,024. These bound GTK-thread work: the search
  cap is the number of `SearchContext::forward` calls one refresh makes, in
  document order. The strip is as tall as the editor (the 64–160 px figure in
  the policy's rustdoc is the map's *width*), so the cap does not saturate
  it: in a document with 1,025–2,000 matches, modified lines or long lines,
  the markers past the 1,024th, which lie lower in the document, are no
  longer drawn. This is listed in the proposal's Impact; the refresh does
  half the GTK work.
- **`MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET`** 8,000 → 4,096.
- **`MAX_RESULTS_PER_SOURCE`** 50 → 32 rows per palette category (visible:
  a category shows at most 32 matches; listed in the Impact).
- **`NOTES_BROWSER_RENDER_LIMIT`** 500 → 256.
- **`MAX_RECENT_LOAD_DIAGNOSTICS`** 20 → 16.
- **`MAX_INLINE_FOOTNOTE_REPLACEMENTS`** is derived (`MAX_MARKDOWN_EVENTS / 4`)
  and follows its input.
- **`MAX_MARKDOWN_EVENTS`** 50,000 → 32,768 **unless** task 4.5's corpus
  count, under D2's materiality rule, finds a loss: Markdown files within
  `MAX_MARKDOWN_SOURCE_BYTES` (4 MiB) whose event count is in
  (32,768, 50,000], as a share of those at or below 50,000. Such files would
  lose their full preview, and the cap then stays exact as an exception.
- **The two table-cell caps** 1,000 → 512, with the same corpus rule (largest
  table in (512, 1,000] cells).
- **`L_MINIMAP`, `L_INVISIBLES`, `L_SEARCH_HIGHLIGHT`** take the values above
  unless task 4.5's corpus count of longest paragraphs finds a loss under the
  same rule; each then keeps its former value as a recorded exception.
- **The text caps**:
  - `DEFAULT_ANNOUNCEMENT_LIMIT` 240 → 128;
  - `EDITOR_IDENTITY_LIMIT` 96 → 128 (stage 4: it cuts user file names, so
    D2's corpus rule decides it, not the shipped-string check);
  - `REDUCED_FEATURES_NOTICE_LIMIT` 320 → 256;
  - `cargo-gtk-proof`'s artifact and warning text limits 1,200 → 1,024.

  Each is subject to D2's shipped-string check.

### D5. Coverage windows rounded up

- **`R_BATCH`** 224 → 256, and `R_FOLD = R_BATCH` follows. 224 covers the
  223 rows a 2000 px batch can touch at 9 px (ledger A31, 50 % zoom) with one
  row to spare. 256 states the margin honestly and equals `MAX_R_FOLD`, the
  fold ring's capacity, so the ring needs no change.

  Consequences:
  - the edit-time watch may read up to `2·R_batch + 2` = 514 line lengths,
    still O(1) per keystroke;
  - more documents count as dense (a 256-paragraph window sums more than a
    224 one);
  - `N_DENSE = V_BATCH / R_BATCH` becomes 218 (from 250), so a dense
    paragraph in Protected View shows 32 fewer characters.

  That loss is the price of the larger window. It is accepted, because a
  coverage window is never rounded down.
- **`MAX_CONSTRUCTION_STAGES`** 48 → 64.
- **`MAX_FRAME_PHASE_FRAMES`** 14,400 → 16,384 (probe-only).
- **`MANIFEST_REPAIR_ENTRY_OVERHEAD_BYTES`** 96 → 128, a conservative charge
  that must not under-count.
- **`REVEAL_SETTLE_FRAME_BUDGET`** 60 → 64 frames.

### D6. Retention tiers and retry budgets rounded up

- `PER_DOCUMENT_SNAPSHOT_CAP` 48 → 64 and `GLOBAL_SNAPSHOT_CAP` 240 → 256.
  At most 256 snapshots of at most 64 MiB each, the same order of disk use
  as today's 240 of 50 MB.
- `MAX_RECENTS` 200 → 256, so the derived candidate bound is 512.
- Search history `MAX_HISTORY` 20 → 32.
- The set-aside `SOFT_BOUND_BODIES` 100 → 128. It now pairs with the 256 MiB
  byte bound (four 64 MiB bodies).
- Retries: `MAX_MIGRATION_ATTEMPTS` and `MAX_BOOKMARK_SAVE_ATTEMPTS` 3 → 4,
  `EDITOR_FOCUS_MAX_ATTEMPTS` 6 → 8, `MAX_UNIQUE_NAME_ATTEMPTS` 1,000 → 1,024.

*Rationale:* rounding any of the first group down would delete stored
history, recents or search history at the first trim after upgrade. Rounding
up loses nothing, **provided the larger store still loads**. A raised
retention tier must stay within every bounded read of its store
(`threshold-constants`):

- `recent-documents.json` is read through `MAX_RECENT_DOCUMENTS_BYTES`
  (1 MiB). An entry holds `path` and `canonical_path`, each up to 4,096 bytes,
  so 256 worst-case entries serialize to about 2.03 MiB. (200 already reached
  about 1.6 MiB, a latent reset that the cap-plus-one path turns into a lost
  recents list.) `MAX_RECENT_DOCUMENTS_BYTES` becomes **4 MiB**, a coverage
  bound rounded up; the load runs on a worker (`spawn_blocking_then` in
  `ui/window/recent_documents_journal.rs`), so GTK never reads it.
- Search history (32 entries) and the local-history index (256 snapshots)
  get the same worst-case load test; a read bound that fails it is raised
  the same way, in the same commit.

### D7. The coupled pair `L_PROTECT` / `N_INSPECT`

`N_INSPECT ≤ min(HEAD_CAP, L_PROTECT)` is a compile-time assertion and a Kani
bound (`core-protected-view`). `L_PROTECT` is C0's stall half: dense
500-character lines pass, 710 fail. Both move to the same power of two:

- **512**, if the lane's new `dense/512` case keeps every validation dispatch
  until settle within half of `B_SETTLE_US` and every post-open dispatch
  within `B_SETTLE_US` in both wrap modes and all three input variants.
  That is the criterion `dense/500` met (task 5.1). The Line Inspector window
  grows by 12 characters.
- **256** otherwise. The inspector pages twice as often, a visible cost
  recorded with the measurement.

`V_BATCH` (56,000 = 500 × A31's 112 rows) is **not** re-derived from the new
`L_PROTECT`, because it stays an exception (D9).

### D8. Slice sizes, re-measured (interleaved with `main`)

- **`V_SLICE`** 11,360: the last measured size below the A70 step, which lies
  between 11,360 (≤ 4.0 ms) and 22,720 (8–9 ms).

  Candidates are 16,384 and 8,192. Adopt **16,384** if the lane's
  `dense/{120,500,710}` cases in both wrap modes and three input variants
  keep every validation dispatch until settle within half of `B_SETTLE_US`
  and every post-open dispatch within `B_SETTLE_US`. Otherwise adopt
  **8,192**, if its activation-to-laid-out time is no worse than `main`'s by
  more than one reference frame.

  If neither holds, 11,360 stays as an exception, with the measured total
  time recorded. 16,384 is not a rounding up of a cap: it is a new measured
  point, the same way C5 measured `L_WRAP_EDIT`.

  This also settles C6's accepted miss ("Revisit when `power-of-two-thresholds`
  re-measures `V_SLICE`").
- **`SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES`** 704 KiB (and
  `PREVIEW_DIRECT_THRESHOLD_BYTES`, which mirrors it) → **512 KiB**. Documents
  of 512–704 KiB then take a sliced install.

  Measure their activation-to-laid-out time at 512 KiB, 600 KiB and 704 KiB
  against `main`. Keep 512 KiB if the branch is within one reference frame of
  `main` at every size. Otherwise keep 704 KiB as an exception with the
  figures.
- **`MAX_SEARCH_EVENTS_PER_TICK` and `SEARCH_RETIREMENT_ROWS_PER_SLICE`**
  250 → **256** if a full turn of 256 events or rows stays within
  `B_SETTLE_US`. Measure on the search-panel widget fixture with the probe
  build's dispatch timing (task 5.4). Otherwise → **128**, recording the
  total-time cost.

### D9. Recorded exceptions

Each exception keeps its exact value. Its rustdoc says "Kind: …; recorded
exception (`power-of-two-thresholds`)", gives the measurement above, and
replaces "deferred to `power-of-two-thresholds`".

- **`L_SYNTAX` 2,000.**
  - Rounding down to 1,024 unhighlights 3–22 % of real Markdown files (the
    baseline).
  - 2,048 is unmeasured, and it breaks
    `MAX_SYNTAX_PARAGRAPH_BYTES (4·L_SYNTAX + 3) ≤ HIGHLIGHT_CHUNK_BYTES`
    (8,195 > 8,192), whose 16 KiB alternative was measured over budget
    (A69).
- **Eleven per-language attach gates, JSON's typing gate, and
  `S_SYNTAX_OTHER`.** Rounding down unhighlights 0.10–7.3 % of real files
  (the baseline table). The twelfth attach gate, CSS, loses nothing at
  32 KiB (0 of 2,353 files) and converts.
- **`L_PROTECT_EDIT` 50,000.**
  - This is the maintainer's editability contract ("editable up to ~50k",
    2026-10-30).
  - 32,768 would make documents with 32–50 k paragraphs view-only.
  - 65,536 is above the accepted keystroke cost: C5 measured 7.7–11.1 ms at
    50,000 and 16–20 ms at 100,000.
- **`V_BATCH` 56,000.** Rounding down to 32,768 folds dense paragraphs to 128
  characters (with `R_BATCH` 256). Rounding up to 65,536 is unmeasured over
  the batch budget.
- **`MAX_INDEXED_FILES` and `MAX_INDEXED_DIRECTORIES` 100,000.** A real
  87,624-file workspace exists, and 65,536 would truncate it. Memory is
  bounded independently by the 128 MiB and 64 MiB byte ledgers.
- **`MAX_PREVIEW_IMAGE_SOURCE_PIXELS` 64,000,000.** 2^25 (33.5 MP) would
  refuse 48–50 MP phone photos. 2^26 is unmeasured against the decode memory
  ceiling.
- *Amended by D12:* each exception that is a presentation cap
  (`L_SYNTAX`, the attach and typing gates, `S_SYNTAX_OTHER`,
  `L_PROTECT_EDIT`, the paragraph gates, the Markdown caps, the index and
  image caps) becomes `presentation cap, exception` or rounds up, as D12's
  table says.
- **Conditional exceptions:** `REFUSE_TO_OPEN` (D3), `V_SLICE` and the
  direct-install threshold (D8), `MAX_MARKDOWN_EVENTS`, the table-cell caps,
  `L_MINIMAP`, `L_INVISIBLES` and `L_SEARCH_HIGHLIGHT` (D4). Each is decided
  by its task.

### D10. The gate: `make check-thresholds`

`docs/thresholds.md` is the committed inventory. It has one row per integer
`const`/`static` in the production crates' `src/`, excluding `#[cfg(test)]`
modules, `kani_proofs.rs` and `gtk-lush-axioms`. Each row gives:

- path and name;
- value;
- kind;
- rounding or reason.

An out-of-scope row names its D1 class.

`scripts/check-threshold-kinds.py`, run by `make check-thresholds`, which is
part of `check-policy`:

1. finds every such constant, keyed by path and module-qualified name (the
   spec-stage scan's bare-name keys merged `attach_threshold::JSON` with
   `typing_threshold::JSON`), including declarations that span lines;
2. evaluates simple literal expressions, including references to other
   constants;
3. fails on an unlisted constant, a stale row, or a value mismatch;
4. fails on an in-scope constant that is not a power of two unless its kind
   is derived, external fact, measurement record or exception;
5. checks that an in-scope constant's rustdoc contains a `Kind:` line naming
   the same kind.

It ships with a self-test, failing first on a fixture tree.

*Alternative rejected:* a lint on names (`*_LIMIT`, `*_CAP`). Names do not
carry the kind: `L_PROTECT_EDIT` is a policy decision, and `R_BATCH` is a
coverage window.

### D11. Order of work

1. The gate and the inventory, at today's values. This proves the check sees
   every constant.
2. The failing-first value tests.
3. Pure conversions: D4, D5, D6, the D3 tiers except the open tier, and D9's
   rustdoc.
4. Kani reruns.
5. The measured items: D3's open tier, D7, D8, and D4's corpus counts.
6. Lane verification.
7. Docs.

Each measured item lands in its own commit with its figures, so a failed
measurement reverts one value.

### D12. Presentation caps round up (maintainer amendment, 2026-11-05)

PR #84 rounded every cap down, and so shipped visible losses against `main`:
palette categories 50 → 32, the notes browser 500 → 256, Replace All skipping
files of 8–10 MiB, every user-visible 10,000 cap → 8,192, minimap markers
capped at 1,024, minimap and per-edit local history lost for 8–10 MB files,
Protected View heads 1,808 characters shorter, dense heads 32 characters
shorter, CSS files of 32–37 KiB unhighlighted. The maintainer's decision
(2026-11-05): LushText must be extremely performant **and** handle high loads
and large workspaces with no visible loss. D4's "caps rounded down" is
therefore split in two kinds.

**The kind.** A **presentation cap** is a cap on what the user sees or gets:
results, rows, notes, markers, entries listed, files a bulk action processes,
characters shown or announced, and the size or length at which a feature turns
off. It rounds **up** to the power of two at or above its pre-PT value
(`main` `fbf0f875`), provided the resource bound it protects still holds **by
measurement**: memory, per-turn GTK time against `B_INTERACT_US` (8 ms) or
`B_SETTLE_US` (16 ms), worker memory. If the measurement says the bound does
not hold, the cap keeps its pre-PT value as a recorded exception
(`presentation cap, exception`) with the measurement. It is never rounded
down.

**The test.** A cap is a presentation cap if rounding it down can hide, skip,
truncate or drop anything a user could observe. User experience first: when
in doubt, it is a presentation cap. Internal-only resource caps (slice sizes,
per-turn event counts, admission budgets, chunk sizes, scan pages that resume)
keep D4's rounding down, because a smaller one only does the same work in more
steps.

**Consequences for the other kinds.**

- The `text cap` kind is retired: accessible-text and announcement lengths are
  characters announced, so they are presentation caps. Each stays at or above
  the longest string the app ships (task 4.6's unit test stays).
- A policy tier at which a feature turns off is a presentation cap too
  (`S_MINIMAP`, `HISTORY_SAVE_ONLY_BYTES`): it rounds up, never down.
- D2's materiality rule no longer decides anything: it only decided whether a
  feature cap could be rounded **down**, and a presentation cap never is. Task
  4.5's corpus counts stay recorded as the evidence that these caps must not
  fall.
- A derived value the user sees (`N_DENSE`, the characters a dense paragraph
  keeps in Protected View) must not fall below its pre-PT value because an
  input moved, without a measurement and a recorded reason.
- An exception kept *because* the power of two above fails its bound needs a
  measurement of that failure. Where the measurement that derived the value
  already shows it (the A69 sweep that derived each per-language attach gate,
  C2's head sweep), it is cited, not repeated.

**The gate.** `docs/thresholds.md` gains the kind `presentation cap`, whose
rounding column records the pre-PT value as `up from <value>`.
`scripts/check-threshold-kinds.py` fails when a presentation cap's value is
not the power of two at or above that value, or, for `presentation cap,
exception`, not that value exactly. Its self-test fails first on a
presentation cap rounded down, on one rounded up past the next power of two,
and on an exception that differs from its recorded value. `text cap` rows
become presentation caps. A temporary `pending-presentation` class covers the
rows between the gate commit and their conversion, and is removed at the end
(the same pattern as `pending-PT`).

**Reclassification.** Every constant whose PR #84 value hides, skips,
truncates or drops something `main` showed, plus every pre-PT presentation cap
that was kept as an exception and could now round up:

| Constant (module) | Pre-PT (`main`) | PR #84 | New | Bound it protects | How the bound is measured (task) |
|---|---|---|---|---|---|
| `MAX_RESULTS_PER_SOURCE` (`ui/command_palette/policy`) | 50 | 32 | **64** | the result splice of one query on GTK, every category full, within `B_INTERACT_US` | widget fixture, probe dispatch timing (M1) |
| `NOTES_BROWSER_RENDER_LIMIT` (`ui/window/notes/policy`) | 500 | 256 | **512** | the sidebar rebuild dispatch within `B_SETTLE_US` | widget fixture (M6) |
| `NOTES_BROWSER_OPEN_EDITOR_SNAPSHOT_LIMIT` | 10,000 | 8,192 | **16,384** | the open-editor snapshot capture on GTK within `B_SETTLE_US`; 4 MiB retained limit unchanged | widget fixture (M6) |
| `NOTES_BROWSER_SOURCE_ENTRY_LIMIT`, `NOTES_BROWSER_SIDECAR_SCAN_LIMIT` | 10,000 | 8,192 | **16,384** | worker memory and time; the 64 MiB text limit unchanged | scan fixture, scope `memory.peak` (M12) |
| `MAX_PALETTE_NOTE_ENTRIES` (`services/palette/notes`) | 10,000 | 8,192 | **16,384** | worker memory; the 64 MiB text and 96 MiB construction limits unchanged | scan fixture (M12) |
| `RESULT_CAP` (`services/content_search/search`) | 10,000 | 8,192 | **16,384** | per-turn dispatch (256 events, unchanged) within `B_SETTLE_US`; retained model memory; retirement within its two-generation backpressure | widget fixture at 16,384 matches, probe timing, RSS delta (M2) |
| `MAX_REPLACE_PREVIEW_ROWS` (`model/content_search`) | 10,000 | 8,192 | **16,384** | the 64 MiB preview byte budget (unchanged); O(1) row binding; per-turn row binding within `B_SETTLE_US` | scale fixture at 16,384 rows (M3) |
| `FIND_RESULT_CAP` (`ui/editor_page/protected_view/policy`) | 10,000 | 8,192 | **16,384** | retained find records ≤ 16,384 × `MAX_FIND_RECORD_BYTES` (512) = 8 MiB; per-turn 256 results / 64 KiB unchanged | widget fixture, RSS delta and per-turn timing (M4) |
| `MAX_DIR_ENTRIES` (`ui/sidebar/workspace_section/scan_execution`, bench mirror) | 10,000 | 8,192 | **16,384** | the scan worker's memory; 256-row append batches per turn (unchanged) within `B_SETTLE_US`; `ViewportSliceBin` keeps realized rows bounded | widget fixture with a 16,384-entry directory (M5) |
| `MAX_SET_ASIDE_SCAN_ENTRIES` (`services/draft_service/set_aside`) | 10,000 | 8,192 | **16,384** | the worker scan's time and memory | scan fixture (M12) |
| `SIDECAR_SCAN_MAX_ENTRIES`, `LOCAL_HISTORY_SCAN_MAX_LINEAGES`, `REPLACE_JOURNAL_SCAN_MAX_ENTRIES` (`services/format_upgrade/diagnostics`) | 10,000 | 8,192 | **16,384** | the startup preflight worker's time and memory | scan fixture (M12) |
| `JOURNAL_SCAN_MAX_ENTRIES` (`services/search_backup`) | 10,000 | 8,192 | **16,384** | startup inspection worker; must stay ≥ the files one Replace All can touch (≤ `RESULT_CAP`), a compile-time assertion | scan fixture (M12) |
| `BOOKMARK_EXCERPT_SCAN_LINE_LIMIT` (`services/bookmark_excerpt`) | 20,000 | 16,384 | **32,768** | the 1 MiB scan byte limit (unchanged) bounds memory; worker time | unit test of the byte bound; scan timing (M12) |
| `MINIMAP_SEARCH_MATCH_CAP`, `MINIMAP_MODIFIED_LINE_MARK_CAP`, `MINIMAP_LONG_LINE_MARK_CAP` (`ui/editor_page/minimap/policy`) | 2,000 | 1,024 | **2,048** | one marker refresh on GTK (`SearchContext::forward` calls, mark walks) within `B_SETTLE_US` | widget fixture with 2,048 of each marker, probe timing, interleaved with `main` at 2,000 (M7) |
| `MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET` | 8,000 | 4,096 | **8,192** | one wrapped-layout analysis slice within `B_SETTLE_US` (lines longer than it fall back to the unwrapped projection, so marker placement is visible) | analysis fixture with 8,192-character lines (M7) |
| `MAX_REPLACE_FILE_BYTES` (`services/content_search/replace`) | 10 MiB | 8 MiB | **16 MiB** | worker peak memory for one file (read, output, undo entry); the aggregate undo payload `MAX_REPLACE_UNDO_BYTES` 64 MiB, unchanged, still bounds the run | Replace All over 16 MiB files under `with-memory-cap`, `memory.peak`, interleaved with `main` at 10 MiB (M9) |
| `MAX_RECENT_LOAD_DIAGNOSTICS` (`services/recent_documents`) | 20 | 16 | **32** | 32 bounded messages; no timed path | unit test |
| `REDUCED_FEATURES_NOTICE_LIMIT` (was `text cap`) | 320 | 256 | **512** | AT text length; the 256-character announcement cap is separate and unchanged | unit test (longest notice, task 4.6) |
| `ARTIFACT_TEXT_LIMIT`, `WARNING_TEXT_LIMIT` (`cargo-gtk-proof`, were `text cap`) | 1,200 | 1,024 | **2,048** | artifact size, inside the unchanged 8 MiB JSON caps | unit test |
| `DEFAULT_ANNOUNCEMENT_LIMIT`, `EDITOR_IDENTITY_LIMIT` (were `text cap`) | 240 / 96 | 256 / 128 | 256 / 128 (kind only) | already rounded up | — |
| `S_MINIMAP` (`model/document_scale`, policy tier) | 10,000,000 | 8 MiB | **16 MiB**, else 10,000,000 (exception) | open and post-open dispatches with the map attached within `B_SETTLE_US`; minimap analysis slices; process memory | lane `total-size` at 16 MiB and 16 MiB + 1, minimap on, interleaved with `main` at 10 MB (M10) |
| `HISTORY_SAVE_ONLY_BYTES` (policy tier) | 10,000,000 | 8 MiB | **16 MiB**, else 10,000,000 (exception) | the periodic capture's chunked snapshot per turn within `B_SETTLE_US`; worker memory; disk (64 snapshots per lineage) | periodic-capture fixture of a 16 MiB buffer, probe timing (M10) |
| `attach_threshold::CSS` (`model/document_scale`) | 37 KiB | 32 KiB | **37 KiB** (exception) | the attach's background analysis within `B_SETTLE_US` | A69's sweep derived 37 KiB as the largest passing CSS size, so 64 KiB fails by that measurement (cited, not rerun) |
| `N_HEAD` (`model/protected_fold`) | 10,000 | 8,192 | **10,000** (exception) | the head's open and settle dispatches within `B_SETTLE_US` | C2's sweep: 16.1 ms at 14,000 (wrap off) and 20.7 ms at 20,000 (wrap on), so 16,384 fails (cited) |
| `V_BATCH` → `N_DENSE` (derived) | 56,000 → 250 | 56,000 → 218 | **65,536 → 256**, else **64,000 → 250** (exception), else 56,000 → 218 (recorded) | a 2000 px validation batch of folded dense paragraphs within `B_SETTLE_US` at the default font and at 50 % zoom | lane dense Protected View cases at each candidate, interleaved with `main`; `core-protected-fold*` (M11) |
| `MAX_MARKDOWN_EVENTS` (`services/markdown_render`) | 50,000 | 50,000 (exception) | **65,536**, else 50,000 | planning worker time and memory; the 8 MiB retained ceiling and 256 events per projection turn unchanged | corpus-sized fixture at 65,536 events, scope `memory.peak`, per-turn timing (M13) |
| `MAX_MARKDOWN_CARRIED_TABLE_CELLS`, `MAX_PREVIEW_TABLE_CELLS` | 1,000 | 1,000 (exception) | **1,024**, else 1,000 | one table's widget build on GTK within `B_SETTLE_US` | preview fixture with a 1,024-cell table, probe timing (M13) |
| `MAX_INLINE_FOOTNOTE_REPLACEMENTS` (derived, events / 4) | 12,500 | 12,500 | follows (16,384) | — | — |
| `L_MINIMAP` | 22,000 | 22,000 (exception) | **32,768**, else 22,000 | C0's criterion for the minimap at that paragraph length (scroll and keystroke dispatches within budget) | lane cases at 32,768 with the feature on (M14) |
| `L_INVISIBLES` | 50,000 | 50,000 (exception) | **65,536**, else 50,000 | C0's criterion for invisibles drawing | lane cases at 65,536 (M14) |
| `L_SEARCH_HIGHLIGHT` | 10,000 | 10,000 (exception) | **16,384**, else 10,000 | C0's criterion for automatic search highlighting | lane cases at 16,384 (M14) |
| `MAX_INDEXED_FILES`, `MAX_INDEXED_DIRECTORIES` (`services/palette/index`) | 100,000 | 100,000 (exception) | **131,072**, else 100,000 | the 64 MiB installed and 128 MiB construction ledgers (unchanged) bound memory; build wall time | Criterion index-build bench and a 131,072-file synthetic tree, `memory.peak` (M15) |
| `MAX_PREVIEW_IMAGE_SOURCE_PIXELS` (`ui/markdown_preview`) | 64,000,000 | 64,000,000 (exception) | **2^26 = 67,108,864**, else 64,000,000 | the decode memory ceiling (four bytes per pixel) | decode of a 2^26-pixel image, `memory.peak`, against 64 MP on `main` (M16) |

`ui/window/local_history/policy` `PREVIEW_DIRECT_THRESHOLD_BYTES` follows
`SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES` and stays internal. Two mirrors
outside the inventory follow their production constants: the benchmark's
`NOTE_RENDER_CAP` (`benches/benchmarks.rs`, 500 on `main`, 256 on PR #84)
becomes 512 with `NOTES_BROWSER_RENDER_LIMIT`, beside the bench mirror of
`MAX_DIR_ENTRIES`; and the test-only `byte_tier_reference`'s
`DISABLE_SYNTAX_HIGHLIGHTING` (the save-only history tier, despite its
retired name) takes `HISTORY_SAVE_ONLY_BYTES`'s value, so the parity tests and
the `core-document-scale` tier harness keep comparing two independent tables.

**Kept as exceptions without a new measurement**, because the measurement
that set them already shows the power of two above breaks the bound:
`L_SYNTAX` (2,048 breaks `MAX_SYNTAX_PARAGRAPH_BYTES ≤ HIGHLIGHT_CHUNK_BYTES`),
the eleven other attach gates, JSON's typing gate and `S_SYNTAX_OTHER` (A69's
sweep), `L_PROTECT_EDIT` (the maintainer's contract; C5 measured the keystroke
cost rising past it). Their kind becomes `presentation cap, exception`.

**Reclassified, value unchanged** (already a power of two on `main`, so
rounding up is the identity; the kind changes so the gate keeps them from
falling): `MAX_LISTED_SET_ASIDE_BODIES`, `MAX_SEARCH_MATCH_LINE_BYTES`,
`MAX_REPLACE_PREVIEW_BYTES`, `MAX_REPLACE_DIAGNOSTIC_ENTRIES`,
`MAX_REPLACE_DIAGNOSTIC_BYTES`, `MAX_REPLACE_UNDO_BYTES`, `L_WRAP_EDIT`,
`typing_threshold::HTML`, `L_PROTECT` and `N_INSPECT` (rounded up by D7),
`AUTOMATION_WORKFLOW_EVENT_LIMIT`, `SNAPSHOT_EXCLUDED_NAMES_LIMIT`,
`SNAPSHOT_TEXT_MAX_BYTES`, `MAX_LOSSY_PREVIEW_ISSUES`,
`PEEK_SAMPLE_BYTE_LIMIT`, `BOOKMARK_EXCERPT_SCAN_BYTE_LIMIT`,
`BOOKMARK_EXCERPT_LINE_CHAR_LIMIT`, `MAX_AUTOMATIC_DRAFT_BYTES`,
`MAX_MARKDOWN_SOURCE_BYTES`, `MAX_MARKDOWN_STRUCTURE_DEPTH`,
`MAX_MARKDOWN_EMBED_DESCRIPTORS`, `MAX_MARKDOWN_RETAINED_BYTES`,
`MAX_MARKDOWN_CARRIED_EMBED_BYTES`, `MAX_MARKDOWN_PLACEHOLDER_WIDGETS`,
`MAX_PREVIEW_IMAGE_SOURCE_BYTES`, `MAX_PREVIEW_CODE_BLOCK_BYTES`,
`MAX_SCAN_DEPTH`, `MAX_FILE_INDEX_RETAINED_BYTES`,
`MAX_FILE_INDEX_BUILD_RETAINED_BYTES`, `MAX_PALETTE_NOTE_TEXT_BYTES`,
`MAX_PALETTE_NOTE_RETAINED_BYTES`, `MAX_PALETTE_NOTE_DIAGNOSTICS`,
`NOTES_BROWSER_SOURCE_TEXT_LIMIT`, `NOTES_BROWSER_DIAGNOSTIC_LIMIT`,
`NOTES_OPEN_EDITOR_SNAPSHOT_RETAINED_BYTE_LIMIT`,
`PREVIEW_RESERVATION_BYTES`, `WORKSPACE_SEARCH_FALLBACK_ENTRY_LIMIT`,
`WORKSPACE_SEARCH_FALLBACK_PATH_BYTE_LIMIT`, `WATCH_ERROR_MESSAGE_CAP_BYTES`,
`MAX_SEARCH_SELECTION_PREFILL_CHARS`, `ENTRY_INSERT_CAP_BYTES`,
`HELD_TEXT_CAP_BYTES`, `MAX_PENDING_ACTIVATION_OPENS`,
`MAX_BACKUP_ITEM_BYTES`, `DEFAULT_MAX_METADATA_BYTES`; and, added at spec
review because the test catches them: `AT_ATTACH_MAX_CHARS` (a larger detached
document reaches an assistive technology only when its tab is shown),
`MAX_INDEX_REPAIR_SNAPSHOT_BYTES` (past it a damaged lineage's history stays
unavailable), `MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET` (past it markers follow the
unwrapped projection, like its line-budget sibling above),
`MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES` (past it palette notes are truncated,
like its retained-bytes sibling), `MAX_NOTE_FUZZY_SCORE_BYTES` (the field
prefix nucleo scores, so it orders the results a user sees),
`SURROUNDING_WINDOW_CHARS` and `SURROUNDING_MAX_SELECTION_CHARS` (the context
an input method receives), and `DATA_DETAILS_MAX_ROWS_PER_GROUP`
(`ui/preferences/data_page`: rows listed before the omitted-count row; it was
classed `out: presentation`, which is reserved for a fixed layout shape such
as the bookmark excerpt's context lines or the peek's sample lines, not a
count that cuts a list). Task 9.1 rechecks the whole inventory under the test,
`out: presentation` and `policy tier` rows included, and adds any it
catches.

**Internal, still rounded down** (they hide nothing): the slice and chunk
sizes (`V_SLICE`, `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES`, the 64 Ki and
256 KiB slices), `N_VISIBLE` (counted by the profile; `HEAD_CAP =
min(N_VISIBLE, N_HEAD, A28_SAFE_CHARS)` is set by `N_HEAD`, so it changes no
head), `BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD` (above it a snapshot is chunked,
not refused), the per-turn caps (`MAX_SEARCH_EVENTS_PER_TICK`,
`SEARCH_RETIREMENT_ROWS_PER_SLICE`, `FIND_RESULTS_PER_TURN`), admission
budgets and concurrency limits, `WORKSPACE_WATCH_PATH_CAP` (overflow becomes a
full refresh), the draft-repair and reconcile scans that resume or stay
untrusted (`MAX_MANIFEST_REPAIR_DRAFT_SCAN`, `MAX_ORPHAN_CLEANUP_DRAFT_SCAN`,
`DRAFT_BODY_SCAN_MAX_ENTRIES`, `DEFAULT_RECONCILE_MAX_LINEAGES`) and
`MAX_REPLACE_UNDO_RETAINED_BYTES`.

**Replace All and the aggregate undo budget.** The per-file cap rises to
16 MiB; memory stays bounded by the unchanged 64 MiB aggregate undo payload
(before and after bytes of every touched file) and the 64 MiB retained-owner
cap. Every run `main` processes in full (each file ≤ 10 MiB, total undo
payload ≤ 64 MiB) is processed in full on the branch; files of 10–16 MiB,
skipped on `main`, are now processed. One residual remains: in a run whose
total undo payload exceeds 64 MiB, a 10–16 MiB file early in the order now
consumes budget that `main` left to later files, so a later file can be
skipped (with its reason) where `main` skipped the large one. Every skipped
file is listed with its reason on both sides. `stream-replace-all` (D13)
removes the in-memory aggregate and with it the residual.

**Measurement rules (task 9.5).** Every measurement in the table runs at host
1-minute load below 4 (`/proc/loadavg`, waiting when higher), pinned with
`taskset -c 8-23`, heavy runs inside `scripts/with-memory-cap.sh`, headless,
`main` and the branch interleaved, load recorded before and after in
`.claude/handoffs/power-of-two-thresholds/runs.log`. A per-turn bound passes
when the branch's worst dispatch at the new value is within the budget named
in the table (or, where `main` already exceeds that budget at its own value,
no worse than `main`'s worst by more than one reference frame, 8.3 ms, which
is how D7 and D8 applied it). A memory bound passes when the branch's
`memory.peak` is within the stated ceiling, or within `main`'s peak scaled by
the value ratio plus 5 % run spread. A failing measurement reverts that row to
its pre-PT value as a recorded exception, in its own commit with the figures.

**Rationale.** Rounding a presentation cap down trades a visible loss for a
margin the budget did not need: every row above was already inside its bound
at the pre-PT value, and the rounding up adds at most 71 % (1,200 → 2,048;
10,000 → 16,384 is 64 %, 10,000,000 → 16 MiB 68 %) to work that is either
sliced per turn (search, find, sidebar), bounded by an
unchanged byte ceiling (preview, notes, palette notes, Replace Preview), or
measured here. *Alternative rejected:* keep the pre-PT values everywhere as
exceptions. That loses nothing either, but leaves non-powers of two the rule
exists to remove, and skips the measurements that show the margin.

### D13. Structural follow-ups, queued after PT merges

The presentation caps above still cut lists at a count. The structural fix is
to bound by retained bytes and per-turn time, not by count, and to show what
is past the cut. Queued as separate changes, in this order, after PT merges
(maintainer, 2026-11-05):

1. **`bound-lists-by-budget-not-count`**: the palette shows an "N more…" row
   per category that opens a category drill-down mode; the notes browser
   becomes incremental and virtualized, bounded by retained bytes, and states
   "showing X of Y"; minimap markers are binned per pixel row, with no count
   cap; workspace search streams without a match count cap, bounded by
   retained bytes; sidebar directories past 10,000 entries load
   incrementally, without the truncation placeholder.
2. **`stream-replace-all`**: chunked replace with an on-disk undo journal, so
   the per-file limit reaches the open tier with constant memory; skipped
   files are listed with their reason and an "Open and replace in editor"
   action.
3. **`scale-file-index`**: an interned path arena and a byte budget instead
   of the 100,000-file count, targeting at least 1,000,000 files, with a
   Criterion benchmark and a measurement on a real monorepo.
4. **Profile and slice the 59–60 ms largest-file open dispatch** (task 3's
   open-tier runs: the largest dispatch of a 512 MiB open).
5. Then the paused **`avx2-health-scan-kernel`** (spec and spec review done,
   branch `avx2-health-scan-kernel`) and **DL `bound-dense-long-line-editing`**.

The programme record (`docs/next/long-line-documents.md`, PT row and PT
section) carries the same queue.

### D14. Displayed sizes in IEC units (amendment, 2026-11-05)

The PT review found three surfaces that display a size and label it
inconsistently with the tiers: the properties panel's file size and the file
peek divide by 1,000 and print "KB"/"MB" (SI, while every tier and limit the
app names is IEC), and the local-history list divides by 1,024 and prints
"KB"/"MB" (a wrong label). All three print through one IEC display formatter
(`model::size_units`, "KiB", "MiB", "GiB", one decimal), so a 10,485,760-byte
file reads "10 MiB" in the panel, the peek and the history row alike, and
matches the "16 MiB" a tier message names. The accessible text of those rows
follows; `docs/accessibility.md` and the matrix are updated if a spoken size
changes form.

## Risks / Trade-offs

- **Presentation caps raised (D12).** Each raised cap is measured against the
  bound it protects before it lands; a failed measurement reverts that one
  row to its pre-PT value. The residual is the Replace All aggregate-undo
  ordering case (D12), removed by `stream-replace-all`.

- **Visible feature losses at the rounded caps** (D4). Each is listed in the
  proposal's Impact section and in the README. All of them affect only
  documents with paragraphs over 8 k characters, directories over 8 k
  entries, or searches over 8 k matches.
- **Kani cost.** `R_BATCH` and `N_INSPECT` appear concretely in some
  harnesses, and `set_aside_retention_notice_follows_the_bound_and_its_rate_limit`
  (shard `core-journal-and-write`) asserts `SOFT_BOUND_BODIES == 100`. The navigation harness runs at the production width
  (≈ 13 min). Each affected shard reruns under `KANI_MEASURE` one at a time,
  inside `scripts/with-memory-cap.sh`. A wall-time move beyond the shard's
  margin updates `scripts/kani-shards.py` in the same change.
- **Raising undo and local history to 64 MiB** keeps more memory and disk for
  50–64 MiB files. The bounds are 192 MiB live, inside the 256 MiB budget,
  and 256 snapshots.
- **The open tier's memory.** If task 3 fails, the tier stays 500 MB. The
  README then says "500 MB (a recorded exception)".

## Migration

No persisted format changes. Raising the retention caps prunes nothing. The
search-history and recents files load unchanged. The set-aside notice appears
later, at 128 bodies. Since D12, no user loses anything `main` showed:
files of 10 MB–16 MiB gain the minimap and per-edit local history (if
task 9.5's M10 passes; otherwise both stay at 10,000,000 bytes), documents of
50 MB–64 MiB regain undo and local history, and every list, result set and
scan shows at least as much as on `main`.

## Inventory (in scope, non-power-of-two on `main` `fbf0f875`)

The Decision column is the stage-1 decision; D12's table supersedes it for
every presentation cap.

| Constant (module) | Value | Kind | Decision |
|---|---|---|---|
| `S_MINIMAP` (`model/document_scale`) | 10,000,000 | policy tier | 8 MiB (D3) |
| `HISTORY_SAVE_ONLY_BYTES` | 10,000,000 | policy tier | 8 MiB |
| `UNDO_TOTAL_BYTES`, `HISTORY_UNAVAILABLE_BYTES` | 50,000,000 | policy tier | 64 MiB |
| `REFUSE_TO_OPEN` | 500,000,000 | policy tier | 512 MiB or exception (task 3) |
| `retired_byte_tiers::*` (cfg test/kani) | 10/50/500 M | reference | becomes `byte_tier_reference` with the new table |
| `BUFFER_SNAPSHOT_SYNC_BYTE_THRESHOLD` (`ui/buffer_snapshot`) | 10,000,000 | cap | 8 MiB |
| `MAX_REPLACE_FILE_BYTES` (`content_search/replace`) | 10 MiB | cap | 8 MiB |
| `L_SYNTAX` | 2,000 | cap | exception (D9) |
| `attach_threshold::*` except CSS (11, HTML 46 KiB and JSON 24 KiB included), `typing_threshold::JSON` (20 KiB), `S_SYNTAX_OTHER` | 10.5–320 KiB | cap | exception (D9) |
| `attach_threshold::CSS` | 37 KiB | cap | 32 KiB (0 of 2,353 files lose highlighting) |
| `L_MINIMAP` | 22,000 | cap | 16,384 |
| `L_INVISIBLES` | 50,000 | cap | 32,768 |
| `L_SEARCH_HIGHLIGHT` | 10,000 | cap | 8,192 |
| `L_PROTECT` / `N_INSPECT` (`model/protected_fold`) | 500 / 500 | cap (coupled) | 512 or 256 (D7) |
| `L_EDIT_STALL` | 3,500 | measurement record | exact |
| `L_PROTECT_EDIT` | 50,000 | policy (maintainer) | exception (D9) |
| `N_VISIBLE` | 500,000 | cap | 262,144 |
| `V_BATCH` | 56,000 | cap | exception (D9) |
| `R_BATCH`, `R_FOLD` | 224 | coverage window | 256 |
| `N_DENSE` | 250 | derived | `V_BATCH / R_BATCH` = 218 |
| `N_HEAD` | 10,000 | cap | 8,192 |
| `A28_MEASURED_ADVANCE_UNITS`, `A28_SAFE_CHARS`, `A28_MAX_ZOOM_PERCENT`, `R_FOLD_MIN_ZOOM_PERCENT` | 9,011 / 59,579 / 400 / 50 | external fact / derived | exact |
| `HEAD_CAP`, `MAX_SYNTAX_PARAGRAPH_BYTES` (N2) | derived | derived | exact; rustdoc kind checked |
| `V_SLICE` (`model/buffer_replacement`) | 11,360 | slice cap | 16,384, 8,192 or exception (D8) |
| `SYNCHRONOUS_REPLACEMENT_THRESHOLD_BYTES`, `PREVIEW_DIRECT_THRESHOLD_BYTES` | 704 KiB | cap | 512 KiB or exception (D8) |
| `MAX_SEARCH_EVENTS_PER_TICK`, `SEARCH_RETIREMENT_ROWS_PER_SLICE` | 250 | per-turn cap | 256 or 128 (D8) |
| `PROGRESS_DISPOSAL_RETAINED_BYTE_CAPACITY` | 72 MiB | derived sum | exact; rustdoc names its terms |
| `MAX_PALETTE_NOTE_CONSTRUCTION_BYTES` | 96 MiB | derived sum (64 + 16 + 16) | exact |
| `FILE_LOAD_OVERWEIGHT_PROGRESS_CAPACITY` | `REFUSE_TO_OPEN × 3` | derived | follows D3 |
| `MAX_INDEXED_FILES`, `MAX_INDEXED_DIRECTORIES` | 100,000 | cap | exception (D9) |
| the twelve 10,000 caps listed in D4 | 10,000 | cap | 8,192 |
| `BOOKMARK_EXCERPT_SCAN_LINE_LIMIT` | 20,000 | cap | 16,384 |
| `MINIMAP_SEARCH_MATCH_CAP`, `MINIMAP_MODIFIED_LINE_MARK_CAP`, `MINIMAP_LONG_LINE_MARK_CAP` | 2,000 | cap | 1,024 |
| `MINIMAP_WRAPPED_LAYOUT_LINE_CHAR_BUDGET` | 8,000 | cap | 4,096 |
| `MINIMAP_LONG_LINE_WARNING_THRESHOLD` | 120 | external fact | 120 (stage 4: the 120-column convention) |
| `MAX_RESULTS_PER_SOURCE` | 50 | cap | 32 |
| `NOTES_BROWSER_RENDER_LIMIT` | 500 | cap | 256 |
| `MAX_MARKDOWN_EVENTS` | 50,000 | cap | 32,768 or exception (task 4.5) |
| `MAX_INLINE_FOOTNOTE_REPLACEMENTS` | events / 4 | derived | follows |
| `MAX_MARKDOWN_CARRIED_TABLE_CELLS`, `MAX_PREVIEW_TABLE_CELLS` | 1,000 | cap | 512 or exception (task 4.5) |
| `MAX_PREVIEW_IMAGE_SOURCE_PIXELS` | 64,000,000 | cap | exception (D9) |
| `DEFAULT_ANNOUNCEMENT_LIMIT`, `EDITOR_IDENTITY_LIMIT`, `REDUCED_FEATURES_NOTICE_LIMIT` | 240 / 96 / 320 | text cap | 256 / 128 / 256 (task 4.6, stage 4) |
| `ARTIFACT_TEXT_LIMIT`, `WARNING_TEXT_LIMIT` (`cargo-gtk-proof`) | 1,200 | text cap | 1,024 |
| `MAX_RECENT_LOAD_DIAGNOSTICS` | 20 | cap | 16 |
| `PER_DOCUMENT_SNAPSHOT_CAP`, `GLOBAL_SNAPSHOT_CAP` | 48 / 240 | retention tier | 64 / 256 |
| `MAX_RECENTS` (and the derived `MAX_RECENT_LOAD_CANDIDATES`) | 200 (400) | retention tier | 256 (512) |
| `MAX_RECENT_DOCUMENTS_BYTES` | 1 MiB | coverage (must load a full store) | 4 MiB (D6) |
| `MAX_HISTORY` (search history) | 20 | retention tier | 32 |
| `SOFT_BOUND_BODIES` | 100 | policy tier | 128 |
| `MAX_MIGRATION_ATTEMPTS`, `MAX_BOOKMARK_SAVE_ATTEMPTS`, `EDITOR_FOCUS_MAX_ATTEMPTS`, `MAX_UNIQUE_NAME_ATTEMPTS` | 3 / 3 / 6 / 1,000 | retry budget | 4 / 4 / 8 / 1,024 |
| `MAX_CONSTRUCTION_STAGES`, `MAX_FRAME_PHASE_FRAMES` | 48 / 14,400 | coverage window | 64 / 16,384 |
| `MANIFEST_REPAIR_ENTRY_OVERHEAD_BYTES` | 96 | coverage (conservative charge) | 128 |
| `REVEAL_SETTLE_FRAME_BUDGET` | 60 frames | coverage | 64 |

Already compliant and pinned by the gate:

- `S_SYNTAX`, `L_WRAP_EDIT`, `MAX_R_FOLD`;
- the 256 KiB and 64 Ki slice constants;
- `HIGHLIGHT_CHUNK_BYTES`, `MODEL_MARKDOWN_CAP`;
- the reanalysis pacing steps, `PRUNE_FLOOR`;
- the 64 MiB, 128 MiB and 256 MiB budgets;
- `WORKSPACE_SEARCH_FALLBACK_ENTRY_LIMIT`.

Out of scope (D1), still listed in `docs/thresholds.md` with their class: the
remaining non-power-of-two constants (about 100) (geometry, percent, priority, factor,
rate, structural, presentation, time and tool or fixture data).

## Spec review (stage 2, 2026-11-05)

Decisions taken at review, each with its reason; they are folded into the
sections above.

- **Scanner collision fixed in the inventory.** HTML's 46 KiB attach gate was
  missing and JSON's was recorded at 20 KiB; both recounted (7.26 %, 7.33 %),
  both stay exceptions. D10 keys rows by module-qualified name, and the
  self-test fails first on the collision.
- **CSS converts** to 32 KiB: 0 of 2,353 files lose highlighting, so there is
  no measured loss to justify an exception.
- **One materiality rule** (D2) replaces the per-cap judgement; the three
  uncounted paragraph caps become conditional on task 4.5, because the rule
  requires the same evidence for them as for the syntax gates.
- **The minimap marker rationale was wrong** (160 px is the width); the
  conversion stays, because the caps bound GTK-thread work and nothing
  measured them as a user need, but the lost lower markers are now in the
  Impact.
- **Recents must still load**: `MAX_RECENT_DOCUMENTS_BYTES` 1 → 4 MiB, and a
  worst-case load test for every raised retention store.
- **Replace All messages** follow the 8 MiB per-file cap in IEC units.
- **Missing deltas added**: the two local-history requirements that still
  said 10/50 MB, the sidebar 10,000-row scenario, the 200-entry recents cap,
  and the Replace All per-file cap.
- **Kani**: `core-journal-and-write` reruns (it pins `SOFT_BOUND_BODIES`);
  `R_BATCH ≤ MAX_DENSITY_WINDOW` (256) is checked with `R_FOLD ≤ MAX_R_FOLD`.


## Spec review of the amendment (stage 2, 2026-11-05)

Checked against `main` `fbf0f875` and the branch: every constant PR #84
lowered (each lowered value in `git diff fbf0f875 -- crates`) is in D12's
table, its internal list, or a mirror; every scenario under a MODIFIED
requirement keeps its name; `openspec validate --strict` passes. Folded in:

- **Two mirrors** were missing: the benchmark's `NOTE_RENDER_CAP` (500 → 256
  on PR #84) and the test-only `byte_tier_reference` tier; both follow their
  production constants (D12, task 9.4).
- **Eight kind-only presentation caps** the test catches were missing from
  D12's list (`AT_ATTACH_MAX_CHARS`, `MAX_INDEX_REPAIR_SNAPSHOT_BYTES`,
  `MINIMAP_WRAPPED_LAYOUT_FILE_BUDGET`, `MAX_PALETTE_NOTE_SIDECAR_PATH_BYTES`,
  `MAX_NOTE_FUZZY_SCORE_BYTES`, the two IME surrounding-text caps,
  `DATA_DETAILS_MAX_ROWS_PER_GROUP`); `out: presentation` is narrowed to fixed
  layout shapes, and task 9.1 rechecks those rows and the policy tiers too.
- **The rationale's growth bound** said "at most 64 %"; the largest step is
  1,200 → 2,048 (71 %), and the 16 MiB tiers are 68 % above 10 MB.
- **The rule** in `.agents/rules/rust.md` now names where the former value is
  recorded (`up from <value>`), so `make check-thresholds` can enforce it.
- Not changed, recorded: `SOFT_BOUND_BODIES` (100 → 128) delays the set-aside
  review notice by up to 28 bodies; it is a policy tier the maintainer set in
  D6 and hides no body, so it stays. `report_startup_diagnostics::DETAIL_LIMIT`
  only bounds log lines and stays `out: presentation`.

## Implementation decisions (stage 3, 2026-11-05)

Each decision follows the rule it names; the figures are in the programme
record's PT implementation record and in
`.claude/handoffs/power-of-two-thresholds/`.

- **The three paragraph caps stay (task 4.5).** On 772,392 corpus text files
  (`~/Workspace` without `.git`, `.claude` worktree copies and `target` build
  output, the crates.io registry, `/usr/include`), rounding down loses the
  feature for 5,556 of 765,618 files (`L_MINIMAP`, 0.73 %), 650 of 767,315
  (`L_INVISIBLES`, 0.085 %) and 2,366 of 757,002 (`L_SEARCH_HIGHLIGHT`,
  0.31 %): each above D2's floor, so each is a recorded exception and nothing
  turns off sooner.
- **The Markdown event and table-cell caps stay (task 4.5).** Of 404,113
  Markdown files of at most 4 MiB with at most 50,000 events, 700 have more
  than 32,768 (0.17 %); of 404,439 with a largest table of at most 1,000
  cells, 1,256 have more than 512 (0.31 %).
- **The announcement cap is 256, not 128 (task 4.6).** The longest
  reduced-features notice the policy can produce outside Protected View is
  229 characters (a unit test enumerates every tier edge and pins it), and
  every status message is announced through `DEFAULT_ANNOUNCEMENT_LIMIT`, so
  128 would cut a shipped notice. 256 is the power of two at or above it; the
  notice cap is 256 (from 320) and the editor identity, first set to 64,
  is 128 after the stage-4 review (below).
- **The inventory key carries `cfg` variants.** `MAX_PROFILED_OFFSETS` and
  `MAX_DENSITY_WINDOW` have a production value and a Kani bound; the gate
  keys them `NAME #[cfg(...)]`, so both are rows.
- **`model::size_units::iec_size`** is the one tier formatter: a tier prints
  whole ("64 MiB"), any other size with one decimal, rounded down, so a size
  just above a tier never prints as the tier.

## Review decisions (stage 4, 2026-11-05)

An independent review of `origin/main..HEAD` (code, tests and the handoff
evidence, not only the stage-3 report). Each decision follows user
experience first.

- **`EDITOR_IDENTITY_LIMIT` is 128, not 64.** It bounds a user's file name
  in the editor's accessible name ("Editor for …"), so the shipped-string
  check of task 4.6 cannot decide it: no shipped string is involved. D2's
  corpus rule does. Of 1,511,259 file names under `~/Workspace` (without
  `.git`, `.claude`, `target`, `node_modules`, `.cache`), 7.96 % exceed 64
  characters, 0.387 % exceed 96 and 0.0020 % exceed 128. 64 would have cut
  the name a screen reader hears for one file in thirteen (for example
  `BUG-0120-fluxo-exclusao-evento-…md`); 128 is the power of two above the
  old 96 and cuts almost none. The label stays far inside
  `DEFAULT_ANNOUNCEMENT_LIMIT` (256). Failing first:
  `ui::editor_page::accessibility::tests::the_editor_identity_keeps_a_long_file_name_whole`.
- **`MINIMAP_LONG_LINE_WARNING_THRESHOLD` stays 120, as an external fact.**
  It bounds no work: it is the 120-column style limit the minimap proposal
  named, which users write code to. At 128 the opt-in long-line markers
  stopped marking 121–128-character lines, which break that limit. It is
  outside D1's scope in substance (a convention, like a presentation shape)
  and is inventoried as an external fact. Failing first:
  `long_line_markers_follow_the_120_column_convention`; the two widget
  fixtures return to 121 characters, the edge they tested on `main`.
- **Overruled by the maintainer (2026-11-05, D12):** the following. Kept as
  designed at stage 4, recorded for the maintainer: the Replace All per-file
  cap at 8 MiB (files of 8–10 MiB are skipped with a message naming the
  limit; the cap bounds worker memory and the undo payload, and D3 aligned it
  with the 8 MiB tier), palette categories 32, the notes browser 256 and the
  minimap marker caps 1,024 (each bounds GTK-thread work, each shows its
  truncation).
- **Evidence read, not re-measured.** The verification lane's 64 MiB
  `total-size` "worst" of 10.1 s on the branch is `lower_bound_only` (the
  background phase hit its cap; the largest late tick in that run is 8.1 ms),
  as the programme record says. The three-repetition `.html` keystroke rerun
  on the branch ran at load 5.1–5.3 and is not a verdict; the one-repetition
  verification pair (load 0.8–2.3) is what shows those cases matching `main`.
  Neither stage-4 change touches a timed path, so no measurement was rerun.

