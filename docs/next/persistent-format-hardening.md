# Persistent Format Hardening

## Status: Proposed

## Summary

LushText should keep its current storage split:

- GSettings for desktop-integrated preferences and window state
- Pretty JSON for app-owned persistent state under `$XDG_DATA_HOME/lushtext`
- Plain UTF-8 files for draft bodies and local-history snapshot bodies

This keeps the current data easy to inspect, recover, and quarantine without
introducing a database before the product needs database-shaped queries. The
next hardening step is a clean public-era format contract for the JSON files we
write from that point forward.

Current implementation note: public-era app-owned JSON now uses v1 envelopes as
the baseline. While v1 remains latest, format-upgrade scans are no-op for v1 and
missing metadata. Future version steps should add converter fixtures under the
sealed `services::format_upgrade::legacy` path, not latest-version runtime
readers.

## Current Format Fit

Pretty JSON is a better fit than TOML for current persistent state because
most files are app-owned state, not user-authored configuration. Session
restore data, workspaces, saved searches, draft manifests, migration ledgers,
sidecars, local-history indexes, and Replace All undo journals are structured
documents that the app reads and writes as whole values.

TOML remains a possible future fit for advanced user-authored configuration in
`$XDG_CONFIG_HOME`, but it should not replace app-owned state under the data
directory.

## SQLite Decision

SQLite is not a good primary persistence fit today. The current state remains
small-document oriented: load a JSON object, mutate it, write it durably, and
preserve damaged inputs through recovery metadata. Moving those files into a
database now would add migration and support complexity without removing a
current bottleneck.

SQLite becomes attractive when a feature needs indexed local queries across
many records rather than durable storage for one small document.

Good future SQLite candidates:

- A global notes and bookmarks knowledge surface with tags, backlinks,
  favorites, archive state, sort/filter facets, and instant search
- A metadata index over note, bookmark, folder-note, and local-history
  sidecars while keeping JSON or text bodies as the inspectable source of truth
- A persistent command-palette file index for very large workspaces with mtimes,
  ignore state, ranking data, and last-opened signals
- Workspace-wide local-history browsing or search across many lineages
- Sync-oriented metadata such as revisions, tombstones, conflict records, and
  change journals

For local history specifically, SQLite should not own snapshot bodies. Snapshot
bodies should stay as plain UTF-8 files. SQLite would become useful only if
history grows from "browse the active file" into cross-document timelines,
global search, diff selection, or thousands of retained snapshots.

## Local History Bottleneck Line

The current local-history MVP is intentionally bounded. It stores one JSON
index per document lineage plus plain-text snapshots, caps retention, and keeps
startup reconciliation bounded. That design is healthy while the feature only
needs per-document browsing and restore.

SQLite becomes worth revisiting if any of these become product requirements:

- Retention grows from a small bounded cache to thousands of snapshots
- The UI needs a global or workspace-wide history browser
- The app needs instant search across history metadata or bodies
- Startup, browsing, or repair spends meaningful time scanning many lineage
  directories
- Retention policy needs efficient cross-document pruning by path, time, origin,
  hash, or workspace

Until then, JSON indexes plus text snapshot bodies are simpler and more
recoverable.

## JSON Hardening Direction

Before public announcement, LushText should make app-owned JSON explicit and
allow a clean break from pre-public bare JSON shapes:

- Add a small versioned envelope for long-lived JSON documents.
- Do not add permanent legacy bare-JSON readers.
- Treat unsupported pre-public JSON as unsupported metadata: preserve it through
  quarantine or in-place diagnostics before writing a v1 replacement.
- If conversion is useful for public-era version steps, keep the old-format
  parser and converter in `services::format_upgrade::legacy`, with tests that
  convert one version step at a time. Latest-version runtime readers should not
  gain permanent old-shape branches.
- For the v1 baseline, no converter is needed. Pre-public bare files are still
  preserved through recovery diagnostics and reset to v1 defaults only when
  replacement is safe.
- Use recovery-aware loading for important user-managed files that still fall
  back to empty state today, especially `workspaces.json` and
  `saved-searches.json`.
- Preserve damaged metadata before replacement through the existing recovery
  quarantine flow.
- Add golden fixtures for valid v1 files, missing optional fields, unknown
  fields, malformed files, oversized files, and unsupported old-shape handling.
