## Context

`consolidate-formal-verification-on-kani` made the draft journal, the durable
write, move, and rename protocols, and the viewport-slice loop Kani-checked. Its
two `/simplify` passes proposed six altitude redesigns that it deliberately did
not take, and N8 of `docs/next/formal-verification-next.md` lists them. Each
one closes a gap between what the verified core guarantees and what the code
around it merely follows by convention. The groups below are **independent**.
Each has its own rationale, risk, Kani re-run set, data-safety audit, and
documentation, so the change can be applied one group at a time. Two pairs
edit the same files and should be applied one after the other, never
interleaved:
- groups 1 and 4 (`durable_write.rs`);
- groups 3 and 5 (`ui/window/drafts/`).

These are the current facts each group starts from, all read at 170212d9.

**Group 1.** `services/durable_write.rs` has three shell loops of the same
shape: start, then loop over match action, one backend call, record
`last_error`, step. They are `atomic_write_*` (about 100 lines),
`run_rename_protocol`, and `move_durable`. Each re-implements termination and
error carriage by hand. `WriteProtocol::step`, `MoveProtocol::step`, and
`RenameProtocol::step` are inherent `const fn`s, and the Kani harnesses in
`write_protocol/kani_proofs.rs` call them directly.

**Group 2.** App-data names are defined in three places, and nothing checks
that they agree:
1. Per-service constants. There are about 17, including `DRAFTS_DIR`,
   `SET_ASIDE_DIR`, `LOCAL_HISTORY_DIR`, `BOOKMARKS_DIR`, `STYLE_SCHEMES_DIR`,
   `QUARANTINE_DIR`, `FORMAT_UPGRADE_BACKUP_DIR`, and `JOURNAL_DIR`, plus the
   `*_FILE` names.
2. The startup sweep's hand list in `app_data_leftovers.rs`.
3. The format-upgrade inventory, which re-spells literals such as
   `"drafts/manifest.json"`, `"bookmarks"`, `"local-history"`, and
   `"replace-backup-journal"` in `format_upgrade/inventory.rs`.

The inventory does not scan `recent-documents.json`, `style-schemes/`,
`recovery-quarantine/`, `drafts/set-aside/`, or `format-upgrade-backups/`, and
nothing records why. Root resolution is `json_store::data_dir()`
(`LUSHTEXT_DATA_DIR` or XDG), with 247 call sites.

**Group 3.** When the set-aside copy of an unrestored body fails,
`journal.rs::attempt_unrestored_copy` keeps the restore hold, which is correct
and matches the Kani model. It re-inserts the entry into
`unrestored_copy_retries`, and `autosave_tick` (every 5 s) retries **every**
entry **every** tick. A persistent failure, such as a full disk or a
read-only data home, therefore costs one blocking worker per id every
5 seconds, indefinitely. It also competes for the eight `spawn_blocking_then`
slots with saves and draft writes.

**Group 4.** `WriteLabel` is a closed set. `filesystem/write.rs` converts it
back with `label.as_str()`, and about eight `durable_write` functions,
`unique_temp_path`, `temp_name::format`, and `kill_point::reach` then carry
`tmp_tag: &str`. `temp_name`'s own test calls
`format("a.txt", "not-a-label", …)`, so a name the sweep will never recognise
is still expressible.

**Group 5.** `DirtyDraftCandidate.registered: Option<RegisteredDraft>` is
filled in by `register_new_draft_ids_then`. Both body-write sites
(`autosave_execution.rs` about line 404, and the close flush at about line 660)
then do `registered.ok_or_else(|| "recovery metadata was not registered")`, a
runtime check for something the pipeline's stage order already guarantees.

**Group 6.** `crash-recovery-smoke-driver.py` launches with
`binary or args.binary`. The external-signal scenarios use the ordinary binary
as the victim, the three kill-point scenarios pass
`binary=args.kill_point_binary`, and every relaunch uses the ordinary binary.
"No kill-point code in a release build" was checked once, by hand, with
`strings` (programme record, K5).

