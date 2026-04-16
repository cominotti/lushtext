## Context

The current encoding toolkit already has the right underlying state model and save/reopen safety behavior, but the UI shell still reflects the earlier "ship the full toolkit" implementation shape:

- The status bar shows separate encoding, line-ending, and issue buttons at all widths.
- The encoding button currently opens a dense `Encoding Toolkit` dialog with direct per-encoding buttons for reopen/save plus invisible-character mode buttons.
- The status-bar encoding label can grow into a summary such as `UTF-8 (save as Windows-1252)`, which is informative but no longer matches the updated requirement for short, always-scannable labels.
- Mixed line endings already use a persistent inline warning, and low-confidence decode findings already stay in the file-health surface, so the non-blocking warning foundation is mostly correct today.

The new change should refine the shell-level UX without disturbing the existing service/editor safety logic.

## Goals / Non-Goals

**Goals:**
- Keep the always-visible status-bar labels compact and document-local.
- Move from one dense encoding toolkit surface to a progressive-disclosure flow that starts compact and escalates to chooser dialogs only when the user asks for broader input.
- Preserve access to encoding, line endings, and issues on narrow windows through one grouped status-bar entry point.
- Keep the existing non-blocking warning stance for mixed line endings and low-confidence decode results.

**Non-Goals:**
- Reworking the encoding detection/transcoding pipeline in `services/editor_io.rs`.
- Changing the underlying save/reopen safety semantics.
- Introducing a new preferences-style window for document-local format actions.
- Expanding the encoding shortlist in this change.

## Decisions

### 1. Keep the current service and editor state boundaries

This change is UI-shell work. `DocumentEncodingState`, file-health findings, and lossy-save analysis already live in the correct service/editor layers, so the new behavior should stay in `ui/status_bar`, `ui/window/encoding.rs`, and the window refresh path.

### 2. Keep status-bar labels short and push richer context into dialogs

The status bar should continue to expose the current document state, but it should no longer use the always-visible encoding label to describe both the opened encoding and the next-save encoding.

- The encoding label should stay short by reflecting the current opened encoding only.
- Richer context such as the next-save encoding remains available in the dialog launched from that entry point.
- The line-ending label keeps the current `Mixed` state when the document needs attention and otherwise shows the next-save line-ending policy.

### 3. Replace the dense encoding toolkit dialog with a summary surface plus dedicated chooser dialogs

The encoding button should open a lightweight summary dialog that shows current state and exposes follow-up actions such as:

- `Reopen with Encoding…`
- `Save Using Encoding…`
- `Invisible Characters…`

Each of those actions should open its own chooser dialog instead of packing every encoding and invisible-character option into the first surface. This keeps the entry point compact and moves broader lists into the place where modal attention is already expected.

### 4. Add a grouped narrow-width document-format control in the status bar

When the status bar becomes too narrow for the separate encoding, line-ending, and issue controls to fit comfortably, it should replace them with one grouped button in the same metadata cluster.

- The grouped entry point should stay local to the status bar rather than moving the workflow into another window region.
- Activating it should expose encoding, line-ending, and issue entry points from one compact surface.
- The compaction logic should live inside the status-bar widget so it can respond to its own allocation instead of coupling another breakpoint into the already-busy window split-view math.

### 5. Preserve the current non-blocking warning model

The change should not introduce new modal interruptions on file open.

- Mixed line endings stay in the persistent editor warning plus the file-health surface.
- Low-confidence decode findings remain non-blocking and discoverable through the file-health surface.
- Lossy save and discard-before-reopen confirmations remain modal because those are the actual destructive boundaries.

## Risks / Trade-offs

- A width-based compact-mode threshold can feel slightly heuristic, but it is still better than letting the metadata cluster degrade into clipping or disappearing controls.
- Splitting the current encoding toolkit dialog into smaller flows introduces a little more navigation, but the result is easier to scan and closer to the GNOME HIG guidance captured in the updated spec.
- Keeping the grouped compact control in `ui/status_bar` avoids new window-shell breakpoint coupling, but it does mean the widget owns a bit more local presentation logic.

## Verification Plan

- Extend widget tests for the encoding toolkit flows to cover the new summary dialog and the dedicated chooser dialogs.
- Add widget coverage for the grouped compact status-bar control in a narrow window.
- Keep the existing mixed-line-ending warning regression coverage and update it only where the visible entry point text changes.