- Keep low-value ephemeral state, such as recent search history, allowed to
  degrade to empty with a diagnostic.
- Use explicit stable hashing for persisted identifiers instead of
  process- or implementation-dependent hashers.

## Compatibility Goal

After the hardening pass, every public-era app-data format should answer these
questions without guessing:

- What logical document type is this?
- Which format version is it?
- Can this version be read directly?
- If not, was the unsupported metadata preserved before replacement?
- If the file is malformed, was it preserved before replacement?
- Which fields are optional and defaultable?

That gives LushText a clear promise: public users can update the app without
their local state becoming invisible, silently discarded, or trapped in a
format the next release cannot explain. Pre-public app data may be reset, but it
should not be silently overwritten without diagnostic evidence.

## Confirmed open data-safety findings, from slot 5a's audit

These were found by the `data-safety` pass in
`migrate-workspace-tree-and-notes-workflow-readability` and are **recorded here
rather than in that change's directory**, because a change directory is archived
and these outlive it. Each is a concrete path by which app data becomes invisible
or is destroyed; none is fixed. Sites are as of that audit.

### M-5 — the startup format gate fails open on a scan failure (MEDIUM)

`format_upgrade::FormatPlan` carries only `groups` (`services/format_upgrade/plan.rs:16-19`),
so `build_plan` **structurally discards** `inventory.diagnostics`, and
`requires_startup_decision()` walks only `groups`. If `bookmarks/` is a file
instead of a directory, or is unreadable, `scan_json_directory` records a
diagnostic and contributes zero items — the plan is empty, startup continues
silently, and **every record in that directory is invisible with no warning**.
The gate's own contract is that consumers wait "until app-owned metadata is known
to be current"; an unreadable directory is not current, and the gate says nothing.
Close: carry diagnostics onto `FormatPlan` and surface an advisory row.

### M-6 — a partial Convert loses the pointer to its own backup (MEDIUM)

A mid-loop `?` at `services/format_upgrade/apply.rs:246-262` discards `failures`,
`converted_count`, **and the backup-manifest handle**. Durability is fine and
retry is idempotent, but backup items use hashed leaf names and the manifest path
exists only in a `tracing::info!`, so after a partial Convert the user sees a bare
I/O error with no indication that a recoverable backup exists or where it is.
Close: return the partial outcome with its manifest path and name it in the
re-presented dialog.

### M-7 — Start Fresh can delete bytes it never copied (MEDIUM)

`services/format_upgrade/backup.rs:262-272` calls
`ensure_regular_file_unchanged` before the copy, the read, and the removal, but
**no `TargetWriteGuard` spans check → write**, and `modified_at_secs` is
whole-second granularity. For Convert this is benign — the old bytes are in the
backup. For **Start Fresh** it deletes a file whose content changed inside the
same second as the check, and that content was never copied anywhere.
Close: hold the target write guard across the check-copy-remove sequence.

### M-8 — a transient read failure quarantines the live workspace file (MEDIUM)

`RecoveryProblem::Unreadable` is classified identically to structural corruption
(`services/recovery_metadata.rs:579-587`, `:825-845`) and triggers
`preserve_original`, which renames the live file away and returns default state
with `replacement_allowed = true` — after which the sidebar persists an **empty**
configuration over it. An `EMFILE`, `ENOMEM`, or `EIO` blip at startup therefore
empties the user's workspace list on disk. Close: separate "unreadable right now"
from "structurally corrupt" and refuse replacement for the former.

### M-9 — no retry route for a `record_pending` failure (MEDIUM, needs-decision)

If the migration ledger cannot record pending work, the rename proceeds and
nothing retries; and after `MAX_MIGRATION_ATTEMPTS = 3` a kind is skipped forever
with only a warning. Close: decide whether exhaustion should surface a
user-actionable state rather than a log line.

### M-10 — `FormatPlanGroupKind::Guarded`'s doc over-promises (LOW)

Group atomicity is enforced only in the backup phase, not in the write loop, so a
mid-loop `?` can split a Guarded group across two format versions. Contained: the
gate re-fires next launch and both halves have backups. Close: either enforce it
in the write loop or narrow the doc.

### M-11 — note editor Escape discards typed prose (LOW, needs-decision)

`ui/window/notes/editor_execution.rs` sets `RESPONSE_CANCEL` as the close response
with no unsaved guard. Escape-as-Cancel is conventional, so this is a product
decision. Note that slot 5a *did* fix the adjacent write-failure case: a failed
save now re-presents the editor pre-filled with the recovered text.

