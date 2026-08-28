# Data-safety passes — `WFR-WORKSPACE-TREE` (tasks 0.8, 8.1–8.7)

This row is `tier-3` throughout and its file operations are the only paths in this
slot that touch **the user's own documents** rather than app-owned metadata. Slot 5's
pass over exactly this code found **eleven** findings, and the programme record's
lesson is explicit: a tier-3 migration slot must budget for the pass finding more
than one defect, because the pass is aimed at exactly the code the migration is
about to restructure.

## Pass 1 — before any code (task 0.8)

Run in **explicit mode**, all five domains, in the skill's deterministic batches.
Scope (normalized suffixes, recorded once so reruns cannot silently broaden or
narrow it):

```
ui/sidebar/{mod,callbacks,dialogs,imp,policy,seams,test_policy,workspaces,file_tree_item}.rs
ui/sidebar/workspace_section/{actions,context_menus,dnd,folders,icon_presentation,
                              imp,mod,peek,refresh,row_accessibility,row_factory,
                              tree_index,tree_loading,watch,watch_targets}.rs
services/{workspace_manager,workspace_watch,file_tree,file_peek}.rs
model/{workspace,workspace_persistence,workspace_scan}.rs
ui/window/{documents,dialogs,imp,mod,drafts}.rs   (rename/delete consumers + close flush)
ui/editor_page/document_identity.rs               (identity republish)
services/{bookmark_service,migration_ledger}.rs   (sidecar migration)
```

**Seven findings confirmed.** Two were fixed in this change; five are cross-cutting
non-tree defects handed on with named durable homes and an explicit scope deviation
(see "Scope deviation" below). These seven are **pass 1's** findings; for the total
this change fixed across all three passes, see "The count, stated once" at the end of
this file.

### Findings, sorted by severity

#### [HIGH] Confirmed delete removed by path with no identity recheck — **FIXED**

**File**: `ui/sidebar/workspace_section/actions.rs` (delete worker in
`show_delete_confirmation`) — **a tree file, this row's own.**

Independently flagged by **two** domain reviewers (`atomic-write` row-specific
check (c), and `draft-integrity` FLAG 3), which is why it is recorded first.

```rust
let path_for_io = path_c.clone();          // captured at dialog-present time
gtk_lush_tasks::spawn_blocking_then(section, move || {
    let guard = fs_write::TargetWriteGuard::acquire(&path_for_io);
    let result = match guard {
        Ok(_guard) => if is_dir { fs_mutate::remove_dir_all_if_exists(&path_for_io) }
                      else       { fs_mutate::remove_file_if_exists(&path_for_io) },
```

**Impact.** `path` and `is_dir` are captured before `dialog.present()`, and the
confirm handler deleted that **name** with no identity comparison. The confirmation
window is user-paced and unbounded, so the sidebar's own inline rename, an editor
Save As, or an external `mv` can make the name refer to a different object; the
confirmed delete then destroys **a different file**, and for the directory branch a
different subtree **recursively** and unrecoverably.

Kind substitution alone fails safe (`remove_dir_all` on a regular file is `ENOTDIR`,
`remove_file` on a directory is `EISDIR`), so the exploitable case is **same-kind**
substitution.

**What made this a clear defect rather than a judgement call**: the *same file*
already states the rule and follows it. `spawn_temp_item_cleanup`'s own doc comment
reads *"record the candidate inode, acquire the stable write guard, and recheck inode
before deleting. **Never delete by path alone.**"* — and the confirmed-delete path,
the more destructive of the two, did exactly that.

**Fix.** The decision was extracted as **pure policy** rather than inlined in the GTK
closure, so it carries mutation coverage:

- `ui/sidebar/policy.rs` — `confirmed_delete_verdict(expected_inode, current_inode)
  -> ConfirmedDeleteVerdict` with `Proceed` / `RefuseIdentityChanged`. A missing
  `expected` is **refused** rather than treated as "nothing to do", because without a
  recorded identity there is nothing to prove the name still means what the dialog
  said, and `remove_*_if_exists` would otherwise delete whatever appeared there
  afterwards.
- `actions.rs` — captures `fs_metadata::inode(&path).ok()` while the user is being
  asked, rechecks **under the acquired `TargetWriteGuard`**, and reports a typed
  `ConfirmedDeleteOutcome` so a **safety refusal stays distinguishable from a
  failure**. That distinction is load-bearing: a refusal must not fire the delete
  callback and must leave the tree row alone.
- The deliberately **recursive** directory branch is preserved exactly — it is the
  user's explicit confirmed intent, and the fix changes only *which object* is acted
  on, never *what* the confirmed action does.

**Verification.** Four pure unit tests in `policy.rs`
(`a_confirmed_delete_proceeds_only_against_the_identity_the_user_was_shown`,
`a_same_name_different_object_is_refused`,
`a_vanished_target_is_refused_rather_than_treated_as_already_done`,
`an_unreadable_original_identity_is_refused`), plus the mutation proof in
`evidence/mutation-workspace-tree-policy.md`. `make check` clean.

