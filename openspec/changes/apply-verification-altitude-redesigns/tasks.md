Each numbered group can be applied on its own. Apply them in the order
0, 1, 4, 2, 6, 5, 3, 7:
- groups 1 and 4 both edit `durable_write.rs`, so one follows the other;
- groups 5 and 3 both edit `ui/window/drafts/`, so they follow each other,
  and they come last because they touch the UI prefixes.

Every group ends with its own gates and doc sync. Every heavy command goes
through `scripts/heavy.sh`, one command per call, in the forms written here.
`make test`, `make kani`, and `make proof-strength` use `--envelope 16G`, and
`make mutants-*` uses `--envelope 36G`. Commits use
`scripts/heavy.sh --mem none -- git commit -F <file>`. Read
`$H/apply-verification-altitude-redesigns/COORDINATOR-NOTE.txt` before each
group's heavy commands.

## 0. Start

- [ ] 0.1 Acquire the implementation slot (parallel-agent-work.md,
  "Implementation slots"), and release it however the stage ends. If
  `origin/main` moved past the branch point, merge it into the branch; never
  rebase.
- [ ] 0.2 Record the Kani baseline:
  `git diff --stat origin/main -- crates/lushtext-core/src/services/filesystem/write_protocol.rs crates/lushtext-core/src/services/content_search/replace/protocol.rs crates/lushtext-core/src/services/draft_service/journal_core.rs crates/lushtext-core/src/services/draft_service/set_aside_retention.rs crates/lushtext-core/src/model/draft.rs ':(glob)**/kani_proofs.rs'`
  must print nothing. Re-run it at the end of every group. If it ever prints
  a file, run the shard that design "Kani, proof strength, axioms, timing"
  names for it, once, on the final code, and record the time.

## 1. One `Protocol` driver for every proven-core shell (design G1)

- [ ] 1.1 Characterize the shells on the unchanged tree. Confirm these tests
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
    body is kept.

  Command: `scripts/heavy.sh -- make test-unit`. It runs the library tests,
  these included, through the temp scope.
- [ ] 1.2 Add `services/protocol_drive.rs`, holding the `Protocol` trait
  (`Action`, `Outcome`, `End`, `end`, `advance`) and `drive`, and register it
  in `services/mod.rs`. Unit-test `drive` with a small counting protocol for
  four properties:
  - it stops at the terminal action without executing it;
  - it executes each other action exactly once, in order;
  - it feeds every outcome back unchanged;
  - it supports a nested `drive` from inside `execute`.
- [ ] 1.3 Implement `Protocol` beside each shell, delegating to the unchanged
  inherent `start` / `step`:
  - in `durable_write.rs`: `WriteProtocol`, `MoveProtocol`, and
    `RenameProtocol`;
  - `ReplaceFileProtocol` in `replace/apply.rs`;
  - `ReplaceUndoProtocol` in `replace/undo.rs`.

  Do not edit `write_protocol.rs` or `replace/protocol.rs`.
- [ ] 1.4 Rewrite the five loops as `start`, then one `drive` with an
  `execute` closure, then a `finish` mapping. The five are:
  - `atomic_write_stream_with_metadata`;
  - `run_rename_protocol`;
  - `move_durable`;
  - the apply loop of `replace/apply.rs`;
  - the entry loop of `replace/undo.rs`.

  Keep each of these exactly where it is:
  - `last_error` carriage;
  - temp removal;
  - every `kill_point::reach` (the `DurableRenamedBeforeDirSync` window and
    the four Replace windows);
  - the guard drop after the loop.

  If the apply closure needs the design's fallback, take it and record why in
  the module doc.
- [ ] 1.5 Re-run 1.1's tests and the `drive` unit tests. Then run
  `scripts/heavy.sh -- make crash-recovery-smoke` where supported, and check
  that the durable-write and Replace kill-point scenarios pass.
- [ ] 1.6 Pre-existing blocker: re-key the stale `durable_write.rs`
  `exclude_re` entries in `.cargo/mutants.toml`:
  - line 131, `FileWriteLock::acquire`, whose function no longer exists;
  - lines 504 and 515, the non-Unix `MetadataPlan` stubs, now at 743 and 754;
  - the unanchored `FileWriteLock` drop entry;
  - the unanchored `!=` in `rename_durable` entry.

  Do it against the mutants that `scripts/heavy.sh -- make mutants-list`
  actually generates after 1.4: re-key each entry, or delete it with the
  reason in the commit.
- [ ] 1.7 Confirm 0.2's diff is still empty. Kani: none.
- [ ] 1.8 Sync the docs:
  - `docs/durable-writes.md` (the shells run through `drive`);
  - the "build on" section of `docs/next/formal-verification.md`, which
    names the single tap site and the five shells for the conformance change;
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
  - every literal the format-upgrade inventory scans.
- [ ] 2.2 Add the GTK-free `services/app_data_layout.rs` and register it in
  `services/mod.rs`. It holds:
  - `AppDataLocation`, with `Root` and `ReplaceJournalBodies`;
  - `ALL`;
  - per location: its relative path, kind (`LineageFamily` carries
    `index.json`, and `RunFamily` carries `items`), and
    `holds_durable_writes`;
  - the `AppDataLayout::new(root)` accessors.
