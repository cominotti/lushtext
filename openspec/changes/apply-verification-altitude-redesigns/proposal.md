## Why

The two `/simplify` passes over the formal-verification work proposed six
redesigns that `consolidate-formal-verification-on-kani` deliberately left out
(N8 in `docs/next/formal-verification-next.md`). Each one removes a place where
a safety property holds only by convention:
- hand-copied shell loops around proven protocol cores (three when proposed,
  five since `stream-replace-all` added the Replace All and Undo shells);
- app-data directory names spelled independently in several places;
- an unbounded once-per-pass retry;
- a `&str` tag that can name a label outside the closed set;
- an `Option` token checked at runtime;
- smoke-binary roles chosen by ad hoc fallbacks.

None of them is urgent on its own. The programme ledger scheduled the change
after `rust-1-100-adoption` and `stream-replace-all` (both merged and archived
by `main` 187c6299), and `verify-shell-conformance-against-proven-cores`, which
comes next, builds on group 1's shared driver. The plan was re-derived against
187c6299 (revision of 2026-11-18, design "Revision against 187c6299"). All six
groups still apply. Groups 1, 3, and 4 changed in substance, groups 2 and 6 in
detail, and group 5 is unchanged except for a recorded scope note. The groups
stay **independently applicable**.

## What Changes

1. **One `Protocol` driver for every proven-core shell.**
   - A GTK-free, I/O-free `services/protocol_drive.rs` holds a `Protocol` trait
     (`Action`, `Outcome`, `End`, `end`, `advance`) and one `drive` loop.
   - Five shells use it:
     - in `services/durable_write.rs`: the atomic write, `run_rename_protocol`,
       and `move_durable`;
     - the Replace All apply shell (`content_search/replace/apply.rs`);
     - the Replace All Undo shell (`content_search/replace/undo.rs`).
   - Each trait implementation sits beside its shell and delegates to the
     core's unchanged inherent `start` / `step`, so no Kani-checked module
     changes.
   - No new harness. Every existing harness already proves that its core
     reaches `Finish` within a bound.
   - No behaviour change.
