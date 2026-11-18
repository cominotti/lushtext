Each numbered group can be applied on its own. Apply them in the order
0, 1, 4, 2, 6, 5, 3, 7:
- groups 1 and 4 both edit `durable_write.rs`, so one follows the other;
- groups 5 and 3 both edit `ui/window/drafts/`, so they follow each other,
  and they come last because they touch the UI prefixes. A commit that
  touches those prefixes needs the three smokes current (testing.md), so
  groups 5 and 3 are committed together, once, after task 7.3 (design,
  "Migration Plan"). If they must land separately, run 7.3 before each of the
  two commits.

Every group ends with its own gates and doc sync. Every heavy command goes
through `scripts/heavy.sh`, one command per call, in the forms written here.
`make test`, `make kani`, and `make proof-strength` use `--envelope 16G`, and
`make mutants-*` uses `--envelope 36G`. Commits use
`scripts/heavy.sh --mem none -- git commit -F <file>`. Read
`$H/apply-verification-altitude-redesigns/COORDINATOR-NOTE.txt` before each
group's heavy commands.

## 0. Start

- [x] 0.1 Acquire the implementation slot (parallel-agent-work.md,
  "Implementation slots"), and release it however the stage ends. If
  `origin/main` moved past the branch point, merge it into the branch; never
  rebase.
- [x] 0.2 Record the Kani baseline:
  `git diff --stat "$(git merge-base HEAD origin/main)" -- crates/lushtext-core/src/services/filesystem/write_protocol.rs crates/lushtext-core/src/services/content_search/replace/protocol.rs crates/lushtext-core/src/services/draft_service/journal_core.rs crates/lushtext-core/src/services/draft_service/set_aside_retention.rs crates/lushtext-core/src/model/draft.rs ':(glob)**/kani_proofs.rs'`
  must print nothing. The merge base keeps a `main` that moved later from
  showing files this branch never touched. Re-run it at the end of every
  group. If it ever prints
  a file, run the shard that design "Kani, proof strength, axioms, timing"
  names for it, once, on the final code, and record the time.

## 1. One `Protocol` driver for every proven-core shell (design G1)

- [x] 1.1 Characterize the shells on the unchanged tree. Confirm these tests
  pass:
  - the durable-write unit and fault-injection tests;
  - the three Kani-consolidation characterization tests (content, rename,
    and probe failure);
  - the Replace All apply, undo, and crash-recovery tests in
    `content_search/replace/tests.rs`.

  Add the missing coverage, failing-first where a test can fail. Each must
  pass on the unchanged tree:
  - `move_durable` when its source removal fails, so the destination is kept
    and the source too;
  - `rename_durable` when a cross-directory destination sync fails
    (`AfterRename`);
  - a Replace apply file whose entry removal fails during cleanup, so the
    body is kept;
  - a journal shrink (`search_backup::shrink_journal_to`) whose retired
    entry cannot be removed, so its body is kept, and one whose entry and
    body are both removed, entry first.

  Command: `scripts/heavy.sh -- make test-unit`. It runs the library tests,
  these included, through the temp scope.
- [x] 1.2 Add `services/protocol_drive.rs`, holding the `Protocol` trait
  (`Action`, `Outcome`, `End`, `end`, `advance`) and `drive`, and register it
  in `services/mod.rs`. Unit-test `drive` with a small counting protocol for
  four properties:
  - it stops at the terminal action without executing it;
  - it executes each other action exactly once, in order;
  - it feeds every outcome back unchanged;
  - it supports a nested `drive` from inside `execute`.
- [x] 1.3 Implement `Protocol` beside each shell, delegating to the unchanged
  inherent `start` / `step`:
  - in `durable_write.rs`: `WriteProtocol`, `MoveProtocol`, and
    `RenameProtocol`;
  - `ReplaceFileProtocol` in `replace/apply.rs`;
  - `ReplaceUndoProtocol` in `replace/undo.rs`;
  - a private `ShrinkRun(ShrinkAction)` in `search_backup.rs`, whose
    `advance` calls the free `shrink_step` with the action it keeps.

  Do not edit `write_protocol.rs` or `replace/protocol.rs`.
