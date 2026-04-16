## Why

LushText still assumes UTF-8 text files and only shows a passive `"UTF-8"` label in the status bar, which makes the app brittle around legacy encodings, BOMs, and cross-platform line-ending cleanup. The repo already has two deferred notes here, but the broader toolkit note now subsumes the earlier encoding-support note closely enough that proposing them separately would leave the requested work blocked on a prerequisite change.

## What Changes

- Fold the foundational work from `docs/next/encoding-support.md` into this change so raw-byte load/save, detected encoding state, and interactive status-bar encoding controls land as the first phase of the toolkit.
- Add per-document encoding and line-ending state so LushText can detect how a file was opened, reopen bytes with a different interpretation, and save back using the chosen encoding and normalized line endings.
- Replace the passive status-bar encoding label with interactive encoding and line-ending controls that surface the current state, reopen/convert actions, and non-destructive warnings before lossy writes.
- Add lightweight file-health reporting for encoding-adjacent issues such as BOMs, mixed line endings, binary-like content, and low-confidence charset detection.
- Add opt-in invisible-character visualization modes that help users inspect whitespace and encoding-related anomalies without mutating the underlying document.
- Keep deeper EditorConfig enforcement (`charset`, `end_of_line`) as an explicit integration point in the design, but sequence it after the core toolkit path so the initial change remains implementable.

## Capabilities

### New Capabilities
- `encoding-toolkit`: Per-document encoding and line-ending awareness, interactive reopen/convert flows, and file-health reporting anchored in the status bar and editor notifications.
- `invisible-character-visualization`: Opt-in visibility modes for whitespace and encoding-adjacent invisible content that support the encoding toolkit workflow.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/lushtext-core/src/services/editor_io.rs`, `ui/editor_page`, `ui/status_bar`, `ui/window`, `ui/info_bar`, GSettings schema/UI resources, and related tests.
- Affected systems: document open/save lifecycle, per-tab metadata refresh, restored-document notifications, and future EditorConfig deferred-property integration.
- Dependencies and APIs: likely adds charset detection/transcoding crates such as `encoding_rs` and `chardetng`, plus new internal models for encoding, line-ending, and file-health state.
