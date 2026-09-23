Each numbered group can be applied on its own. Apply groups 1 and 4 one after
the other (both edit `durable_write.rs`), and likewise groups 3 and 5 (both
edit `ui/window/drafts/`). Every group ends with its own gates and doc sync.
Group 7 closes the change.

## 1. Common `Protocol` driver (design G1)

- [ ] 1.1 Characterize the shell before touching it. On the unchanged tree,
  confirm the durable-write unit tests, the fault-injection tests, and the
  three Kani-consolidation characterization tests (content, rename, and probe
  failure) pass. Add any missing coverage for:
  - `move_durable` when its source removal fails;
  - `rename_durable` when a cross-directory destination sync fails.
- [ ] 1.2 Add the `Protocol` trait and `drive` to
  `services/filesystem/write_protocol.rs`. Implement the trait for
  `WriteProtocol`, `MoveProtocol`, and `RenameProtocol` by delegating to the
  unchanged inherent `const fn start` / `step`, and give each a `MAX_ACTIONS`.
- [ ] 1.3 Add a Kani harness per protocol. Each drives `drive` with a
  `kani::any()` executor and asserts a terminal within `MAX_ACTIONS`. Make
  sure it matches exactly one shard, and confirm with
  `make check-kani-shards`.
- [ ] 1.4 Confirm the harness fails first. Temporarily set one `MAX_ACTIONS`
  one below the true bound and check the harness fails; then restore it.
- [ ] 1.5 Rewrite the three shells in `services/durable_write.rs`
  (`atomic_write_*`, `run_rename_protocol`, and `move_durable`) as an
  `execute` closure plus a `finish` mapping over `drive`. Keep `last_error`
  carriage, temp removal, and the `DurableRenamedBeforeDirSync` kill point
  exactly where they are.
- [ ] 1.6 Run the checks:
  - `cargo kani -p lushtext-core --harness services::filesystem::write_protocol::kani_proofs::`
    (all green; record the times);
  - the tests from 1.1;
  - `make crash-recovery-smoke`, where supported.
- [ ] 1.7 Re-verify every `durable_write.rs` `exclude_re` entry in
  `.cargo/mutants.toml` against a mutant `make mutants-list` actually
  generates, since the entries at lines 131, 504, and 515 shift. Re-key or
  delete each one, then run `make mutants-diff`.
- [ ] 1.8 Sync the docs:
  - `docs/durable-writes.md` (the shell description);
  - the phase-5 status in `docs/next/formal-verification.md` (the new
    termination harness and its time);
  - N8 in `docs/next/formal-verification-next.md` (group 1 done).

## 2. One owner for the app-data layout (design G2)

- [ ] 2.1 Write the characterization test first, against the unchanged
  services. For a fixed root, assert the exact path from every existing
  helper and literal:
  - `drafts_dir`, `set_aside_dir`, `local_history_dir`, `bookmarks_dir`,
    `document_notes_dir`, `folder_notes_dir`, `style_schemes_dir`,
    `journal_dir`, and `ledger_path`;
  - the quarantine directory and the format-upgrade backup directory with its
    `items/` child;
  - every `*_FILE` constant joined to the root;
  - every literal the format-upgrade inventory scans.
- [ ] 2.2 Audit `recent-documents.json`. Establish whether it carries the
  versioned envelope.
  - **If it does:** its absence from the format-upgrade inventory is a
    pre-existing gap. Add a failing test showing that a future-version file is
    not reported, then scan it.
  - **If it does not:** record the reason as its `NotInventoried` exemption.
- [ ] 2.3 Add the GTK-free `services/app_data_layout.rs`. It holds the
  `AppDataLocation` enum, including `Root`; `ALL`; the relative path, kind,
  and `holds_durable_writes` for each location; and
  `AppDataLayout::new(root)` accessors. Register it in `services/mod.rs`.
- [ ] 2.4 Switch the startup sweep in `app_data_leftovers.rs` to iterate the
  layout. Keep the family expansion, `MAX_LINEAGES_PER_PASS`,
  `MAX_BACKUP_RUNS_PER_PASS`, and the budget. Confirm the existing sweep
  tests, including the style-schemes, set-aside, and backup-run cases, pass
  unchanged.
