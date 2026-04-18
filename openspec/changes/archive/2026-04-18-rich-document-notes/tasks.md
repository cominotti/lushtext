## 1. Core Note Models And Persistence

- [x] 1.1 Add shared rich-note model primitives and persistence helpers that can be reused by range, document, and workspace note flows without changing source-file bytes.
- [x] 1.2 Add saved-file document-note storage keyed by canonical document identity, including load/save/delete behavior and Save As / in-app rename migration coverage.
- [x] 1.3 Add workspace-note storage keyed by canonical workspace-root identity, including clear behavior, in-app root rename migration, replace-root reset behavior, and unlist/re-add restoration.
- [x] 1.4 Extend existing range-annotation persistence and export flows so note bodies remain compatible while user-facing workflows shift to richer range notes and `Export Range Notes…`.

## 2. Note Editing And Render Surfaces

- [x] 2.1 Build a shared note surface that supports editable text mode and read-only rendered markdown mode by reusing `LushtextMarkdownPreview`.
- [x] 2.2 Replace the current range-annotation editor flow with the shared note surface while preserving line-range selection, style editing, and delete behavior.
- [x] 2.3 Add document-note open, create, clear, restore, and saved-file guardrail workflows on the main window shell.
- [x] 2.4 Add workspace-note open, create, clear, restore, and aggregate-scope guardrail workflows on the main window shell.

## 3. Unified Notes Navigation

- [x] 3.1 Update the `Notes` menu actions, labels, and sensitivity rules to expose `Add Range Note…`, `Edit Range Note…`, `Open Document Note…`, `Open Workspace Note…`, `Browse Notes…`, and `Export Range Notes…` in the correct scope sections.
- [x] 3.2 Implement a unified `Browse Notes…` dialog patterned after the local-history viewer that lists workspace, document, and range notes for the current shared workspace scope.
- [x] 3.3 Wire browser row activation so range notes open the right file and range, document notes open the saved file's document-note surface, and workspace notes open without requiring an active document tab.

## 4. Scope And Lifecycle Integration

- [x] 4.1 Integrate document-note and range-note restore flows with file open, session restore, Save As, and in-app file or directory rename handling.
- [x] 4.2 Integrate workspace-note restore flows with current workspace scope changes, concrete-vs-aggregate behavior, workspace rename, root replacement, and unlist/re-add flows.
- [x] 4.3 Keep workspace-scoped browsing and export behavior aligned with the shared workspace scope, including aggregate `All workspaces` behavior and explicit scope metadata in browser rows.

## 5. Verification

- [x] 5.1 Add unit and service tests for document-note and workspace-note persistence, identity rules, migration behavior, and empty-state cleanup.
- [x] 5.2 Add widget or integration coverage for note edit/render switching, `Notes` menu sensitivity, unified notes browsing, and browser activation flows.
- [x] 5.3 Run the relevant Rust test suites and targeted widget tests, then capture any remaining manual checks for aggregate scope, saved-file guardrails, and range-note export behavior.