- [x] 1.4 Rewrite the six loops as `start`, then one `drive` with an
  `execute` closure, then a `finish` mapping. The six are:
  - `atomic_write_stream_with_metadata`;
  - `run_rename_protocol`;
  - `move_durable`;
  - the apply loop of `replace/apply.rs`;
  - the entry loop of `replace/undo.rs`;
  - the per-entry shrink loop of `search_backup::shrink_journal_to`, keeping
    its warning per failed removal.

  Keep each of these exactly where it is:
  - `last_error` carriage;
  - temp removal;
  - every `kill_point::reach`: the `DurableRenamedBeforeDirSync` window in
    the `SyncDir` arm, and `ReplaceBodyBeforeEntry` in the apply loop's
    `WriteBody` arm. The other three Replace windows are not in a loop body
    and must not move: `apply.rs` 297 runs after a file's protocol has
    finished, and `apply.rs` 973 and `undo.rs` 396 sit in the helpers the
    `WriteTarget` and `WriteRestore` arms call (`write_target_from_body`,
    `write_restore`);
  - the guard drop after the loop.

  If the apply closure needs the design's fallback, take it and record why in
  the module doc.
- [x] 1.4a Add the "no loop of their own" source check (design G1): in each
  of the four files that hold the six shells, the core's `step` (or
  `shrink_step`) is called only inside its
  `Protocol::advance` implementation, unless the module documentation
  records the fallback exception. Write it as a `#[test]` over
  `include_str!` of the files, or as a rule in an existing policy script.
  Confirm it fails on the tree before 1.4, then passes after.
- [x] 1.5 Re-run 1.1's tests and the `drive` unit tests. Then run
  `scripts/heavy.sh -- make crash-recovery-smoke` where supported, and check
  that the durable-write and Replace kill-point scenarios pass.
- [x] 1.6 Pre-existing blocker: re-key the stale `durable_write.rs`
  `exclude_re` entries in `.cargo/mutants.toml`:
  - line 131, `FileWriteLock::acquire`, whose function no longer exists;
  - lines 504 and 515, the non-Unix `MetadataPlan` stubs, now at 743 and 754;
  - the unanchored `FileWriteLock` drop entry;
  - the unanchored `!=` in `rename_durable` entry.

  Do it against the mutants that `scripts/heavy.sh -- make mutants-list`
  actually generates after 1.4: re-key each entry, or delete it with the
  reason in the commit. The listing applies `exclude_re`, so check that an
  entry still matches by listing with that entry removed and finding the
  mutant it should hide.
- [x] 1.7 Confirm 0.2's diff is still empty. Kani: none.
- [x] 1.8 Sync the docs:
  - `docs/durable-writes.md` (the shells run through `drive`);
  - the "build on" section of `docs/next/formal-verification.md`, which
    names the single tap site and the six shells for the conformance change;
  - N8 in `docs/next/formal-verification-next.md` (group 1 done);
  - the `AGENTS.md` module layout (`services/protocol_drive.rs`);
  - `README.md`: evaluate it per `documentation.md`. Its architecture section
    lists crates, not modules, so record "no change" unless that has
    changed.

## 4. `WriteLabel` down to the temp name (design G4)

- [ ] 4.1 Write the characterization test first. For each of the 11
  `WriteLabel::KNOWN` entries, with a fixed file, pid, and sequence, assert
  the exact temp name the current builder emits. It must pass on the
  unchanged tree.
- [ ] 4.2 Change these signatures to take `WriteLabel`, and remove the four
  `as_str()` conversions in `filesystem/write.rs`:
  - `temp_name::format` and `unique_temp_path`;
  - the private `atomic_write_bytes`;
  - the four `*_classified` entry points and
    `atomic_write_stream_with_metadata`;
  - `copy_durable` and `move_durable`.

  `kill_point::reach` keeps its `&str` key. Update its documentation to say
  that the key is a write label or a draft id.
- [ ] 4.3 Switch the tests that pass literal tags (`"test"` 15 times, `"json"`
  twice, and the `leftovers.rs` `unique_temp_path` callers) to existing
  `WriteLabel` constants. Add no test-only label. Move the `"not-a-label"`
  negative case to `parse`, as a crafted name.