#### [HIGH] A sidebar rename left the draft journal holding the OLD path — **FIXED**

**File**: `ui/window/documents.rs` (`update_tab_path`) — **not a tree file, but the
tree's rename is its only driver, and the fix is one line at the point of identity
change.**

**Impact — silent loss on normal usage.** `handle_sidebar_file_renamed` updated
tabs, note sidecars, local history, and the palette index, but **nothing re-stamped
the draft journal**. A settled autosave has already cleared `draft_dirty`, so
`collect_dirty_draft_candidates` produced no candidate and the 5-second tick never
rewrote the entry: the persisted `DraftEntry` kept `original_path = old` indefinitely.

Sequence: user edits a file → autosave settles → user renames it in the sidebar →
user stops typing → process is `SIGKILL`ed. On relaunch `resolve_file_draft_restore`
finds no file at the old path and returns `Skip(Unavailable)` — **the one arm with
neither a notification nor a restore** — so the unsaved edits sit in an on-disk draft
body that no UI surface will ever offer.

Graceful close is unaffected: the close flush re-reads `editor.file_path()`, and the
id-vs-path mismatch is absorbed by `check_draft_on_open`'s `find_by_path` fallback.
The loss is specific to the crash path, which is exactly where a draft is supposed to
be the safety net.

**Fix.** `editor.set_draft_dirty(true)` immediately after `editor.set_file_path()`
inside `update_tab_path`'s per-editor loop — the point where identity actually
changes, so every caller is covered rather than only the sidebar route.

**Safe by construction, not merely convenient**: autosave eligibility requires
`is_modified()` **as well as** `draft_dirty()`, so a clean tab gains no spurious
draft from this; a modified tab simply has its entry rewritten with the live path on
the next tick.

**Why fixed here despite being a non-tree file**: it is silent loss on normal usage,
the driver is this row's rename, and the fix is a single line at the identity seam.
Handing it on would have meant knowingly shipping a tier-3 slot whose own primary
operation strands the user's unsaved work. `WFR-DRAFT-RECOVERY` owns the journal and
is named here so the row's future work knows the re-stamp exists.

### Findings handed on with durable homes

Each is a **confirmed** defect, not a speculative one. None is in a tree file, and
each needs work in a row this change must not restructure.

#### [MEDIUM] No durable record covers the gap between `rename()` and the ledger

**File**: `ui/window/notes/journal.rs` — **`WFR-NOTES-BOOKMARKS`'s** (migrated).
`migration_ledger::record_pending` is the first durable write and happens in a
*later* `spawn_blocking_then` dispatch than the filesystem rename. A crash after
`rename_durable_no_replace` succeeded but before `record_pending` landed leaves
bookmarks and notes keyed to the old path with **no ledger entry**, so
`reconcile_pending_migrations_on_startup` has nothing to retry.
**Fix owed**: record the migration intent durably *before* the guarded rename runs,
and retire it on completion, so recovery covers the whole window.
**Durable home**: `docs/next/persistent-format-hardening.md`, owner
`WFR-NOTES-BOOKMARKS`.

#### [MEDIUM] The file monitor is never re-armed on the new path

**File**: `ui/editor_page/document_identity.rs` — **`WFR-DOCUMENT-LOAD`'s**.
`set_file_path_with_canonical` → `republish_document_identity` does not call
`start_file_monitor`. After a sidebar rename the open tab keeps a `gio::FileMonitor`
on the vanished old path, so "File Has Changed on Disk" can no longer fire — an
external edit after the rename is then **silently overwritten** by the next save.
**Fix owed**: restart the monitor whenever the display path changes.
**Durable home**: `docs/next/` + the `external-file-monitor-coverage` capability,
owner `WFR-DOCUMENT-LOAD`.

#### [MEDIUM] Sidecar migration's read-merge-write is not inside the write guard

**File**: `services/bookmark_service.rs` (and the same shape in the two sibling
sidecar services) — **a shared service; a `services -> ui` relocation is forbidden
and this change does not touch it.** `merge_bookmark_target` reads the destination
sidecar **unguarded**, and only `save_document` acquires `TargetWriteGuard`. A
bookmark toggled around the rename can be written to the new-path sidecar and then
overwritten by the migration's merge of a pre-toggle snapshot.
**Fix owed**: hold the destination guard across the whole load-merge-save sequence.
**Durable home**: the `document-notes` / `line-bookmarks` capabilities, owner
`WFR-NOTES-BOOKMARKS`.

#### [MEDIUM] Delete tears the editor down before the user can cancel the close

**File**: `ui/window/documents.rs` (`close_tab_for_path`) — the teardown
(`cancel_load`, `stop_file_monitor`, `untrack_editor_memory`) runs **before**
`close_page`, which for a modified tab routes to a save-changes dialog the user may
**cancel**. The tab then survives with its load cancelled, monitor dead, and memory
untracked — and a cancelled in-flight load sets `has_incomplete_load_installation`,
which makes autosave **skip that tab's draft**.
**Fix owed**: move the teardown to the confirmed-close terminal.
**Durable home**: the `unsaved-close-safety-coverage` capability, owner
`WFR-SHELL-LAYOUT` (slot 7).

