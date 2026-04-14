# Tasks: File Peek

**Input**: Design documents from `/var/home/danilo/Workspace/github/cominotti/lushtext/specs/001-file-peek/`
**Prerequisites**: `plan.md`, `spec.md`, `research.md`, `data-model.md`, `contracts/file-peek-ui.md`, `quickstart.md`

**Tests**: Verification is required for this feature. Each user story includes the automated coverage and live GTK validation called for by the specification and plan.

**Organization**: Tasks are grouped by user story so each slice can be implemented, verified, and reviewed independently.

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Prepare the real code and test surfaces for the file-peek workstream

- [X] T001 Add the new feature module declarations in `crates/lushtext-core/src/services/mod.rs` and `crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs` for `file_peek.rs` and `peek.rs`
- [X] T002 [P] Extend sidebar widget test fixtures in `crates/lushtext/tests/widget/common.rs` and `crates/lushtext/tests/widget/workspace_section.rs` so file-peek scenarios can build deterministic workspaces and selected rows
- [X] T003 [P] Add reusable widget assertions for tab counts, sidebar focus, and selected-row state in `crates/lushtext/tests/widget/sidebar.rs` and `crates/lushtext/tests/widget/window.rs`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that must exist before any user story can land

**CRITICAL**: No user story work should start until this phase is complete

