## Context

LushText already has several recovery-oriented building blocks, but they are currently split across workflows that users do not experience as "local history." Draft persistence protects unsaved work during crashes and restart restore, session persistence rebuilds the tab set and cursor position, note sidecars already use canonical-path identity plus in-app rename migration, and the editor stack already applies size-based degradation for large files.

The new change crosses multiple layers:
- persistence and identity in `model/` and `services/`
- window action wiring and restore safety in `ui/window/`
- a new adaptive browse surface in GTK/Libadwaita resources and widgets
- follow-up documentation that keeps the MVP scope distinct from later enhancements

The strongest GTK-native fit for this repo is an on-demand browse flow, not another always-open panel. The current window shell already uses the right sidebar for narrow document metadata and global editor toggles, while browse-style secondary workflows already use `AdwDialog`.

## Goals / Non-Goals

**Goals:**
- Provide a deliberate local-history MVP for saved, file-backed documents.
- Capture snapshots automatically without blocking the GTK main thread.
- Let users browse snapshots in an adaptive GTK-native surface with a simple read-only preview.
- Make the browser easy to discover from keyboard and native context menus, not only the main menu or command palette.
- Make restore safe by taking a safety snapshot before replacement and offering an immediate undo path.
- Reuse canonical-path identity, in-app rename migration, and large-file policy instead of inventing parallel rules.
- Fix the browser's text-surface spacing using a reusable GTK pattern and capture that pattern as a permanent rule.
- Keep the initial scope small enough to implement and verify reliably.

**Non-Goals:**
- Full diff or side-by-side compare UI in the MVP.
- Local history for untitled tabs.
- Workspace-wide or multi-document history browsers.
- Save As lineage merging.
- Background compaction, delta storage, or database-backed indexing.
- User-configurable retention controls in the MVP.

## Decisions

### 1. Present local history in an adaptive dialog, not the persistent properties pane

The MVP will use a dedicated `AdwDialog` containing an `AdwNavigationSplitView` (or equivalent adaptive split/navigation composition) so the snapshot list and preview can appear side by side on wide windows and as navigable pages on narrow ones.

Rationale:
- This is a browse-and-act workflow triggered by deliberate user intent, which matches how the repo already presents bookmark and annotation browsers.
- The current properties pane is intentionally narrow and tuned for passive metadata plus toggles; history browsing needs more horizontal and vertical room.
- The adaptive dialog keeps the main editor visible in the background while avoiding another permanently visible pane competing with content.

Alternatives considered:
- Reusing the properties pane: rejected because the pane is too narrow and semantically focused on passive document metadata.
- Adding a bottom panel: rejected because it would compete with the existing search panel and compress editor content during a task that benefits from room.
- Launching a separate window: rejected as heavier than needed and less integrated with the existing shell.

### 2. Store snapshots as full UTF-8 text copies under a stable canonical-path identity

The MVP will store one history directory per saved document under the app data directory, using the same canonical-path hashing approach already used for note sidecars instead of `DefaultHasher`. Each snapshot will store full normalized text plus enough metadata to list timestamps and origins.

Rationale:
- Full copies are simple to reason about, easy to restore, and sufficient for the expected snapshot counts in the MVP.
- Canonical-path identity is already the repo’s robust answer for persistence that must survive restarts and in-app rename migration.
- Avoiding `DefaultHasher` removes cross-version/process stability concerns for on-disk identifiers.

Alternatives considered:
- Diffs only: rejected for the MVP because restore simplicity matters more than storage efficiency.
- SQLite index from day one: rejected because the MVP does not need cross-document search or large-scale indexing yet.
- Path-string-only directories: rejected because rename migration becomes fragile and duplicate representations of the same file become harder to reconcile.

### 3. Capture snapshots at meaningful boundaries with deduplication

The MVP will capture snapshots for saved documents at these boundaries:
- when a clean file-backed document first becomes dirty in an editing cycle, capturing the pre-edit baseline
- periodically while it remains modified, but no more often than once every five minutes
- after successful saves

The service will skip writing a new snapshot when the candidate content matches the most recent stored snapshot for that document.

Rationale:
- A first-dirty baseline provides a predictable "before I started this round of edits" restore point.
- Periodic capture covers longer unsaved sessions without writing on every draft sweep.
- Save-boundary capture gives users a stable sequence of known-good milestones.
- Deduplication prevents pathological snapshot growth when autosave cadence outpaces real content changes.

Alternatives considered:
- Snapshotting on every autosave tick: rejected because the current 5-second draft sweep is far too frequent for full-copy history.
- Save-only capture: rejected because it leaves large unsaved editing sessions uncovered.
- Snapshotting on every open: rejected because it creates noise and duplicates without representing user editing intent.

### 4. Restore is implemented as a reversible content replacement, not a confirmation-heavy action

When the user restores a snapshot, the system will first persist the current buffer as a fresh history snapshot, then replace the editor buffer with the selected historical text, mark the editor modified, and surface an immediate undo affordance through in-window feedback.