#### [HIGH] Close proceeds while a pre-persist workspace mutation is in flight

**File**: `ui/sidebar/workspaces.rs` (`handle_add_folder_to_workspace`) +
`model/workspace_persistence.rs` (`close_decision`) — **this IS a tree file, and it
is nonetheless handed on; the reason is recorded rather than elided.**

The Add Folder mutation is only applied — and `persist()` only called — in the
**worker completion** that resolves folder identity off the GTK thread. Between
chooser confirmation and that completion the sidebar is dirty but
`requested == durable`, so `close_decision()` returns `Durable` and `close_request`
has no guard for a sidebar mutation flight. A user who adds a workspace folder on a
slow or network path and immediately closes loses the folder silently.

**Why handed on rather than fixed here**: the fix is to advance the mutation
request at **dispatch** time rather than at completion, which changes the
persistence state machine's request/durable invariant — the same invariant the
close-flush terminal matrix, the retry ladder, and the `StaleFolderSnapshot`
re-dispatch all read. Task 0.4 additionally established that folder add/remove is its
**own stage order**, so this code is about to move into a new coordination module in
this very change. Changing a persistence invariant and relocating its owner in one
step is how a tier-3 workflow acquires an unreviewable defect. The fix belongs
immediately after the structural move, with the driven race test the relocated module
makes cheap.
**Durable home**: this change's own handoff (Appendix B.2) plus the
`workspace-state-persistence` capability, owner `WFR-WORKSPACE-TREE` — i.e. the
next slot touching this row.

### Sub-critical notes (not data loss; recorded for completeness)

- `ui/sidebar/workspaces.rs` `change_scope_from_selector` writes
  `workspaces.set_current_scope(scope)` into the in-memory file **before** the
  `normalized_scope == current_scope` early return, so a no-op-after-normalization
  selection mutates the stored raw scope without scheduling a write. Consistency wart
  only: the raw value is re-normalized on load and written by any later persist.
- `ui/sidebar/workspaces.rs` `StartNewest` while `close_waiting` restarts the worker
  with no defensive waiter resolution. Currently unreachable, but it is an unguarded
  invariant **on a hang path** — if `start()` ever returned `None` the close waiters
  would never resolve and the window would stay permanently insensitive.

### Domains and checks that came back clean

- **`atomic-write`**: AW-1, AW-2, AW-4, AW-5, AW-6 all SAFE. The only persistent
  write this row owns is `workspaces.json` via `workspace_manager::save` →
  `atomic_replace_stream`, generation-guarded, single-in-flight, debounced, with a
  bounded retry ladder and a close flush. No raw `fs::write` / `write_all` /
  `File::create` anywhere in scope. The only `std::thread::spawn` occurrences are
  inside `#[cfg(test)]` mailbox-race tests — the historical detached temp cleanup now
  runs on the guarded pool **with** an inode recheck, which is stronger than the
  calibration's "acceptable" baseline.
- **Rename, checked as its own contract and PROVEN sound**: `rename_target_guarded`
  is the single rename entry point, resolves both endpoints, dedups the
  self-referential case, and acquires both guards in **resolved-key order** —
  including refusing the symlink-onto-its-own-target case that would otherwise
  self-deadlock and leak a worker slot forever. The destination-collision refusal is
  **not** a TOCTOU `exists()` check: `rustix::renameat_with(RenameFlags::NOREPLACE)`
  makes existence and rename one syscall, and only `ErrorKind::Unsupported` falls
  back, leaving a residual window against *external* writers only.
- **Prefix matching PROVEN** in both consumers: `update_tab_path` uses
  `strip_prefix` + rejoin, `close_tab_for_path` uses `== || starts_with`. Neither
  uses exact equality alone.
- **Section teardown SAFE**: every scan/watch/refresh worker completion guards on
  `section_weak.upgrade()`, then `lifetime_generation`, then `targets.generation()`.
  No panicking `TemplateChild` accessor on any completion path, and **no** scan,
  watch, or refresh completion writes `workspaces.json` — `workspaces_file.borrow_mut()`
  occurs only in `ui/sidebar/workspaces.rs`, reached from sections solely through
  synchronous sidebar callbacks.
- **The close-flush contract holds on all four questions** (task 8.x): a flush
  failure **aborts close** and restores sensitivity, re-marks modified editors
  draft-dirty and reschedules autosave; there is **no** never-fires path; an older
  completion **cannot** report success for a superseded snapshot (`apply_success`
  returns `Settled` only when `requested == durable`, else `StartNewest` keeps the
  waiters pending); and an armed-but-unrun retry **cannot** strand close, because
  `StartNow` invalidates the debounce and starts an immediate `CloseFlush` that
  bypasses the `failed` gate for one final attempt.