2. **One owner for the app-data layout.**
   - A new GTK-free `services/app_data_layout.rs` owns every relative name
     under the data home. Today these are 25 name constants in 17 files (one
     is the inventory's duplicate of `workspace-notes`), plus about a dozen
     raw literals in the inventory.
   - The startup leftover sweep (`app_data_leftovers.rs`) and the
     format-upgrade inventory (`format_upgrade/inventory.rs`) derive what they
     visit from it through exhaustive matches.
   - Services resolve their paths through it.
   - `json_store::data_dir()` is unchanged.
   - No on-disk path changes, proved by a characterization table captured
     before the move.
3. **Bounded backoff for unrestored-copy retries.**
   - Retries of a failed set-aside copy run only from the periodic 5-second
     autosave timer. They no longer run from every autosave pass, which also
     runs from the first-dirty debounce and pending reruns.
   - The spacing grows exponentially in timer ticks, up to a cap of 12 ticks
     (one minute).
   - The autosave hold on the id is released only by a copy that exists, never
     by the backoff.
4. **`WriteLabel` all the way to the temp name.**
   - `temp_name::format`, `unique_temp_path`, and every `durable_write` entry
     point, `copy_durable` and `move_durable` included, take `WriteLabel`
     instead of `tmp_tag: &str`.
   - `kill_point::reach` keeps its `&str` narrowing key, because two of its
     seven callers narrow by draft id rather than by write label.
   - Internal API only.
5. **Registered and unregistered draft candidates as distinct types.**
   - `DirtyDraftCandidate.registered: Option<RegisteredDraft>` becomes an
     unregistered candidate plus a `WritableDraftCandidate` that owns its
     `RegisteredDraft`.
   - Registration is the only conversion.
   - The two asynchronous body-write sites take only the writable type. Their
     runtime "recovery metadata was not registered" errors become
     unrepresentable.
6. **Explicit smoke-binary roles.**
   - The crash-recovery smoke driver gets one role table in place of the
     `binary or args.binary` fallback:
     - `seed`, ordinary;
     - `victim`, the kill-point binary when supplied;
     - `relaunch`, always ordinary.
   - Running the kill-point binary as the victim of the external-signal
     scenario shows on every run that the binary is inert without
     `LUSHTEXT_KILL_AT`.
   - An automated guard fails the smoke if the ordinary or release binary
     carries kill-point markers. It replaces the one-off manual `strings`
     check (programme record, K5).
   - A policy check keeps the `crash-kill-points` feature gating only
     `kill_point.rs`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `durable-file-write-contract`:
  - ADDED "Proven-protocol shells run through one shared driver" and
    "Proven-protocol shells hold no loop of their own" (group 1).
    Together they pin the single per-action site where the conformance
    change observes every action and injects crashes.
  - ADDED "The app-data layout has one owner" and "The leftover sweep and the
    upgrade inventory take their coverage from the layout owner" (group 2).
- `draft-session-recovery` (group 3): ADDED "A failed set-aside copy of an
  unrestored body keeps its hold" and "Unrestored-copy retries back off on
  the periodic autosave timer".
- `crash-restart-recovery-coverage` (group 6): ADDED "Smoke binary roles are
  explicit" and "The ordinary binary is checked free of kill points".

Groups 4 and 5 change no requirement. They re-express, in types, behaviour
the existing requirements already demand: closed write labels, and "a draft
body is never written for an unregistered id". design.md justifies each.

## Impact

- **Group 1:**
  - new `services/protocol_drive.rs`;
  - `services/durable_write.rs`;
  - `services/content_search/replace/{apply,undo}.rs`.

  Untouched: `services/filesystem/write_protocol.rs` and
  `services/content_search/replace/protocol.rs`, which are Kani oracles.
- **Group 2:**
  - new `services/app_data_layout.rs`;
  - `app_data_leftovers.rs` and `format_upgrade/{inventory,backup}.rs`;
  - `draft_service.rs`, `draft_service/set_aside.rs`,
    `local_history_service.rs`, `bookmark_service.rs`,
    `document_note_service.rs`, `folder_note_service.rs`, `json_store.rs`,
    `recovery_metadata.rs`, `search_backup.rs`, `session_service.rs`,
    `workspace_manager.rs`, `saved_searches.rs`, `search_history.rs`,
    `recent_documents.rs`, and `migration_ledger.rs`.
- **Group 3:**
  - `ui/window/drafts/{policy,journal,autosave_execution,evidence}.rs` and
    `ui/window/imp.rs` (the retry map's entry type);
  - the `WFR-DRAFT-RECOVERY` row;
  - a widget test in `crates/lushtext/tests/widget/window.rs`.
- **Group 4:**
  - `services/filesystem/{temp_name,write,leftovers}.rs`;
  - `services/durable_write.rs`, with `services/kill_point.rs` documentation
    only.
- **Group 5:**
  - `ui/window/drafts/{seams,autosave_execution}.rs`;
  - the `WFR-DRAFT-RECOVERY` row.

  `journal_core.rs` and `model/draft.rs` are not edited.
- **Group 6:**
  - `scripts/crash-recovery-smoke-driver.py` and
    `scripts/run-crash-recovery-smoke.sh`;
  - a new `scripts/check-kill-point-gating.sh`, wired into `make check-policy`.
- **Kani:** none. No proven module is touched. That means `write_protocol.rs`,
  `replace/protocol.rs`, `journal_core.rs`, and `set_aside_retention.rs`, as
  well as every `kani_proofs.rs`. A guard task in each group confirms this.
  If any of them changes after all, the affected shard is run as
  verification-runs.md prescribes. No proof-strength run, no new shard
  budget, and no `kani.yml` dispatch are owed.
- **GTK axioms:** none. No new toolkit behaviour is relied on.
- **Timing verdict:** none. No group changes a hot path (design, "Timing").
- **Mutation:**
  - the `durable_write.rs` `exclude_re` entries in `.cargo/mutants.toml` are
    **already stale on `main`**: `FileWriteLock` is gone, and the non-Unix
    `MetadataPlan` stubs moved from lines 504/515 to 743/754. Group 1 or 4,
    whichever lands first, fixes them as a pre-existing blocker;
  - `make mutants-diff` runs once on the final tree.
- **Smokes:**
  - `make crash-recovery-smoke` (groups 1, 2, 4, and 6);
  - the three UI-prefix smokes (visual-geometry, accessibility, visual) for
    groups 3 and 5, whose files land in the final commits.
- **Docs:**
  - `docs/next/formal-verification-next.md` (N8 status per group, and the
    change row);
  - `docs/next/formal-verification.md` (phase 4 retry wording, K5, the
    deferral inventory, and the "build on" section);
  - `docs/durable-writes.md`, `docs/workflow-readability-matrix.md`,
    `docs/end-user-coverage.md`, and `AGENTS.md` (module layout, "Filesystem
    boundary", "Draft persistence");
  - `.agents/rules/rust.md` (app-data layout ownership) and
    `.agents/rules/build.md` (the crash smoke's roles and guard).