## Goals / Non-Goals

**Goals:**
- Each group lands alone, with no behaviour change except in group 3 (retry
  cadence) and group 6 (a stronger smoke).
- Every persisted path, temp name, and on-disk byte stays identical. Each
  refactor that touches persistence is proved identical by a characterization
  test captured before the move.
- Every Kani harness that covers a touched core stays green, and each group
  names its re-run set.

**Non-Goals:**
- An inter-process data-directory lock (N6).
- The N5 `fixture::write_body` gate.
- A size bound on `drafts/set-aside/` (N10).
- Moving root resolution out of `json_store::data_dir()`.
- Renaming any on-disk location.
- Adding Kani coverage to the I/O shells.

## Decisions

### G1. A `Protocol` trait and one `drive` function, both in the I/O-free core

The trait and driver go in `services/filesystem/write_protocol.rs`:

```rust
pub trait Protocol: Copy {
    type Action: Copy;
    type Outcome;
    type Terminal;
    /// Proven upper bound on actions before a terminal (see harness below).
    const MAX_ACTIONS: usize;
    fn step(self, outcome: Self::Outcome) -> (Self, Self::Action);
    fn terminal(action: Self::Action) -> Option<Self::Terminal>;
}

pub fn drive<P: Protocol>(
    (protocol, action): (P, P::Action),
    execute: impl FnMut(P::Action) -> P::Outcome,
) -> P::Terminal;
```

Each implementation delegates `step` to the existing inherent `const fn`. The
inherent functions stay, because a trait method cannot be `const` on stable
Rust and the harnesses call them. The three shells shrink to their
per-action `execute` closure, which keeps its own resources (the metadata
plan, the temp file, the `FnOnce` content writer taken from an `Option`, and
`last_error`), plus a `finish` mapping from the terminal to the result.

Because `drive` is I/O-free, a new cheap harness can check it. For each
protocol, `drive` with a `kani::any()` executor reaches a terminal within
`MAX_ACTIONS`, which proves loop termination for all three shells at once.
Today termination is an unstated property of three hand loops. The harness is
added to `core-journal-and-write`, whose budget has room, because the existing
write harnesses total about 8 s.

*Alternatives considered:*
- A `macro_rules!` shell. It hides control flow and cannot be Kani-checked.
- Leaving the three loops as they are. Termination and error carriage would
  stay three hand-written, unproven copies, each free to diverge from the
  others.

*Rationale:* one loop, proven to terminate, whose only per-protocol code is
the execute and finish mappings.

*Kani re-run:* all `services::filesystem::write_protocol::kani_proofs::`
harnesses, plus the new termination harness. `make check-kani-shards` must
pass.

### G2. `services/app_data_layout.rs`: an enum of locations, one table, exhaustive consumers

A GTK-free module defines:
- `pub enum AppDataLocation`, with one variant per location: `Root` (the
  data home itself, which the sweep visits today because the top-level JSON
  files are durably written there), `Workspaces`,
  `Session`, `DraftsDir`, `DraftManifest`, `SetAside`, `LocalHistoryDir`,
  `Bookmarks`, `DocumentNotes`, `FolderNotes`, `LegacyWorkspaceNotes`,
  `RecoveryQuarantine`, `ReplaceJournalDir`, `ReplaceJournalManifest`,
  `ReplaceCleanupMarker`, `RetiredReplaceBackup`, `SavedSearches`,
  `SearchHistory`, `RecentDocuments`, `MigrationLedger`, `StyleSchemes`, and
  `FormatUpgradeBackups`;
- `ALL`;
- `relative(self) -> &'static Path`;
- `kind(self)`, one of `File`, `Directory`, `LineageFamily` (children one
  level down), or `RunFamily { items: "items" }`;
- `holds_durable_writes(self) -> bool`.

`AppDataLayout::new(root)` resolves paths from a root.