- [X] T004 Implement the shared `PeekSnapshot`, `PeekPreviewState`, bounded text sampling, and generation-token helpers in `crates/lushtext-core/src/services/file_peek.rs`
- [X] T005 [P] Extend `crates/lushtext-core/src/services/file_limits.rs` with preview-safe size classification and `open_allowed` decisions consumed by the peek service
- [X] T006 [P] Add section-owned peek session fields, popover widgets, and preview child references in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` and `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`
- [X] T007 Expose public section methods and callback storage for opening, refreshing, dismissing, and promoting peek state in `crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs`
- [X] T008 Integrate `spawn_blocking_then` request dispatch and stale-result dropping across `crates/lushtext-core/src/services/file_peek.rs` and `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`

**Checkpoint**: Shared peek service and section state exist, so story work can begin

---

## Phase 3: User Story 1 - Glance At Files Without Tab Noise (Priority: P1)

**Goal**: Let users inspect sidebar-selected files temporarily without creating tabs or disturbing the active editor

**Independent Test**: From a workspace with multiple files, pressing `Space` previews the selected file, Up and Down refresh that preview in place, and no new tab is created until the user explicitly opens one

### Verification for User Story 1

- [X] T009 [P] [US1] Add unit coverage for eligible text previews, truncation metadata, and stale-generation suppression in `crates/lushtext-core/src/services/file_peek.rs`
- [X] T010 [P] [US1] Add widget coverage for `Space` toggling, Up and Down refresh, repeated-`Space` or `Escape` dismissal, click-away close, focus return to the selected sidebar row, and no-new-tab behavior in `crates/lushtext/tests/widget/workspace_section.rs` and `crates/lushtext/tests/widget/sidebar.rs`
- [ ] T011 [US1] Run live `make run` validation for text-file peeking, dismissal and focus-return behavior, no-pane-resize behavior, and the `SC-002` timing spot-check from `specs/001-file-peek/quickstart.md` across the `Small`, `Comfy`, and `Large` sidebar presets using `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`

### Implementation for User Story 1

- [X] T012 [P] [US1] Implement file-row-only `Space` toggling, repeated-`Space` dismissal, and selection-driven peek request dispatch in `crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs` and `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`
- [X] T013 [P] [US1] Build the anchored loading and text-preview card layout with filename, absolute path, size, and modified-time metadata in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`
- [X] T014 [US1] Wire `Escape`, click-away, selection changes, non-file rows, workspace filter changes, and row invalidation to dismiss or refresh peek without leaving stale overlay state in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`, `crates/lushtext-core/src/ui/sidebar/workspace_section/roots.rs`, `crates/lushtext-core/src/ui/sidebar/workspace_section/tree_loading.rs`, and `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`
- [X] T015 [US1] Enforce the read-only contract and restore sidebar focus after non-promotion dismissal so peek never creates tabs, drafts, session entries, undo state, or file monitors in `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs` and `crates/lushtext-core/src/services/file_peek.rs`

**Checkpoint**: User Story 1 delivers the MVP sidebar peek flow

---

## Phase 4: User Story 2 - Commit A Previewed File Into Normal Editing (Priority: P2)

**Goal**: Let users promote a previewed file into the normal open-document workflow without duplicate tabs

**Independent Test**: Starting from an open preview, `Enter` or the preview `Open` action opens the file through the existing document flow, closes the preview, and focuses the already-open tab when the file was open before

### Verification for User Story 2

- [X] T016 [P] [US2] Add widget coverage for `Enter` promotion, preview `Open`, and duplicate-tab reuse in `crates/lushtext/tests/widget/window.rs` and `crates/lushtext/tests/widget/workspace_section.rs`
- [ ] T017 [US2] Run live `make run` validation for promotion handoff, preview close-on-open, and editor focus behavior using `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs` and `crates/lushtext-core/src/ui/window/documents.rs`

### Implementation for User Story 2

- [X] T018 [P] [US2] Add preview-promotion callback plumbing from the workspace section into the sidebar shell in `crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs` and `crates/lushtext-core/src/ui/sidebar/callbacks.rs`
- [X] T019 [P] [US2] Reuse the existing sidebar-to-window open-document path for promoted files in `crates/lushtext-core/src/ui/sidebar/mod.rs` and `crates/lushtext-core/src/ui/window/documents.rs`
- [X] T020 [US2] Close the peek surface and hand focus off correctly after promotion in `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs` and `crates/lushtext-core/src/ui/window/imp.rs`

**Checkpoint**: User Story 2 turns the temporary peek into a normal editing entry point without tab duplication

---

## Phase 5: User Story 3 - Understand Unsupported Or Risky Files Quickly (Priority: P3)

**Goal**: Show explicit fallback states for files that cannot or should not be previewed inline

**Independent Test**: From the sidebar, binary, unreadable, and too-large files always show a visible fallback explanation and the preview never hangs, shows blank content, or misrepresents whether normal open is allowed

### Verification for User Story 3

- [X] T021 [P] [US3] Add unit coverage for binary, unreadable, and too-large classification in `crates/lushtext-core/src/services/file_peek.rs` and `crates/lushtext-core/src/services/file_limits.rs`
- [X] T022 [P] [US3] Add widget coverage for fallback copy, disabled promotion, and dismissal from unsupported states in `crates/lushtext/tests/widget/workspace_section.rs` and `crates/lushtext/tests/widget/window.rs`
- [ ] T023 [US3] Run live `make run` validation with binary, unreadable, and oversized files against `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs` to confirm explicit fallback states and warning-free dismissal

### Implementation for User Story 3

- [X] T024 [P] [US3] Implement fallback classification and preview payload generation for `Loading`, `BinaryOrUnsupported`, `Unreadable`, and `TooLargeToOpen` in `crates/lushtext-core/src/services/file_peek.rs` and `crates/lushtext-core/src/services/file_limits.rs`
- [X] T025 [P] [US3] Render fallback copy, button enabled-state, and user-facing unavailable messaging in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`
- [X] T026 [US3] Wire fallback-state rendering and open-allowed behavior into the live peek workflow in `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`

**Checkpoint**: Unsupported and risky files fail clearly and safely inside the same peek surface

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Sync docs and run the full acceptance gate after all story work is complete