- [ ] 4.4 Confirm 4.1 passes unchanged, and confirm that the leftover
  predicate tests pass, older-build names included. Confirm that the
  kill-point `@label` narrowing still works by running
  `scripts/heavy.sh -- make crash-recovery-smoke`, where supported.
- [ ] 4.5 Re-check the `durable_write.rs` `exclude_re` entries as in 1.6.
  Confirm 0.2's diff is empty. Kani: none.
- [ ] 4.6 Sync the docs: `docs/durable-writes.md` (label plumbing, and the
  kill-point key kinds) and N8 (group 4 done).

## 2. One owner for the app-data layout (design G2)

- [ ] 2.1 Write the characterization test first, against the unchanged
  services. For a fixed root, assert the exact path from every existing
  source:
  - the helpers: `drafts_dir`, `set_aside_dir`, `local_history_dir`,
    `bookmarks_dir`, `document_notes_dir`, `folder_notes_dir`,
    `style_schemes_dir`, `journal_dir`, `bodies_dir`, and `ledger_path`;
  - the quarantine directory, and the format-upgrade backup directory with
    its `items/` child;
  - each of the 25 name constants, joined to the root;
  - every literal the format-upgrade inventory scans;
  - for a populated fixture data home (one file in every scanned location),
    the ordered `item.path.relative()` list that `format_upgrade::scan`
    returns, because the upgrade backups are named by those paths.
- [ ] 2.2 Add the GTK-free `services/app_data_layout.rs` and register it in
  `services/mod.rs`. It holds:
  - `AppDataLocation`, with `Root` and `ReplaceJournalBodies`;
  - `ALL`;
  - per location: its relative path, kind (`LineageFamily` carries
    `index.json`, and `RunFamily` carries `items`), and `sweep` (`Swept`,
    `SweptLast` for `LegacyWorkspaceNotes` only, or `ParentSwept` for files;
    design G2);
  - the `AppDataLayout::new(root)` accessors.
- [ ] 2.3 Switch the startup sweep in `app_data_leftovers.rs` to iterate the
  layout. Keep the family expansion, `MAX_LINEAGES_PER_PASS`,
  `MAX_BACKUP_RUNS_PER_PASS`, and the budget. `ALL` must reproduce today's
  directory visit order, because the shared budget is spent in that order.
  The families are expanded in an explicit `FAMILY_SWEEP_ORDER`: the
  format-upgrade runs with their `items/`, then the local-history lineages,
  as today and unlike their order in `ALL`. `LegacyWorkspaceNotes`
  (`SweptLast`) is visited after the lineages. Add a test that pins the whole
  visit order (flat directories, runs, lineages, then the legacy directory),
  failing-first against a deliberately reordered list and against families
  expanded in `ALL` order. Add a failing-first test that a stale,
  provably-ours temp in `workspace-notes/` is removed and a fresh one kept,
  which fails on the unchanged tree. Add a test asserting that every file entry's parent is a
  swept entry. Confirm the existing sweep tests pass unchanged, including the
  style-schemes, set-aside, bodies, and backup-run cases.
- [ ] 2.4 Switch the format-upgrade inventory to an exhaustive `match` over
  `AppDataLocation`, in `format_upgrade/inventory.rs`, scanned in an explicit
  `INVENTORY_ORDER` that reproduces today's scan order, and delete its
  duplicated literals. Mark the replace journal's manifest, cleanup marker,
  and bodies directory `CoveredBy(ReplaceJournalDir)`. Add a test that
  `INVENTORY_ORDER` holds exactly the locations with a scan of their own, and
  that every `CoveredBy` target is in it. Give `Root` and each exempt entry
  `NotInventoried(reason)`:
  - `recent-documents.json` (no envelope; resolved in design G2);
  - `style-schemes/`;
  - `recovery-quarantine/`;
  - `drafts/set-aside/`;
  - `format-upgrade-backups/`.

  Confirm the existing inventory and apply tests pass.
- [ ] 2.5 Route each service's path helpers through the layout, keeping the
  public helper names as delegations, and delete the per-service name
  constants. This removes both `workspace-notes` spellings. Re-key the
  `recent_documents.rs:252:.*load_recent_file` `exclude_re` anchor in
  `.cargo/mutants.toml` against `scripts/heavy.sh -- make mutants-list`, as
  in 1.6, and check every other `exclude_re` that names a file this group
  edits (testing.md, "Path-keyed gates").