### M-3 — premature teardown survives a refused close — **CLOSED**

**Fixed in slot 7a (`complete-residual-workflow-readability`, task 7.4).** Recorded
as closed rather than deleted, because slot 5b found the same defect independently
from the delete path (its finding 4, via `close_tab_for_path`) and a later reader
meeting either report needs to know it is one defect and it is done.

The defect as found: `ui/window/documents.rs` ran `cancel_load()`,
`stop_file_monitor()`, and `untrack_editor_memory()` — and also retired the
editor's three `open_paths` keys — **before** `close_page`, which
`handle_tab_close_request` routes to the save-changes dialog for a modified tab and
which the user may cancel. `start_file_monitor` is only re-armed by a load or
buffer-replacement completion, so a tab surviving a cancelled close permanently
lost external-change detection; and because a cancelled in-flight load sets
`has_incomplete_load_installation`, **autosave then skipped that tab's draft**, so
unsaved work lost its recovery record after an action the user declined.

The fix was to **delete** the eager block, not move it: `handle_tab_detached`
(now `ui/window/tab_strip/close_execution.rs`) already performs all four operations and is wired to
`AdwTabView::page-detached`, so it runs exactly once the page really detaches.
Slot 5b's handoff said "move", which taken literally would have **duplicated** the
teardown. The same deletion also removed a premature `open_paths` retirement from that
block. An earlier revision of this entry called that a second user-visible defect
(a duplicate tab on re-open); **that claim was withdrawn after measurement.**
Reverting only the `open_paths` removal does not reproduce a duplicate tab,
because the load-completion path calls `reconcile_open_paths_from_tabs()` and
heals the set from the live tabs. The removal was redundant and premature, and
deleting it is still correct, but it was not demonstrably reachable.

Regression test: `test_close_tab_for_path_defers_teardown_until_the_page_detaches`
in `crates/lushtext/tests/widget/sidebar.rs`, proved to fail without the fix by
deliberate revert. It needed no new actuation seam — the unanswered dialog *is*
the pending-confirmation state, and the two load facts are read from the migrated
load row's existing `LoadEvidence` surface.

## Confirmed open data-safety findings, from slot 5b's audit

Found by the `data-safety` pass in `migrate-workspace-tree-workflow-readability`
and **landed here in slot 7a**. They are recorded here for the same reason as the
slot 5a set above, and for a sharper one: slot 5b's handoff named durable homes
that were never written, one of them *"this change's own Appendix B.2"* — a
directory that is now archived. A grep for these symbols across `docs/` and
`.agents/` returned **zero hits** for five consecutive slots. Sites below were
**re-verified against the code in slot 7a**, and one had already moved.

### S5B-1 — the note-sidecar rename/ledger window (MEDIUM)

`ui/window/notes/journal.rs`. The migration intent must be recorded durably
**before** the guarded rename, not after. As written, a crash between the rename
and the ledger write leaves a sidecar whose on-disk name no longer matches any
recorded intent, so recovery cannot tell a completed migration from an abandoned
one. Close: write the pending intent, then rename under the guard, then clear.
Owner: `WFR-NOTES-BOOKMARKS` (migrated) — the fix is independent of that row's
structure. Related capability: `persistent-json-format-contract`.

### S5B-2 — the file monitor is never re-armed on the new path (MEDIUM)

`ui/editor_page/document_identity.rs`. `set_file_path_with_canonical()` →
`republish_document_identity()` (`:44`, `:65`, definition `:74`) **never calls
`start_file_monitor()`**, so after a rename or Save As the editor keeps watching
the *old* path — or nothing. An external edit to the new path is therefore
undetected, and **the next save silently overwrites it**. This is the same
re-arming gap M-3 above depended on, reached from a different direction, which is
why closing M-3 does not close this. Close: re-arm the monitor on the new path as
part of republishing identity. Owner: `WFR-DOCUMENT-LOAD` / `WFR-DOCUMENT-SAVE`
boundary. Capability: **`external-file-monitor-coverage`**.

### S5B-3 — unguarded sidecar read-merge-write (MEDIUM)