Rationale:
- This follows GNOME HIG guidance that undo is often preferable to a confirmation dialog for reversible actions.
- The pre-restore safety snapshot prevents the history browser itself from becoming a destructive dead end.
- Marking the buffer modified keeps the user in control of whether the restored state becomes the new on-disk version.

Alternatives considered:
- Confirmation dialog before every restore: rejected because it interrupts a recoverable workflow.
- Restoring directly to disk: rejected because it would bypass the editor’s normal save semantics and raise data-safety risk.
- Replace without safety snapshot: rejected because it would make accidental restore harder to recover from.

### 5. Reuse the existing large-file safety thresholds

Local history will follow the existing large-file policy already used by the editor:
- normal capture and preview for files up to 10 MB
- save-boundary-only capture for files above 10 MB and at or below 50 MB
- history unavailable above 50 MB

Rationale:
- The repo already has explicit, tested expectations for when document features become too costly.
- Reusing those thresholds keeps the feature consistent with the app’s broader responsiveness story.
- It avoids surprising users with a history feature that is more aggressive than the editor itself.

Alternatives considered:
- Independent local-history thresholds: rejected because divergent policies are harder to explain and maintain.
- No large-file degradation: rejected because full-copy snapshots for huge files would create storage and UI-risk quickly.

### 6. Follow-ups are documented explicitly, but deferred out of the MVP contract

The change will document these follow-ups while keeping them out of the v1 implementation contract:
- diff and compare UI
- local history for untitled documents
- workspace-wide history browser
- richer retention and storage controls
- richer timeline metadata and filtering

Rationale:
- The user asked for these directions to be preserved, but mixing them into the MVP would blur acceptance criteria and implementation order.
- Making the deferrals explicit prevents the common failure mode where "good future ideas" silently become day-one requirements.

### 7. Local history should be reachable from a shortcut and native context menus

The MVP will keep the main-menu and command-palette entry points, but it will also add:
- a dedicated keyboard shortcut for opening local history on the active saved document
- a sidebar file-row context-menu item for the selected saved file
- an editor-content context-menu item attached through the native `GtkTextView` / `GtkSourceView` extra-menu path

Rationale:
- A recovery feature is most useful when users can reach it from the exact place where they notice the need.
- The repo already uses native menu models for sidebar rows and action-based shortcuts, so these entry points fit the existing shell architecture.
- Reusing the text view's built-in extra-menu support is more GTK-native and less fragile than layering a separate right-click gesture over the editor content.

Alternatives considered:
- Main menu plus command palette only: rejected because discoverability remains too low for a feature positioned between undo and version control.
- Custom editor right-click gesture and popover: rejected because the toolkit already exposes a native extra-menu extension point.

### 8. Text surfaces inside dialogs need explicit inner spacing, not only outer shell margins

The local-history preview should keep the dialog's outer 18px shell margins, but the read-only preview text surface must also provide deliberate inner padding so text does not sit flush against the scroll frame. This should be documented as a permanent UI rule for dialog-contained text/document surfaces.

Rationale:
- Outer dialog margins shape the shell layout, but they do not pad the document content inside a `GtkScrolledWindow`.
- The repo has already solved this correctly in the annotation editor by using `TextView` inner margins; local history should follow the same pattern.
- Capturing the rule in `.agents/rules/ui.md` reduces repeated regressions in future dialog/browser work.

Alternatives considered:
- Relying on outer box spacing alone: rejected because it leaves the text content visually cramped inside the frame.
- Solving this only in local-history code without a permanent rule: rejected because the same issue is likely to recur in other dialog-based text surfaces.

## Risks / Trade-offs

- [Snapshot storage grows faster than expected] → Deduplicate consecutive snapshots, cap retention per document and globally, and start with full-copy storage only for text sizes the editor already treats as comfortable.
- [Restore flow feels destructive or confusing] → Always take a pre-restore safety snapshot, keep restore in-buffer until explicit save, and surface immediate undo feedback.
- [History identity diverges from file renames] → Use canonical-path sidecar identity plus the same in-app rename migration pattern already used for bookmarks and annotations.
- [Dialog UX becomes awkward on narrow windows] → Use an adaptive split/navigation container so the same widget hierarchy supports both wide and narrow layouts.
- [The MVP grows into an accidental diff project] → Keep diff/compare explicitly out of scope in both specs and tasks, and ship only a read-only snapshot preview in v1.

## Migration Plan

1. Add the new local-history capability with its own data directory under the existing app data root.
2. Ship the feature with no required migration of existing user data; users without history simply start accumulating snapshots after upgrade.
3. Reuse current action registration, notification, and rename hooks so rollout is additive rather than invasive.
4. If rollback is needed, the app can ignore the history directory without affecting drafts, session restore, or note sidecars.

## Open Questions

- None blocking for the MVP. The key follow-up questions are intentionally deferred rather than left ambiguous: diff UI, untitled coverage, workspace-wide browsing, and richer retention controls will be handled as subsequent changes if the MVP proves valuable.
