# Feature Specification: File Peek

**Feature Branch**: `[001-file-peek]`
**Created**: 2026-04-13
**Status**: Draft
**Input**: User description: "You must design and implement docs/next/file-peek.md."

## Clarifications

### Session 2026-04-13

- Q: What should be the primary action for invoking file peek from the sidebar? → A: Press `Space` on the selected sidebar file row.
- Q: Where should the preview surface appear once peek is opened? → A: Show a floating card anchored beside the selected sidebar row, overlapping the center area without changing pane widths.
- Q: After dismissing a preview without opening the file, where should keyboard focus return? → A: Return focus to the currently selected sidebar row.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Glance At Files Without Tab Noise (Priority: P1)

A user browsing the workspace sidebar can temporarily inspect a selected file
without opening a new tab or disturbing the file they are currently editing.

**Why this priority**: This is the core value of the feature. If the user cannot
quickly inspect candidate files without creating tabs, the feature does not solve
the stated navigation problem.

**Independent Test**: From a workspace with several files, the user can preview
multiple files in succession from the sidebar and keep their existing tab state
unchanged until they explicitly choose to open one.

**Acceptance Scenarios**:

1. **Given** a file is selected in the workspace sidebar and another document is
   already open, **When** the user invokes file peek, **Then** a temporary preview
   appears for the selected file and no new tab is created.
2. **Given** a temporary preview is open, **When** the user moves to another file
   in the sidebar, **Then** the preview updates to the newly selected file without
   opening or replacing tabs.

---

### User Story 2 - Commit A Previewed File Into Normal Editing (Priority: P2)

After using peek to confirm the correct file, a user can promote that file into
the normal open-file workflow without needing to search for it again.

**Why this priority**: Peek is only useful if it shortens the path to the real
editing action once the right file is found.

**Independent Test**: Starting from an open preview, the user can open the
previewed file as a normal tab and continue editing with the same duplicate-tab
behavior the app already uses.

**Acceptance Scenarios**:

1. **Given** a file preview is open, **When** the user chooses the open action,
   **Then** the file opens through the app's normal document-opening flow and the
   temporary preview closes.
2. **Given** the previewed file is already open in a tab, **When** the user
   promotes it from the preview, **Then** the existing tab is focused instead of
   creating a duplicate tab.

---

### User Story 3 - Understand Unsupported Or Risky Files Quickly (Priority: P3)

A user who tries to peek an unsupported, unreadable, or oversized file still gets
clear feedback about why inline preview is unavailable and what action remains
possible.

**Why this priority**: The feature must fail clearly and safely when a file is
not suitable for inline preview, especially because the sidebar can contain very
large or non-text files.

**Independent Test**: From the sidebar, the user can attempt to peek binary,
unreadable, and oversized files and always receives a visible explanation instead
of a hang, blank surface, or silent no-op.

**Acceptance Scenarios**:

1. **Given** a selected file cannot be previewed inline because it is binary,
   unreadable, or exceeds preview safety limits, **When** the user invokes peek,
   **Then** the system shows an explicit fallback state describing the issue.
2. **Given** a selected file cannot be opened under the app's existing file-size
   policy, **When** the user invokes peek, **Then** the fallback state makes that
   limitation clear and does not present the file as normally openable.

### Edge Cases

- Triggering peek on a directory row, empty-folder placeholder, truncated-row
  placeholder, or workspace header control keeps existing behavior and does not
  open a preview.
- If the user changes selection, hides the workspace section, or closes the
  window before preview loading finishes, stale results are discarded cleanly.
- If the user dismisses the preview or the app closes while a preview is open,
  no drafts, session entries, or modified state are created because peek is
  read-only and temporary.
- If the sidebar width preset or workspace filter changes while a preview is
  open, the preview closes or repositions without changing the split layout or
  leaving focus stranded.

## UX, Safety & Verification Constraints *(mandatory)*

### Interaction Contract

- File peek is a deliberate, temporary sidebar action for files only. It is not a
  replacement for opening a tab.
- The primary peek trigger is pressing `Space` while a sidebar file row is
  selected. Existing open-file activation behavior remains the normal way to
  commit a file into editing.
- Invoking peek shows enough content and identity information for the user to
  decide whether the file is the one they want, while keeping the current tab and
  cursor position intact until the user explicitly opens the previewed file.
- The preview surface is a floating card anchored beside the selected sidebar
  row. It may overlap the center editing area, but it does not consume pane
  width or behave like a persistent panel.
- The preview can be dismissed quickly from the keyboard or pointer and should
  stay aligned with the currently selected sidebar file while the user navigates.
- Dismissing a preview without promotion returns keyboard focus to the currently
  selected sidebar row so the user can continue scanning files immediately.