`services/bookmark_service.rs`, `merge_bookmark_target` (`:351`). Only
`save_document` acquires a `TargetWriteGuard`, so a concurrent bookmark merge can
read, merge, and write the sidecar across another writer's replacement and drop
that writer's bookmarks. Close: acquire the same stable target guard for the
read-merge-write sequence. Owner: `WFR-NOTES-BOOKMARKS` (migrated); the fix is
structure-independent. Cross-referenced from `docs/next/bookmarks.md`.

### S5B-4 — **HIGH** — close proceeds while a pre-persist workspace mutation is in flight

`ui/sidebar/membership_execution.rs:41` (`handle_add_folder_to_workspace`) plus
`close_decision` (`ui/sidebar/policy.rs:512`). A window close can be confirmed
while a workspace-membership mutation has been applied in memory but not yet
persisted, so the mutation is lost with no warning — the user adds a folder, closes
the window, and the folder is gone. **Slot 5b recorded this at HIGH severity**, and
its site has already **moved once** under slot 5b's own dissolution, which is
precisely what an archived-only handoff produces. Close: make the close decision
account for pending workspace persistence, as it already does for drafts and
sessions. Owner: `WFR-WORKSPACE-TREE` (migrated). Capability:
**`workspace-state-persistence`**. Cross-referenced from
`docs/next/workspace-context-switching.md`.

### S7B-1 — **LOW** — a startup-critical metadata file that cannot be *read* opens the gate

`services/format_upgrade/plan.rs:188` maps `FormatClassification::Damaged` to
`FormatPlanAction::ReportOnly`, and `requires_startup_decision` fires only for
`ConvertToLatest | StartFreshOnly`. Every **error** path in
`services/format_upgrade/inventory.rs` — status failure, facts failure, over the
byte ceiling, read failure, JSON parse failure — produces `Damaged`. So a
startup-critical file the preflight could not read does **not** hold the gate.

**Audited and cleared for the question task 7.6 actually asks**, with the
reasoning recorded rather than the conclusion alone: the preflight cannot admit
an **unmigrated** format on an error path, because `UpgradeableOld` and
`FutureVersion` are only ever produced from a *successfully parsed* envelope. An
error yields `Damaged` and nothing else.

**And the downstream defence is real**: an incompletely read draft inventory
produces an untrusted `DraftManifestAuthority`, whose
`DraftManifestReplacementEligibility::Ineligible` refuses destructive cleanup and
manifest replacement. The conservative direction holds without the gate.

Left open deliberately: making `Damaged` hold the gate would block startup on any
transient I/O hiccup, which is worse than the state it prevents. Close condition:
only if a `Damaged` startup-critical file is shown to reach a consumer that
*rewrites* rather than refuses. Owner: `WFR-STARTUP-PREFLIGHT` (cross-cutting,
terminal). Capability: **`persistent-format-compatibility`**.

### S7B-2 — **LOW** — `load_recent_documents_async` has no re-entrancy guard

`ui/window/recent_documents_journal.rs`. The function clears
`removed_while_loading` at entry. A second concurrent call would clear the list
while the first load is still in flight, so a removal the user made against the
first load would be lost and the removed entry would come back.

**Cleared today on the call graph, not on the code**: there is exactly **one**
call site, `ui/window/mod.rs`'s construction path, once per window. The guard is
**absent rather than present**, which is why this is recorded instead of closed —
a second caller added later reintroduces the defect with nothing to stop it, and
the neighbouring startup gate (`startup_data_flow.running.replace(true)`) shows
the shape a guard would take. Close: add the guard when a second caller appears,
or now if that is cheaper than remembering. Owner: `WFR-RECENT-DOCUMENTS`
(migrated).

### S7B-3 — **INFORMATIONAL** — in-session recent entries are never re-checked for existence

`services/recent_documents.rs` prunes entries whose files are gone at **load**
(`dedupe_sort_prune_existing`), but entries added during the session by
`record_recent_open_for_editor` are not re-checked until the next start. A file
deleted outside LushText mid-session stays in the Open popover.

No data is lost: activating such a row runs the ordinary load path, which shows a
failed-load placeholder and its inline alert. Recorded so a future reader does
not mistake the asymmetry for an oversight. Owner: `WFR-RECENT-DOCUMENTS`.

### S7B-4 — **INFORMATIONAL** — a smoke-lane staleness guard that could never pass

