## Context

Document-note and folder-note dialogs share `build_note_editor_surface()` in `crates/lushtext-core/src/ui/window/notes.rs`. The shared surface already supports `NoteViewMode::Edit` and `NoteViewMode::Render`, and it already pre-renders non-empty note text so the Render page keeps the established modal-geometry contract.

The callers currently pass `NoteViewMode::Edit` for every note, so existing saved notes open as editable source even when the user is likely reviewing stored content. The same dialogs always expose an enabled Save response, even when the buffer matches the loaded note or contains only whitespace that cannot be saved as a meaningful note body.

## Goals / Non-Goals

**Goals:**

- Make document-note and folder-note dialogs open in Render mode when loaded note text has non-whitespace content.
- Keep empty, missing, cleared, or whitespace-only notes opening in Edit mode.
- Keep Save visible but disable it while there is no meaningful unsaved change.
- Keep Save enablement based on normalized buffer content, not on the currently selected Edit/Render page.
- Preserve Clear, Cancel, existing save/load/clear persistence behavior, and modal layout stability.
- Add broad coverage: pure helper tests, property coverage for normalization-driven state, document-note widget flows, folder-note widget flows, mode-switch flows, and visual/geometry validation.

**Non-Goals:**

- Do not hide or rename Save.
- Do not introduce autosave, live preview while typing, or a separate read-only viewer dialog.
- Do not change note sidecar formats, note identity, markdown rendering semantics, or Browse Notes taxonomy.
- Do not redesign the dialog footer, Clear behavior, or warning/status messages beyond response sensitivity.

## Decisions

### 1. Derive dialog state from normalized note text

Use the same normalization semantics as `RichNoteBody` persistence: trim leading and trailing whitespace before deciding whether text is meaningful or dirty. The initial mode is Render when the normalized loaded text is non-empty; otherwise it is Edit. Save is enabled only when the normalized current buffer is non-empty and differs from the normalized loaded baseline.

Alternative considered: compare raw buffer text. That would enable Save for trim-only changes that persistence immediately discards, making the button look actionable when saving would produce no durable change.

### 2. Keep Save visible and mode-independent

The Save response remains present in every note dialog. Its sensitivity follows dirty state only. If a user edits in Edit mode, switches to Render to review, and then presses Save, the dialog should save the current buffer text from Render just as it does from Edit.

Alternative considered: disable Save whenever Render is selected. That breaks the review-then-save flow and treats Render as a separate viewer instead of a reading mode inside the editor.

### 3. Centralize the state calculation

Implement a small pure decision helper for note dialogs rather than duplicating `trim` and equality checks across document-note and folder-note wiring. The helper should return enough state for both callers to select the initial mode and update Save sensitivity from buffer changes.

Alternative considered: inline the checks at each call site. The behavior has enough edge cases to deserve one tested rule, and both note kinds must remain identical.

### 4. Update response sensitivity from buffer changes

Connect the note buffer's change signal to recompute Save sensitivity. The first sensitivity update should run during dialog construction so existing notes open with Render selected and Save disabled, while empty notes open with Edit selected and Save disabled. Response sensitivity must also update after revert-to-original, trim-only edits, and changes made before switching pages.

Alternative considered: recompute only when the user presses Save. That would preserve correctness but leave the footer misleading while the dialog is open.

### 5. Test the state matrix, not only the happy path

Coverage should include:

- no saved note, whitespace-only text, and representative saved markdown;
- document-note and folder-note dialogs;
- open paths from normal command/menu surfaces and at least one existing browse/sidebar entry path where practical;
- Save disabled on initial clean state, enabled after meaningful edit, disabled again after revert or whitespace-only content;
- Save remains enabled after editing and switching to Render, and saving from Render persists the edit;
- modal geometry remains stable with Render selected initially and across Edit/Render transitions;
- dense or awkward text content does not push footer actions, switcher, or close/dismiss behavior out of reach.

## Risks / Trade-offs

- [Disabled suggested response may look unusual in an AlertDialog] -> Keep the response visible and suggested, but ensure tests assert sensitivity and keyboard/default behavior remain sane.
- [Dirty-state comparisons could drift from persistence normalization] -> Reuse or wrap `normalize_note_text` and prove the rule with unit and property tests.
- [Render-first opening could expose stale preview content] -> Keep the existing render-on-initial-content path and render again when switching to Render after edits.
- [Widget tests can flake around async note loading] -> Use shared `wait_until` helpers with generous waits for `spawn_blocking_then` paths, and treat any `FLAKY:` output from `make test-widget-headless` as a blocker.

## Migration Plan

No data migration is required. Existing note sidecars load unchanged; the new behavior is presentation state and response sensitivity only. Rollback is straightforward: return existing-note dialogs to Edit initial mode and restore always-enabled Save while leaving persisted data untouched.

## Open Questions

- None.