### Unresolved candidates (not classified either way)

- **Session descriptor freshness after rename.** `update_tab_path` does not call
  `save_session_debounced`, and it could not be statically proven whether an
  unrelated notify path re-saves the session before a crash. This decides only
  *which* of two routes the (now-fixed) FLAG 1 would have taken, not whether it was a
  defect. Missing evidence: a live trace of session writes between rename and crash.
- **Whether orphaned note/bookmark sidecars for deleted paths are ever reconciled.**
  `reconcile_lineages` covers local history; no equivalent sweep was found for
  bookmark/document-note sidecars of deleted files. Retention is the *safer* choice —
  a draft for a deleted file still restores — but the entries accumulate. Missing
  evidence: a startup reconcile enumerating sidecars against existing paths.

## Scope deviation, recorded rather than absorbed

Task 0.8 requires that if the pass consumes this change's capacity, that is a
**recorded deviation** with the scope decision and its reason — never a silent
absorption, and never a partially migrated row.

**The decision**: two findings fixed in this change (the tree-file delete-identity
defect, and the one-line draft re-stamp for a silent-loss path this row drives);
five confirmed findings handed on, each with a named owning row and a durable home,
including one tree-file finding whose fix would require changing a persistence
invariant in the same step as relocating its owner.

**The reason**: `.agents/rules/preexisting-blockers.md` has no exceptions, and the
five handed-on findings are all genuine blockers for *their* rows. Fixing all seven
here would repeat exactly what split slot 5 — seven data-safety fixes consumed that
change's whole capacity and the structural migration did not land — and would do so
while touching four rows this change is explicitly forbidden to restructure.
The programme record's own lesson is the authority for treating this as a scope
decision rather than as a licence to skip.

**What this does not license**: the row is **not** marked `migrated` on the strength
of a partial pass, and none of the five handed-on findings is recorded as "accepted
debt". Each is recorded above with severity, site, scenario, and the fix owed.

## Pass 2 — over the finished diff (task 8.7)

**Ran.** Its findings and fixes are recorded in full under "Pass 2 — over the finished
diff (task 8.7)" further down this file; this heading is kept only as the pointer a
reader following the pass-1 narrative expects. An earlier revision left "_Pending_"
here while the completed pass sat below it in the same file, which is the kind of
stale scope statement this change's own records rule exists to prevent.

**What pass 1's two fixes owed pass 2, and how it discharged them**: the delete path
gained a three-outcome completion, so pass 2 confirmed no caller treats
`RefusedIdentityChanged` as success (see "Categories pass 2 verified clean"); and
`update_tab_path` marking draft-dirty on every identity change turned out to be a
**defect of its own**, latching a clean tab's flag and disabling its 750 ms autosave.

---

# Post-migration re-confirmation of the candidates the move could affect (tasks 8.2, 8.4, 8.5)

The structural migration moved twelve stage orders across fourteen modules, so the
candidates whose guarantee is an **ordering** had to be re-checked after the move rather
than before it. Each was re-read at its new location.

## 8.2 — `TargetWriteGuard` on every file-operation path

**Re-confirmed.** `rename_target_guarded` remains the single rename entry point in
`file_execution.rs` (formerly `actions.rs`), and the module moved **as a whole file
rename**, so no path was reordered or duplicated. It still resolves both endpoints, dedups
the self-referential case, and acquires both guards in resolved-key order — including
refusing the symlink-onto-its-own-target case that would self-deadlock and leak a worker
slot.

The confirmed-delete path now acquires the guard **and** rechecks inode under it, which is
stronger than at slot 5a's assessment: the recheck is this change's own HIGH fix.

**Verdict: no driven test added.** The paths are unchanged by the move, and the identity
decision that *was* changed is covered by four pure unit tests plus 3 caught mutants.

## 8.4 — rename ordering against watcher, expansion set, and sidecars

**Re-confirmed after the move.** The ordering is intra-function in `file_execution.rs` and
survived the file rename intact: expansion set → directory state → row path → watch row →
item cache → rename callbacks → **then** sidecar migration.

Two things the migration could have broken and did not:

- `clear_dir_state` moved from `tree_loading.rs` to `scan_execution.rs`, so the call is now
  `super::scan_execution::clear_dir_state`. It is the same function, reached at the same
  point in the same order.
- `migrate_note_sidecars_after_rename` remains a **call** into the migrated notes
  workflow, made after the row updates settle. This change did not restructure that row.

The facade's narration now states this ordering explicitly, which is new: it was
previously discoverable only by reading the rename body.

## 8.5 — the deferred expansion restore reads live state at apply time

**Re-confirmed, and this was the highest-risk item in the move**, because
`schedule_child_state_restore` relocated from `tree_loading.rs` into `scan_execution.rs`
while the expansion set it reads relocated from `tree_index.rs` into the **same** file.