`scripts/run-performance-smoke.sh` asserted four **widget-harness** logs through
`smoke_assert_ran`'s default pattern `test result: ok\. [1-9]`. The custom widget
harness (`crates/gtk-lush/proof-harness/src/lib.rs:423`) prints
`test result: ok. all tests passed` — **no digit, ever** — so those four
assertions failed on a healthy tree, and the staleness condition they existed
for was unchecked in both directions.

Fixed in slot 7b with a shared `smoke_assert_widget_ran` in
`scripts/smoke-common.sh` asserting `^running [1-9][0-9]* tests`, the harness's
own header, which is precisely the staleness condition. **Not deferred** —
recorded here because the *class* is worth carrying: a guard whose pattern
cannot match the output it guards is indistinguishable from a guard that is
working, until the day the lane runs. Owner: the performance smoke lane.

### S7B-5 — **INFORMATIONAL** — forcing `run_dispose()` on a workspace section panics in its own dispose path

Driving the template-child clearing question for `LushtextWorkspaceSection`
(slot 7b, review item S3) produced:

```
Failed to retrieve template child ... fields of type `GtkListView`
panic in a function that cannot unwind
```

— raised **during** `run_dispose()`, inside the section's own `dispose()`, before
any evidence read. So a composite-template widget's children **are** cleared on
dispose even though no imp in this tree calls `dispose_template()`, which settles
the contradiction the three evidence modules carried; `ui/sidebar/evidence.rs`'s
reasoning is corrected there.

**Not reachable through normal teardown**, which is why this is informational
rather than a blocker: every window-closing path in the widget suite, the
crash-recovery smoke lane, and the visual lanes tear sections down by refcount
without tripping it. Only an explicit `run_dispose()` — which forces dispose
while references remain — reaches the ordering that does. Close condition: if a
future change disposes sections explicitly, guard the reads in `dispose()` first.
Owner: `WFR-WORKSPACE-TREE`.

### S7B-6 — **MEDIUM** — destroying a focused `GtkEntry` segfaults in GTK's Wayland input-method backend

Found by slot 7b's no-retry widget lane, which reported
`workspace_section::test_inline_rename_refuses_to_replace_an_existing_sibling` as
`FLAKY`. The "flake" was a **SIGSEGV**, reproducible in **9 of 40** isolated runs
across the four inline-rename tests, with this stack:

```
wl_proxy_get_version
gtk_im_context_wayland_global_get
gtk_im_context_wayland_get_global
notify_im_change              <- a zwp_text_input_v3 listener callback
wl_display_dispatch_queue_pending
gdk_event_source_dispatch
... wait_until -> flush_events
```

preceded by `gtk_widget_get_display: assertion 'GTK_IS_WIDGET (widget)' failed`
and `gdk_wayland_display_get_wl_display: assertion 'GDK_IS_WAYLAND_DISPLAY
(display)' failed`. GTK's per-display input-method global still referenced the
destroyed inline-rename entry when the compositor's next text-input event
arrived.

**The application side was probed and exonerated by measurement, not by
argument.** Three orderings were implemented and each measured over 30–40
isolated runs: grabbing focus to the owning `GtkListView` before unparenting
(**worse**: 22/30), clearing the window focus before unparenting (**worse**:
22/30), and keeping the entry alive past removal (**no help**). A first attempt
also showed why the probe matters — it guarded on `entry.has_focus()`, which is
`false` for a `GtkEntry` whose internal `GtkText` owns focus, so the guard never
fired and the "fix" was a no-op that looked correct in review. `cancel_rename`'s
production code is therefore **unchanged**.

**Mitigated in the test lane only**, by adding
`GTK_IM_MODULE=gtk-im-context-simple` to
`gtk_lush_proof_harness::recommended_pre_gtk_environment()` — the same family as
the `NO_AT_BRIDGE`, `no-portals`, and `GSK_RENDERER=cairo` entries already there:
the private compositor advertises a desktop service it cannot provide, so the
client half is not started against it. **0 failures in 40 runs** after.

**What is not fixed, and is why this entry exists:** the GTK race itself. A real
Wayland session runs the same backend, and the same sequence — confirm an inline
rename, entry destroyed while focused — is ordinary use. No crash has been
reported against the installed app, and the live-display walkthrough remains
user-gated, so this is recorded rather than claimed either way. The next owner
should reproduce against a real session before deciding whether to file upstream
or to carry an application-side workaround; note that the three obvious
workarounds are already measured above and none of them helps.