- [ ] 2.6 Run 2.1's characterization test against the layout and check it is
  byte-identical. Add a unit test asserting that every location is `Swept`
  or `SweptLast`, or `ParentSwept` with a `Swept` parent, and that
  `LegacyWorkspaceNotes` is the only `SweptLast` entry.
- [ ] 2.7 Data-safety audit: run the `data-safety` skill over the diff. Then
  run `scripts/heavy.sh --envelope 16G -- make test` and
  `scripts/heavy.sh -- make crash-recovery-smoke`, where supported. Confirm
  0.2's diff is empty. Kani: none.
- [ ] 2.8 Sync the docs:
  - `.agents/rules/rust.md` (filesystem boundary: the app-data layout's
    single owner);
  - in `AGENTS.md`, the module layout (`services/app_data_layout.rs`) and the
    "Filesystem boundary" decision;
  - `docs/durable-writes.md` (sweep coverage comes from the layout);
  - the deferral inventory in `docs/next/formal-verification.md`: mark the
    phase-0 entry "the startup leftover sweep skips the legacy folder-note
    sidecar directory" fixed (swept last since this group), keeping its
    second half, the per-pass budget, as it is;
  - N8 (group 2 done);
  - `README.md` evaluated as in 1.8, and
    `grep -rn 'leftover\|sweep' .agents/skills/*/references/` checked for a
    hand-written sweep list that now needs to point at the layout.

## 6. Explicit smoke-binary roles (design G6)

- [ ] 6.1 Add `scripts/check-kill-point-gating.sh` and wire it into
  `make check-policy`. It keys on gating sites (design G6):
  - `feature = "crash-kill-points"` in Rust only in
    `services/kill_point.rs`;
  - the feature declared only by the two `Cargo.toml` lines;
  - `--features crash-kill-points` only in the Makefile's smoke build line.

  Comments, scripts, and docs may mention it.

  Confirm it fails first: add a temporary second
  `cfg(feature = "crash-kill-points")` site, see the failure, then remove the
  site. Add a `--self-test` if `check-agent-helpers`' pattern requires one.
- [ ] 6.2 Add the role table to `scripts/crash-recovery-smoke-driver.py`.
  `seed` and `relaunch` are always `--binary`. `victim` is
  `--kill-point-binary` when supplied, and `--binary` otherwise.
  `launch_app(role=…)` replaces `binary=`, and every one of its 11 calls
  names a role. Each phase writes `logs/lushtext-<phase>.role` with its role
  and binary.
- [ ] 6.3 Add the marker guard. It reads the ordinary binary's bytes, and
  `target/release/lushtext` if present, and fails on `LUSHTEXT_KILL_AT` or
  `LUSHTEXT_KILL_AT: aborting inside`. It writes
  `assertions/no-kill-points.json`, and keeps its marker list in one place.
  Confirm it fails first by passing the kill-point binary as `--binary`.
- [ ] 6.4 Run `scripts/heavy.sh -- make crash-recovery-smoke`. Confirm three
  things:
  - the external-signal scenario passes with the kill-point victim;
  - every `seed` and `relaunch` launch is the ordinary binary;
  - the guard passes.

  Record the artifacts in the handoff directory.
- [ ] 6.5 Sync the docs:
  - `docs/end-user-coverage.md` and the crash-smoke section of
    `.agents/rules/build.md` (roles, guard, and gating check);
  - K5 in `docs/next/formal-verification.md` (the release check is now
    automated);
  - N8 (group 6 done).

## 5. Registered and unregistered draft candidates as distinct types (design G5)

- [ ] 5.1 Characterize first. On the unchanged tree, ensure these tests exist
  and pass, adding any that are missing:
  - first-body registration ordering;
  - a registration commit failure that leaves the candidate dirty and
    retryable, with no body written;
  - a steady-state, already registered id;
  - the asynchronous close pipeline registering before its write.

  Run the widget ones with `scripts/heavy.sh --envelope 16G -- make
  test-widget-shard WIDGET_SHARD=<the owning shard>`, as in 3.6.