- [ ] 2.5 Switch the format-upgrade inventory to an exhaustive `match` over
  `AppDataLocation`, with `NotInventoried(reason)` for the exempt entries.
  Delete its duplicated literals. Confirm the existing inventory and apply
  tests pass.
- [ ] 2.6 Route each service's path helpers through the layout, keeping the
  public helper names as delegations, and delete the per-service name
  constants.
- [ ] 2.7 Run 2.1's characterization test against the layout and check it is
  byte-identical. Add a unit test asserting that every location is either
  swept or has `holds_durable_writes = false` with a reason.
- [ ] 2.8 Data-safety audit: run the `data-safety` skill over the diff. Then
  run `make test` and `make crash-recovery-smoke`, where supported.
- [ ] 2.9 Sync the docs:
  - `.agents/rules/rust.md` (filesystem identity: app-data layout ownership);
  - the `AGENTS.md` module layout (`services/app_data_layout.rs`) and the
    "Filesystem boundary" decision;
  - `docs/durable-writes.md` (sweep coverage);
  - the deferral inventory in `docs/next/formal-verification.md` (the legacy
    `workspace-notes/` entry now points at the layout exemption);
  - N8 (group 2 done).

## 3. Bounded backoff for unrestored-copy retries (design G3)

- [ ] 3.1 Write a failing-first widget test using the drafts `test_policy`
  copy-failure hook and `autosave_tick_for_test`. It shows that a persistent
  copy failure currently retries on every tick; after the change it must
  retry at ticks 1, 2, 4, and so on, while the id stays in
  `restore_held_draft_ids` throughout.
- [ ] 3.2 Add the pure `unrestored_copy_retry_delay_ticks` and
  `UNRESTORED_COPY_MAX_RETRY_TICKS` to `ui/window/drafts/policy.rs`. Unit-test
  the doubling, the cap, and saturation at `u32::MAX` failures.
- [ ] 3.3 Add `autosave_tick_count` to the drafts state, and extend each
  retry entry with `next_due_tick`. `retry_unrestored_copies` attempts only
  the due entries and leaves the hold logic in `attempt_unrestored_copy`
  untouched.
- [ ] 3.4 Extend `DraftEvidence` with each retry's next due tick. If the field
  reaches an automation snapshot through the Evidence Projection Map, update
  `docs/automation.md` and `docs/automation-reference.md`, then run
  `make check-automation-docs`.
- [ ] 3.5 Confirm 3.1 passes and
  `test_a_failed_set_aside_copy_keeps_autosave_off_the_unshown_draft` stays
  green. Add a test where a retry succeeds after backoff, checking that the
  hold is released and the outcome published once.
- [ ] 3.6 Data-safety audit. Confirm that:
  - no error path releases the hold;
  - Discard, close flush, and teardown behave as before while an id is held;
  - the backoff state is never persisted.
- [ ] 3.7 Confirm `journal_core.rs` is unchanged (`git diff --stat`). If it
  changed, run `make kani KANI_SHARD=core-journal-and-write`.
- [ ] 3.8 Re-derive the `WFR-DRAFT-RECOVERY` row cells in
  `docs/workflow-readability-matrix.md` (policy and evidence), keeping the
  facade within the line budget. Then run `make check-workflow-boundaries` and
  `make mutants-diff`, which covers `policy.rs` by convention.
- [ ] 3.9 Sync the docs:
  - phase 4 in `docs/next/formal-verification.md` (the retry now backs off);
  - the `AGENTS.md` "Draft persistence" decision;
  - N8 (group 3 done).

## 4. `WriteLabel` into `temp_name::format` (design G4)

- [ ] 4.1 Write the characterization test first. For each
  `WriteLabel::KNOWN` entry and fixed file, pid, and sequence, assert the
  exact temp name the current builder emits.
- [ ] 4.2 Change the signatures to take `WriteLabel`: `temp_name::format`,
  `unique_temp_path`, every `durable_write` entry point that takes `tmp_tag`,
  `copy_durable`, `move_durable`, and `kill_point::reach`. Remove the
  `as_str()` conversions in `filesystem/write.rs`.
- [ ] 4.3 Update the tests that pass literal tags so they use `WriteLabel`
  constants. Move the unknown-tag negative case to the `parse` side as a
  crafted name.
