## Context

The formal-verification programme (`docs/next/formal-verification.md`) started
by extracting three subsystems as explicit state machines:

- the crash-durable write (`services/durable_write.rs`);
- the draft journal (`services/draft_service.rs` plus `ui/window/drafts/`);
- the `ViewportSliceBin` feedback loop
  (`crates/gtk-lush/widgets/src/viewport_slice_bin/`).

The extraction found six defects before any model or proof existed. Phase 0
closes them. Later phases model the resulting system, and a model of known-wrong
behaviour would only prove the wrong theorem.

The facts this design depends on, established during the reading (paths
relative to `crates/lushtext-core/src/`):

- Autosave writes every candidate body first, then calls
  `draft_service::update_manifest` once with all accepted entries
  (`ui/window/drafts/autosave_execution.rs:593-621`).
- `update_manifest` always reconciles a full inventory. It returns `Err`
  whenever the inventory is not `Complete` (`services/draft_service.rs:420`).
- An unregistered body is `ambiguous`, which makes the inventory `Partial`,
  unless its id is an untitled id or a session-untitled id
  (`services/draft_service.rs:1063-1066`). A stable-path-hash body from a crash
  inside the window therefore fails every later update.
- Stale file-backed drafts are deleted at startup (`draft_service.rs:1381-1417`
  and `ui/window/drafts/restore_execution.rs:175-177`). The spec
  `draft-restore-validation` currently requires this.
- In `ViewportSliceBin::size_allocate`, when `outer_scroll_request` returns
  `None`, the `else` branch writes `published` back into the child adjustment
  (`viewport_slice_bin/imp.rs:290-297`). A `None` produced by the
  `reconfigure_shift` guard (`scroll_request.rs:117-122`) is therefore erased,
  not deferred.
- `rename_durable` returns `io::Result<()>`. The local-history migration does
  `.or_else(|_| copy_file_durable(..))` (`local_history_service.rs:1275`).
- Temp names are `.{file}.{tag}.{pid}.{seq}.tmp` (`durable_write.rs:43-54`).
  Nothing ever removes them after a crash.

## Goals / Non-Goals

**Goals:**

- Every defect gets a test that fails before its fix.
- After this change, the draft invariant "every body on disk has a persisted
  entry or an untitled id" holds at every crash point, by construction.
- Already wedged installations recover without user action.
- No code path deletes unsaved user content without a user decision.

**Non-Goals:**

- Writing any formal model, Kani harness, or Lean proof. Those are phases 1–5.
- Redesigning the autosave pipeline's single-commit batching beyond what
  registration needs.
- Multi-process or NFS guarantees. These are recorded as programme axioms, not
  fixed here.

## Decisions

### D1. Write-ahead registration, not per-body commits or self-describing bodies

Before the first body write of any id absent from the persisted manifest, the
pipeline commits an entry for that id, using the same `update_manifest` path.
The entry carries the original path and the backing mtime captured at snapshot.
Registration is batched: one manifest update per pass registers every new id in
it. Ids that are already registered skip the step, so steady-state autosave cost
does not change.

Alternatives considered:

- **Commit after each body.** This multiplies full reconciliations by N on every
  pass and still leaves a window.
- **Self-describing bodies** (a path header inside the body). This changes a
  persisted plain-UTF-8 format for all drafts, and the bodies are specified as
  plain text.

Write-ahead makes the bad state unreachable and reuses the missing-body path
that already exists, and that path is safe: restore skips such an entry, and
cleanup removes it only on a fingerprint match.

Interaction with cleanup: orphan cleanup already refuses to run while a draft
mutation is in flight (GTK-side exclusion). A registered-but-bodiless entry is
therefore never retired between registration and the body write of the same
pass. A test pins this.

### D2. Recovering already wedged installations: session-hash attribution, then set-aside

Reconciliation gains one attribution step. For an unregistered
stable-path-hash body, it looks for a file-backed session tab whose
`stable_path_hash(path)` equals the id. If it finds one, it reconstructs the
entry from that tab, with `original_mtime_secs` unknown.

