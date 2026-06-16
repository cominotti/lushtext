## 1. Dialog State Rules

- [x] 1.1 Add a GTK-free helper for note dialog presentation state that uses `normalize_note_text` to choose initial `NoteViewMode` and Save sensitivity.
- [x] 1.2 Cover the helper with unit tests for missing/empty text, whitespace-only text, existing markdown, trim-only equality, meaningful edits, reverted edits, and clearing to whitespace.
- [x] 1.3 Add bounded property tests proving Save is enabled exactly when normalized current text is non-empty and differs from normalized loaded text, and initial Render is selected exactly when normalized loaded text is non-empty.

## 2. Dialog Wiring

- [x] 2.1 Use the shared helper when presenting document-note dialogs so existing meaningful notes open on Render and empty or missing notes open on Edit.
- [x] 2.2 Use the shared helper when presenting folder-note dialogs so existing meaningful notes open on Render and empty or missing notes open on Edit.
- [x] 2.3 Wire the Save response sensitivity to the note buffer's current normalized dirty state, including an initial update before presentation.
- [x] 2.4 Keep Save visibility, suggested response styling, Clear, Cancel, and the existing save/clear persistence handlers intact.
- [x] 2.5 Ensure Save remains mode-independent: edits made in Edit mode can be reviewed in Render mode and saved from Render without losing the current buffer text.
- [x] 2.6 Update comments or helper names that still state existing notes open in Edit mode.

## 3. Widget Coverage

- [x] 3.1 Update document-note widget coverage so an existing saved note opens with Render selected and Save visible but disabled.
- [x] 3.2 Add document-note widget coverage for missing or empty notes opening in Edit with Save disabled until meaningful text is typed.
- [x] 3.3 Add document-note widget coverage for Save enabling after meaningful edits, staying enabled after switching to Render, persisting when saved from Render, disabling after revert, and staying disabled for whitespace-only text.
- [x] 3.4 Add folder-note widget coverage matching the document-note state matrix for existing notes, empty notes, dirty edits, revert, whitespace-only text, and saving from Render.
- [x] 3.5 Cover representative opening paths beyond the primary header actions where practical, including Browse Notes or sidebar/context-menu entry points that reuse the same note dialogs.
- [x] 3.6 Preserve and extend modal geometry assertions so Render-first existing notes, Edit-first empty notes, first Render after typing, dense markdown, and awkward long text do not resize the modal or push the switcher/footer actions out of reach.

## 4. Validation

- [x] 4.1 Run `openspec validate polish-note-dialog-render-state --strict`.
- [x] 4.2 Run `make test-unit`.
- [x] 4.3 Run `make test-prop`.
- [x] 4.4 Run `make test-int`.
- [x] 4.5 Run targeted headless widget tests for the changed note-dialog cases.
- [x] 4.6 Run `make test-widget-headless` and treat any `FLAKY:` output as a blocker.
- [x] 4.7 Run `make visual-geometry-smoke` to confirm the note dialog geometry/readability surface remains stable.
- [x] 4.8 Run `git diff --check` and `make pre-commit`.
