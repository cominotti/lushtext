## Why

`.agents/rules/rust.md` ("Thresholds are powers of two, rounded the
conservative way") has governed every threshold written since 2026-11-01, but
the constants that predate it were left for this change (row PT of
`docs/next/long-line-documents.md`). The inventory in the design (scanned on
`main` `fbf0f875`) finds 652 integer constants in the workspace's production
crates, 198 of them not a power of two. About 95 of those are thresholds in the
rule's sense (they bound work, memory or a document feature by bytes,
characters, paragraphs, rows or counts); the rest are geometry, priorities,
percentages, factors, external facts and test data, which the rule does not
cover.

The mix costs three things today:

- **Dishonest margins.** `L_MINIMAP` 22,000 is "a lower bound of the boundary"
  measured under load. `R_BATCH` 224 is a coverage window one row above the
  223 it must cover. `N_HEAD` 10,000 is a measured edge with no margin. A power
  of two rounded the conservative way says how precise the measurement was.
- **Mixed units in what users read.** The size tiers are decimal (10 MB, 50 MB,
  500 MB), while the rest of the app's limits and `glib::format_size` use IEC
  units. Several messages print "MB" for a value divided by MiB.
- **No gate.** Nothing stops a new threshold from skipping the rule. C6b, C6c
  and CF complied (`HIGHLIGHT_CHUNK_BYTES`, `MODEL_MARKDOWN_CAP`, the
  reanalysis step sizes, the font-zoom prune floor) only because their reviews
  checked by hand.

## What Changes

- **User-visible size tiers in IEC units** (design D3):
  - minimap and save-only local history: 10 MB → **8 MiB**;
  - undo off and local history unavailable: 50 MB → **64 MiB**;
  - refuse to open: 500 MB → **512 MiB**. This tier changes only if the
    measured exclusive-load memory peak at 512 MiB scales with the size
    (task 3). Otherwise it stays 500 MB as a recorded exception.

  The messages and the accessible text that name a tier print it in IEC
  units.
- **Caps rounded down, coverage windows rounded up** (D4, D5):
  - `L_MINIMAP` 22,000 → 16,384;
  - `L_INVISIBLES` 50,000 → 32,768;
  - `L_SEARCH_HIGHLIGHT` 10,000 → 8,192;
  - `N_VISIBLE` 500,000 → 262,144;
  - `N_HEAD` 10,000 → 8,192;
  - every 10,000-entry scan, result and directory cap → 8,192;
  - the minimap marker caps 2,000 → 1,024;
  - `R_BATCH` and `R_FOLD` 224 → 256, with the derived `N_DENSE` 250 → 218;
  - span-record, frame-record and per-entry charge windows rounded up.

  Retention tiers and retry budgets round **up** (D6) so that no user's stored
  history, recents or search history is pruned on upgrade:
  - local-history snapshots 48 / 240 → 64 / 256;
  - recents 200 → 256;
  - search history 20 → 32;
  - set-aside soft bound 100 → 128 bodies.
- **The coupled pair** `L_PROTECT` and `N_INSPECT` (500, tied by the
  `core-protected-view` Kani bound) both become **512** if the lane's new
  `dense/512` case keeps the stall basis that 500 was measured on. Otherwise
  both become **256** (D7).
- **Re-measured slice sizes** (D8): `V_SLICE` (11,360 → 8,192 or 16,384), the
  direct-install threshold (704 KiB → 512 KiB, or a recorded exception if the
  total-time cost is visible), and the 250-per-turn search event and
  retirement caps (256 if one turn fits its budget, else 128). These are
  interleaved with `main` on the lane.
- **Recorded exceptions with their measurements** (D9). Each keeps its exact
  value, with the reason and the evidence beside the constant:
  - `L_SYNTAX` 2,000: rounding down unhighlights 3–22 % of real Markdown
    files, and rounding up breaks the 8 KiB highlight-chunk bound;
  - the per-language syntax gates and `S_SYNTAX_OTHER`: rounding down
    unhighlights 0.1–6.5 % of real files;
  - `L_PROTECT_EDIT` 50,000: the maintainer's "editable up to ~50k";
  - `V_BATCH` 56,000: rounding down folds more text;
  - the 100,000-file palette index: a real 87,624-file workspace;
  - the 64-megapixel preview image cap: common phone sensors.
- **N2 closed and every kind named.** Every in-scope constant's rustdoc names
  its kind (cap, coverage window, policy tier, external fact, derived, or
  exception) and its rounding. `MAX_SYNTAX_PARAGRAPH_BYTES` already says
  "external bound, derived"; the check pins it.
- **A gate** (D10): `make check-thresholds`, in `check-policy`, classifies
  every integer `const`/`static` in the production crates against a
  committed inventory (`docs/thresholds.md`). An in-scope constant must be a
  power of two or a listed exception, derived value or external fact with its
  reason. A new constant fails the check until it is classified.
- **Kani**: the byte-tier parity harness against the retired `FileSizeCheck`
  becomes a harness against the new tier table. Every shard whose harnesses
  read a changed constant reruns under `KANI_MEASURE`, one CBMC at a time
  (task 6).

## Capabilities

### New Capabilities

- `threshold-constants`: the inventory, the classification and rounding rules,
  recorded exceptions, and the `check-thresholds` gate.

### Modified Capabilities

- `long-line-document-presentation`: tier values and IEC units; the byte-tier
  Kani agreement is now with the new table; the `N_inspect` bound value.
- `local-history`: the 8 MiB and 64 MiB tiers, and the 64 / 256 retention.
- `command-palette-source-groups`: the note-source entry cap is 8,192.
- `search-replace-safety`: the Replace Preview row cap and its coverage
  fixture are 8,192.
- `search-history-and-saved-searches`: recent history keeps 32 entries.
- `main-thread-responsiveness`: the per-turn search event and retirement caps.
- `performance-regression-coverage`: the evidence that pins those caps.

## Impact

- **Code**:
  - `model/document_scale.rs`, `model/protected_fold.rs`,
    `model/buffer_replacement.rs`, `model/paragraph_boundary.rs` and their
    `kani_proofs.rs`;
  - `ui/editor_page/protected_view/policy.rs` and the minimap policy;
  - the search panel, palette, notes, recent-documents, local-history,
    draft-service and format-upgrade services;
  - accessibility text caps, the `cargo-gtk-proof` text caps, and the
    messages that name tiers;
  - a new `scripts/check-thresholds.py`.
- **Behaviour users notice**:
  - files of 8–10 MB lose the minimap and per-edit local history;
  - files between 50 MB and 64 MiB keep undo and local history;
  - files of 500 MB–512 MiB open, if task 3 passes;
  - a directory of 8,192–10,000 entries shows its truncation row;
  - workspace search stops at 8,192 matches;
  - Protected View heads show 8,192 characters, and dense heads 218;
  - the minimap, invisibles and search highlighting turn off at the rounded
    paragraph lengths.
- **Data**: no persisted format changes. Retention caps only grow, so nothing
  stored is pruned on upgrade.
- **Docs**:
  - README (features, local history, Markdown ceilings, set-aside bound);
  - AGENTS.md (the design-decision figures);
  - `.agents/rules/rust.md` (the conversion is done; the gate exists) and
    `build.md` (the new check);
  - the programme record's PT row and an implementation record;
  - the accessibility documents, if a notice or description changes.
  - AppStream: the historical release notes stay as written; the next
    release's notes name the new tiers.