- [X] T027 [P] Update `docs/next/file-peek.md`, `README.md`, and `AGENTS.md` to match the shipped file-peek UX contract and the added module layout
- [X] T028 Fix any GTK warning, focus leak, or anchor invalidation regression uncovered during verification in `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs`, `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`, and `crates/lushtext/tests/widget/workspace_section.rs`
- [ ] T029 Run the full acceptance gate with `make test-unit`, `make test-widget-headless`, `cargo bench -p lushtext-core --no-run`, `make check`, and the `SC-002` quickstart timing run against `crates/lushtext-core/src/services/file_peek.rs`, `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs`, and `specs/001-file-peek/quickstart.md`
- [ ] T030 Validate the implementation against `specs/001-file-peek/quickstart.md` and reconcile any final workflow drift in `specs/001-file-peek/quickstart.md` and `docs/next/file-peek.md`

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1: Setup** has no dependencies and can start immediately
- **Phase 2: Foundational** depends on Phase 1 and blocks all story work
- **Phase 3: User Story 1** depends on Phase 2 and is the MVP slice
- **Phase 4: User Story 2** depends on Phase 3 because promotion extends the preview workflow introduced in US1
- **Phase 5: User Story 3** depends on Phase 3 because fallback states extend the same preview workflow introduced in US1
- **Phase 6: Polish** depends on the completion of the user stories you plan to ship

### User Story Dependencies

- **US1** can start as soon as the foundational phase is complete
- **US2** requires the reusable peek surface from **US1**
- **US3** requires the reusable peek surface from **US1**
- After **US1** lands, **US2** and **US3** can proceed in parallel if staffing allows

### Within Each User Story

- Add the required automated coverage before or alongside code changes
- Land GTK-free service and classification changes before deeper widget wiring when practical
- Complete live GTK validation before calling the story done
- Fix any warning, focus, or invalidation regression uncovered during that story before moving on

### Parallel Opportunities

- `T002` and `T003` can run in parallel once the feature surface is agreed
- `T005` and `T006` can run in parallel after `T004`
- `T009` and `T010` can run in parallel for US1 verification
- `T012` and `T013` can run in parallel once the foundational phase is complete
- `T016` can proceed while `T018` and `T019` are being implemented for US2
- `T021` and `T022` can run in parallel for US3 verification
- `T024` and `T025` can run in parallel before `T026` wires the final live behavior

---

## Parallel Example: User Story 1

```bash
Task: "T009 Add unit coverage in crates/lushtext-core/src/services/file_peek.rs"
Task: "T010 Add widget coverage in crates/lushtext/tests/widget/workspace_section.rs and crates/lushtext/tests/widget/sidebar.rs"

Task: "T012 Implement Space toggling in crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs and crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs"
Task: "T013 Build the anchored preview card in crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs"
```

## Parallel Example: User Story 2

```bash
Task: "T018 Add preview-promotion callback plumbing in crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs and crates/lushtext-core/src/ui/sidebar/callbacks.rs"
Task: "T019 Reuse the open-document path in crates/lushtext-core/src/ui/sidebar/mod.rs and crates/lushtext-core/src/ui/window/documents.rs"
```

## Parallel Example: User Story 3

```bash
Task: "T021 Add unit coverage in crates/lushtext-core/src/services/file_peek.rs and crates/lushtext-core/src/services/file_limits.rs"
Task: "T022 Add widget coverage in crates/lushtext/tests/widget/workspace_section.rs and crates/lushtext/tests/widget/window.rs"

Task: "T024 Implement fallback classification in crates/lushtext-core/src/services/file_peek.rs and crates/lushtext-core/src/services/file_limits.rs"
Task: "T025 Render fallback messaging in crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs"
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1
4. Validate the MVP with `make test-unit`, targeted widget coverage, and live `make run`

### Incremental Delivery

1. Ship **US1** first to deliver the core scan-without-tabs workflow
2. Add **US2** next to turn the preview into a fast path into normal editing
3. Add **US3** last to harden unsupported and risky file handling
4. Finish with Phase 6 polish, docs sync, and the full acceptance gate
