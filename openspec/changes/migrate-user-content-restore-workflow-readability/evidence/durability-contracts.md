# Durability contracts as implemented today (task 0.5)

Written **before** any code change so the after-section (task 7.3) can be
diffed against a recorded baseline rather than a remembered one. These are
behavior this change must preserve exactly.

## 1. Orphan-body cleanup identity

Contract from `.agents/rules/rust.md`: inspection records the candidate inode;
execution reloads the latest trusted manifest, acquires the same stable
`TargetWriteGuard` used by atomic replacement, **then rechecks inode before
deleting**. Manifest serialization alone is insufficient, because an autosave may
finish replacing the body before it acquires the manifest lock.

### Current call order, verbatim

**Inspection** — `services/draft_service.rs`
`inspect_orphan_cleanup_from()` (line 1549), body-candidate arm around line 1722:

```
let inode = match fs_metadata::inode(&entry.path) { Ok(inode) => inode, ... };
...
DraftOrphanCleanupCandidate { ..., inode, ... }
```

The recorded inode is carried on the candidate value
(`services/draft_service/cleanup_types.rs:57`, field `inode: u64`).

**Execution** — `execute_orphan_cleanup_impl()` (line 1833), in this order:

1. `let _guard = manifest_write_lock().lock()` — the mutation-serialization gate.
2. `let mut latest = load_trusted_manifest_for_cleanup(data_dir)` — **reload the
   latest trusted manifest**; a load failure retains the whole unexecuted plan
   as `StatusUncertain`, pushes the failure, and sets `has_more_work = true`.
3. Build `latest_by_id`, where `Some(entry)` means the ID is unique and `None`
   marks ambiguous duplicates that destructive cleanup must preserve.
4. Per candidate:
   a. `candidate.path != expected_path` → retain `CandidatePathMismatch`, and
      `has_more_work = true`.
   b. `latest_by_id.contains_key(draft_id)` → retain `ManifestEntryPresent`.
   c. `let _body_guard = fs_write::TargetWriteGuard::acquire(&candidate.path)`
      — **the same stable guard atomic draft replacement uses**. Acquire failure
      retains `StatusUncertain`, records the failure, sets `has_more_work`.
   d. `fs_metadata::path_status(&candidate.path)`:
      - `Missing` → `already_absent_files`.
      - `File` **and** `fs_metadata::inode(&candidate.path).ok() !=
        Some(candidate.inode)` → retain `BodyGenerationChanged`. **This is the
        inode recheck, and it happens after the guard, not before.**
      - `File` with a matching inode → `fs_mutate::remove_file_if_exists`.

The ordering invariant: guard **before** recheck, recheck **before** delete,
manifest reload **before** both, and the manifest write lock held across the
whole pass. The path-mismatch and duplicate-ID arms are additional refusals
beyond the rule's minimum.

**Cross-operation return** — `outcome.latest_persisted_manifest` is taken on the
worker (`ui/window/drafts.rs`, `run_orphan_cleanup_pass`) before crossing back to
GTK, and only `committed_manifest_removals` fingerprints reach the main thread,
where `draft_service::merge_committed_orphan_removals` merges **exact
generations** into live state rather than replacing it — so an autosave accepted
while the worker ran survives.

## 2. Paragraph-boundary bounded installation

Owner: `model/buffer_replacement.rs::next_replacement_boundary` (line 72).
Contract, verbatim from its own doc comment and enforced by its tests:

- every slice ends **just after a newline**, so paragraphs installed by earlier
  turns are never re-laid-out;
- a single paragraph longer than `REPLACEMENT_INSERT_SLICE_BYTES` (256 KiB) is
  installed in **one turn**, because GTK cannot lay it out incrementally anyway;
- the budget edge is walked back to a UTF-8 char boundary before the newline
  search, so a multibyte char straddling the budget cannot panic either side.

