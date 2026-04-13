---

description: "Task list template for feature implementation"
---

# Tasks: [FEATURE NAME]

**Input**: Design documents from `/specs/[###-feature-name]/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Verification is REQUIRED for any behavior, logic, or UI change. Each
user story should include the needed automated coverage, and GTK-sensitive work
should include live runtime validation tasks unless the spec explicitly records
why a category is not applicable.

**Organization**: Tasks are grouped by user story to enable independent
implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Path Conventions

- **Core application logic**: `crates/lushtext-core/src/model/`,
  `crates/lushtext-core/src/services/`, `crates/lushtext-core/src/ui/`
- **Binary crate and higher-level tests**: `crates/lushtext/`,
  `crates/lushtext/tests/`
- **User-facing and agent guidance**: `README.md`, `AGENTS.md`,
  `.agents/rules/*.md`
- **Packaging and install-time assets**: `build-aux/`, `data/`, `resources/`,
  `meson.build`

<!--
  ============================================================================
  IMPORTANT: The tasks below are SAMPLE TASKS for illustration purposes only.

  The /speckit.tasks command MUST replace these with actual tasks based on:
  - User stories from spec.md (with their priorities P1, P2, P3...)
  - Feature requirements from plan.md
  - Entities from data-model.md
  - Endpoints or file contracts from contracts/

  Tasks MUST be organized by user story so each story can be:
  - Implemented independently
  - Tested independently
  - Delivered as an MVP increment

  DO NOT keep these sample tasks in the generated tasks.md file.
  ============================================================================
-->

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Identify the real working surface and prepare shared scaffolding

- [ ] T001 Identify the touched paths in `crates/lushtext-core/src/`,
      `crates/lushtext/tests/`, and supporting assets
- [ ] T002 Add or update feature scaffolding, resources, settings keys, or
      manifests required by the plan
- [ ] T003 [P] Prepare the targeted test locations (`#[cfg(test)]`,
      integration, or widget harness files) needed for the feature

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core infrastructure that MUST be complete before ANY user story can
be implemented

**CRITICAL**: No user story work can begin until this phase is complete

Examples of foundational tasks (adjust based on your project):

- [ ] T004 Add or update shared domain types in
      `crates/lushtext-core/src/model/`
- [ ] T005 [P] Add or refactor GTK-free service logic in
      `crates/lushtext-core/src/services/`
- [ ] T006 [P] Define or adjust window, widget, action, or state wiring
      surfaces in `crates/lushtext-core/src/ui/`
- [ ] T007 Establish persistence, async, or data-safety primitives required
      before story work can proceed
- [ ] T008 Capture documentation, rule, and packaging fallout that every story
      will need to satisfy
- [ ] T009 Prepare any build, schema, resource, or Flatpak inputs the feature
      depends on

**Checkpoint**: Foundation ready - user story implementation can now begin in
parallel

---

## Phase 3: User Story 1 - [Title] (Priority: P1)

**Goal**: [Brief description of what this story delivers]

**Independent Test**: [How to verify this story works on its own]

### Verification for User Story 1

- [ ] T010 [P] [US1] Add or update unit coverage in the nearest `#[cfg(test)]`
      module for the changed service or model logic
- [ ] T011 [P] [US1] Add or update widget or integration coverage in
      `crates/lushtext/tests/` for the user-visible workflow
- [ ] T012 [US1] Run live validation (`make run` and other targeted checks) for
      the affected GTK flow and confirm warning-free behavior

### Implementation for User Story 1

- [ ] T013 [P] [US1] Implement or update domain and service support in
      `crates/lushtext-core/src/model/` and
      `crates/lushtext-core/src/services/`
- [ ] T014 [P] [US1] Implement or update GTK wiring in the relevant
      `crates/lushtext-core/src/ui/` modules
- [ ] T015 [US1] Integrate persistence, actions, notifications, or status
      updates needed for the story's workflow
- [ ] T016 [US1] Add data-safety guards, error handling, and recovery behavior
      required by the spec
- [ ] T017 [US1] Update story-specific docs or rules if this slice changes
      visible behavior or contributor workflow

**Checkpoint**: User Story 1 should now be fully functional and testable
independently

---

## Phase 4: User Story 2 - [Title] (Priority: P2)

**Goal**: [Brief description of what this story delivers]

**Independent Test**: [How to verify this story works on its own]

### Verification for User Story 2

- [ ] T018 [P] [US2] Add or update unit coverage for the changed logic
- [ ] T019 [P] [US2] Add or update widget or integration coverage for the
      user-visible workflow
- [ ] T020 [US2] Run live validation for any GTK warnings, focus, sizing, or
      animation risk introduced by this story

### Implementation for User Story 2

- [ ] T021 [P] [US2] Implement or update the affected model or service files
- [ ] T022 [US2] Implement or update the affected UI modules and action wiring
- [ ] T023 [US2] Integrate with User Story 1 components while preserving
      independent testability
- [ ] T024 [US2] Update data-safety, docs, or packaging artifacts required by
      this story

**Checkpoint**: User Stories 1 and 2 should both work independently

---

## Phase 5: User Story 3 - [Title] (Priority: P3)

**Goal**: [Brief description of what this story delivers]

**Independent Test**: [How to verify this story works on its own]

### Verification for User Story 3

- [ ] T025 [P] [US3] Add or update unit coverage for the changed logic
- [ ] T026 [P] [US3] Add or update widget or integration coverage for the
      user-visible workflow
- [ ] T027 [US3] Run live validation for any GTK warnings, focus, sizing, or
      animation risk introduced by this story

### Implementation for User Story 3

- [ ] T028 [P] [US3] Implement or update the affected model or service files
- [ ] T029 [US3] Implement or update the affected UI modules and action wiring
- [ ] T030 [US3] Update data-safety, docs, or packaging artifacts required by
      this story

**Checkpoint**: All user stories should now be independently functional

---

[Add more user story phases as needed, following the same pattern]

---

## Phase N: Polish & Cross-Cutting Concerns

**Purpose**: Improvements that affect multiple user stories

- [ ] TXXX [P] Update `README.md`, `AGENTS.md`, and relevant `.agents/rules/*.md`
- [ ] TXXX Fix any pre-existing blocker uncovered during implementation or
      verification before sign-off
- [ ] TXXX Run the full required verification set (`make test`, `make check`,
      targeted widget runs, and live runtime checks)
- [ ] TXXX Regenerate `build-aux/cargo-sources.json` and related packaging
      metadata if dependencies changed
- [ ] TXXX Validate `quickstart.md` or equivalent user workflow documentation

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies - can start immediately
- **Foundational (Phase 2)**: Depends on Setup completion - blocks all user
  stories
- **User Stories (Phase 3+)**: All depend on Foundational phase completion
  - User stories can then proceed in parallel if staffing allows
  - Or sequentially in priority order (P1 -> P2 -> P3)
- **Polish (Final Phase)**: Depends on all desired user stories being complete

### User Story Dependencies

- **User Story 1 (P1)**: Can start after Foundational (Phase 2)
- **User Story 2 (P2)**: Can start after Foundational (Phase 2) and may
  integrate with US1 while staying independently testable
- **User Story 3 (P3)**: Can start after Foundational (Phase 2) and may
  integrate with US1 or US2 while staying independently testable

### Within Each User Story

- Required automated tests should be added before or alongside implementation
  and should fail before the fix when practical
- Model and service changes before GTK wiring when the workflow allows it
- Core implementation before cross-story integration
- Data-safety and recovery behavior before declaring the story complete
- Story complete before moving to the next priority

### Parallel Opportunities

- All Setup tasks marked [P] can run in parallel
- All Foundational tasks marked [P] can run in parallel within Phase 2
- Once Foundational phase completes, user stories can start in parallel if
  capacity allows
- Verification tasks for a single story marked [P] can run in parallel
- Independent model/service and UI tasks can run in parallel when they do not
  touch the same files

---

## Parallel Example: User Story 1

```bash
# Launch all verification work for User Story 1 together when file ownership allows:
Task: "Add unit coverage in the nearest #[cfg(test)] module"
Task: "Add widget or integration coverage in crates/lushtext/tests/"

# Launch independent implementation tasks together:
Task: "Update service logic in crates/lushtext-core/src/services/..."
Task: "Update UI wiring in crates/lushtext-core/src/ui/..."
```

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1
4. **STOP and VALIDATE**: Verify User Story 1 independently
5. Deploy or demo if ready

### Incremental Delivery

1. Complete Setup + Foundational -> foundation ready
2. Add User Story 1 -> verify independently -> deploy or demo
3. Add User Story 2 -> verify independently -> deploy or demo
4. Add User Story 3 -> verify independently -> deploy or demo
5. Each story adds value without breaking previous stories

### Parallel Team Strategy

With multiple developers:

1. Team completes Setup + Foundational together
2. Once Foundational is done:
   - Developer A: User Story 1
   - Developer B: User Story 2
   - Developer C: User Story 3
3. Stories complete and integrate independently

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story should be independently completable and testable
- Include documentation, packaging, and live runtime validation tasks whenever
  the constitution makes them relevant
- Fix pre-existing blockers discovered in the work stream instead of deferring
  them
- Commit after each task or logical group
- Stop at any checkpoint to validate story independently
- Avoid vague tasks, same-file conflicts, and cross-story dependencies that
  break independence