A reconstructed entry with an unknown mtime cannot pass freshness validation.
It is therefore preserved as a local-history snapshot (D3) rather than restored over the file.
That is the conservative choice: the edits are offered, and they never silently
replace newer on-disk content.

A body that nothing attributes is moved (rename within the data directory) into
`drafts/set-aside/`, with a bounded diagnostic. This moved body, and every body
D3 sets aside, is excluded from the recoverable inventory and from orphan
cleanup. The inventory can then reach `Complete` again.

This deliberately refines "ambiguous bodies are preserved": they are still
preserved, but out of the way, so they stop blocking the journal.

### D3. Set-aside destination: local history as a `Periodic` snapshot, set-aside directory as fallback

The question is what a stale draft *becomes* once the backing file changed
externally. While the file is unchanged, the draft is the current session's
state, and the draft journal owns it. After an external change it is an
alternative version of that file. The subsystem that already owns "versions of
this file that are not its current on-disk content" is local history. It keys
lineages by the same canonical path, previews each version beside the current
text, restores reversibly (`Undo Restore`), bounds retention, follows in-app
renames, and applies large-file policy. Reusing it inherits every one of those
guarantees, and each is already tested and specified. A new destination would
have to rebuild each of them, and each would add state to the formal models of
later phases.

The stale body is therefore captured as a local-history snapshot of its backing
file, with the **existing `LocalHistorySnapshotOrigin::Periodic`** origin
("While editing"). Its timestamp is the draft's `saved_at_secs`. The label is
literally true: `Periodic` means the buffer's content captured while it was
being edited, and that is exactly what a draft body is. **No persisted-format
change is needed.** There is no new enum variant, no index version bump, and no
format-upgrade step.

What sets this snapshot apart is the inline alert, not a new label. The alert
reads, in substance, "Unsaved edits from <time> were kept in Local History
because the file changed on disk", and it carries a **Show in Local History**
action through the existing `LushtextInfoBar` action connectors. That action
opens the local-history browser for the file.

Destination by case:

| Case | Destination |
|---|---|
| Stale draft of a saved file within local-history policy (≤ 50 MB) | local history, `Periodic` |
| Local history disabled for the file (> 50 MB) or its capture fails | `drafts/set-aside/`, and the alert names that location |
| D2 body attributed through the session (path known, mtime unknown) | local history, `Periodic`, because the path is known |
| D2 body attributed to nothing | `drafts/set-aside/`, because without a path there is no lineage |

The body leaves the drafts directory only after its preservation is durable.
If preservation fails, the draft body and its entry stay where they are, and
the draft is not restored over the changed file.

Consequence: the mtime comparison has second granularity, and a `git
checkout` or a `touch` is enough to trip it. Today that false positive is
destructive. With this design it costs one click, so the heuristic can stay
coarse.

Accepted trade-off: the preserved snapshot is subject to local-history
retention, the same promise local history makes for every version. Exempting
these snapshots from pruning would require marking them in the index, which
brings the format change back. That is deferred to the programme record.

### D4. Swallowed request: prove first, then defer instead of erasing

A widget probe comes before any fix. In one allocation it must trigger both a
child `scroll_to` whose target falls within `reconfigure_shift` of the
published offset, and an upper or page reconfiguration. Two ways to do that:

- a model change that alters the content height in the same frame as a
  `scroll_to`;
- a variable-row-height fixture in the adoption lab.

The probe asserts that the row ends inside the outer viewport.

Fix direction, if the probe confirms the bug: when the shift guard suppresses a
divergence, skip the write-back for that allocation and queue one
re-allocation. With stable geometry (shift = 0), the next allocation classifies
the persisting divergence with the ordinary rule.

The risk is re-opening 129a7e61, where a settle forwarded as a request made the
view creep. The settles 129a7e61 measured occurred when the published
upper/page were rewritten by the child. Since ecf2c791, ordinary allocations
publish in the child's content-box frame and provoke no rewrite, so a settle
should not survive into a stable-geometry frame. The existing rendered-bounds
stillness tests and the allocation/correction-count tests are the regression
guard, and all of them must stay green.