Clear-side counterpart: `delete_one_slice()` in
`ui/editor_page/buffer_replacement.rs` (line 426) extends the deletion end to
the next line start when `!end.is_end() && !end.starts_line()`.

Consumers of the boundary arithmetic, all of which must keep **calling** it:

| Consumer | Call form |
| --- | --- |
| `ui/editor_page/buffer_replacement.rs::run_insert_slice` | `next_replacement_boundary` directly |
| `ui/window/local_history.rs::run_preview_install_slice` | `next_replacement_boundary` directly |
| `ui/editor_page/load/execution.rs` | via the 1-line alias `model::file_load::next_install_boundary` |
| `benches/benchmarks.rs`, `tests/properties/file_load.rs` | via the alias |

It must not be re-derived, duplicated, or "simplified". See task 2.1 for the
alias decision.

## 3. Durable draft, session, and sidecar write ordering

All three records write through `services/filesystem::write`, which provides the
ordered Linux contract (probe metadata, create temp with safe permissions, write
and flush, apply required metadata, `sync_all()` on the temp **after** those
metadata mutations, `rename()`, then sync the parent directory).

Failure classification honesty, which this change must not flatten:

- `DurableWriteError::BeforeRename` — previous bytes intact; report as an
  unwritten/failed operation and keep the record retryable.
- `DurableWriteError::AfterRename` — new bytes are on disk but the directory
  `fsync` did not complete; surface a distinct durability-unconfirmed warning,
  never a generic lost-write.

Draft-specific ordering already in place and preserved:

- `drive_pending_draft_mutations()` keeps the **persisted manifest as the durable
  retry marker until the body is gone**: a failed body deletion leaves a fully
  recoverable pre-delete state across unrelated manifest mutations and process
  restart. The manifest removal runs only when `body_error.is_none()`.
- `accept_draft_manifest_commit()` re-applies compact pending tombstones over the
  committed manifest under `mutation_order.is_current`, so a delete accepted
  while a manifest worker ran is not resurrected.
- `reject_draft_manifest_authority()` revokes destructive cleanup immediately
  when a manifest command loses completeness or durable-replacement eligibility:
  it clears `orphan_cleanup_pending_offset`, clears `orphan_cleanup_timer_pending`
  and invalidates the timer.
- `commit_close_draft_pipeline()` sets `autosave_pending = false` deliberately so
  an edit-coalesced regular tick cannot clear close retry state before the close
  caller observes success or failure.

## After: comparison against the recorded baseline (task 7.3)

Every contract above is preserved, and the strongest available evidence for each
is that the code implementing it **was not edited**.

### 1. Orphan-body cleanup identity — unchanged, by non-edit

`git diff --stat -- crates/lushtext-core/src/services/` is **empty**. The entire
inspect/execute path — the recorded inode on the candidate, the manifest write
lock, the trusted-manifest reload, the path-mismatch and duplicate-ID refusals,
the `TargetWriteGuard::acquire`, the inode recheck **after** the guard, and the
delete — is byte-identical. So is `merge_committed_orphan_removals` and the
`latest_persisted_manifest.take()` that keeps the full manifest off the GTK
thread.

### 2. Paragraph-boundary bounded installation — unchanged on both sides

- `model/buffer_replacement.rs` is **not in the diff at all**, so
  `next_replacement_boundary`, `next_clear_char_count`,
  `BufferReplacementPlan::for_sizes`, and the three constants are byte-identical,
  as are their five co-located unit tests. The
  `lushtext-core::properties file_load::install_boundaries_end_after_a_newline_or_consume_one_whole_paragraph`
  and `..._reconstruct_exact_unicode` property tests pass.
- The clear-side counterpart moved file but **not a character**. Diffing
  `delete_one_slice` between `HEAD:ui/editor_page/buffer_replacement.rs` and
  `ui/editor_page/buffer_replacement/execution.rs` reports **IDENTICAL**,
  including the `!end.is_end() && !end.starts_line()` line-start extension and
  its comment.
