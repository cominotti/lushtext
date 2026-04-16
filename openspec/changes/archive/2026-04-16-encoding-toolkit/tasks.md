## 1. Encoding and line-ending foundations

- [x] 1.1 Add typed document-state models for opened encoding, save encoding, BOM presence, line-ending policy, decode confidence, and file-health findings.
- [x] 1.2 Extend `services/editor_io.rs` to load raw bytes, detect/decode supported encodings, classify line endings, and return the richer document snapshot needed by `EditorPage`.
- [x] 1.3 Extend the save pipeline to normalize line endings and transcode output bytes off the GTK thread, including explicit lossy-conversion reporting before write.
- [x] 1.4 Add unit coverage for encoding detection, BOM handling, mixed line-ending classification, unsupported/binary-like inputs, and save-time conversion outcomes.

## 2. Editor and status-bar integration

- [x] 2.1 Store encoding, save-policy, line-ending, and file-health state on `LushtextEditorPage` and update window refresh paths to read metadata from that state.
- [x] 2.2 Replace the passive status-bar encoding label with interactive encoding and line-ending controls plus a conditional file-health indicator.
- [x] 2.3 Integrate the new metadata flow with open, discard/reload, session restore, save, and Save As so the status bar always reflects the active tab's real document state.
- [x] 2.4 Add any required GSettings keys, template resources, and action wiring for encoding controls, line-ending controls, and invisible-character modes.

## 3. Reopen, conversion, and health workflows

- [x] 3.1 Implement `Reopen with Encoding...` for file-backed tabs, reusing existing unsaved-changes safety flows before rereading on-disk bytes.
- [x] 3.2 Implement `Save using Encoding...` with a bounded lossy-conversion preview or confirmation flow and persistent save-policy updates after approval.
- [x] 3.3 Add mixed line-ending warnings and one-click normalization actions that update the document's save policy without changing its semantic text content.
- [x] 3.4 Add a file-health details surface for BOMs, low-confidence decode results, mixed line endings, and other encoding-adjacent findings.

## 4. Invisible-character visualization

- [x] 4.1 Add per-editor `Off`, `Whitespace Only`, and `All` visibility modes using GTK-native whitespace drawing where possible.
- [x] 4.2 Add discoverable anomaly markers or linked affordances for non-breaking spaces, zero-width characters, BOMs, and line-ending boundaries in `All` mode without mutating the buffer.
- [x] 4.3 Wire actions, shortcuts, and status messaging so invisible-character mode changes stay synchronized with the active editor and document state.

## 5. Verification and follow-up docs

- [x] 5.1 Add widget or integration coverage for status-bar metadata, reopen-with-encoding safety, save-encoding confirmation, and mixed line-ending normalization.
- [x] 5.2 Extend benchmark or compile-check coverage for the new decode/transcode and line-ending normalization paths so the toolkit does not regress large-file responsiveness.
- [x] 5.3 Update the relevant docs and future-work notes to show that `encoding-support.md` was folded into the broader toolkit plan and to clarify the remaining EditorConfig follow-up scope.