If the probe shows that settles do persist into stable frames, the deferral
needs a discriminator. The candidates are the direction and magnitude of the
move relative to `Δupper`, and a focus/selection signal that marks a real
intent. That discriminator is decided with evidence and recorded as axiom A8 in
the programme's GTK axiom ledger.

If the probe cannot reach the case in the sidebar or the adoption lab, the
case is still fixed. The spec obligates non-erasure, and GTK Lush consumers
with variable-height rows are in scope. The probe then uses a synthetic
`GtkScrollable` fixture.

### D5. `EXDEV`-only copy fallback

Match `err.raw_os_error() == Some(libc::EXDEV)`, through the filesystem
boundary's error classification rather than a raw `libc` import in the service.
Any other error propagates. The existing source-absent, target-present branch
already completes a retry after a rename that took effect.

### D6. Leftover sweep scope

The sweep is a pure name-and-metadata predicate plus a bounded executor in the
filesystem boundary. A file is swept only when all of these hold:

- the name parses as `.{file}.{tag}.{pid}.{seq}.tmp` with a known `tag`;
- `pid` is not the current process id;
- it is a regular file;
- its mtime is at least 24 h old.

Where it runs:

- **Startup:** the app-data root, `drafts/`, `local-history/` lineages, and
  sidecar directories. It runs off GTK, capped per pass.
- **Workspace directories:** only after a successful durable write of target
  `T`, for leftovers whose `{file}` equals `T`'s name, in `T`'s directory.

There is never a traversal of workspace trees. A 24 h age plus a foreign pid
avoids racing a concurrently running second instance, which is outside the
single-process axiom but cheap to respect.

### D7. Relative parent normalisation

Add one helper, `parent_or_current(path) -> &Path`, which maps `None` and
`Some("")` to `.`. Use it in `sync_parent_dir`, `missing_ancestors`, and every
`parent()` call inside the durable-write module. Unit-test it with a bare name
and with `./name`.

### Implementation notes (recorded during apply)

- **D2's "unknown mtime" is encoded, not `None`.** `resolve_file_draft_restore`
  skips the freshness check when `original_mtime_secs` is `None` and restores
  the body, and that is a pinned contract: a draft written while its file was
  unavailable must still restore when a failed-load tab is retried. So a
  session-reconstructed entry records `UNPROVEN_BACKING_MTIME_SECS`
  (`u64::MAX`, an mtime no real file has) instead of `None`. Freshness can then
  never pass, and the entry follows the D3 stale path. The schema is
  unchanged. Rollback: an older build would read such an entry as stale, but
  reconstruction and preservation happen in the same startup pass, so an
  entry only survives to a rollback when its preservation failed.
- **D1 needed an additive fallback.** Registering only through
  `update_manifest` made every first body write depend on a *complete*
  reconciliation. A journal that stays partial for a reason reconciliation
  cannot resolve (for example a non-regular `*.draft` entry) then refused to
  write any new draft at all, which protects less than before the change.
  `draft_service::register_draft_entries` therefore tries the trusted commit
  first and, when the inventory is not complete, adds the absent entries to the
  persisted manifest without granting authority (only if its recovery
  evidence permits replacement). Adding entries never drops anything the
  persisted manifest recorded, so the invariant "every body has an entry or an
  untitled id" still holds and trust is never claimed without a complete
  inventory. Registration fails, and the body is not written, only when even
  the additive write fails.
