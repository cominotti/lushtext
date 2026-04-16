## 1. Status-bar compaction and short labels

- [x] 1.1 Keep the always-visible encoding label short by showing the opened encoding in the status bar while preserving richer open/save context in follow-up dialogs.
- [x] 1.2 Add a grouped narrow-width document-format status-bar control that preserves access to encoding, line endings, and issues when the full metadata cluster does not fit comfortably.

## 2. Progressive-disclosure encoding flows

- [x] 2.1 Replace the current dense encoding toolkit dialog with a lightweight summary surface that launches dedicated modal choosers for `Reopen with Encoding…`, `Save Using Encoding…`, and invisible-character mode selection.
- [x] 2.2 Keep the existing line-ending and file-health flows reachable from the compact grouped control without introducing new modal interruptions on file open.

## 3. Verification

- [x] 3.1 Add widget coverage for the updated encoding dialog flow and lossy/reopen chooser entry points.
- [x] 3.2 Add widget coverage for the narrow-width grouped document-format control and the short status-bar labels.
