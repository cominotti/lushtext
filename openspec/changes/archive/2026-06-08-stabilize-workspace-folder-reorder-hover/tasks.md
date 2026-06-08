## 1. Row Shield Structure

- [x] 1.1 Add a transparent full-row reorder DnD shield overlay in the workspace-section row factory, above `GtkTreeExpander` and separate from the fixed-height insertion-line overlay.
- [x] 1.2 Keep the shield inert outside active workspace-folder reorder drags so ordinary row expansion, activation, context menus, file peek, focus-folder hover, and inline rename interactions continue to target their existing widgets.
- [x] 1.3 Ensure the shield covers the realized row allocation during active reorder drags without changing row height, label measurement, drag-handle placement, focus-folder overlay placement, or sidebar horizontal-scroll behavior.
- [x] 1.4 Reset shield visibility/targetability and insertion-line state during row bind/unbind so recycled `GtkListView` rows cannot leak stale reorder hover state.

## 2. DnD Routing

- [x] 2.1 Move workspace-folder reorder `GtkDropTarget` ownership from the parent row overlay to the full-row shield while preserving existing drag payload encoding/decoding.
- [x] 2.2 Route shield hover through the existing same-workspace top-level folder validity checks so only legal targets show the single rounded before/after insertion line.
- [x] 2.3 Ensure descendant rows, expander regions, placeholder rows, empty states, section headers, drill-down rows, and other workspaces are owned by the shield during active reorder hover but show no insertion feedback and reject drops.
- [x] 2.4 Preserve absolute-index reorder callback behavior, persistence dirty marking, workspace-aware structure notifications, and Move Up/Move Down behavior.
- [x] 2.5 Clear active drag and shield state on drag leave, drop, cancel, drag end, row unbind, and section rebuild.

## 3. Defensive Fallbacks And Observability

- [x] 3.1 Keep the `tree_loading.rs` drag-hover empty-child-model fallback as defensive protection, but ensure the shield path prevents normal reorder hover from reaching it.
- [x] 3.2 Add test-only instrumentation or helpers that can prove whether active reorder hover reached drag-hover child-model fallback creation.
- [x] 3.3 Add test-only helpers that exercise the same shield accept/motion/drop decision logic used by real row DnD without relying on brittle full pointer synthesis.
- [x] 3.4 Ensure active reorder hover does not restart workspace watches, change selection, focus/drill down, or alter section collapse/filter state.

## 4. Widget Tests

- [x] 4.1 Add a valid top-level reorder hover test proving the shield owns hover, exactly one insertion line is visible, disclosure icon state stays stable, no `notify::expanded` transition fires, no child store is created, and workspace watch generation is unchanged.
- [x] 4.2 Add an invalid descendant/expander-region hover test proving the shield owns hover, no insertion line is visible, dropping is rejected, and neither descendant nor ancestor folders expand or collapse.
- [x] 4.3 Add a normal-interaction test proving the shield is inactive outside active reorder drag and folder expansion, file activation, file peek, context menu targeting, focus-folder controls, and inline rename setup still work.
- [x] 4.4 Add a row-recycling test proving recycled rows clear shield targetability, insertion-line visibility, valid-target state, drag-handle state, and ordinary expansion/activation behavior for the newly bound row.
- [x] 4.5 Add a constrained-geometry test with many folders and long names proving the shield does not change row height, does not introduce a horizontal scrollbar, keeps the fixed workspace scope row visible, and keeps drag handles/header controls reachable.
- [x] 4.6 Add a filesystem-safety test or extend the existing reorder safety test to prove shield-routed DnD reorder does not create, delete, move, rename, copy, or rewrite user files.
- [x] 4.7 Add or update non-pointer reorder coverage proving Move Up/Move Down remains available and follows the same state/persistence path after shield installation.
- [x] 4.8 Update any existing tests that codify "expand then collapse" so they instead prove reorder hover never requests expansion in the intended path.

## 5. Documentation And Local Guidance

- [x] 5.1 Update the local UI guidance to describe the row-shield ownership rule: reorder DnD hover belongs to the shield, tree expansion belongs to `GtkTreeExpander`, and idle collapse is defensive only.
- [x] 5.2 Update comments around the row factory/DnD setup so future maintainers understand why the shield is separate from the visual insertion-line overlay.

## 6. Validation

- [x] 6.1 Run the focused workspace-section tests for reorder hover, insertion indicator, row recycling, constrained geometry, filesystem safety, and non-pointer reorder.
- [x] 6.2 Run `scripts/run-widget-tests.sh --headless -- workspace_section`.
- [x] 6.3 Run `scripts/run-widget-tests.sh --headless -- sidebar`.
- [x] 6.4 Run `cargo fmt --all -- --check`.
- [x] 6.5 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 6.6 Run `openspec validate --all --strict`.
- [x] 6.7 Run `git diff --check`.