- **Data-safety audit follow-ups (task 6.3).**
  - *Retention backstop.* The accepted D3 trade-off ("preserved snapshots are
    subject to local-history retention") turned out to be a real loss path: a
    back-dated snapshot is the first thing retention prunes, and the draft
    body is already gone by then. Every stale body is therefore also copied
    byte-identically into `drafts/set-aside/` (never pruned) before local
    history is attempted; the alert still points at Local History.
  - *Copy at reconstruction.* A D2-rebuilt entry records an mtime older
    builds read as stale-and-delete, so reconciliation keeps a set-aside copy
    of each attributed body immediately.
  - *Preserve before overwrite.* Registration keeps a copy of an existing
    body whose persisted entry has a different backing mtime before the new
    body write can overwrite it, and a failed runtime preservation stays
    queued so any later delete of that id preserves first. A startup `Kept`
    outcome is retried through the journal when the tab opens.
  - *Pending restore.* (Pre-existing, found by the audit.) Autosave could
    overwrite a recovery body while its lazy restore was still resolving if
    the user typed first. Autosave now skips ids with a pending restore, and a
    restore rejected because the tab was edited is preserved before retiring.
  - *Set-aside `Copy` really copies.* `copy_file_durable` removes its source,
    so the copy transfer now reads the body (bounded) and writes it with
    `atomic_replace`, moving only bodies over the automatic-draft limit; a
    transfer whose final sync failed after placement counts as placed.
  - *Launch nonce in temp names.* A pid alone repeats across launches inside
    a Flatpak pid namespace, so leftovers from an earlier launch would never
    be swept and a restarted counter could hit `EEXIST`. The sequence field
    now carries a per-launch nonce in its high 32 bits (the name shape is
    unchanged), "ours" means same pid **and** nonce, and temp creation retries
    a bounded number of fresh names on `AlreadyExists`.
  - *No symlinked lineages.* The startup sweep skips local-history lineage
    entries that are symlinks.
  - Runtime draft reconciliation attributes bodies against the session
    including descriptors progressive restore has not admitted yet.
- **Two set-aside transfers.** Unattributable bodies found by reconciliation
  are *moved* (rename-no-replace, after the traversal's directory-identity
  check), because nothing else in the journal owns them. Stale bodies are
  *copied*, and the original is then retired by the journal's existing
  body-then-entry deletion, so the deletion ordering and its tombstone/intent
  guards stay the only path that removes a journal body.
- **Lazy stale path goes through the delete queue.** A stale draft found when a
  file is opened later is preserved inside the serialized delete worker
  (`retire_stale_draft`), so preservation cannot interleave with an autosave of
  the same id; a preservation failure deletes nothing and keeps the tombstone.
- **Local-history acceptance** requires the file and body within size policy,
  text that snapshot normalization leaves byte-identical (no `\r`), and that
  retention keeps the back-dated snapshot; otherwise the set-aside fallback
  runs. If a lineage index is later repaired from snapshot files, the repaired
  timestamp derives from the snapshot id (capture time), not `saved_at_secs`.

## Risks / Trade-offs

- [The write-ahead registration adds one manifest reconciliation to a pass
  containing new ids] → It is bounded by the existing paged inventory, and it
  runs only on first-dirty for an id. The benchmark smoke confirms there is no
  steady-state change.
- [Session-hash attribution depends on `session.json` listing the tab] → When
  it does not, the body is set aside, never lost. The diagnostic makes it
  discoverable.
- [Preserved snapshots are subject to local-history retention and may be
  pruned after many newer snapshots] → This is the same promise local history
  makes for every version, and far better than today's immediate deletion. A
  pruning exemption is deferred because it would need an index format change.
- [Deferring suppressed requests could re-open the 129a7e61 creep] → The probe
  comes first, the stillness and count tests act as guards, and a
  discriminator is recorded if one is needed.
- [The sweep could remove a file a user deliberately named like our temp
  pattern] → It requires a known tag, a foreign pid, a regular file, a 24 h
  age, and in workspaces the exact target name. The residual risk is accepted
  and documented.

## Migration Plan

- No user action is needed. The first startup on the new build runs D2
  attribution, recovers or sets aside wedged bodies, and sweeps app-data
  leftovers.
- Rollback: an older build ignores `drafts/set-aside/`. It is outside the
  manifest and outside the `*.draft` pattern, which task 2.7 verifies.
  Preserved snapshots use the existing `Periodic` origin, so older builds read
  them as ordinary history.

## Open Questions

- Resolved: the D3 destination reuses the existing `Periodic` origin, so no
  local-history format change is needed.
- Is the D4 case reachable in the LushText sidebar, or only in a
  variable-height GTK Lush consumer? This affects only the probe's fixture, not
  the obligation to fix.