The consumers take it over as follows:
- **The sweep** iterates `AppDataLocation::ALL` filtered by
  `holds_durable_writes`, and expands the two families. It keeps its current
  budget and bounds unchanged.
- **The format-upgrade inventory** maps each location to its scan through an
  **exhaustive `match`**: a fixed-JSON scan with a `FormatMetadataKind`, a
  bounded directory scan, the draft-body or local-history or replace-journal
  special scans, or `NotInventoried(reason)`. Adding a variant then fails to
  compile until both consumers decide it. The layout module does not depend
  on `format_upgrade`, so no dependency cycle forms.
- **Each service** replaces its constant and `join` with its layout accessor.
  The existing public helpers (`drafts_dir`, `set_aside_dir`,
  `bookmarks_dir`, and so on) stay as one-line delegations, so the 247
  `data_dir()` sites and the tests do not churn.

*Characterization first:* a test written **before** the move records, for a
fixed root, every path each existing helper and inventory literal resolves.
After the move the same test runs against the layout and must be
byte-identical.

*Recorded exemptions:*
- `recent-documents.json`, `style-schemes/`, `recovery-quarantine/`,
  `drafts/set-aside/`, and `format-upgrade-backups/` each get
  `NotInventoried` with its reason. `recent-documents.json` is the one to
  check (task 2.2): if it carries a versioned envelope, its absence from the
  inventory is a pre-existing gap. Under the pre-existing blockers rule it is
  then added to the inventory behind a failing-first test.
- The legacy `workspace-notes/` directory stays unswept, which is the
  existing phase-0 deferral, and records `holds_durable_writes = false` with
  that reason.

*Alternatives considered:*
- A `const` table of structs. Coverage would then depend on a runtime test
  rather than the compiler.
- Moving `data_dir()` in as well. That churns 247 sites for no safety gain.

*Kani re-run:* none. No Kani-checked core names a path. `journal_core` and the
write-protocol disk abstraction are path-free.

### G3. A tick-counted exponential backoff that never touches the hold

The design has three parts:
- A pure `unrestored_copy_retry_delay_ticks(failures: u32) -> u32` in
  `ui/window/drafts/policy.rs`. It returns 1, 2, 4, …, capped at
  `UNRESTORED_COPY_MAX_RETRY_TICKS = 60`, which is 5 minutes at the 5 s
  tick. This is the same shape as `orphan_cleanup_follow_up`, whose
  `saturating_mul` and `min(31)` exponent guard it reuses.
- The drafts state gains a monotonically increasing `autosave_tick_count`.
  Each retry entry becomes `(DraftEntry, failures, next_due_tick)`.
- `retry_unrestored_copies` attempts only the entries that are due, and
  re-inserts the rest untouched.

The hold is left exactly as it is: taken in `preserve_unrestored_draft`, and
released only on `Ok(Some(_))` or `Ok(None)`. The status message is still
published once, on the first failure.

`DraftEvidence` gains the next due tick per retry, alongside the existing
count. The workflow's matrix row is re-derived under the convention.

*Alternatives considered:*
- A separate `glib::timeout` per retry. It adds timer ownership and teardown,
  whereas the tick already exists and is torn down with the window.
- Retrying on user attention (window focus). That is noisy and unrelated to
  the cause.

*Kani re-run:* none required. `journal_core` is unchanged, and its model
already treats a failed copy as keeping the hold. Retry timing is not a
modelled action, and delaying a retry cannot violate S1–S4, because it only
removes attempts. If any line of `journal_core.rs` changes while the group is
applied, re-run `core-journal-and-write`.

### G4. `WriteLabel` replaces `tmp_tag: &str` end to end

The signatures change as follows:
- `temp_name::format(file, label: WriteLabel, pid, seq)`;
- `unique_temp_path(path, label)`;
- every `durable_write` entry point and `copy_durable` / `move_durable`;
- `kill_point::reach(window, label: WriteLabel)`, which compares
  `label.as_str()` against the environment value.