The property holds: the `expanded_paths` borrow lives **inside** the deferred closure, not
in the scheduling scope, so a user collapse between scheduling and the callback is never
resurrected. Both `scan_execution.rs`'s module doc and the facade's narration now state it,
and `.agents/rules/ui.md` was corrected in this change to name the one real implementing
site (it had named a second function that never existed).

**The evidence surface did not become a second source of truth for expansion.** It derives
`expanded_path_count` from `expanded_paths` and never writes it, and its inertness proof
asserts the expansion-capture counters are unchanged across reads taken both collapsed and
expanded.

## 8.1, 8.3, 8.6, 8.7 — all four resolved

Recorded plainly, because an earlier revision of this section listed all four as
outstanding and that text survived the work that closed them:

- **8.1**, the M-4 driven race test, **ran**: the budgeted configuration seam is spent,
  and there are now **two** driven tests, one per half of the race. See "M-4, driven at
  last" at the end of this file. The guard is no longer proved by shape only.
- **8.3**, whether this row's persistence path can refuse to write empty state it did not
  derive from user intent, is **answered** below: this row is a genuine amplifier for M-8
  and the fix still belongs upstream, with the additive step this row should carry
  recorded so it is not rediscovered.
- **8.6** is a boundary record, already written above.
- **8.7**, the second `data-safety` pass over the finished diff, **ran**, and it was the
  most valuable artifact in the change: six findings, every one introduced by this change
  or by a fix inside it — the same shape slot 5a's pass 2 found.

## 8.6 — the boundary for the coverage gap that is not this row's

**Recorded with its owner named, so a later slot does not read the omission as an
oversight.** Slot 5a left four unproven pass-2 defects — the empty-set `sidecar_resolved`
deferral, the flush-versus-in-flight race, and the Save As reset (P2-2 through P2-5) — in
`ui/window/notes/bookmark_execution.rs`. Their drivers are **tab close** and **Save As**,
neither a tree file nor a tree entry point. Owner: **`WFR-NOTES-BOOKMARKS`**.

**Confirmed after this change's restructuring: the tree-side rename path does not become a
fourth driver.** The rename calls `migrate_note_sidecars_after_rename`, which is the
migration route, not the `sidecar_resolved` deferral route; and this change moved
`file_execution.rs` as a whole file without reordering or adding a call into
`bookmark_execution.rs`. `git diff` confirms `ui/window/notes/**` is untouched.

## 8.3 — Is this row an amplifier for M-8? **Yes, and the fix still belongs upstream.**

**M-8 restated**: `services/recovery_metadata.rs` classifies a *transient* read failure
identically to structural corruption, quarantines the live `workspaces.json`, and
returns default (empty) state with `replacement_allowed = true`. The defect is
upstream and is **not** this row's.

**The amplification question is whether this row's persistence path can then write that
empty state over the user's real workspace list.** Traced from the code:

1. `list_execution::load_workspaces` adopts `load.value` into `workspaces_file`. After a
   transient-failure quarantine that value is **empty**, so in-memory state is empty.
2. **`load_workspaces` does not call `persist()`.** Adoption alone writes nothing —
   verified: every `persist()` call site in this row is in a mutation handler
   (`list_execution` ×5, `membership_execution` ×3, `filter_execution` ×1), none on the
   load path.
3. **But `start_persist_worker` clones `workspaces_file` and writes it with no guard
   against emptiness or provenance.** So the *next* mutation makes the empty state
   durable.
4. **The sharpest case is `filter_execution::change_scope_from_selector`, which calls
   `persist()`.** Changing the scope filter is a *view* action, not a workspace
   mutation — a user who opens the app, sees an empty sidebar, and touches the scope
   dropdown has now durably overwritten their workspace list without performing
   anything they would recognise as an edit.

**Verdict: this row is a genuine amplifier, and the fix still belongs upstream.**

Could this row refuse? A guard would have to distinguish "empty because the user
deleted everything" from "empty because a load was quarantined" — and that is exactly
the classification `RecoveryLoadOutcome` already computes and that `load_workspaces`
currently discards. Re-deriving it downstream would mean a second, independently
drifting notion of whether the loaded state is trustworthy, which is the class of
duplication this migration spent effort removing (see the two `scope_kind` matches).

Fixing it upstream — not quarantining on a transient read failure — removes the empty
state at its source and needs no new downstream policy.

**What this row should carry when that lands**, recorded so it is not rediscovered:
`load_workspaces` should keep the `RecoveryLoadOutcome` rather than dropping it, so a
defaulted-because-quarantined load can be marked non-persistable until an explicit user
mutation. That is a small, additive change to this row **once** the upstream
classification is trustworthy; doing it first would only harden the wrong layer.

**Owner: the `recovery-metadata-integrity` capability**, tracked in
`docs/next/persistent-format-hardening.md` as M-8. This row's amplifying step, and the
`change_scope_from_selector` path that makes it reachable without an edit, are recorded
here so the upstream fix can be verified end to end rather than in isolation.

---

# Pass 2 — over the finished diff (task 8.7)