- [ ] 2.3 Switch the startup sweep in `app_data_leftovers.rs` to iterate the
  layout. Keep the family expansion, `MAX_LINEAGES_PER_PASS`,
  `MAX_BACKUP_RUNS_PER_PASS`, and the budget. `ALL` must reproduce today's
  directory visit order, because the shared budget is spent in that order:
  add a test that pins the order, failing-first against a deliberately
  reordered list. Add a test asserting that every file entry's parent is a
  swept entry. Confirm the existing sweep tests pass unchanged, including the
  style-schemes, set-aside, bodies, and backup-run cases.
- [ ] 2.4 Switch the format-upgrade inventory to an exhaustive `match` over
  `AppDataLocation`, scanned in an explicit `INVENTORY_ORDER` that
  reproduces today's scan order, and delete its duplicated literals. Add a
  test that `INVENTORY_ORDER` holds exactly the non-exempt locations. Give each exempt
  entry `NotInventoried(reason)`:
  - `recent-documents.json` (no envelope; resolved in design G2);
  - `style-schemes/`;
  - `recovery-quarantine/`;
  - `drafts/set-aside/`;
  - `format-upgrade-backups/`.

  Confirm the existing inventory and apply tests pass.
- [ ] 2.5 Route each service's path helpers through the layout, keeping the
  public helper names as delegations, and delete the per-service name
  constants. This removes both `workspace-notes` spellings.
- [ ] 2.6 Run 2.1's characterization test against the layout and check it is
  byte-identical. Add a unit test asserting that every location is either
  swept or has `holds_durable_writes = false` with a reason.
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
  - the deferral inventory in `docs/next/formal-verification.md` (the legacy
    `workspace-notes/` entry now points at the layout exemption);
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
- [ ] 5.2 Add `WritableDraftCandidate` and `DirtyDraftCandidate::into_writable`
  to `ui/window/drafts/seams.rs`. `into_writable` checks the id and gives back
  both values on a mismatch. Remove `registered` from `DirtyDraftCandidate`.
- [ ] 5.3 Make `register_new_draft_ids_then` hand `then` its
  `WritableDraftCandidate`s plus the refused ids. Change
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

- [ ] 3.1 Write a failing-first widget test using the drafts `test_policy`
  copy-failure hook and `autosave_tick_for_test`. It shows that a persistent
  copy failure currently retries on every tick. After the change, attempts
  must happen at cumulative ticks 1, 3, 7, 15, and 27, while the id stays in
  `restore_held_draft_ids` throughout. Add the widget module to its shard in
  `scripts/widget-shards.py` if it is new.
- [ ] 3.2 Write a failing-first widget test showing that an autosave pass
  started by the first-dirty debounce or by a pending rerun attempts no copy
  and leaves the retry entry unchanged.
- [ ] 3.3 Add the pure `unrestored_copy_retry_delay_ticks` and
  `UNRESTORED_COPY_MAX_RETRY_TICKS = 12` to `ui/window/drafts/policy.rs`.
  Unit-test the doubling, the cap, and saturation at `u32::MAX` failures.
- [ ] 3.4 Make these changes:
  - add `autosave_timer_ticks` to the drafts state;
  - add `periodic_autosave_tick`, which counts the tick, retries the due
    entries, and then calls `autosave_tick`;
  - point the 5 s timer and `autosave_tick_for_test` at it;
  - remove `retry_unrestored_copies` from `autosave_tick`;
  - extend each retry entry (`ui/window/imp.rs`) with `next_due_tick`.

  Leave the hold logic in `attempt_unrestored_copy` untouched.
- [ ] 3.5 Extend `DraftEvidence` with `unrestored_copy_next_due_ticks` and
  `autosave_timer_ticks`. If either reaches an automation snapshot through the
  Evidence Projection Map, update `docs/automation.md` and
  `docs/automation-reference.md`, then run `make check-automation-docs`.
- [ ] 3.6 Confirm three things:
  - 3.1 and 3.2 pass;
  - `test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft`
    stays green unchanged;
  - a new test, where a retry succeeds after backoff, finds the hold
    released and the outcome published once.
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
  `crates/lushtext/tests/widget/`. On the final tree, run
  `scripts/heavy.sh -- make visual-geometry-smoke`,
  `scripts/heavy.sh -- make accessibility-smoke`, and
  `scripts/heavy.sh -- make visual-smoke`.
- [ ] 7.4 Record in the handoff:
  - "Kani: none (no proven module touched)", with 0.2's empty diff;
  - "proof-strength: not owed";
  - "GTK axioms: none";
  - "no timing verdict", with design's reasons.
- [ ] 7.5 In `docs/next/formal-verification-next.md`, mark N8 and change row
  8 done, or re-defer each unapplied group with its trigger.
- [ ] 7.6 Run `openspec validate apply-verification-altitude-redesigns --strict`.
