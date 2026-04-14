# Data Model: File Peek

## Overview

File peek adds transient, read-only state only. Nothing in this feature is
persisted to session, drafts, GSettings, or workspace JSON.

## Entities

### PeekTarget

- Purpose: Identify the currently requested preview candidate from the sidebar.
- Fields:
  - `absolute_path`: canonical file path used for metadata and reads.
  - `display_path`: absolute path shown in the card.
  - `workspace_id`: owning section for local invalidation and callbacks.
  - `generation`: monotonic request token used to reject stale async results.
  - `row_anchor_state`: enough UI context to reposition or dismiss when the
    realized row disappears.
- Validation rules:
  - Must come from a selected sidebar row that represents a real file.
  - Directories, placeholder rows, empty-folder placeholders, and workspace
    header controls cannot become `PeekTarget`s.
  - A later `generation` supersedes every earlier in-flight request.

### PeekSnapshot

- Purpose: Read-only payload rendered in the floating card.
- Fields:
  - `absolute_path`
  - `display_name`
  - `display_path`
  - `byte_size`
  - `modified_at_secs`
  - `preview_state`
  - `sample_text`
  - `sample_line_count`
  - `truncated`
  - `open_allowed`
- Validation rules:
  - `sample_text` is present only for eligible UTF-8 text previews.
  - `truncated` is true only when the service intentionally clipped the sample.
  - `open_allowed` is false when the file cannot be opened under the existing
    app policy, including oversized or invalid UTF-8 files.
  - `preview_state` must explain every non-text outcome explicitly.

### PeekPreviewState

- Purpose: Encode the UI state without forcing the widget to infer behavior from
  raw I/O errors.
- Variants:
  - `Loading`
  - `Text`
  - `BinaryOrUnsupported`
  - `Unreadable`
  - `TooLargeToOpen`
- Validation rules:
  - `Text` may still represent a large file preview, but only for sizes that
    remain openable under the existing file-size policy.
  - `TooLargeToOpen` means both inline preview and normal open must be presented
    as unavailable.

### PeekSession

- Purpose: Describe the visible lifetime of one section-owned preview surface.
- Fields:
  - `visible_target`: the `PeekTarget` currently bound to the card, if any.
  - `active_generation`: latest request token.
  - `focus_return_mode`: whether dismissal should restore sidebar focus or let
    promotion hand off to normal editor focus behavior.
  - `pending_request`: optional background request token or cancellation handle.
- Validation rules:
  - Only one active session exists per `LushtextWorkspaceSection`.
  - Session state is cleared on dismissal, section rebuild, workspace hide, or
    row invalidation.
  - Promotion transitions the session to closed state after delegating to the
    normal document-opening flow.

## Relationships

- One `PeekTarget` produces zero or one resolved `PeekSnapshot`.
- One `PeekSession` owns the currently visible `PeekTarget` and whichever
  `PeekSnapshot` is safe to render for that target.
- The window layer does not own peek state. It only receives the final
  "promote this path" action through the existing file-open callback surface.

## State Transitions

| From | Event | To | Notes |
|------|-------|----|-------|
| Idle | `Space` on previewable file row | Loading | Create `PeekTarget`, open or show the card, dispatch background snapshot load |
| Loading | Snapshot resolves for current generation | Ready | Render text or fallback state for the current target |
| Loading | Selection changes to another previewable file | Loading | Increment generation and dispatch replacement request |
| Loading | Selection changes to non-file row, section hides, or user dismisses | Idle | Drop visible state and ignore any later stale completion |
| Ready | Up or Down selects another previewable file | Loading | Keep the card open and refresh in place |
| Ready | `Space` on same file, `Escape`, click-away, or invalidation | Idle | Restore sidebar focus and keep current selection |
| Ready | `Enter` or `Open` action | Idle | Delegate to normal `open_document()` flow, then let editor focus rules take over |

## Derived Rules

- Peek never creates `EditorPage`, `TextBuffer`, draft manifest entries, session
  tabs, or file monitors.
- Existing size thresholds from `services/file_limits.rs` remain authoritative.
- Fallback copy must always reflect whether normal open is still available.