Run in explicit mode against `git diff origin/main -- crates/lushtext-core/src
crates/lushtext/tests`, aimed specifically at hazards **this change's own restructuring
and its own fixes** could introduce.

**Six findings, every one introduced by this change or by a fix inside it.** That is the
same shape slot 5a's pass 2 found (seven, all self-inflicted), and it is the strongest
argument in the programme for making pass 2 non-optional: pass 1 audited the code as it
was, and none of these existed then.

## [CRITICAL] The M-4 guard destroyed the whole workspace list instead of one workspace — **FIXED**

`ui/sidebar/list_execution.rs` (the guard) with `persist_execution.rs` (snapshot timing).

The guard slot 5a landed — and that this change moved and tested — **skipped adoption**
when a mutation superseded the load. Three facts combine into data loss far worse than
the defect being fixed:

1. `workspaces_file` starts **empty** (`RefCell::default()`);
2. `handle_new_workspace_name` mutates that empty file and calls `persist()`;
3. `start_persist_worker` snapshots `workspaces_file` **when the worker starts**, not
   when the write was requested.

So if the user creates a workspace *before the first load lands*, skipping adoption
leaves memory holding only the new workspace, and the already-scheduled write commits
that over `workspaces.json` — **destroying every pre-existing workspace and all its
folder memberships.** Before the guard, adoption at least restored the stored list and
only the just-created workspace was lost.

**This change's own M-4 driven test did not catch it**, and the reason is instructive:
the test waits for the startup load to settle before driving the race, so memory already
held the stored list. The catastrophic window is the one where it does not.

**Fix: merge, because neither side may win.** `merge_superseded_load` takes the loaded
file as the base — the only source carrying every workspace on disk — and layers the
in-memory workspaces on top, winning on id collision because a mutation that bumped the
persistence generation is by definition newer. The merged state is then **persisted**,
not merely displayed: the superseding write already truncated the file, so repairing only
the UI would leave disk truncated.

**Proved**: `test_a_workspace_created_before_the_first_load_does_not_destroy_the_stored_list`
fails with `left: 1, right: 4` against skip-adoption and passes against the merge, and
asserts all four workspaces by name **on disk**.

## [HIGH] The retired destructive seam's reset was never called — **FIXED**

`crates/lushtext/tests/widget/workspace_section.rs`.

`take_watch_target_rows_touched_for_test` was a `take`, and one test used it **purely for
its reset**. The retirement rewrote that line into a discarded read, and
`reset_watch_target_rows_touched_for_test` — which was added for exactly this — had
**zero call sites**. The test mounts and expands 32 child directories first, so the
assertion `<= 2` was measuring ~70 cumulative touches.

**This is the finding the widget lane caught independently**, failing
`test_one_row_collapse_touches_only_its_incremental_watch_delta`. Fixed by calling the
reset. **The lesson is specific and worth carrying: when a destructive read is split into
a read plus a reset, every former call site must be classified as observation *or* reset —
the mechanical rewrite silently drops the ones that were resets.**

## [HIGH] The draft re-stamp latched a clean tab's flag and disabled its 750 ms autosave — **FIXED**

`ui/window/documents.rs`.

Pass 1's own fix. Its safety argument — "autosave eligibility requires `is_modified()`,
so a clean tab gains no draft" — was right about the *draft* and wrong about the *timer*.
Because a clean tab is never an autosave candidate, `set_draft_dirty(false)` never runs
and the flag stays `true` forever. The buffer's `connect_changed` decides whether to
schedule the **750 ms first-dirty** autosave from `!draft_dirty()`, so the user's first
keystroke saw `false` and the first durable draft fell back to the **5 s** tick.

Scenario: rename a clean file, start typing, crash after 2 s — precisely the edits the
fast path exists to capture are lost. **Fixed** by gating the re-stamp on
`editor.is_modified()`, with the reason recorded at the call site so it is not
"simplified" back.

## [MEDIUM] The delete identity recheck made dangling symlinks undeletable, silently — **FIXED**

`ui/sidebar/workspace_section/file_execution.rs`.

Pass 1's other fix. `filesystem::metadata::inode` uses `rustix::fs::stat`, which
**follows** symlinks, so a dangling symlink row yields `expected_inode == None` and the
verdict refuses unconditionally — while the previous code correctly unlinked the link
itself. And the refusal arm only logged, so a confirmed destructive action did **nothing
at all** with no user feedback.

**Fixed** two ways: a new `filesystem::metadata::link_inode` (`lstat`, added through the
filesystem boundary rather than around it) identifies the entry the user pointed at, and
the refusal now emits a user-visible warning. A destructive action that silently does
nothing is its own defect.

## [MEDIUM] `record_touched_rows` was ungated while its call sites stayed gated — **FIXED**

`ui/sidebar/workspace_section/watch_targets.rs`.

The ungating pass reached the recorder but not its three `#[cfg]`-gated call sites, so
the counter was **always 0 in a default build** while the evidence field's doc presented
it as live state — and the dead private fn warned only in a configuration
`--all-features` cannot see. **Fixed** by gating the counter symmetrically and stating
in the field's doc that it is instrumentation, always `0` without `test-utils` — the same
honesty the `process_*` scan counters owe about their scope.