- [ ] 5.2 Add `WritableDraftCandidate` and `DirtyDraftCandidate::into_writable`
  to `ui/window/drafts/seams.rs`. `into_writable` checks the id and gives back
  both values on a mismatch. Remove `registered` from `DirtyDraftCandidate`.
- [ ] 5.3 Make `register_new_draft_ids_then` hand `then` its
  `WritableDraftCandidate`s plus the refused ids. Keep
  `journal_core::may_write_after_registration` as the partition's gate: a
  candidate becomes writable only when the gate admits it and it holds its
  token, and every other candidate's id is refused, as today. Change
  `drive_dirty_draft_pipeline` and `drive_close_draft_pipeline` to take only
  `WritableDraftCandidate`, and delete both `ok_or_else` "not registered"
  branches. Leave the synchronous close flush in `journal.rs` as it is (design
  G5, scope note).
- [ ] 5.4 Add a unit test showing that `into_writable` refuses a token for a
  different id.
- [ ] 5.5 Confirm that 5.1 passes, that `RegisteredDraft` gained no `Clone`,
  and that `journal_core.rs` and `model/draft.rs` are unchanged. G2's
  path-helper edits to `draft_service.rs` reach no harness. Kani: none. If any of them changed, run
  `setsid nohup scripts/heavy.sh --envelope 16G -- make kani KANI_SHARD=core-journal-and-write`
  and then `KANI_SHARD=core-second-writer`, once each, detached, on the final
  code, and record the times.
- [ ] 5.6 Data-safety audit (`data-safety` skill). Check two things:
  - refused candidates stay draft-dirty and retryable, with their failure
    accounting intact;
  - no close-pipeline candidate is dropped silently.
- [ ] 5.7 Re-derive the `WFR-DRAFT-RECOVERY` seam cell (the candidate seam is
  now a typestate pair). Then run
  `scripts/heavy.sh -- make check-workflow-boundaries`.
- [ ] 5.8 Sync the docs:
  - phase 4 in `docs/next/formal-verification.md` (the token is now carried
    by type to the asynchronous writes);
  - N8 (group 5 done).

## 3. Bounded backoff for unrestored-copy retries (design G3)

- [ ] 3.1 First add `unrestored_copy_failures` (each pending retry's failure
  count, sorted by draft id) to `DraftEvidence`, which changes no behaviour,
  so the test below fails on its cadence rather than on compilation. Then
  write a failing-first widget test. It makes every copy fail as the
  existing failed-copy test does, with a regular file where the set-aside
  directory belongs: the drafts `test_policy.rs` has no copy-failure hook,
  and none is added. It drives `autosave_tick_for_test`, and after each tick
  it waits for any attempt to complete (the entry back in the evidence)
  before the next one. It observes an attempt as a change of the entry's
  failure count. On the unchanged tree every tick attempts a copy. After the
  change, counted from the tick at which the first failure was recorded,
  attempts must happen at ticks 1, 3, 7, 15, and 27 and at no other tick,
  while the id stays in `restore_held_draft_ids` throughout. Add the widget
  module to its shard in `scripts/widget-shards.py` if it is new.
- [ ] 3.2 Write a failing-first widget test showing that an autosave pass
  started by the first-dirty debounce or by a pending rerun attempts no copy
  and leaves the retry entry's failure count (and, after the change, its next
  due tick) unchanged. Use the same copy-failure fixture. If the test needs
  `autosave_tick_for_test` to set up a pending rerun, first let the entry's
  next due tick move past the ticks the test drives, so only the non-timer
  passes could attempt a copy.
- [ ] 3.3 Add the pure `unrestored_copy_retry_delay_ticks` and
  `UNRESTORED_COPY_MAX_RETRY_TICKS = 12` to `ui/window/drafts/policy.rs`.
  Unit-test the doubling, the cap, and saturation at `u32::MAX` failures.
  Give the cap its rustdoc `Kind:` line and its `docs/thresholds.md` row as
  `out: time` (one minute of 5 s ticks, design G3), and run
  `scripts/heavy.sh -- make check-thresholds`.