- [ ] 4.4 Confirm 4.1 passes unchanged, and that the leftover predicate tests
  and the kill-point `@label` narrowing still work: build with
  `--features crash-kill-points` and run `make crash-recovery-smoke`, where
  supported.
- [ ] 4.5 Run the `write_protocol` Kani harnesses.
- [ ] 4.6 Re-verify the line-anchored `durable_write.rs` `exclude_re` entries
  as in 1.7, then run `make mutants-diff`.
- [ ] 4.7 Sync the docs: `docs/durable-writes.md` (label plumbing) and N8
  (group 4 done).

## 5. Registered and unregistered draft candidates as distinct types (design G5)

- [ ] 5.1 Characterize first. On the unchanged tree, ensure these tests exist
  and pass, adding any that are missing:
  - first-body registration ordering;
  - a registration commit failure that leaves the candidate dirty and
    retryable, with no body written;
  - a steady-state, already registered id;
  - close-flush registration before its write.
- [ ] 5.2 Add `WritableDraftCandidate` and `DirtyDraftCandidate::into_writable`
  (it checks the id and gives back both values on a mismatch) to
  `ui/window/drafts/seams.rs`, and remove `registered` from
  `DirtyDraftCandidate`.
- [ ] 5.3 Make `register_new_draft_ids_then` and the close-flush registration
  produce `WritableDraftCandidate`s plus the refused ids. Change
  `drive_dirty_draft_pipeline` and the close flush to take only
  `WritableDraftCandidate`, and delete both `ok_or_else` "not registered"
  branches.
- [ ] 5.4 Add a unit test that `into_writable` refuses a token for a
  different id.
- [ ] 5.5 Confirm that 5.1 passes and that `RegisteredDraft` gained no
  `Clone`.
- [ ] 5.6 Run `make kani KANI_SHARD=core-journal-and-write` and
  `make kani KANI_SHARD=core-second-writer`. Both must pass, the second as its
  `should_panic`. Record the times.
- [ ] 5.7 Data-safety audit (`data-safety` skill). Check that:
  - refused candidates stay draft-dirty and retryable, with failure
    accounting intact;
  - no close-flush candidate is dropped silently.
- [ ] 5.8 Re-derive the `WFR-DRAFT-RECOVERY` seam cell (the candidate seam is
  now a typestate pair), then run `make check-workflow-boundaries`.
- [ ] 5.9 Sync the docs:
  - phase 4 in `docs/next/formal-verification.md` (the token is now carried
    by type to the write);
  - N8 (group 5 done), noting that N5 (`fixture::write_body`) remains open.

## 6. Explicit smoke-binary roles (design G6)

- [ ] 6.1 Confirm by grep that `crash-kill-points` gates only
  `services/kill_point.rs`, the two `Cargo.toml` feature lines, and the
  Makefile build. Add a small policy check asserting this to
  `make check-policy`, and confirm it fails first by adding a temporary
  second `cfg(feature = "crash-kill-points")` site.
- [ ] 6.2 Add the role table to `scripts/crash-recovery-smoke-driver.py`:
  - victim is the kill-point binary when supplied, and the ordinary binary
    otherwise;
  - relaunch is always the ordinary binary;
  - every `launch_app` call names a role;
  - each scenario's artifacts record the role and binary.
- [ ] 6.3 Add the marker guard. It reads the ordinary binary's bytes, and
  `target/release/lushtext` if present, fails on `LUSHTEXT_KILL_AT` or the
  abort message, and writes `assertions/no-kill-points.json`. Confirm it fails
  first by pointing it at the kill-point binary as the ordinary one.
- [ ] 6.4 Run `make crash-recovery-smoke`. Confirm that every external-signal
  scenario passes with the kill-point victim, every relaunch is the ordinary
  binary, and the guard passes. Record the artifacts.
- [ ] 6.5 Sync the docs:
  - `docs/end-user-coverage.md` and the crash-smoke section of
    `.agents/rules/build.md` (roles and guard);
  - K5 in `docs/next/formal-verification.md` (the release check is now
    automated);
  - N8 (group 6 done).

## 7. Close the change

- [ ] 7.1 After each applied group, run `make check` and `make test`.
- [ ] 7.2 In `docs/next/formal-verification-next.md`, mark N8 done or
  re-defer each unapplied group with its trigger.
- [ ] 7.3 Run `openspec validate apply-verification-altitude-redesigns --strict`.