## [MEDIUM] The polled snapshot allocated every section's collections for ten scalars — **FIXED**

`ui/automation.rs`.

Projecting from the full surface was correct and inert but turned an O(1) read-only D-Bus
poll into O(sections x rows) allocation: per section a watch-target vector, an expansion
set, and both file-row identity sets. **Fixed** with `workspace_snapshot_evidence`, a
scalar-only read whose every field comes from the same place the full surface reads it,
so the two agree by construction. The full surface remains the single thing widget tests
read and the drift gate attributes against.

## Categories pass 2 verified clean, explicitly

- **Evidence-surface borrow discipline** — clean. Every borrow is scoped to a block or a
  single statement and dropped before the next; the `sections` borrow held across
  `workspace_section_evidence` is safe because that call reaches only the section's own
  imp and never re-enters the sidebar.
- **`WorkspaceWatchTicket` disposition** — exact. Lifetime checked first, then targets,
  matching the deleted sequential `if`s in order and effect; the `targets` borrow
  building the facts is a statement temporary dropped before the `Restart` arm re-enters.
- **Confirmed-delete recursion and caller handling** — preserved. The recursive branch
  still runs for the confirmed directory case, and no caller treats
  `RefusedIdentityChanged` as success.
- **Test-only load delay gating** — clean; production compiles neither the sleep nor the
  policy static.
- **Module-move behavior neutrality** — verified by normalized whole-file comparison. The
  dissolutions differ only in module paths and deleted seams; the generation `next()`
  change is arithmetically identical.
- **Rename ordering** — unchanged and byte-identical apart from the module rename.
- **Newly reachable panicking accessors on teardown** — none.

## One flake risk pass 2 raised, hardened

Comparing the **full** surface twice with live watchers on real tempdirs could differ if
an inotify notice lands between the reads. Hardened with
`evidence_without_live_mailbox`, which normalizes only the mailbox and poll-notice count
and compares **everything else exactly**. The reentrancy claim is that *reading* does not
mutate — not that the kernel is quiescent — and asserting over the mailbox would have
made the proof a flake detector for inotify.

---

# M-4, driven at last (task 8.1) — and what driving it exposed

Slot 5a proved the M-4 guard **by its shape only** and named a driven test as this
slot's highest-value remaining coverage. Driving it needed the one budgeted seam, and it
did not merely confirm the guard — it showed the guard was **wrong in the case that
matters most**.

## The driven test

`test_a_workspace_created_during_a_load_is_not_reverted_by_the_load`
(`crates/lushtext/tests/widget/sidebar.rs`), using
`set_workspace_load_worker_delay_for_test(600)` and interposing at 150 ms.

**Proved to fail without its fix**, which is the bar this project sets:

| Guard state | Result |
| --- | --- |
| guard present | passes: 4 workspaces in memory **and on disk**, the created one by name |
| guard reverted to `if false` | **fails, `left: 3, right: 4`** — the created workspace is reverted |

Two seam-design details the first attempt got wrong, recorded because both are easy to
repeat:

1. **The delay must come *after* the read, not before it.** Delaying first let the worker
   read the *post*-mutation file, so adoption was harmless and the test passed against
   the reverted guard too — a test that cannot fail.
2. **The precondition must not assume an unloaded sidebar.** Presenting the window runs
   its own startup load, so the test waits for that to settle first and drives the race
   against a load it dispatched itself.

## What driving it exposed: the guard was catastrophic before the first load

Pass 2 found it, and the driven test's own design is what had hidden it — by waiting for
the startup load to settle, the test always ran in the *safe* half of the race.

`workspaces_file` starts **empty** and `start_persist_worker` snapshots it **when the
worker starts**. So if the mutation lands before any load has been adopted, skipping
adoption leaves memory holding only the new workspace, and the already-scheduled write
commits that over every workspace on disk.

**The corrected rule needs exactly one bit**, and it is now pure policy:

| `any_load_adopted` | Action | Why |
| --- | --- | --- |
| `true` | `KeepMemory` | memory holds the full list, so an absent workspace is one the user **deleted**; merging would resurrect it |
| `false` | `MergeAndPersist` | memory is one mutation on top of the empty initial file; discarding the load would truncate the stored list |

`policy::superseded_load_action` and `policy::merge_superseded_workspace_load` are both
pure, both in the mutation scope, and covered by four unit tests — including
`merging_cannot_express_a_deletion_which_is_why_it_is_gated`, which **pins the
resurrection hazard as real** rather than describing it. That test exists because the
first version of the merge fix *did* resurrect a deleted workspace, and the widget lane
caught it as `left: 3, right: 2`.

