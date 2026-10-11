## ADDED Requirements

### Requirement: Protected View is a workflow with its own role home
`docs/workflow-readability-matrix.md` SHALL gain a `WFR-PROTECTED-VIEW` row with its role home at `ui/editor_page/protected_view/`:
- `mod.rs`, the narrative facade, which names each resumption point (source-capture turns, find worker completion, inspector window reads, whole-line clipboard writes, and the edit-time entry inside the `insert-text`/`delete-range` after-handler) and stays within the facade line budget;
- `policy.rs`, pure: gesture, key and action dispositions, inspector window arithmetic and find batch budgets. It reads `B_interact` and `B_settle` from C0's responsiveness-budget module and does not restate them;
- `seams.rs`, with the `ProtectedViewTicket` and `ProtectedViewFacts` seam;
- `execution.rs`, `find_execution.rs` and `inspector_execution.rs`, coordination;
- `evidence.rs`, the single typed source for tests and automation;
- `test_policy.rs`, only if a test-only value is required, within the matrix's externally reachable `*_for_test` ceiling.

The Line Inspector (`inspector.rs`) and the fold-marker gutter (`fold_gutter.rs`) SHALL be declared called presentation surfaces. The fold executor `ui/protected_fold.rs` and its pure half `model/protected_fold.rs` SHALL be cross-cutting with no role. `policy.rs` and `model/protected_fold.rs` SHALL carry `#![forbid(clippy::float_arithmetic, clippy::disallowed_methods)]` and appear in the whole-pixel table at ceiling 0. The change SHALL re-derive the measured cells of every row it materially restructures: at least `WFR-DOCUMENT-LOAD`, `WFR-BUFFER-REPLACEMENT`, `WFR-EDITOR-FIND`, `WFR-DOCUMENT-SAVE` (the protected save mode), `WFR-LOCAL-HISTORY`, `WFR-PRINT` and `WFR-SESSION-RESTORE` (scroll-only restore), plus `WFR-NOTES-BOOKMARKS`, `WFR-ENCODING` and `WFR-SEARCH-REPLACE` if their gating changes more than a call site. `make check-workflow-boundaries` SHALL pass.

#### Scenario: Boundary check with the new row
- **WHEN** `make check-workflow-boundaries` runs after the change
- **THEN** every `.rs` file under `ui/editor_page/protected_view/` is declared by the row, the policy is pure, and the whole-pixel table matches the script