- [ ] 3.4 Make these changes:
  - add `autosave_timer_ticks` to the drafts state;
  - add `periodic_autosave_tick`, which counts the tick, retries the due
    entries, and then calls `autosave_tick`;
  - point the 5 s timer and `autosave_tick_for_test` at it;
  - remove `retry_unrestored_copies` from `autosave_tick`;
  - extend each retry entry (`ui/window/imp.rs`) with `next_due_tick`,
    computed when the failed attempt's completion records the entry: the
    current tick count plus the delay.

  Leave the hold logic in `attempt_unrestored_copy` untouched. Update the
  rustdoc that states a per-tick retry: `drafts/mod.rs`, `journal.rs`
  (`preserve_unrestored_draft`, `retry_unrestored_copies`), and the field
  doc in `imp.rs`.
- [ ] 3.5 Extend `DraftEvidence` with `unrestored_copy_next_due_ticks` and
  `autosave_timer_ticks`. Compute the retry fields and drop the map's `Ref`
  before building the struct, and keep the draft evidence's driven proofs
  green (workflow-convention.md, evidence surface invariants). If any new
  field reaches an automation snapshot through the Evidence Projection Map,
  update `docs/automation.md` and `docs/automation-reference.md`, then run
  `make check-automation-docs`.
- [ ] 3.6 Confirm three things:
  - 3.1 and 3.2 pass;
  - `test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft`
    stays green unchanged;
  - a new test, where a retry succeeds after backoff, finds the hold
    released and the outcome published once.

  Run them with `scripts/heavy.sh --envelope 16G -- make test-widget-shard
  WIDGET_SHARD=<the shard that owns window>`: the shard runner admits its own
  capped scopes, so it needs the envelope.
- [ ] 3.7 Data-safety audit. Confirm that:
  - no error path releases the hold;
  - Discard, close flush, and teardown behave as before while an id is held;
  - the backoff state is never persisted.
- [ ] 3.8 Confirm 0.2's diff is empty, `journal_core.rs` included. Kani: none.
- [ ] 3.9 Re-derive the `WFR-DRAFT-RECOVERY` row cells in
  `docs/workflow-readability-matrix.md` (policy and evidence), row-scoped and
  excluding `#[cfg(test)]` code, keeping the facade within the line budget.
  Then run `scripts/heavy.sh -- make check-workflow-boundaries`.
- [ ] 3.10 Sync the docs:
  - phase 4 in `docs/next/formal-verification.md` (the retry now backs off,
    on timer ticks only);
  - the `AGENTS.md` "Draft persistence" decision;
  - N8 (group 3 done);
  - `grep -rn 'unrestored' .agents/skills/*/references/ docs/` for a stated
    per-tick retry cadence that is now stale.

## 7. Close the change

- [ ] 7.1 Run `scripts/heavy.sh -- make check` after each group, and
  `scripts/heavy.sh --envelope 16G -- make test` on the final tree. A `FLAKY`,
  `OOM-KILL:`, or `SESSION-HYGIENE:` line is a blocker.
- [ ] 7.2 Run `scripts/heavy.sh --envelope 36G -- make mutants-diff` once on
  the final tree. It covers `policy.rs`, `protocol_drive.rs`, and
  `app_data_layout.rs`. Triage every survivor, and confirm the re-keyed
  `exclude_re` entries match real mutants. If every slot is busy (exit 75),
  retry later, never in a loop inside one `heavy.sh`.
- [ ] 7.3 Groups 3 and 5 touch `crates/lushtext-core/src/ui/` and
  `crates/lushtext/tests/widget/`. On the tree that holds both groups, before
  their commit, run `scripts/heavy.sh -- make visual-geometry-smoke`,
  `scripts/heavy.sh -- make accessibility-smoke`, and
  `scripts/heavy.sh -- make visual-smoke`. Then commit groups 5 and 3
  together; the pre-commit `make check-policy` requires these proofs current
  for the UI-prefix files in `git status`.
- [ ] 7.4 Record in the handoff:
  - "Kani: none (no proven module touched)", with 0.2's empty diff;
  - "proof-strength: not owed";
  - "GTK axioms: none";
  - "no timing verdict", with design's reasons.
- [ ] 7.5 In `docs/next/formal-verification-next.md`, mark N8 and change row
  8 done, or re-defer each unapplied group with its trigger.
- [ ] 7.6 Run `openspec validate apply-verification-altitude-redesigns --strict`.
