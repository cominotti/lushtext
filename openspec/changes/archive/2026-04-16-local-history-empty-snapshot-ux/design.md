## Context

LushText's local-history capture model already stores valid empty snapshots, and
some of them are worth keeping. A saved file can legitimately be empty before
the next editing cycle begins, so an empty baseline can be a real historical
state.

The problem is that draft-restored files blur two different timelines:

- the last saved state that existed on disk
- the unsaved working document the user experiences as continuous after restore

Right now, draft restore reapplies buffer text, marks the editor modified, and
falls straight into the normal "first dirty transition" baseline capture path.
That means the browser can show new baseline entries for the stale on-disk file
even though the user never experienced that state as part of the current
working document. At the same time, the browser still exposes legitimate empty
snapshots badly by rendering them as a blank preview plus `0 B`.

## Goals / Non-Goals

**Goals:**
- Make empty local-history snapshots read as intentional historical states, not
  rendering failures.
- Distinguish empty-snapshot UX from both "no snapshots yet" and "preview could
  not be loaded" states.
- Stop draft-restored files from generating fresh noisy baseline entries that
  only reflect the pre-restore disk file.
- Clarify empty snapshot metadata in both the list and the selected-preview
  surface.
- Keep restore available for empty snapshots because restoring to an empty
  document is a valid action.
- Add widget coverage for the empty-snapshot UX so the browser cannot regress
  back to a confusing blank surface silently.

**Non-Goals:**
- Reclassifying empty snapshots as invalid or removing them from history.
- Adding diff/compare UI or deeper timeline semantics.
- Reworking the large viewer-scale layout beyond the empty-preview state and
  related copy.
- Rewriting the broader local-history retention model or save-boundary capture
  semantics outside the draft-restore path.

## Decisions

### 1. Treat empty snapshots as a first-class preview state

The preview stack should gain a dedicated empty-snapshot state alongside the
existing loading, error, and content states.

Rationale:
- A blank text view is visually indistinguishable from "nothing rendered."
- Local history already uses explicit status-page UX for "no snapshots yet" and
  preview failures; empty snapshots deserve the same clarity.
- This keeps the interpretation burden out of the user's head and inside the
  browser where it belongs.

Alternatives considered:
- Keep the blank preview and only improve row labels: rejected because the main
  confusion happens in the preview pane itself.
- Show the text view with placeholder copy inside it: rejected because it
  blurs "previewing text" and "explaining no text."

### 2. Suppress draft-restored disk baselines as fresh timeline entries

When a file-backed draft is restored into an editor at open time, local history
should treat that restoration as continuity of the user's unsaved work rather
than as the start of a new editing cycle that deserves a fresh baseline
snapshot for the pre-restore disk file.

Rationale:
- The user experiences draft restore as "my document came back," not as "show
  me the stale saved file again in the timeline."
- The pre-restore disk content is already available through discard/reload and
  does not need to be promoted into a new local-history row just because draft
  restore toggled the modified flag.
- This keeps the visible history aligned with the working document rather than
  with an implementation detail of startup recovery.

Alternatives considered:
- Keep the baseline but relabel it as disk state: rejected because it still
  clutters the active timeline with entries the user did not meaningfully visit.
- Remove all empty snapshots everywhere: rejected because some empty snapshots
  are legitimate document history outside draft-restore flows.

### 3. Describe empty snapshots semantically, not only as `0 B`

The list row and selected metadata should use human-friendly empty wording such
as `Empty` or `Empty file` rather than relying entirely on `0 B`.

Rationale:
- `0 B` is technically correct but does not explain whether the snapshot is
  empty, missing, or broken.
- Semantic metadata helps users understand the state before they even inspect
  the preview pane.

Alternatives considered:
- Leave metadata unchanged and rely only on the preview explanation: rejected
  because the list itself still looks suspicious and repetitive.

### 4. Keep restore, suppress copy, for empty snapshots

An empty snapshot should still allow `Restore`, but `Copy` should be disabled
when there is no text body.

Rationale:
- Restoring to an empty historical state is a valid recovery action.
- Copying an empty snapshot is not useful and can feel like a broken button or
  a clipboard-destructive action.

Alternatives considered:
- Leave copy enabled for empty snapshots: rejected because it adds little value
  and muddies the UX we are trying to clarify.
- Disable both actions: rejected because it would incorrectly suggest empty
  snapshots are not real history.

### 5. Explain the remaining empty baseline meaning directly

The empty preview should explain that the snapshot itself contained no text when
captured, and for baseline-style states it should make clear that this can mean
the saved file was empty before the current unsaved edits.

Rationale:
- The most confusing real-world case is "draft restored into a buffer whose
  saved file was empty."
- Naming that case directly answers the user's natural question: "what was
  supposed to appear here?"

Alternatives considered:
- Use completely generic empty wording with no historical context: rejected
  because it leaves the most important interpretation gap untouched.

### 6. Add widget tests for both empty-snapshot UX and draft-restore timeline behavior

Widget coverage should explicitly verify empty snapshot metadata, empty preview
copy/action state, and the absence of a fresh noisy baseline row when draft
restore makes a file-backed editor modified at open.

Rationale:
- This is a user-facing interpretability bug, so a real widget regression test
  is the right guardrail.
- The existing local-history widget suite already owns the browser contract.

Alternatives considered:
- Service-only coverage for `byte_len == 0`: rejected because the bug is not in
  persistence, it is in browser presentation and editor-capture orchestration.

## Risks / Trade-offs

- [The empty-state wording becomes too specific to one workflow] → keep the copy
  grounded in "snapshot contained no text" and only mention the saved-empty-file
  interpretation where it helps most.
- [Suppressing draft-restored baselines hides a disk state some users might want]
  → preserve discard/reload as the path to inspect on-disk state and keep
  normal saved-file history for later meaningful capture points.
- [Semantic labels hide the true byte size entirely] → keep the underlying
  empty meaning prominent without removing technical accuracy from the detailed
  metadata if it still helps elsewhere.
- [Disabling Copy surprises users who expect every snapshot to be copyable] →
  pair the disabled state with explicit empty-snapshot explanation so the reason
  is obvious.

## Migration Plan

1. Update the local-history browser and capture requirements so empty snapshots
   and draft-restored baseline behavior are both part of the accepted contract.
2. Adjust the draft-restore/local-history interaction so restored drafts do not
   immediately mint fresh noisy disk-baseline rows.
3. Add the dedicated empty-preview state and semantic metadata in
   `ui/window/local_history.rs`.
4. Extend widget coverage for empty snapshot rows, preview state, action
   availability, and the draft-restored timeline behavior.
5. No data migration is required; existing empty snapshots remain valid and
   simply become easier to understand, while future draft-restored sessions
   stop adding noisy baseline rows.

## Open Questions

- None blocking. The remaining detail is copywriting tone: the implementation
  can choose the exact short labels and explanatory sentences as long as the
  browser clearly communicates "this snapshot was empty" and draft-restored
  timelines stop surfacing fresh stale-disk baselines as if they were
  meaningful user history.