- Every call site still calls the one owner. The `next_install_boundary` synonym
  gained only a doc comment.

### 3. Durable write ordering — unchanged, by non-edit

No file under `services/filesystem`, `services/durable_write.rs`, or
`services/editor_io.rs` appears in `git diff --name-only`. The
`BeforeRename`/`AfterRename` classification and its mapping to
`EditorSaveError::{WriteTemp, DurabilityUnconfirmed}` are untouched, as are the
three draft-specific orderings recorded above: the persisted manifest as the
durable retry marker until the body is gone, the tombstone re-application in
`accept_draft_manifest_commit`, and the deliberate `autosave_pending = false` in
the close commit.

### The one deliberate behavior change, and its exact extent

The behavior change cannot be shown as a diff of `ui/window/drafts.rs`, because
this change **deletes** that file and replaces it with the `ui/window/drafts/`
role directory. A whole-file diff would show every line as moved. What follows is
the real extent, expressed as the guard's live call sites:

```
$ grep -rn has_incomplete_load_installation crates/lushtext-core/src/
ui/editor_page/load/mod.rs:268                # the accessor WFR-DOCUMENT-LOAD exposes
ui/window/drafts/journal.rs:156               # synchronous close-time guard
ui/window/drafts/autosave_execution.rs:179    # shared admission collector, both passes
ui/window/drafts/autosave_execution.rs:246    # post-snapshot recheck, close flush
ui/window/drafts/autosave_execution.rs:497    # post-snapshot recheck, autosave
```

(The doc-comment mention in `ui/window/drafts/mod.rs`'s shared-state table is
elided; it names the flag rather than reading it.)

**Five decision points, four evaluation sites, and not one `||` clause.** The
autosave and close admission passes are two decision points reached through two
named entry points, but they share one collector, so the flag is read there once.
Every one of the four sites passes it as a named argument to one of the two pure
predicates (`policy::draft_candidate_is_eligible` and
`policy::captured_snapshot_is_current`), which is why the guard is
mutation-covered: the decision itself lives in `policy.rs`, and each site is a
call rather than a copy of the logic. The synchronous close flush was the one
exception — it restated the terms as a `||` chain outside the mutation scope — and
it now calls the predicate too, so a term added to the policy reaches the worst
place to miss it automatically.

The behavioral extent is unchanged from what was claimed. The guard's effect is
strictly *more* refusal — it can only cause a candidate to be skipped, never to be
admitted — so it cannot weaken any ordering above. Its regression test
(`test_incomplete_load_installation_blocks_draft_autosave_over_a_good_draft`) is
proven to fail without it.

### Two behavior-preservation slips made during the migration, both caught

Recorded because the pass exists to catch exactly these, and both were found by
re-reading the diff rather than by a failing test:

1. **A metrics-ordering difference.** The first cut recorded an insertion turn's
   metrics even when the session was already `terminal`, where the original
   returned first. The original ordering is restored: a terminal session's metrics
   were already copied out and reported, so a later turn must not append to them.
2. **A latent `BorrowMutError`.** `match start_disposition(cell.borrow().is_some())`
   holds the shared `Ref` for the whole match, so the arm that calls `begin` —
   which takes `borrow_mut()` on the same cell — would panic. The pre-migration
   `if` condition dropped its temporary before the block. Fixed by binding the
   boolean to a local, with the reason recorded at the site.

Neither reached a verification lane, and the second is a generalisable hazard of
this convention's mechanical work; both are recorded in
`evidence/mutation-buffer-replacement.md` and in the programme record's slot-4
friction section.

### Data-safety pass over the actual diff

No new confirmed finding. The diff contains no new durable write, no new
deletion, no new worker handoff, and no new freshness predicate on a durable
path. The one seam it adds — `has_incomplete_load_installation` — is a read-only
cheap accessor over a single `Cell<bool>` on a migrated workflow's facade, and it
is consumed only inside refusal guards.