`write.rs` stops calling `as_str()`. Tests that pass literal tags (`"json"`,
`"test"`) switch to the matching constant. The negative case in `temp_name`
(`"not-a-label"`) moves to the `parse` side, as a crafted name, because the
builder can no longer express it.

*Characterization first:* for every `WriteLabel::KNOWN` entry, record the
exact temp name the builder emits for fixed inputs before the change, and
assert it is identical after. The leftover sweep's predicate reads these
names from disk left by older builds.

*Kani re-run:* none required, because temp names are not modelled and
`CreateTemp`'s `AlreadyExists` is an abstract outcome. The `write_protocol`
harnesses (about 8 s) are run anyway because the shell file changes.

*Mutation:* `.cargo/mutants.toml` pins `durable_write.rs:131`, `:504`, and
`:515` by line, and these shift. Re-verify each against a generated mutant
and re-key or delete it, as the configuration requirement demands. This also
applies to G1.

### G5. `WritableDraftCandidate` owns its token; registration is the only conversion

`seams.rs` changes in three ways:
- `DirtyDraftCandidate` loses `registered`.
- A new `WritableDraftCandidate` holds the same fields plus
  `registered: RegisteredDraft`.
- The only constructor is
  `DirtyDraftCandidate::into_writable(self, token: RegisteredDraft) -> Result<WritableDraftCandidate, (Self, RegisteredDraft)>`.
  It checks `token.draft_id() == self.draft_id`, so a mismatched pairing is
  refused rather than written.

`register_new_draft_ids_then` returns `(Vec<WritableDraftCandidate>, refused)`.
The token comes from `RegisteredDraft::without_registration` when the journal
core says no registration is needed, or from a committed registration.
`drive_dirty_draft_pipeline` and the close flush take
`Vec<WritableDraftCandidate>`, and the two `ok_or_else` sites disappear.

`journal_core` decides whether a registration is required and whether a write
may follow it. Its API and `RegisteredDraft`'s minting rules do not change.
The group only moves the decision's **result** into the type.

*Characterization first:* before the refactor, ensure tests exist, adding any
that are missing, and confirm they pass on the unchanged tree, for:
- the first-body registration ordering
  (`test_new_file_backed_drafts_are_registered_before_their_first_body_write`);
- a registration commit failure that leaves the candidate dirty and
  retryable, with no body written;
- a steady-state, already registered id that needs no registration;
- the close-flush path registering before writing.

*Kani re-run:* `core-journal-and-write` (the journal harnesses) and
`core-second-writer`. The second-writer counterexample depends on the token a
registration mints, so the `should_panic` must still find it. Run both even
though `journal_core` is untouched, because the token surface in
`draft_service.rs` is adjacent.

### G6. One role table in the smoke driver, kill-point victims, and an automated marker guard

The driver gains `ROLE_BINARIES = {victim, relaunch}`, resolved once from the
arguments:
- `relaunch` is always `--binary`.
- `victim` is `--kill-point-binary` when supplied, and `--binary` otherwise.

Every `launch_app` call names a role instead of a path. The existing
`env.pop("LUSHTEXT_KILL_AT")` already guarantees the external-signal
scenarios launch the kill-point victim inert. Each scenario's artifacts record
the role and binary path.

Before any scenario, a guard reads the ordinary binary's bytes, and
`target/release/lushtext` if it exists, and fails if either contains
`LUSHTEXT_KILL_AT` or the abort message. It writes
`assertions/no-kill-points.json`. The `make crash-recovery-smoke` target and
its build are otherwise unchanged.

The kill-point feature gates only `kill_point::reach`, which task 6.1 checks
by grep. So using the kill-point binary as victim in the signal scenarios
tests the same code the ordinary victim did, and additionally shows it is
inert.

*Alternatives considered:*
- Building the kill-point binary into the shared target directory under a
  distinct binary name. That needs a separate package whose feature
  unification would leak kill points into `cargo build --workspace`, which is
  rejected on the production-build requirement.
