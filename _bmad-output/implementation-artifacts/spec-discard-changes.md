---
title: 'Discard Changes hamburger menu action'
type: 'feature'
created: '2026-04-04'
status: 'done'
baseline_commit: 'd50494d'
context:
  - '.claude/rules/ui.md'
  - '.claude/rules/widget-wiring.md'
---

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** There is no way for the user to revert an edited file to its last saved state from the hamburger menu. GNOME Text Editor provides "Discard Changes…" in its menu for this purpose.

**Approach:** Add a `win.discard-changes` action to the hamburger menu's Save section (after Save As…). The action shows an `AdwAlertDialog` confirmation before reloading the file from disk, matching GNOME Text Editor's exact UX: heading "Discard Changes?", body warning about permanent loss, Cancel + destructive Discard buttons. The action is only enabled when the active tab is modified AND file-backed.

## Boundaries & Constraints

**Always:** Match GNOME Text Editor label `_Discard Changes…` (with ellipsis). Always show confirmation dialog before discarding. Delete any associated draft on discard. Disable the action for untitled tabs and unmodified buffers.

**Ask First:** Adding a keyboard shortcut (GNOME Text Editor has none for this action).

**Never:** Discard without confirmation. Support untitled (path-less) tabs — there is no disk file to revert to.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|--------------|---------------------------|----------------|
| Happy path | Modified file tab active, user confirms | File reloaded from disk, modified dot cleared, draft deleted | N/A |
| User cancels | Modified file tab, user clicks Cancel | No change to buffer or draft | N/A |
| No tabs open | Empty window | Action disabled (grayed out in menu) | N/A |
| Untitled tab active | New unsaved tab, no file path | Action disabled | N/A |
| Unmodified tab active | Clean file tab | Action disabled | N/A |
| File deleted while editing | Modified tab, backing file removed | `load_file_async` shows access error info bar | Existing error flow handles it |
| Tab switch | User switches between modified/clean tabs | Action enabled state updates immediately | N/A |
| Draft restored tab | Tab with restored draft, user discards | Draft file deleted, draft_restored flag cleared | N/A |

</frozen-after-approval>

## Code Map

- `resources/ui/window.ui:147-156` -- Save section of hamburger menu; add new item after Save As
- `crates/lushtext-core/src/ui/window/mod.rs:417-484` -- `setup_actions()`; add `SimpleAction` for `discard-changes`
- `crates/lushtext-core/src/ui/window/mod.rs:262-279` -- `update_content_stack()`; add to disabled-when-no-tabs list
- `crates/lushtext-core/src/ui/window/mod.rs:185-214` -- `wire_modified_indicator()`; update discard action enabled state on modified-changed
- `crates/lushtext-core/src/ui/window/mod.rs:281-310` -- `refresh_status_bar()`; update discard action on tab switch
- `crates/lushtext-core/src/ui/window/dialogs.rs` -- existing `AlertDialog` patterns to follow
- `crates/lushtext-core/src/ui/window/session.rs:382-408` -- `delete_draft_for_path()` for draft cleanup

## Tasks & Acceptance

**Execution:**
- [x] `resources/ui/window.ui` -- Add `_Discard Changes…` menu item with `win.discard-changes` action in the Save section after Save As
- [x] `crates/lushtext-core/src/ui/window/mod.rs` -- Create `discard-changes` `SimpleAction` in `setup_actions()`, wire handler to show `AdwAlertDialog` then reload file + delete draft on confirm
- [x] `crates/lushtext-core/src/ui/window/mod.rs` -- Add `update_discard_action()` helper; call from `update_content_stack()`, `wire_modified_indicator()` callback, and `refresh_status_bar()`
- [x] `crates/lushtext-core/src/ui/window/dialogs.rs` -- Add `show_discard_changes_dialog()` with AdwAlertDialog: heading "Discard Changes?", body "Unsaved changes will be permanently lost.", Cancel + destructive Discard responses

**Acceptance Criteria:**
- Given a modified file-backed tab is active, when user opens hamburger menu, then "Discard Changes…" is enabled
- Given user clicks "Discard Changes…", when confirmation dialog appears and user clicks "Discard", then buffer reloads from disk and modified indicator clears
- Given user clicks "Discard Changes…", when confirmation dialog appears and user clicks "Cancel", then buffer is unchanged
- Given an untitled tab or unmodified tab is active, when user opens hamburger menu, then "Discard Changes…" is grayed out
- Given no tabs are open, when user opens hamburger menu, then "Discard Changes…" is grayed out
- Given a draft-restored tab, when user discards changes, then the draft file is also deleted

## Verification

**Commands:**
- `make check` -- expected: clippy + fmt pass with no warnings
- `make test` -- expected: all existing tests pass (no regressions)

**Manual checks:**
- Open a file, edit it, use hamburger menu "Discard Changes…" → confirm → buffer reverts
- Verify action is disabled for untitled tabs, unmodified tabs, and empty window
- Verify draft file is cleaned up after discard (check `~/.local/share/lushtext/drafts/`)

## Suggested Review Order

**Menu & action wiring**

- Menu item placement in the Save section, after Save As
  [`window.ui:156`](../../resources/ui/window.ui#L156)

- Action handler: guard clauses + confirmation dialog + reload-on-confirm
  [`mod.rs:170`](../../crates/lushtext-core/src/ui/window/mod.rs#L170)

- `SimpleAction` creation with `enabled=false` initial state
  [`mod.rs:514`](../../crates/lushtext-core/src/ui/window/mod.rs#L514)

**Enabled state lifecycle**

- Per-tab enabled state: modified AND file-backed check
  [`mod.rs:200`](../../crates/lushtext-core/src/ui/window/mod.rs#L200)

- Integrated into `wire_modified_indicator` for real-time tracking
  [`mod.rs:253`](../../crates/lushtext-core/src/ui/window/mod.rs#L253)

- Added to `update_content_stack` disabled-when-no-tabs list
  [`mod.rs:321`](../../crates/lushtext-core/src/ui/window/mod.rs#L321)

- Called from `refresh_status_bar` for tab-switch updates
  [`mod.rs:350`](../../crates/lushtext-core/src/ui/window/mod.rs#L350)

**Confirmation dialog**

- `AdwAlertDialog` with destructive Discard button, matching GNOME Text Editor
  [`dialogs.rs:101`](../../crates/lushtext-core/src/ui/window/dialogs.rs#L101)

**Tests**

- 6 widget tests covering the full enabled/disabled lifecycle
  [`window.rs:1632`](../../crates/lushtext/tests/widget/window.rs#L1632)
