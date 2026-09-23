## Why

The formal-verification programme (`docs/next/formal-verification.md`, created
by this change) started by writing LushText's crash-durable write, draft
journal, and viewport-slice feedback loop down as explicit state machines. That
reading surfaced defects before any model or proof existed. One of them can
hide a user's unsaved edits and wedge the draft journal after a single crash.
Phase 0 fixes what the reading found, so that the later phases model a system
whose known gaps are already closed and whose specs say what the code does.

## What Changes

- **Draft journal wedge (data safety).** Today the body of a *new*
  file-backed draft is written before its manifest entry exists, and the
  manifest is committed once, after every body in the pass. A crash inside
  that window leaves an unregistered stable-path-hash body. Reconciliation
  classifies it as ambiguous, so every later manifest update fails as
  `Partial`: the body is never offered, dirty flags never clear, and orphan
  cleanup stays disabled until the user happens to edit that file again, which
  overwrites the orphaned edits. The fix registers an entry before the first
  body write of an unregistered id. Reconciliation then recovers already
  wedged installations by matching unregistered hash bodies against session
  tab paths. Any body that is still unattributable is preserved out of the
  inventory's way, so it stops blocking the journal.
- **Stale file-backed drafts are set aside, not deleted. BREAKING (spec
  behaviour).** When the backing file's mtime changed, the draft body is
  currently deleted without confirmation. The comparison has second
  granularity, and a `git checkout` or a `touch` is enough to trigger it. The
  body will instead be preserved as a local-history snapshot of the file (the
  existing `Periodic` origin), falling back to `drafts/set-aside/`, and the
  alert will offer "Show in Local History".
- **ViewportSliceBin swallowed scroll request.** In `imp.rs`, a genuine child
  request that coincides with a geometry reconfiguration is suppressed by
  `reconfigure_shift` and then overwritten by the settle write-back, rather
  than deferred. A widget probe proves reachability first, and then the fix
  lands. The test comment that claims deferral is corrected in either case.
- **Local-history migration copy fallback.** `rename_durable(...).or_else(copy)`
  currently falls back to a copy on any error, including a rename that
  succeeded but whose parent-directory sync failed. The fallback will run only
  for a cross-device rename (`EXDEV`).
- **Durable-write crash leftovers.** Hidden `.{name}.{tag}.{pid}.{seq}.tmp`
  files are never swept. The fix adds bounded, provably-ours cleanup to the
  app-data directories, plus same-target cleanup when LushText writes a
  workspace file.
- **Relative-path parent.** `Path::parent()` of a bare file name is `Some("")`,
  which makes the parent-directory sync `openat("")`. Such parents will be
  normalised to `.`.
- **Programme record.** Create `docs/next/formal-verification.md`, which
  records phases 0–5 and the pragmatic tool mix the maintainer chose on
  2026-09-23. Update the stale `docs/next/draft-mtime-validation.md`, which
  still says "Deferred" for behaviour that has shipped.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `draft-session-recovery`: adds write-ahead registration for unregistered
  draft ids, and adds a requirement that no unattributable draft body can
  wedge the manifest.
- `draft-restore-validation`: a stale file-backed draft is set aside
  recoverably instead of deleted. The cleanup requirement changes accordingly.
- `durable-file-write-contract`: adds bounded crash-leftover cleanup and
  relative-path parent normalisation.
- `gtk-lush-viewport-slice`: a request suppressed during a geometry
  reconfiguration is deferred and re-decided, never erased.
- `local-history`: lineage migration falls back to a copy only for a
  cross-device rename.

## Impact

- **Code:**
  - `crates/lushtext-core/src/services/draft_service.rs`
  - `crates/lushtext-core/src/ui/window/drafts/` (autosave execution, journal,
    policy, restore execution)
  - `crates/lushtext-core/src/services/durable_write.rs`
  - `crates/lushtext-core/src/services/filesystem/`
  - `crates/lushtext-core/src/services/local_history_service.rs`
  - `crates/lushtext-core/src/ui/info_bar/`, for the stale-draft alert action
  - `crates/gtk-lush/widgets/src/viewport_slice_bin/imp.rs`
  - `crates/gtk-lush/widgets/src/scroll_request.rs`
- **Persisted formats:** none. Stale drafts reuse the existing `Periodic`
  local-history origin, and `drafts/set-aside/` sits outside the manifest.
- **Tests:**
  - new crash-window integration tests for drafts
  - a widget probe for the swallowed request
  - unit tests for the `EXDEV` classification, leftover sweep, and relative
    parent
  - every fix gets a test that fails before the fix
- **Docs:**
  - `docs/next/formal-verification.md` (new)
  - `docs/next/draft-mtime-validation.md`
  - `docs/workflow-readability-matrix.md` rows touched by the drafts workflow
  - the GTK Lush widgets CHANGELOG
  - AGENTS.md "Draft persistence" design decision
