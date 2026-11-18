## Context

`consolidate-formal-verification-on-kani` put several cores under Kani: the
draft journal, the durable write, move, and rename protocols, and the
viewport-slice loop. Its two `/simplify` passes proposed six altitude
redesigns that it deliberately did not take. N8 of
`docs/next/formal-verification-next.md` lists them.

Each redesign closes a gap between what a verified core guarantees and what
the code around it merely follows by convention. The groups below are
**independent**. Each has its own rationale, risk, Kani decision, data-safety
audit, and documentation, so the change can be applied one group at a time.
Two pairs edit the same files and are applied one after the other, never
interleaved:
- groups 1 and 4 (`durable_write.rs`);
- groups 3 and 5 (`ui/window/drafts/`).

### Revision against 187c6299 (2026-11-18)

The change was proposed at 170212d9 and was then held back until
`rust-1-100-adoption` and `stream-replace-all` (#103) had merged. Both are
archived by `main` 187c6299. Every fact below was re-read at 187c6299. The
programme record lists what #103 left for this change ("What
`apply-verification-altitude-redesigns` and
`verify-shell-conformance-against-proven-cores` build on"). Since the
proposal, `main` has also gained:
- the envelope admission of `close-heavy-admission-window`, so every
  `make test`, `kani`, `proof-strength`, and `mutants-*` command below is
  written in its `scripts/heavy.sh --envelope` form;
- the compiler front-end threads (#114).

What the revision changed:

| Group | Verdict | What changed |
|---|---|---|
| 1 | keep, widened | #103 added two more shells of the same shape: Replace All apply and Undo. The proposed termination harness is **dropped**, because every core's existing harness already proves termination within a bound. The trait and `drive` move out of the Kani oracle `write_protocol.rs`, so no proven module changes. |
| 2 | keep, re-derived | The layout gains `replace-backup-journal/bodies/` (#103). There are 25 name constants, not about 17. Task 2.2 is resolved now: `recent-documents.json` carries no envelope. The open question is closed. |
| 3 | keep, revised | `autosave_tick` is not a 5 s tick: it also runs from the first-dirty debounce and pending reruns. Retries therefore move to the periodic timer only, and the backoff counts those ticks. The cap is 12 ticks (1 minute), not 60, for data-safety reasons. The open question is closed. |
| 4 | keep, narrowed | `kill_point::reach` keeps `&str`, because `draft_service` narrows it by draft id. `KNOWN` has 11 labels (#103 added `REPLACE_UNDO_BODY`). Tests use about 15 `"test"` tags, which have no label. |
| 5 | keep | It is unchanged apart from a recorded scope note: the synchronous close flush is a third site, and it is deliberately left as is. The two Kani shard runs are dropped, because no module they read changes. |
| 6 | keep, re-derived | The scenarios now use **three** roles: `kill_inside_later_launch_undo` and the draft scenarios start a `seed` launch before the victim. The kill-point scenarios grew from three to four call sites (#103). |

### Spec review (stage 2, 2026-11-18)

An independent review re-read every claim against 187c6299 and kept all six
groups. It corrected these points in place:
- **G1:** the "shared driver" requirement now carries the same documented
  exception as "no loop of their own", which the fallback relies on. A source
  check makes "no loop of their own" testable failing-first.
- **G2:** the sweep's family order (format-upgrade runs, then local-history
  lineages) is the reverse of their order in `ALL`, so it is now explicit and
  pinned. The layout records a truthful `holds_durable_writes` plus a separate
  sweep decision: `workspace-notes/` does hold durable writes, because a
  format upgrade rewrites its sidecars in place. The inventory's decision for
  each entry, including "covered by another entry's scan", lives in the
  inventory's own match, so the layout does not depend on `format_upgrade`.
  The characterization also pins the inventory's recorded relative paths,
  which name the upgrade backups. The constant count recipe includes
  `*_FILENAME`. The `recent_documents.rs:252` mutants anchor moves with the
  edit.
- **G3:** there is no `test_policy` copy-failure hook; the tests block the
  set-aside directory with a regular file, as the existing test does. The due
  tick is counted from the tick at which the failure is recorded, the
  evidence exposes each retry's failure count, and the new cap gets its
  thresholds row.
- **G5:** the partition keeps `journal_core::may_write_after_registration` as
  its gate.
- **Kani guard:** the diff runs against the merge base, so a later `main`
  cannot make it print a file this branch never touched.
- **Smokes:** each commit that touches the UI prefixes needs the three smokes
  current (testing.md), so groups 5 and 3 land together after one smoke run.

### Measured baseline at 187c6299

Line counts are from `wc -l`, and counts from `grep` over
`crates/lushtext-core/src`.

**Group 1.** Five shell loops share one shape: start, then loop over the
action, one call, feed the outcome back.
- In `services/durable_write.rs` (1,688 lines), there are three:
  `atomic_write_stream_with_metadata` (lines 269–385, about 115 lines),
  `run_rename_protocol` (582–613), and `move_durable` (652–672).
- `content_search/replace/apply.rs` (1,047 lines) drives
  `ReplaceFileProtocol`. Its loop spans lines 482–821.
- `content_search/replace/undo.rs` (402 lines) drives
  `ReplaceUndoProtocol`. Its loop spans lines 181–255.

The cores step in two styles:
- `WriteProtocol`, `MoveProtocol`, and `RenameProtocol` use
  `const fn step(self, outcome) -> (Self, Action)`. The outcome is
  `StepOutcome` for writes and `bool` for move and rename.
- The two Replace protocols use `fn step(&mut self, outcome) -> Action`. Their
  shells read `protocol.journal()` and `protocol.rearm_needed()` after the
  loop.

Termination within a bound is **already proved** for every core:
- `run_write` panics "the write did not finish within WRITE_STEPS actions";
- the move and rename harnesses panic "did not finish";
- both Replace harnesses `expect("every … reaches Finish within its bound")`.

`scripts/kani-shards.py` names four oracle modules in this area:
- `filesystem/write_protocol.rs`, shard `core-journal-and-write`, scheduled;
- `content_search/replace/protocol.rs`, shard `core-replace-protocol`,
  pull-request gated;
- `draft_service/journal_core.rs`;
- `draft_service/set_aside_retention.rs`.

No harness file imports `durable_write`, `apply`, `undo`, `draft_service.rs`,
or `ui::window`.

**Group 2.** App-data names are spelled in three places:
1. 25 name constants in 17 files, counted as the `const *_DIR`, `*_FILE`,
   and `*_FILENAME` strings minus `CURRENT_DIR` and the palette fixture's
   `LINKED_FILE`.
   `LEGACY_FOLDER_NOTES_DIR` (`folder_note_service.rs`) and
   `LEGACY_FOLDER_NOTE_SIDECAR_DIR` (`format_upgrade/inventory.rs`) both spell
   `workspace-notes`.
2. The startup sweep's hand list in `app_data_leftovers.rs`: the root, 11
   directories, and two families. It sweeps the flat list first, then the
   format-upgrade runs with their `items/`, then the local-history lineages.
   #103 added `search_backup::bodies_dir`.
3. The format-upgrade inventory, which re-spells about a dozen literals:
   `"drafts/manifest.json"`, `"bookmarks"`, `"local-history"`,
   `"replace-backup-journal"`, `"replace-backup-journal/bodies"`, `"index.json"`,
   `"manifest.json"`, `"cleanup-in-progress.json"`, and others.

This drift class has already happened once. The sweep missed
`style-schemes/`, `format-upgrade-backups/`, and `drafts/set-aside/` until
67aef874 (`formal-verification-evolution.md`, "Extend the leftover sweep").
#103 had to remember both lists by hand. The inventory does not scan
`recent-documents.json`, `style-schemes/`, `recovery-quarantine/`,
`drafts/set-aside/`, or `format-upgrade-backups/`, and nothing records why.
`recent-documents.json` is a plain `RecentDocumentFile { entries }`, with no
`KIND_*` in `json_format.rs`. Its loader resets a corrupt file to an empty
list. Root resolution is `json_store::data_dir()` (`LUSHTEXT_DATA_DIR` or
XDG).

The legacy `workspace-notes/` directory is inventoried (a fixed JSON directory
scan), and `format_upgrade/apply.rs` rewrites an upgradeable item in place
with `atomic_replace`, so durable-write temps can appear there. The sweep
skips it, which is the recorded phase-0 deferral ("the startup leftover sweep
skips the legacy folder-note sidecar directory"). Format-upgrade backups are
named by `stable_path_hash(item.path.relative())` (`format_upgrade/backup.rs`),
so the inventory's recorded relative paths are on-disk names too.

**Group 3.** When the set-aside copy of an unrestored body fails,
`journal.rs::attempt_unrestored_copy` keeps the restore hold, which is correct
and matches the Kani model. It re-inserts `(DraftEntry, failures)` into
`unrestored_copy_retries` (in `imp.rs:270`).

`retry_unrestored_copies` takes **every** entry and is called first in
`autosave_tick`. That function runs from three places:
- the 5 s periodic timer (`autosave_execution.rs:67`);
- the first-dirty debounce (`:266`);
- the pending rerun in `drive_pending_draft_mutations` (`journal.rs:694`).

A persistent failure, such as a full disk or a read-only data home, therefore
starts a blocking worker per held id at least every 5 s, indefinitely, and
more often while the user edits. Each attempt reads the whole body, up to the
64 MiB draft bound, and tries two durable writes: the local-history snapshot
and the set-aside copy. The attempts compete for the eight
`spawn_blocking_then` slots with saves and draft writes.

`DraftEvidence.unrestored_copy_retries` is a count.
`test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft`
(`widget/window.rs:8497`) makes every copy fail by writing a regular file
where the set-aside directory belongs. It drives retries through
`autosave_tick_for_test` in `wait_until` loops. The drafts `test_policy.rs`
has no copy-failure hook.

**Group 4.** `WriteLabel` is a closed set of 11 labels, `KNOWN` included.
- `filesystem/write.rs` converts it back with `label.as_str()` at four sites.
- The `&str` tag then travels through `atomic_write_bytes`, the four
  `*_classified` entry points, `atomic_write_stream_with_metadata`,
  `copy_durable`, `move_durable`, `unique_temp_path`, and `temp_name::format`.
- #103's streaming entry points (`atomic_replace_stream` and
  `atomic_replace_workspace_file_stream`) already take `WriteLabel` at the
  boundary.

`kill_point::reach(window, label: &str)` is called at seven sites:
- four in Replace All (`apply.rs` three, `undo.rs` one), each with a
  `WriteLabel` constant's `as_str()` (`REPLACE`, or `REPLACE_UNDO_BODY` for
  `ReplaceBodyBeforeEntry`);
- one in `durable_write`, with the tag;
- **two in `draft_service.rs` (:2025, :2058), with a draft id**.

`temp_name`'s own test calls `format("a.txt", "not-a-label", …)`.
`durable_write.rs`'s tests pass `"test"` 15 times, and `"json"` twice.

**Group 5.** `DirtyDraftCandidate.registered: Option<RegisteredDraft>` is
filled by `register_new_draft_ids_then`, which serves both asynchronous
pipelines. The autosave body write (`autosave_execution.rs:702`) and the
asynchronous close body write (`:445`) then do
`registered.ok_or_else(|| "recovery metadata was not registered")`. That is a
runtime check for something the stage order already guarantees.

The synchronous close flush in `journal.rs` (about 370–413) makes its token
inline per editor: `without_registration`, or the registration call just
above. It handles `None` as a recorded registration error in the same block.

**Group 6.** `launch_app` resolves its binary with `binary or args.binary`:
- the external-signal scenario uses the ordinary binary for `before-crash`
  and `after-relaunch`;
- the kill-point scenarios, now four call sites, pass
  `binary=args.kill_point_binary` for their `kill-*` launch;
- the `seed-*` launches (draft and later-launch-undo scenarios) and every
  relaunch use the ordinary binary.

"No kill-point code in a release build" was checked once, by hand, with
`strings` (programme record, K5). The `crash-kill-points` feature appears in
these places only:
- `kill_point.rs` (two `cfg`s);
- the two `Cargo.toml` feature lines;
- the Makefile build line.

## Goals / Non-Goals

**Goals:**
- Each group lands alone, with no behaviour change except in group 3 (retry
  cadence) and group 6 (a stronger smoke).
- Every persisted path, temp name, and on-disk byte stays identical. Each
  refactor that touches persistence is proved identical by a characterization
  test captured before the move.
- No Kani-checked core and no harness changes. Each group carries a guard that
  confirms this, and the shard to run if it is ever violated.

**Non-Goals:**
- An inter-process data-directory lock (N6).
- A size bound on `drafts/set-aside/` (N10, done).
- Moving root resolution out of `json_store::data_dir()`.
- Renaming any on-disk location, or giving `recent-documents.json` an
  envelope.
- Adding Kani coverage to the I/O shells. That is the conformance change's
  job, by testing rather than proof.
- The conformance tap itself. Group 1 only provides the site it needs.

## Decisions

### G1. A `Protocol` trait and one `drive` loop in a neutral I/O-free module

The trait and `drive` go in a new module, `services/protocol_drive.rs`:

```rust
pub(crate) trait Protocol {
    type Action: Copy;
    type Outcome;
    type End;
    /// The end `action` denotes, when it is the terminal action.
    fn end(action: Self::Action) -> Option<Self::End>;
    /// Feed the outcome of the last action; get the next one.
    fn advance(&mut self, outcome: Self::Outcome) -> Self::Action;
}

/// Run `protocol` from `first`, executing each action with exactly one
/// `execute` call and feeding its outcome back unchanged.
pub(crate) fn drive<P: Protocol>(
    protocol: &mut P,
    first: P::Action,
    mut execute: impl FnMut(P::Action) -> P::Outcome,
) -> P::End {
    let mut action = first;
    loop {
        if let Some(end) = P::end(action) {
            return end;
        }
        action = protocol.advance(execute(action));
    }
}
```

The five implementations sit **beside their shells**:
- `durable_write.rs` holds `WriteProtocol`, `MoveProtocol`, and
  `RenameProtocol`, whose `advance` is `let (next, a) = self.step(o);
  *self = next; a`.
- `replace/apply.rs` holds `ReplaceFileProtocol`, and `replace/undo.rs` holds
  `ReplaceUndoProtocol`. For these two, `advance` is `step`.

`&mut P` lets the Replace shells read `journal()` and `rearm_needed()` after
`drive` returns. Each shell becomes three parts:
1. `start`;
2. one `drive` call, whose `execute` closure keeps the shell's own resources:
   - the metadata plan, temp file, `FnOnce` writer in an `Option`, and
     `last_error` for writes;
   - the guard, facts, charge, and digests for Replace;
3. a `finish` mapping from the end to the result.

`move_durable`'s `Copy` action calls `copy_durable`, which runs one nested
`drive` of a `WriteProtocol`. The nesting is therefore explicit.

*Why not in `write_protocol.rs`, as first proposed:* that file is a Kani
oracle, and so is `replace/protocol.rs`. Adding unproven generic code there
would make the next proof-strength run mutate code no harness reaches. It
would also force the scheduled `core-journal-and-write` run and the
pull-request `core-replace-protocol` run (verification-runs.md, "Kani"). The
trait describes how a shell runs a core, not a decision of the core, so it
lives with the shells. The cores, and every harness, stay byte-identical.

*Why no new harness:* the proposal added a per-protocol harness to prove that
`drive` terminates within `MAX_ACTIONS`. At 187c6299 each core's existing
harness already proves the core reaches its terminal action within a bound
(Context), and `drive` is exactly the loop those harnesses run. A new harness
would prove nothing new, and adding it to a scheduled shard would cost:
- two `kani.yml` dispatches for the shard budget;
- a PENDING DISPATCH record;
- an `ORACLES` entry;
- a proof-strength run.

`MAX_ACTIONS` is dropped with it.

*What `verify-shell-conformance-against-proven-cores` inherits:*
- **One per-action site for all five shells.** `drive` is the one place
  where every action and every outcome pass, for all five shells. That
  change's D3 tap therefore lives once around the single `execute(action)`
  call, gated as it describes, instead of at a call site in each loop. That
  one site is enough to **observe** every action and outcome, and to
  **inject a crash** (a panic before or after `execute`).
- **What the site does not give.** A generic `drive` sees only `P::Action`
  and `P::Outcome`. It cannot make a shell's `Fail` or `AlreadyExists`
  faithful: the shell builds `last_error`, and the temp file's existence,
  inside its `execute` arms. Typed fault injection therefore stays with the
  conformance change, which designs it in its own revision. Its D3 already
  has a fallible-call seam in `sys.rs`, or it can add a trait hook. Group 1
  promises no more than observation and crash injection.
- **The kind tag.** The conformance change adds it to the trait, for example
  an associated `const KIND`, with the tap. It is not added here, because
  nothing would use it yet.
- **The nested trace.** A nested `Copy` is a nested `drive` call, so the tap
  sees the child run on the same thread, between the parent's `before` and
  `after`. That is its "child trace".
- **Two more shells.** The Replace apply and Undo shells become drivable shells
  with `ReplaceFileProtocol` and `ReplaceUndoProtocol` as their oracles. The
  programme record already says to treat them that way.
- **The marker guard from G6.** It extends the guard's marker list with the
  tap's markers, and does not add a second `strings` step.

*No loop of their own, checked:* a source check covers the five shell files.
In each, the core's `step` may be called only inside that core's
`Protocol::advance` implementation, unless the module documentation records
the fallback exception below. It is a `#[test]` over `include_str!` of the
files, or a rule in an existing policy script, and it fails on the unchanged
tree, where every shell steps its core in its own loop.

*Fallback:* the Replace apply closure moves about 340 lines into one
`FnMut`. Readability may suffer, or the borrow checker may force a worse
shape, for example because `run` is mutably borrowed across `cancel` and
`errors`. If so, either of two refactors is allowed:
- extract the closure's arms into private functions of a small per-file
  context struct;
- keep that one loop, and record why in the module doc and the programme
  record.

In the second case the conformance change taps that loop directly.

*Alternatives considered:*
- A `macro_rules!` shell. It hides control flow.
- Leaving the loops as they are. Five hand copies of termination and outcome
  feedback would stay, each free to diverge, and the conformance change would
  need five tap sites.

### G2. `services/app_data_layout.rs`: an enum of locations, exhaustive consumers

A GTK-free module defines:
- `pub enum AppDataLocation`, with one variant per location. The swept
  directories and families come first, in **today's sweep order**:
  `Root`, `DraftsDir`, `LocalHistoryDir`, `Bookmarks`, `DocumentNotes`,
  `FolderNotes`, `RecoveryQuarantine`, `ReplaceJournalDir`,
  `ReplaceJournalBodies`, `StyleSchemes`, `SetAside`, and
  `FormatUpgradeBackups`. Then come `LegacyWorkspaceNotes` and the files:
  `Workspaces`, `Session`, `DraftManifest`, `SavedSearches`, `SearchHistory`,
  `RecentDocuments`, `MigrationLedger`, `ReplaceJournalManifest`,
  `ReplaceCleanupMarker`, and `RetiredReplaceBackup`;
- `ALL`, in that declaration order;
- `relative(self) -> &'static Path`;
- `kind(self)`, one of:
  - `File`;
  - `Directory`;
  - `LineageFamily { index: "index.json" }`, children one level down;
  - `RunFamily { items: "items" }`;
- `holds_durable_writes(self) -> bool`, which is truthful: it is `true` for
  every directory that any durable write, a format-upgrade rewrite included,
  can create a temp in;
- `sweep(self)`, one of:
  - `Swept`, for a directory or family entry the startup sweep visits;
  - `ParentSwept`, for a file entry, whose temps land in its parent;
  - `NotSwept(reason)`, either because the entry holds no durable writes or
    as a recorded deferral.

`AppDataLayout::new(root)` resolves paths from a root.

The consumers take it over as follows:
- **The sweep** iterates `AppDataLocation::ALL`, keeping the entries whose
  `sweep` is `Swept`, and expands the two families after the flat
  directories, as it does today. Its budget, `MAX_LINEAGES_PER_PASS`, and
  `MAX_BACKUP_RUNS_PER_PASS` are unchanged.
  - **Order is behaviour.** The sweep charges one shared budget in visit
    order. `ALL` reproduces today's flat directory order exactly. The family
    order is **not** `ALL`'s: today the format-upgrade runs (`RunFamily`) are
    expanded before the local-history lineages (`LineageFamily`), although
    `LocalHistoryDir` comes first in `ALL`. The sweep therefore expands the
    families in an explicit `FAMILY_SWEEP_ORDER`, runs then lineages. One
    test pins the whole visit order: flat directories, runs with their
    `items/`, then lineages.
  - **File entries are never swept themselves.** Their `sweep` is
    `ParentSwept`, and a unit test asserts that each one's parent is a
    `Swept` entry.
  - **Coverage is checked.** A unit test asserts that every entry that
    `holds_durable_writes` is `Swept` or `ParentSwept`, or is `NotSwept` with
    a reason. The only such deferral is `LegacyWorkspaceNotes`.
- **The format-upgrade inventory** decides each location through an
  **exhaustive `match`** in `format_upgrade/inventory.rs`, so the layout
  does not depend on `format_upgrade`. The decision is one of:
  - a fixed-JSON scan with a `FormatMetadataKind`;
  - a bounded JSON-directory scan;
  - the draft-body, local-history, or replace-journal special scans;
  - `CoveredBy(location)`, for an entry another entry's scan reads: the
    replace journal's manifest, cleanup marker, and bodies directory are
    covered by the replace-journal scan;
  - `NotInventoried(reason)`, including `Root` (the data home itself, whose
    files are entries of their own).

  Adding a variant then fails to compile until both consumers decide it. The
  scans run in an explicit `INVENTORY_ORDER` list that reproduces today's
  order:
  1. workspaces, session, and the draft manifest;
  2. draft bodies;
  3. saved searches and search history;
  4. the four sidecar directories;
  5. local history, the migration ledger, the replace journal with its
     bodies, and the retired backup.

  A unit test asserts that the list holds exactly the locations whose
  decision is a scan of their own, and that every `CoveredBy` target is in
  the list. The inventory keeps each scan's own relative paths: the replace
  journal's entries and bodies are still recorded under
  `replace-backup-journal/` exactly as today.
- **Each service** replaces its constant and `join` with a layout accessor.
  The public helpers stay as one-line delegations, so the `data_dir()` call
  sites and the tests do not churn. The helpers are `drafts_dir`,
  `set_aside_dir`, `local_history_dir`, `bookmarks_dir`,
  `document_notes_dir`, `folder_notes_dir`, `style_schemes_dir`,
  `journal_dir`, `bodies_dir`, `ledger_path`, and the others.

*Characterization first:* a test written **before** the move records, for a
fixed root, every path each existing helper, constant, and inventory literal
resolves. It also records, for a populated fixture data home (one file in
every scanned location), the ordered `item.path.relative()` list `scan`
returns, because the upgrade backups are named by those paths. After the
move the same test runs against the layout and must be byte-identical.

*Mutation anchor:* `.cargo/mutants.toml` pins
`recent_documents.rs:252:.*load_recent_file` by line. Removing
`RECENT_DOCUMENTS_FILE` moves it, so the group re-keys it against
`make mutants-list` in the same change (testing.md, "Path-keyed gates").

*Recorded exemptions* (`NotInventoried` reasons):
- `recent-documents.json`: unversioned, with no envelope. The loader recovers
  a corrupt file as an empty list, so there is no format to classify. This
  resolves task 2.2 and the former open question. Giving it an envelope is a
  format change, out of scope.
- `style-schemes/`: user-installable GtkSourceView XML, not LushText JSON.
- `recovery-quarantine/`: byte copies of broken files, kept as evidence.
- `drafts/set-aside/`: raw preserved bodies with no metadata.
- `format-upgrade-backups/`: the upgrade's own backups. Inventorying them
  would be circular.

The legacy `workspace-notes/` directory is inventoried but not swept, which
is the existing phase-0 deferral. It records `holds_durable_writes = true`
(a format upgrade rewrites its sidecars in place) and `sweep` =
`NotSwept("phase-0 deferral: legacy folder-note sidecars kept for older
releases")`. Sweeping it would close that deferral, but it changes what the
sweep visits and spends, so it is left to the deferral's own trigger.

*Alternatives considered:*
- A `const` table of structs. Coverage would then depend on a runtime test
  rather than the compiler.
- Moving `data_dir()` in as well. That churns every call site for no safety
  gain.
- Dropping the group. That leaves a drift class that has already caused one
  sweep gap and that #103 had to avoid by hand.

*Kani:* none. No Kani-checked core names a path. `journal_core` and the
write-protocol disk abstraction are path-free.

### G3. A periodic-tick exponential backoff that never touches the hold

The design has three parts:
- **Only the periodic timer retries.** The 5 s timer callback calls a new
  `periodic_autosave_tick`, which does three things in order:
  1. increments `autosave_timer_ticks: Cell<u64>` in the drafts state;
  2. calls `retry_unrestored_copies` for the due entries;
  3. calls `autosave_tick`.

  `autosave_tick` stops calling `retry_unrestored_copies`. The first-dirty
  debounce and the pending rerun therefore no longer retry. Neither has
  anything to do with the copy's failure cause.
- **A pure policy.** `unrestored_copy_retry_delay_ticks(failures: u32) -> u64`
  lives in `ui/window/drafts/policy.rs`. It returns `min(2^(failures-1), 12)`
  for `failures >= 1`, with `UNRESTORED_COPY_MAX_RETRY_TICKS = 12`, and reuses
  `orphan_cleanup_follow_up`'s `saturating` and `min(31)` exponent guard. Each
  retry entry becomes `(DraftEntry, failures, next_due_tick)`.
  `retry_unrestored_copies` attempts only the entries with
  `next_due_tick <= now`, and leaves the rest in the map untouched.
  `next_due_tick` is computed when the failed attempt's completion records
  the entry: the current `autosave_timer_ticks` plus the delay. An attempt
  still in flight is not in the map, so a tick during it starts nothing.
- **The schedule.** The first failure happens in `preserve_unrestored_draft`,
  outside the timer. Its retry is due at the next timer tick. Counted from
  the tick at which that first failure is recorded, and with each attempt
  completing before the next tick, later attempts follow at ticks 1, 3, 7,
  15, 27, 39, and so on, every 12 ticks after the cap.

The hold is untouched. It is taken in `preserve_unrestored_draft` and released
only on `Ok(Some(_))` or `Ok(None)`. The first failure still logs a warning
and publishes the status message, and later failures log at debug level.

`autosave_tick_for_test` drives `periodic_autosave_tick`. That is a superset
of today's behaviour: a timer tick with no due retries is the same autosave
pass. The existing failed-copy test needs at most three ticks inside its 10 s
`wait_until` loops, so it stays green unchanged.

`DraftEvidence` keeps `unrestored_copy_retries` (the count) and gains
`unrestored_copy_failures` and `unrestored_copy_next_due_ticks` (each sorted
by draft id), and `autosave_timer_ticks`. The accessor computes them and
drops the retry map's `Ref` before building the struct
(workflow-convention.md, evidence surface invariants). A test observes an
attempt as a change of that entry's failure count.

*Threshold:* `UNRESTORED_COPY_MAX_RETRY_TICKS` is a time cap, one minute of
5 s ticks. It gets a `docs/thresholds.md` row as `out: time`, like
`FIRST_DIRTY_AUTOSAVE_DEBOUNCE_MS` in the same file, and its rustdoc
`Kind:` line, so `make check-thresholds` passes.

*Why a cap of 12, not 60:* while an id is held, autosave does not write the
user's **newer** edits to that id. Its draft protection is suspended until the
copy exists. Recovering within a minute of the cause clearing (a disk freed, a
directory made writable) therefore outweighs the cost: one worker a minute
reading at most 64 MiB, which is already 12 times less I/O than today in
steady state. This resolves the former open question.

*Alternatives considered:*
- Counting every `autosave_tick` call. The spacing would then shrink to under
  a second while the user types, which defeats the backoff.
- A wall-clock due time (`glib::monotonic_time`). It is equivalent at the
  5 s cadence, but needs a clock seam for the widget test. The tick count is
  deterministic.
- A separate `glib::timeout` per retry. That adds timer ownership and
  teardown.
- Retrying on window focus. That is unrelated to the cause.

*Kani:* none. `journal_core` is unchanged, and its model already treats a
failed copy as keeping the hold. Retry timing is not a modelled action, and
delaying a retry only removes attempts, which cannot violate S1–S4.

### G4. `WriteLabel` replaces `tmp_tag: &str` down to the temp name

The signatures change as follows:
- `temp_name::format(file, label: WriteLabel, pid, seq)`;
- `unique_temp_path(path, label)`;
- the private `atomic_write_bytes`;
- the four `*_classified` entry points and
  `atomic_write_stream_with_metadata`;
- `copy_durable` and `move_durable`.

`write.rs` stops calling `as_str()`. `kill_point::reach` keeps
`(window, key: &str)`, because its narrowing key is a write label for the
durable-write and Replace windows but a **draft id** for
`StalePreservedBeforeRetire` and `DraftBodyBeforeCommit`. The shell passes
`label.as_str()`. Its documentation names both key kinds.

Tests that pass `"test"` (15 sites) or `"json"` switch to an existing
constant. No test-only label is added, because `KNOWN` must be the production
set the sweep trusts. The `"not-a-label"` negative case moves to the `parse`
side, as a crafted name, because the builder can no longer express it.

*Characterization first:* for every one of the 11 `WriteLabel::KNOWN`
entries, record the exact temp name the builder emits for fixed inputs before
the change, and assert it is identical after. The leftover sweep's predicate
reads these names from disk, left by older builds too.

*Kani:* none. Temp names are not modelled, and `durable_write.rs` is not an
oracle.

*Mutation:* `.cargo/mutants.toml` pins `durable_write.rs:131`, `:504`, and
`:515` by line, and all three are already stale at 187c6299. `FileWriteLock`
no longer exists, and the non-Unix `MetadataPlan::probe` and `create_mode`
stubs sit at 743 and 754. Under the pre-existing blockers rule, whichever of
G1 and G4 lands first does three things:
- re-keys or deletes each entry against a mutant `make mutants-list` actually
  generates;
- also re-checks the two unanchored `durable_write` entries (the
  `FileWriteLock` drop, and the `!=` in `rename_durable`);
- re-checks again after the other group.

### G5. `WritableDraftCandidate` owns its token; registration is the only conversion

`seams.rs` changes in three ways:
- `DirtyDraftCandidate` loses `registered`.
- A new `WritableDraftCandidate` holds the same fields plus
  `registered: RegisteredDraft`.
- The only constructor is
  `DirtyDraftCandidate::into_writable(self, token: RegisteredDraft) -> Result<WritableDraftCandidate, (Self, RegisteredDraft)>`.
  It checks `token.draft_id() == self.draft_id`, so a mismatched pairing is
  refused rather than written.

`register_new_draft_ids_then` hands `then` a
`(Vec<WritableDraftCandidate>, Vec<String>)`. Each token comes either from
`RegisteredDraft::without_registration`, when the journal core says no
registration is needed, or from a committed registration. The partition
keeps `journal_core::may_write_after_registration(required, committed)` as
its gate: a candidate becomes writable only when that gate admits it **and**
it holds its token. Every other candidate is refused, keeping its id in the
refused list, exactly as today. Both
`drive_dirty_draft_pipeline` and `drive_close_draft_pipeline` take
`Vec<WritableDraftCandidate>`, and both `ok_or_else` sites disappear.

*Scope note, the synchronous close flush:* `journal.rs`'s flush makes and
consumes its `Option<RegisteredDraft>` in one block per editor. No stage
boundary lies between the registration call and the write, and `None` there
is a real outcome (no token minted), recorded as a registration error. A
typestate would buy nothing, so the flush stays as it is.

`journal_core`, `model/draft.rs`, and `RegisteredDraft`'s minting surface
are **not edited**. The group only moves the decision's result into the type.
G2 edits `draft_service.rs` (path helpers) for its own reasons, which reaches
no harness. The review checks
that `RegisteredDraft` gains no `Clone`.

*Characterization first:* before the refactor, ensure tests exist, adding any
that are missing, and confirm they pass on the unchanged tree, for:
- the first-body registration ordering
  (`test_new_file_backed_drafts_are_registered_before_their_first_body_write`);
- a registration commit failure that leaves the candidate dirty and
  retryable, with no body written;
- a steady-state, already registered id that needs no registration;
- the asynchronous close pipeline registering before writing.

*Kani:* none. The proposal ran `core-journal-and-write` and
`core-second-writer` "because the token surface is adjacent". Under
verification-runs.md a shard runs only when a harness or a module it reads
changes. The harnesses import only `journal_core` and `model::draft`, neither
of which changes. If an implementation finds it must edit `journal_core.rs`
or `model/draft.rs`, it runs both scheduled shards once, detached, after the
final code.

### G6. One role table in the smoke driver, kill-point victims, an automated marker guard

The driver resolves `ROLES` once from the arguments:
- `seed` is always `--binary`. It covers launches that prepare state and are
  stopped normally.
- `victim` is `--kill-point-binary` when supplied, and `--binary` otherwise.
  It covers the launch that is killed, by signal or at a kill point.
- `relaunch` is always `--binary`. It covers the launch that must recover the
  user's work.

`launch_app` takes `role=` instead of `binary=`. Each of its 11 calls names
one:
- `before-crash` is `victim`;
- `after-relaunch`, `relaunch-*`, and the Undo relaunch are `relaunch`;
- `seed-*` is `seed`;
- `kill-*` is `victim`.

`launch_app` already runs `env.pop("LUSHTEXT_KILL_AT")` before the overrides,
so the external-signal victim runs the kill-point binary inert. Each phase's
log directory records its role and binary path in
`logs/lushtext-<phase>.role`.

Before any scenario, a guard reads the bytes of the ordinary binary, and of
`target/release/lushtext` if it exists. It fails if either contains
`LUSHTEXT_KILL_AT` or the abort prefix `LUSHTEXT_KILL_AT: aborting inside`,
and writes `assertions/no-kill-points.json` with each binary, its size, and
the markers found. The guard keeps its marker list in one place, which the
conformance change extends.

`scripts/check-kill-point-gating.sh` is added to `make check-policy`. It
keys on the places that can gate code:
- every `feature = "crash-kill-points"` in Rust sources must be in
  `services/kill_point.rs`;
- the feature may be declared only by the two `Cargo.toml` lines (the core
  feature, and the binary's forward);
- `--features crash-kill-points` may appear only in the Makefile's smoke build
  line.

Mentions in comments, scripts, and docs (three Makefile lines, both smoke
scripts, and the check itself) are not gating sites, and are allowed.

A second gated site would make the kill-point victim diverge from the ordinary
binary.

*Alternatives considered:*
- A distinct binary name in the shared target directory. That needs a
  separate package, whose feature unification would leak kill points into
  `cargo build --workspace`.
- Keeping the ordinary victim in the signal scenario. That forgoes the
  inertness evidence and saves nothing.

*Kani:* none.

## Data-safety audit (groups that touch persistence)

**G1** rewrites the shell of every durable write and of Replace All and
Undo. The audit covers:
- the action-to-call mapping: exactly one call per action, and the outcome
  passed back unchanged;
- `last_error` carriage into `BeforeRename` and `AfterRename`;
- temp removal on failure;
- the `DurableRenamedBeforeDirSync` kill point staying inside `SyncDir`;
- the Replace kill points staying at their actions;
- the guard dropped after `drive` returns, as today.

Evidence:
- the durable-write unit and fault-injection tests;
- the Replace All apply, undo, and crash tests (`replace/tests.rs`);
- the three Kani-consolidation characterization tests;
- `make crash-recovery-smoke` (the durable-write and four Replace kill-point
  windows).

**G2** touches every app-data path. The hazard is a wrong relative path. The
user's data would not be deleted, but it would become invisible: the next
session or draft write would create fresh state beside it, and orphan cleanup
would inspect the wrong directory. Mitigations:
- the byte-identical characterization table;
- no rename of any location;
- orphan-cleanup inspection and execution both resolving through the same
  owner;
- the sweep's provably-ours predicate, unchanged.

Evidence: the full test suite and `make crash-recovery-smoke`.

**G3** concerns the hold semantics. The audit confirms that:
- no path releases the hold on an error outcome;
- Discard, close flush, and window teardown behave exactly as before while an
  id is held;
- the backoff state is dropped with the window and never persisted.

Evidence: the existing failed-copy test unchanged, plus the new
failing-first cadence and recovery tests.

**G4** concerns temp names on disk. Evidence: the per-label byte-identical
characterization, and the leftover predicate tests, including names written
by an older build.

**G5** concerns the body-write gate. The audit confirms that every refused
candidate stays draft-dirty and retryable, and keeps its failure accounting.
It also confirms that no close-pipeline candidate is dropped silently, and
that a token cannot be paired with the wrong id. Evidence: the
characterization tests above.

## Kani, proof strength, axioms, timing

- **Kani: none (no proven module touched).** Every group confirms with
  `git diff --stat "$(git merge-base HEAD origin/main)" -- <paths>` that the
  four oracle modules, `model/draft.rs` (which the draft harness reads), and
  every `kani_proofs.rs` are unchanged. The merge base keeps a later `main`
  from showing files this branch never touched.
  If one changes after all:
  - `write_protocol.rs` or `journal_core.rs` means
    `setsid nohup scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=core-journal-and-write`,
    run once, detached, on the final code (`core-second-writer` and
    `core-multi-window` too for `journal_core`);
  - `replace/protocol.rs` means
    `scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=core-replace-protocol`.

  No shard's harness set changes, so no budget or `kani.yml` dispatch is owed.
- **Proof strength: not owed.** No harness and no asserted property changes.
- **GTK axioms: none.** G3 uses the existing `glib::timeout_add_local` timer
  and adds no reliance on toolkit behaviour. No other group touches GTK. Ids
  A134 and later stay unused.
- **Timing: no timing verdict.** No group changes a hot path:
  - G1's `drive` is generic, and monomorphizes to the same loop with one
    closure call per action. The actions are syscalls (`fsync`, `rename`) or
    streamed file passes, so a call's cost is nothing beside them.
  - G2 resolves the same paths.
  - G3 removes work.
  - G4 is a type change, with `as_str` at the same points.
  - G5 removes a branch.
  - G6 is test tooling.

  The Replace All responsiveness lane cases are untouched, because the shell
  executes the same actions in the same order.

## Risks / Trade-offs

- [G1: the Replace apply shell's 340-line `execute` closure hurts readability
  or meets the borrow checker] → Use the fallback above: arms as functions of
  a per-file context, or that loop kept, with the reason recorded.
- [G2: exhaustive matches add ceremony for every new location] → That
  ceremony is the point. It is the only moment the sweep and upgrade coverage
  decision is forced.
- [G3: a one-minute cap still delays recovery after the cause clears] → The
  body is safe the whole time, and the delay is at most one cap interval. The
  cap is a named constant in pure policy, so it is mutation-tested.
- [G3: an id held through many first-dirty passes is never retried between
  timer ticks] → That is intended. The timer keeps running while the window
  lives.
- [G5: typestate across GTK callbacks may add clones] → The token and
  candidate are moved, not cloned.
- [G6: the kill-point victim diverges if the feature ever gates more than
  `reach`] → The gating policy check fails first.
- [Applying groups in parallel conflicts in `durable_write.rs` or `drafts/`]
  → Apply the stated pairs sequentially. The UI-prefix groups (5, then 3)
  land last.

## Migration Plan

Each group is one commit, or a small series, revertible alone. The order is
1, 4, 2, 6, then 5 and 3. The last two touch the UI prefixes. A commit that
touches those prefixes needs the three smokes current, because the proofs
digest the files' contents (testing.md, "Files under the UI prefixes need the
three smokes"). Groups 5 and 3 therefore land in one commit, made after the
three smokes run on that tree. The commit message and the N8 record still
describe them as two groups. If they must land separately, the three smokes
run again before each commit. There is no on-disk format, path, or
GSettings change, so rollback needs no data migration. N8 in
`docs/next/formal-verification-next.md` is updated per group as it lands, and
the change is archived once all six are applied, or the unapplied ones are
explicitly re-deferred there.

## Open Questions

None. The G3 cap (12 ticks) and G2's `recent-documents.json` question were
decided in this revision, with the rationale above.