- Keeping the ordinary victim in the signal scenarios. That forgoes the
  inertness evidence at no saving.

*Kani re-run:* none.

## Data-safety audit (groups that touch persistence)

**G1** — shell rewrite of every durable write. The audit covers:
- the action-to-backend mapping (exactly one call per action, outcome
  unchanged);
- `last_error` carriage into `BeforeRename` and `AfterRename`;
- temp removal on failure;
- the kill point staying inside `SyncDir`.

Evidence: the existing durable-write unit and fault-injection tests, the three
characterization tests from the Kani consolidation, the `write_protocol`
harnesses plus the new termination harness, and `make crash-recovery-smoke`
(the `durable-renamed-before-dirsync` window).

**G2** — every app-data path. The hazard is a wrong relative path. The user's
data would not be deleted, but it would become invisible: the next session or
draft write would create fresh state beside it, and orphan cleanup would
inspect the wrong directory. Mitigations:
- the byte-identical characterization table;
- no rename of any location;
- the orphan-cleanup inspection and execution both resolving through the
  same owner;
- the sweep's provably-ours predicate is unchanged.

Evidence: the full integration and widget suites, and
`make crash-recovery-smoke`.

**G3** — hold semantics. The audit confirms that:
- no path releases the hold on an error outcome;
- Discard, close-flush, and window teardown behave exactly as before while an
  id is held;
- the backoff state is dropped with the window, never persisted.

Evidence: `test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft`
stays green, plus a new failing-first widget test for the cadence.

**G4** — temp names on disk. Evidence is the per-label byte-identical
characterization, plus the leftover predicate tests, including names written
by an older build.

**G5** — the body-write gate. The audit confirms that every refused
candidate:
- stays draft-dirty;
- stays retryable;
- keeps its failure accounting.

It also confirms that the close flush cannot drop a candidate silently, and
that a token cannot be paired with the wrong id. Evidence: the
characterization tests above plus the two Kani shards.

## Risks / Trade-offs

- [G1: a generic `drive` complicates the write shell's borrow of its `FnOnce`
  writer and temp file] → The execute closure owns them through `Option`s
  exactly as today. If the borrow checker forces a less readable shape for
  the write shell, apply G1 to move and rename only and record why.
- [G2: exhaustive matches add ceremony for every new location] → That
  ceremony is the point. It is the only moment the sweep and upgrade coverage
  decision is forced.
- [G3: a cap of 5 minutes delays recovery after the cause clears] → The body
  is safe the whole time, and the cost is at most one cap interval before the
  copy exists. The cap is a named constant in pure policy, so it is
  mutation-tested.
- [G5: typestate across GTK callbacks may add clones] → The token and
  candidate are moved, not cloned; the review checks that no `Clone` derive
  is added to `RegisteredDraft`.
- [G6: the kill-point victim diverges if the feature ever gates more than
  `reach`] → Task 6.1's grep becomes a small policy check wired into
  `make check-policy`. It asserts that `crash-kill-points` appears only in
  `kill_point.rs`, the two `Cargo.toml` feature lines, and the smoke build
  command.
- [Applying groups in parallel conflicts in `durable_write.rs` or `drafts/`]
  → Apply the stated pairs sequentially.

## Migration Plan

Each group is one commit, or a small series, revertible alone. There is no
on-disk format, path, or GSettings change, so rollback needs no data
migration. N8 in `docs/next/formal-verification-next.md` is updated per group
as it lands, and the change is archived once all six are applied, or the
unapplied ones are explicitly re-deferred there.

## Open Questions

- G3 cap: is 60 ticks (5 minutes) right? A shorter cap, such as 12 ticks
  (1 minute), recovers faster after a transient full disk, at 5 times the
  worker cost during a persistent failure.
- G2: should `recent-documents.json` join the format-upgrade inventory
  regardless of task 2.2's finding, for uniformity?
