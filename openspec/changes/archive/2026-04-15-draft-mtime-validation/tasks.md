## 1. Draft restore validation foundations

- [x] 1.1 Add a file-backed draft restore resolution path in `services/draft_service.rs` that compares stored and current mtimes and returns explicit apply-or-skip outcomes.
- [x] 1.2 Handle confirmed stale drafts, ambiguous metadata failures, and legacy file-backed drafts without recorded mtimes according to the design's safety rules.

## 2. Startup and file-open integration

- [x] 2.1 Extend startup restore loading so file-backed tabs preload validated restore outcomes instead of only raw draft text.
- [x] 2.2 Update `check_draft_on_open()` and related window restore plumbing to consume the new file-backed outcome path while leaving untitled `check_draft_by_id()` recovery behavior unchanged.

## 3. Feedback and cleanup behavior

- [x] 3.1 Add editor-scoped warning feedback for skipped stale drafts and ensure normal draft-restored messaging appears only when draft content is actually applied.
- [x] 3.2 Delete confirmed-stale draft files and manifest entries after skip so reopening the same file does not re-offer or re-warn about the stale draft.

## 4. Verification

- [x] 4.1 Add unit coverage for file-backed draft validation, including matching mtimes, changed mtimes, legacy entries without stored mtimes, and metadata-read failures.
- [x] 4.2 Add integration or widget coverage for restore flows so unchanged file-backed drafts restore normally, changed file-backed drafts keep disk contents and warn once, and untitled drafts still restore.
