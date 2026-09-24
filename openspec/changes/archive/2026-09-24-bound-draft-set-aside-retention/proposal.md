## Why

`drafts/set-aside/` is where the journal puts every draft body that left it
without being applied. That covers stale file-backed drafts, unrestored bodies,
unattributable crash leftovers, and bodies overwritten by a re-registration.
`services/draft_service/set_aside.rs` owns those bytes, and by design nothing
but the user's confirmed Delete on `Preferences > Data` removes one. The area
therefore has **no size bound** (programme deferral inventory, and N10 in
`docs/next/formal-verification-next.md`).

It grows with every stale draft, and it grows silently. The only control is a
per-row Delete on a page the user has no reason to visit. The listing also
degrades as it grows: `set_aside::list` stops after
`MAX_LISTED_SET_ASIDE_BODIES` (256) directory entries **before** sorting, so
past 256 bodies the "newest first" group shows an arbitrary subset and does
not say that others exist.

Step 8a of the formal-verification programme bounds this growth. It must not
break the phase-0 goal, "no code path deletes unsaved user content without a
user decision".

## What Changes

- **Fix first: a newer body is never skipped as "already kept" (E1).** The
  Quint vs TLA+ evaluation found that `set_aside::keep_copy` treated an
  existing `{id}.{stamp}.draft` as proof the body was kept. After a crash
  between a body write and its manifest commit, the body on disk is newer than
  its entry's stamp; a second unapplied restore then reported it set aside
  without copying it and released the restore hold, so autosave could replace
  the only copy. A name now counts as kept only when it holds a
  byte-identical copy; a different body takes the next free name, and neither
  is overwritten. The decision is a pure `journal_core` function with a Kani
  harness. This lives in the same module, so the pre-existing-blockers rule
  puts it in this change.
- **Retention policy: surface, never auto-delete.** A soft bound on the
  set-aside area, 100 bodies or 256 MiB, is evaluated off GTK once per process
  after startup restore settles, and again after any set-aside placement.
  Crossing it publishes a window status notification pointing to a new
  `app.review-preserved-drafts` action. The action opens `Preferences > Data`
  at the Preserved Drafts group. Crossing the bound **never deletes anything**.
  The notice is rate-limited in memory: at most one per process at startup,
  again after a placement only on material growth, and not at all once the
  user has opened the review in that process. No acknowledgment is persisted.
- **Review surface.** The Preserved Drafts group gains:
  - a summary row with count, total size, the soft-bound state, and a
    truncation note when not every body is listed;
  - one bulk action, "Delete All Preserved Drafts…", behind a destructive
    confirmation that states the exact count and total size. It deletes only
    the set the dialog showed: a body added or changed after the dialog opened
    is kept.
  - Per-row Open and Delete are unchanged.
- **No opened-state tracking.** No new persisted file is introduced
  (maintainer decision, 2026-09-23). Which bodies the user has opened is not
  recorded, so there is no per-row opened state and no opened-only bulk action.
- **A pure, Kani-checked retention core.** A new
  `services/draft_service/set_aside_retention.rs` (GTK-free, I/O-free)
  decides the bound status, whether a notice is due, and the bulk-deletion
  plan. A Kani harness and a proptest prove the plan's safety properties: with
  no user decision the plan is empty, and the plan never reaches beyond the
  bodies, as fingerprinted, that the user confirmed.
- **Listing fix (pre-existing defect).** Listing scans the whole area, within a
  bounded scan budget, before sorting. It reports the true total, or a
  lower bound, alongside at most 256 rows, so the newest bodies are the ones
  shown and truncation is visible.
- `set_aside.rs`'s ownership rule is restated: bodies still leave only by a
  user's confirmed Delete, now per row or in bulk. Its module doc is updated to
  say so.

## Capabilities

### New Capabilities

<!-- none -->

### Modified Capabilities

- `draft-session-recovery`:
  - MODIFIED "Preserved set-aside drafts have a recovery surface": it gains
    the summary, the confirmed Delete All bulk action, and truthful listing.
  - ADDED "Set-aside retention never deletes a draft without a user decision".
  - ADDED "Set-aside growth past a soft bound is surfaced for review".
  - ADDED "A set-aside copy counts as kept only when it holds the same bytes".

## Impact

- **Code:**
  - `crates/lushtext-core/src/services/draft_service/set_aside.rs` (listing
    totals, fingerprinted delete);
  - new `services/draft_service/set_aside_retention.rs` with its
    `#[cfg(kani)]` harness;
  - `services/draft_service.rs` (`list_set_aside_drafts`);
  - `ui/preferences/data_page.rs` (summary row, Delete All bulk action);
  - `app.rs` (`app.review-preserved-drafts`);
  - `services/action_catalog/` (catalog entry);
  - the window notification path that publishes the notice;
  - `scripts/kani-shards.py`.
- **Persisted data:** none. No new file, no new GSettings key, and no existing
  format changes; the format inventory is untouched.
- **Docs:**
  - `docs/accessibility.md`, `docs/accessibility-matrix.md`
    (`A11Y-PREFERENCES-DATA-SET-ASIDE` plus a notice row), and
    `docs/accessibility-orca-checklist.md`;
  - `docs/automation.md` and `docs/automation-reference.md` (new action);
  - `docs/next/formal-verification.md` (deferral inventory and a step 8a
    record), `docs/next/formal-verification-next.md` (N10);
  - README features and AGENTS.md;
  - `docs/end-user-coverage.md` if lane expectations change.
- **No** new dependency. The thresholds are constants in the pure core.