- Promoting a previewed file into full editing must use the same normal open-file
  behavior the user already knows, including duplicate-tab avoidance.
- The preview surface must not change the left, center, or right pane width
  allocations. It behaves as an overlay, not as a third persistent panel.

### Data Safety & Recovery

- File peek is strictly read-only. It must not modify file contents, buffer
  state, undo history, drafts, or session restore data.
- Cancelling or dismissing a preview must simply drop the temporary result. No
  recovery flow is needed because no user data is changed.
- Existing large-file and unsupported-file protections remain authoritative. Peek
  must not bypass safety checks that already govern whether a file can be opened.

### Verification & Delivery Impact

- Automated coverage is required for preview eligibility rules, truncated-content
  formatting, dismissal and promotion behavior, duplicate-tab reuse, and focus
  restoration after the preview closes.
- Presented UI validation is required for keyboard navigation, overlay
  positioning, and dismissal behavior across the supported sidebar width presets.
- Live runtime validation is required for the preview overlay to confirm that the
  feature does not introduce focus regressions, layout warnings, or visual
  clipping in a real application session.
- The implementation-facing design and delivery work must stay aligned with the
  accepted user-facing contract in this specification.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: Users MUST be able to invoke a temporary preview for the currently
  selected sidebar file by pressing `Space` without opening a tab.
- **FR-002**: The system MUST show enough preview content and metadata for users
  to identify the selected file before committing to open it.
- **FR-003**: The preview MUST remain read-only and MUST NOT create modified
  state, undo history, drafts, or session entries.
- **FR-004**: The preview MUST be dismissible without losing the user's current
  editor state or sidebar selection context.
- **FR-005**: While a preview is open, users MUST be able to move sidebar
  selection to another file and have the preview update in place without opening
  tabs.
- **FR-006**: Users MUST be able to promote the currently previewed file into the
  existing open-file workflow from the preview itself.
- **FR-007**: When a previewed file is already open, promotion MUST reuse the
  existing tab instead of creating a duplicate.
- **FR-008**: The system MUST ignore or gracefully reject peek requests for rows
  that do not represent previewable files.
- **FR-009**: The system MUST provide explicit fallback states for files that are
  binary, unreadable, still loading, or unsafe to preview inline.
- **FR-010**: The system MUST preserve the app's existing large-file safety
  policy so preview never stalls the UI or misrepresents whether a file can be
  opened normally.
- **FR-011**: The preview MUST close cleanly when the user dismisses it,
  promotes the file, switches to a non-file row, or otherwise invalidates the
  current preview target.
- **FR-012**: After dismissal, keyboard focus MUST return to a sensible target so
  the user can continue navigation or editing immediately. Dismissal without
  promotion MUST return focus to the currently selected sidebar row.
- **FR-013**: The preview experience MUST remain usable at all supported sidebar
  width presets, MUST appear as an anchored floating card beside the selected
  sidebar row, and MUST NOT resize the split-view layout.
- **FR-014**: Preview errors and unsupported-file states MUST explain what
  happened in user-facing language and indicate any remaining action that is
  available.

### Key Entities *(include if feature involves data)*

- **Peek Request**: A temporary user action tied to the currently selected
  sidebar file and the context needed to decide whether the preview can open,
  update, or should be discarded as stale.
- **Peek Snapshot**: The read-only preview payload shown to the user, including
  bounded content, file identity metadata, and any unsupported or error state.
- **Peek Session**: The visible lifetime of one temporary preview surface,
  including how it opens, updates as selection changes, and closes or promotes
  into normal editing.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In acceptance testing, users can inspect at least 5 candidate files
  from the sidebar and open the intended one while ending with no more than 1
  newly created tab.
- **SC-002**: For eligible local text files, 95% of peek requests display usable
  preview content or a clear fallback state within 0.25 seconds of the trigger.
- **SC-003**: In manual regression runs, 100% of dismissal and promotion flows
  leave the user with a usable focus target that matches the interaction contract
  for that flow, including dismissal returning focus to the selected sidebar row.
- **SC-004**: In validation across normal, binary, unreadable, and oversized
  files, 100% of peek attempts end in either a visible preview or an explicit
  fallback explanation; none hang or silently do nothing.

## Assumptions

- The first release is scoped to files selected from the workspace sidebar, not
  to search results, command-palette items, or other navigation surfaces.
- The first release prioritizes text-oriented file inspection; rich media
  thumbnails and other specialized previews are out of scope.
- Existing document-opening behavior, duplicate-tab handling, and large-file
  safety rules remain the source of truth whenever a preview is promoted.
- A bounded preview sample is sufficient for users to identify a file without
  loading the full document.
