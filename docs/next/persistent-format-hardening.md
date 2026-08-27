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

### M-3 — premature teardown survives a refused close (MEDIUM, tabs workflow)

`ui/window/documents.rs:1107-1109` runs `cancel_load()`, `stop_file_monitor()`,
and `untrack_editor_memory()` **before** `close_page`, which
`handle_tab_close_request` can refuse. `start_file_monitor` is only re-armed by a
load or buffer-replacement completion, so a tab that survives a cancelled close
**permanently loses external-change detection**. `handle_tab_detached` already
does this cleanup on real detach, so the close is to delete the three eager calls.
Owner: `WFR-SHELL-LAYOUT` / the tabs workflow.

### M-11 — note editor Escape discards typed prose (LOW, needs-decision)

`ui/window/notes/editor_execution.rs` sets `RESPONSE_CANCEL` as the close response
with no unsaved guard. Escape-as-Cancel is conventional, so this is a product
decision. Note that slot 5a *did* fix the adjacent write-failure case: a failed
save now re-presents the editor pre-filled with the recovered text.
