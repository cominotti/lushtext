## Why

The two `/simplify` passes over the formal-verification work proposed six
redesigns that `consolidate-formal-verification-on-kani` deliberately left out
(N8 in `docs/next/formal-verification-next.md`). Each one removes a place where
a safety property holds only by convention:
- three hand-copied protocol shell loops;
- app-data directory names spelled independently in three places;
- an unbounded once-per-tick retry;
- a `&str` tag that can name a label outside the closed set;
- an `Option` token checked at runtime;
- smoke-binary roles chosen by ad hoc fallbacks.

None of them is urgent. N8 says to take them up "when a change touches that
area anyway". This change therefore packages them as **six independently
applicable groups**, so any one group can land with, or ahead of, the work
that touches its area.

## What Changes

1. **Common `Protocol` driver.**
   - A `Protocol` trait (`Action`, `Outcome`, `terminal`, `step`) plus one
     `drive` function replaces the three shell loops in
     `services/durable_write.rs` (`atomic_write_*`, `run_rename_protocol`,
     `move_durable`).
   - The inherent `const fn start` / `step` of `WriteProtocol`,
     `MoveProtocol`, and `RenameProtocol` stay; the trait delegates to them.
     The Kani harnesses keep driving the same functions.
   - No behaviour change.
2. **One owner for the app-data layout.**
   - A new GTK-free `services/app_data_layout.rs` owns every relative name
     under the data home: files, directories, and nested families such as
     local-history lineages and format-upgrade runs and their `items/`.
   - Its one entry table is what the startup leftover sweep
     (`app_data_leftovers.rs`) and the format-upgrade inventory
     (`format_upgrade/inventory.rs`) iterate, instead of their hand lists and
     duplicated literals.
   - Services resolve their paths through it.
   - Root resolution (`json_store::data_dir()`) is unchanged.
   - No on-disk path changes, proved by a characterization table captured
     before the move.
3. **Bounded backoff for unrestored-copy retries.**
   - A pure `unrestored_copy_retry_delay` in `ui/window/drafts/policy.rs`
     spaces the autosave-tick retries of a failed set-aside copy
     exponentially, up to a cap, instead of spawning a worker every 5 s
     forever.
   - The autosave hold on that id is never released by the backoff, only by
     a copy that exists.
4. **`WriteLabel` all the way down.**
   - `temp_name::format`, `unique_temp_path`, `kill_point::reach`, and the
     `durable_write` entry points take `WriteLabel` instead of `tmp_tag: &str`.
   - "Every name the builder emits parses" becomes a type guarantee rather
     than a tested convention.
   - Internal API only.
5. **Registered and unregistered draft candidates as distinct types.**
   - `DirtyDraftCandidate.registered: Option<RegisteredDraft>` splits into an
     unregistered candidate and a `WritableDraftCandidate` that owns its
     `RegisteredDraft`.
   - The registration stage is the only conversion, and both body-write
     sites take only the writable type.
   - The two runtime "recovery metadata was not registered" errors become
     unrepresentable.
6. **Explicit smoke-binary roles.**
   - The crash-recovery smoke driver gets one role table (victim, relaunch)
     in place of `binary or args.binary` fallbacks.
   - The kill-point binary serves as the victim in every crash scenario it
     can serve, since it is inert without `LUSHTEXT_KILL_AT`. That proves its
     inertness on every run. Relaunch always uses the ordinary binary.
   - A new automated guard fails the smoke if the ordinary or release binary
     contains kill-point code. Today that check is a manual `strings` step
     recorded once.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `durable-file-write-contract` (group 2): ADDED "The app-data layout has one
  owner". The sweep's directory coverage and the upgrade inventory come from
  the same table as the services' paths.
- `draft-session-recovery` (group 3): ADDED "A failed set-aside copy of an
  unrestored body keeps its hold and retries with bounded backoff". This
  codifies the hold invariant, which today is recorded only in the programme
  record, and the new retry cadence.
- `crash-restart-recovery-coverage` (group 6): ADDED "Smoke binary roles are
  explicit and the ordinary binary is checked free of kill points".

Groups 1, 4, and 5 change no requirement. They re-express, in types, behaviour
the existing requirements already demand: the I/O-free verified core,
closed write labels, and "never written for an unregistered id". design.md
justifies each.

## Impact

- **Group 1:** `services/durable_write.rs`, `services/filesystem/write_protocol.rs`
  (the trait implementations only).
- **Group 2:**
  - new `services/app_data_layout.rs`;
  - `app_data_leftovers.rs` and `format_upgrade/inventory.rs`;
  - `draft_service.rs` and `draft_service/set_aside.rs`;
  - `local_history_service.rs`, `bookmark_service.rs`,
    `document_note_service.rs`, and `folder_note_service.rs`;
  - `json_store.rs`, `recovery_metadata.rs`, `search_backup.rs`,
    `session_service.rs`, `workspace_manager.rs`, `saved_searches.rs`,
    `search_history.rs`, `recent_documents.rs`, and `migration_ledger.rs`.
- **Group 3:**
  - `ui/window/drafts/{policy,journal,autosave_execution,evidence}.rs`;
  - the `WFR-DRAFT-RECOVERY` row.
- **Group 4:**
  - `services/filesystem/{temp_name,types,write,leftovers}.rs`;
  - `services/durable_write.rs` and `services/kill_point.rs`.
- **Group 5:**
  - `ui/window/drafts/{seams,autosave_execution,journal}.rs`;
  - `services/draft_service.rs` (the `RegisteredDraft` minting surface);
  - the `WFR-DRAFT-RECOVERY` row.
- **Group 6:**
  - `scripts/crash-recovery-smoke-driver.py` and
    `scripts/run-crash-recovery-smoke.sh`;
  - `Makefile` (`crash-recovery-smoke`).
- **Kani:**
  - groups 1 and 4 re-run the `write_protocol` harnesses;
  - group 5 re-runs the `core-journal-and-write` and `core-second-writer`
    shards;
  - groups 2, 3, and 6 touch no checked core.
- **Mutation:** the calibrated `durable_write.rs` `exclude_re` entries are
  anchored to lines, so they must be re-verified after groups 1 and 4.
- **Docs:**
  - `docs/next/formal-verification-next.md` (N8 status per group);
  - `docs/next/formal-verification.md` (the deferral inventory, where it
    applies);
  - `docs/durable-writes.md`, `docs/workflow-readability-matrix.md`, and
    `AGENTS.md`;
  - `.agents/rules/rust.md` (filesystem layout ownership).