**A widget test for the pre-first-load half was written and deliberately removed** — and
that removal was a mistake, corrected in the fix cycle below. The stated reason (racing
`present_window`'s own startup load is not deterministic) was true of a *windowed* test
and false of the arrangement that actually works: a **standalone** `LushtextSidebar` has
no startup gate at all, so the load the test dispatches is unambiguously the first one.
Removing it left the only proof of the CRITICAL fix at the pure-policy level, which
could not detect that the bit fed to that policy meant something else entirely.

---

# Fix cycle — findings from the independent review of the finished change

An independent review of the completed diff found two further **data-safety** defects,
both in code this change had itself written. They are recorded here with the same
severity discipline as the passes above, and neither is "accepted debt".

## [CRITICAL] The M-4 fix was inert: the bit meant "rebuilt", not "a load was adopted" — **FIXED**

`ui/sidebar/list_execution.rs` with `ui/sidebar/imp.rs`.

The pass-2 fix above is correct policy fed the **wrong input**.
`superseded_load_action(any_load_adopted: bool)` was passed `imp.workspaces_loaded`,
whose only writer was `build_sections_from_file` — which `rebuild_sections_from_state`
calls from *every* mutation. `handle_new_workspace_name` persists and rebuilds
synchronously, so the bit was already `true` by the time the startup load completed.
The superseded load therefore chose `KeepMemory` in exactly the case
`MergeAndPersist` exists for, and the 150 ms debounced write committed the single new
workspace over every workspace on disk. **`MergeAndPersist` had no reachable production
caller at all.**

This is the archetype seam defect `.agents/rules/rust.md` names: a value meaning one
thing passed into a parameter naming it another, invisible to review and invisible to
tests while both names denote "some bool about loading".

**Fix**: the bit is renamed `load_adopted` and written **only** by
`adopt_loaded_workspaces`, the single entry point both load-adoption branches take.
`build_sections_from_file` no longer records adoption, and says why at the line where it
used to.

**Proved**, with the widget test the earlier revision had removed, now written against a
standalone sidebar so the interleaving is deterministic:
`test_a_workspace_created_before_the_first_load_completes_merges_instead_of_overwriting`
seeds three workspaces, holds the load worker 600 ms, asserts memory is still empty,
creates a workspace, and asserts all four survive in memory **and on disk** by name.

| Bit derivation | Result |
| --- | --- |
| set by `build_sections_from_file` (as landed) | **fails, `left: 1, right: 4`** — every stored workspace destroyed |
| set only by `adopt_loaded_workspaces` | passes: four in memory and four on disk |

`test_a_workspace_created_during_a_load_is_not_reverted_by_the_load` and the
resurrection-pin policy test stay green: the `KeepMemory` gate on the post-adoption
half is unchanged.

## [MEDIUM] The confirmed-delete recheck refused an already-vanished target — **FIXED**

`ui/sidebar/policy.rs` with `workspace_section/file_execution.rs`.

`confirmed_delete_verdict` refused whenever the current identity was unreadable, which
folds together two genuinely different situations. If the confirmed object is simply
**gone**, there is nothing to destroy and nothing to protect: before the recheck existed,
`remove_*_if_exists` returned `Ok`, the row was reconciled, and the delete callback
fired. Refusing there regressed that to a stale row plus a user-visible "That item
changed on disk and was not deleted" for an object that is not there.

**Fix**: a third verdict, `ReconcileAlreadyGone`, for `expected = Some(_), current =
None`. The refusal is kept for a **different** object under the same name, and for an
original identity that was never readable — the two cases where something could still be
destroyed. Covered by `a_vanished_target_reconciles_the_row_instead_of_reporting_a_refusal`
and `a_vanished_target_is_still_distinguished_from_a_substituted_one`, which pins the
pair that must never collapse.

## Non-data-safety findings from the same review, recorded but not counted above

- **`touched_rows` was still ungated** while every writer and reader was gated: pass 2's
  fix reached the recorder and its call sites but not the field, so the default-feature
  build carried a `dead_code` warning that `--all-features` cannot see. Reproduced with
  `cargo check -p lushtext-core --lib` (`field \`touched_rows\` is never read`) and fixed
  by gating the field identically.
- **The evidence surface carried two hand-copied derivations** of the same ten fields —
  the duplication its own module doc forbids. The full surface now builds from
  `workspace_snapshot_evidence`, so there is one derivation and the polled snapshot still
  never allocates the per-section collections.
- **The inertness proof had dropped its registry reads**, leaving hazards 1 and 6 —
  `find_store_for_dir` **inserts** while looking up, `find_dir_row` **evicts** on a
  lookup — proved only by counters that an insert-plus-evict pair leaves unchanged. The
  probe again captures both key sets, and now asserts they are non-empty in the expanded
  case so the proof cannot pass vacuously.

## The count, stated once

**Seven confirmed data-safety defects fixed in this change**: two from pass 1, three
from pass 2, and two from the fix cycle. Pass 2 raised **six** findings in total and the
fix cycle **five**; the remainder are test-correctness, instrumentation, performance, and
duplication fixes, recorded in their own sections above rather than counted here. Any
other number in this change's records is superseded by this paragraph.
