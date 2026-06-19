## 1. Accessibility Matrix And Baseline Audit

- [x] 1.1 Create `docs/accessibility-matrix.md` with one row per surface/state extreme and columns for semantics, keyboard path, announcements, visual expectation, proof lane, stable anchors, manual Orca expectation, and artifact owner.
- [x] 1.2 Populate matrix rows for shell/header/status/tab controls, editor, Markdown preview, workspace sidebar/file tree, Open popover, command palette, in-tab search, workspace search, document properties, notes/bookmarks, local history, preferences, save/close/destructive dialogs, context menus, focus mode, preview mode, minimap, compact/bottom-sheet layouts, recovery surfaces, and error surfaces.
- [x] 1.3 Mark every matrix state extreme as no-context, representative, dense/awkward, constrained/compact, hidden/dismissed, busy/loading, error, recovery, or destructive confirmation as applicable.
- [x] 1.4 Map every existing `scripts/run-accessibility-smoke.sh` case to matrix row ids and identify uncovered matrix rows.
- [x] 1.5 Map every existing `scripts/run-visual-smoke.sh` and visual-geometry case to matrix row ids where visual accessibility proof applies.
- [x] 1.6 Audit current UI code for direct GTK accessibility calls outside `ui::accessibility` and decide whether each call should be normalized or documented as an exception.
- [x] 1.7 Audit current list/factory rows for row metadata apply/clear coverage and record gaps in the matrix.
- [x] 1.8 Audit current transient surfaces for accessible names, focus restoration, dismissal behavior, hidden-state cleanup, and keyboard proof gaps.
- [x] 1.9 Audit current keyboard/context menu paths for pointer-only or hover-only operations and record missing accessible fallbacks.
- [x] 1.10 Update `docs/accessibility.md`, `docs/end-user-coverage.md`, and `.agents/rules/*.md` to reference the matrix as the accessibility completion source of truth.

## 2. Shared Accessibility Helper Boundary

- [x] 2.1 Extend `crates/lushtext-core/src/ui/accessibility.rs` with any missing helpers needed by the matrix, including typed helpers for common label/description/state/relation combinations and documented reset paths.
- [x] 2.2 Normalize direct metadata calls in `crates/lushtext-core/src/ui/sidebar/imp.rs` through `ui::accessibility`.
- [x] 2.3 Normalize direct metadata calls in `crates/lushtext-core/src/ui/sidebar/workspace_section/folders.rs` through `ui::accessibility`.
- [x] 2.4 Normalize direct metadata calls in `crates/lushtext-core/src/ui/window/dialogs.rs` through `ui::accessibility`.
- [x] 2.5 Normalize direct metadata calls in `crates/lushtext-core/src/ui/info_bar/imp.rs` through `ui::accessibility` or document a narrow exception.
- [x] 2.6 Add a local allowlist file or inline policy convention for any direct GTK accessibility calls that remain necessary.
- [x] 2.7 Add widget tests proving new helper APIs set and clear labels, descriptions, relations, key shortcuts, popup/value text, readonly, multiline, hidden, busy, invalid, disabled, expanded, selected, pressed, and bounded announcements.
- [x] 2.8 Add widget tests proving row metadata helpers clear stale selected state, position metadata, descriptions, and item labels after unbind/model replacement.
- [x] 2.9 Run the focused helper and row-accessibility widget tests.

## 3. Surface Metadata And Dynamic State Completion

- [x] 3.1 Complete shell/header/status/tab control metadata, including icon-only labels, toggle pressed states, value text, relations, and keyboard shortcut metadata.
- [x] 3.2 Complete editor accessibility metadata for active document identity, loading state, saving state, failed-load state, large-file readonly policy, preview-only readonly state, and editable restoration.
- [x] 3.3 Complete Markdown preview and side-by-side preview metadata for read-only region identity, scroll area, rendered text, embedded code/table/image/link/fallback states, and hidden-surface cleanup.
- [x] 3.4 Complete workspace sidebar and file-tree metadata for workspace scope selector, workspace headers, collapse/expand state, zero-folder state, dense/deep rows, file peek, focus-folder affordance, drag handles, and drop targets.
- [x] 3.5 Complete Open popover metadata for empty, dense, filtered, no-match, remove-recent, open-another-file, and row-recycling states.
- [x] 3.6 Complete command palette metadata for files, commands, notes, dense results, mode changes, no-results, selected/current row state, and focus restoration.
- [x] 3.7 Complete in-tab search metadata for query/replacement entries, result counts, invalid/no-result state, next/previous controls, and close behavior.
- [x] 3.8 Complete workspace search metadata for no-workspace, representative results, no-results, dense constrained results, replace preview, include/exclude checkboxes, Replace All completion, undo availability, saved searches, and history rows.
- [x] 3.9 Complete document properties metadata for wide pane, compact bottom sheet, all row values, file-health states, formatting source, line-ending and encoding controls.
- [x] 3.10 Complete notes/bookmarks metadata for empty, populated, no-results, dense, constrained, preview, open/copy/edit/delete actions, and row recycling.
- [x] 3.11 Complete local-history metadata for empty, populated, empty snapshot, preview, copy, restore, destructive restore confirmation, and row recycling.
- [x] 3.12 Complete preferences metadata for every page, row, switch, combo, spin/control row, data page scan state, and scan announcements.
- [x] 3.13 Complete save/close/destructive dialog metadata for grouped modified documents, per-document checkboxes, response labels, suggested/destructive appearance, focus order, and keyboard cancellation.
- [x] 3.14 Complete focus mode, minimap, invisible-character, preview mode, recovery, migration, format-upgrade, and inline alert metadata required by the matrix.
- [x] 3.15 Add or update widget tests for every dynamic state projection that can go stale.

## 4. Keyboard Parity, Context Menus, And Focus Semantics

- [x] 4.1 Verify every matrix operation has a keyboard, menu, command palette, context menu, or equivalent accessible path.
- [x] 4.2 Add keyboard-operable context menu coverage for file tree rows, workspace headers, note/bookmark rows, local-history rows, editor/tab actions, search-result rows, and preview surfaces.
- [x] 4.3 Add accessible fallbacks for hover-only, overlay-only, drag-only, or pointer-convenience actions that lack keyboard parity.
- [x] 4.4 Ensure `Menu`, `Shift+F10`, command-palette, or documented fallback behavior works without pointer coordinates where the desktop session supports it.
- [x] 4.5 Verify transient surface opening stores the correct focus target and dismissal restores the documented target or fallback.
- [x] 4.6 Verify hidden or destroyed widgets are never final focus targets after command palette, Open popover, search bar, dialogs, file peek, context menus, bottom sheets, preview modes, or focus mode close.
- [x] 4.7 Add widget tests for focus restoration and Escape behavior across no-context, representative, dense, and constrained transient states.
- [x] 4.8 Verify destructive keyboard paths preserve normal confirmation, durable-write, undo, recovery, and error behavior.

## 5. Accessibility Smoke Expansion

- [x] 5.1 Refactor `scripts/run-accessibility-smoke.sh` manifests so each case records matrix row ids, anchors asserted, focus/text evidence, host caveats, and fixture-only versus public anchors.
- [x] 5.2 Add accessibility smoke coverage for save and unsaved-close dialogs, including per-document checkboxes and response buttons.
- [x] 5.3 Add accessibility smoke coverage for destructive confirmations, including delete, discard, restore, Replace All, and migration/format-upgrade warning paths where existing automation can reach them safely.
- [x] 5.4 Add accessibility smoke coverage for context menus and keyboard context-menu fallbacks.
- [x] 5.5 Add accessibility smoke coverage for focus mode, minimap toggle/state, invisible-character cycling, and preview mode transitions.
- [x] 5.6 Add accessibility smoke coverage for compact document-properties bottom sheet and compact secondary-surface conflict behavior.
- [x] 5.7 Add accessibility smoke coverage for editor loading, saving, failed-load, large-file policy, preview-only readonly, caret metadata, and selection metadata where AT-SPI exposes them.
- [x] 5.8 Add accessibility smoke coverage for read-only file peek, note preview, bookmark preview, local-history preview, and Markdown preview text-interface evidence.
- [x] 5.9 Add accessibility smoke coverage for workspace tree context actions, zero-folder, dense/awkward, deep expanded, file peek, focus-folder, refresh busy/error state, and watcher-related status.
- [x] 5.10 Add accessibility smoke coverage for Open popover, command palette, workspace search, notes/bookmarks, and local-history row recycling after filtering or model replacement.
- [x] 5.11 Add accessibility smoke coverage for alert and announcement outcomes that can be proven through AT-SPI-visible labels/states.
- [x] 5.12 Ensure every smoke case has focused `--case` support and no case requires real user workspace data.
- [x] 5.13 Update `docs/automation-reference.md` stable AT-SPI anchor table and helper flag list after smoke changes.
- [x] 5.14 Run focused smoke cases as they are added, then run full `make accessibility-smoke`.

## 6. Visual Accessibility Proof

- [x] 6.1 Update `scripts/run-visual-smoke.sh` manifests to reference matrix row ids for visual accessibility coverage groups.
- [x] 6.2 Add or update visual-smoke cases for focus visibility across shell, editor, rows, dialogs, context menus, bottom sheets, and preview surfaces.
- [x] 6.3 Add or update visual-smoke cases for dark, high-contrast where supported, large text, reduced motion where supported, transparency/readability, compact layout, narrow width, short height, dense rows, long labels, and destructive/error states.
- [x] 6.4 Add or update visual proof for color-not-only communication of warning, error, success, selection/current row, search match, modified tab, disabled action, file health, local-history restore state, bookmark state, destructive action, and replacement preview state.
- [x] 6.5 Add visual-geometry invariants where same-session pixel proof is needed for focus rings, fixed controls, minimap/focus interactions, compact bottom sheet, and protected chrome.
- [x] 6.6 Ensure unsupported visual variants skip explicitly and are not counted as passing evidence.
- [x] 6.7 Run focused visual smoke and visual-geometry cases while adding coverage.
- [x] 6.8 Run full `make visual-smoke`, `make visual-geometry-smoke`, and `make check-visual-proof-policy` before completing the change.

## 7. Manual Orca Validation Workflow

- [x] 7.1 Add a manual Orca checklist template under `docs/` or `scripts/` that records build, install mode, OS, GNOME/session details, display backend, theme, text scale, screen reader version, workflows, outcomes, caveats, and linked automated artifacts.
- [x] 7.2 Document expected manual coverage for shell navigation, editor focus, typing, caret feedback, selection feedback, in-tab search, command palette, Open popover, workspace search, workspace sidebar/file tree, document properties, preferences, Markdown preview, notes/bookmarks, local history, destructive/close dialogs, context menus, and changed workflows.
- [x] 7.3 Add release guidance that skipped AT-SPI, compositor, visual, or text-interface automation remains unverified until another runner or manual Orca environment covers it.
- [x] 7.4 Add a bounded sample manual validation artifact or template example that does not include private document text.
- [x] 7.5 Run or explicitly record a manual Orca validation pass for the final changed workflows if the host session supports it.

## 8. Policy, Drift, And Freshness Guardrails

- [x] 8.1 Extend `scripts/check-accessibility-policy.py` with strict current-tree mode for helper bypasses, direct GTK calls, row factories without apply/clear logic, icon-only controls without names, hover-only affordances without fallback evidence, and transient surfaces without matrix coverage.
- [x] 8.2 Add or update self-tests for every new accessibility policy rule.
- [x] 8.3 Extend docs drift checks so accessibility matrix rows, smoke cases, helper flags, stable AT-SPI anchors, and automation reference entries stay synchronized.
- [x] 8.4 Add freshness checks or documented review steps for unfiltered passed accessibility summaries tied to the current relevant tree or exact release commit.
- [x] 8.5 Ensure filtered smoke summaries are marked as focused diagnostics and cannot satisfy full release-grade coverage unless a scoped release note explicitly allows them.
- [x] 8.6 Ensure fixture-only anchors are marked as fixture-only and are not presented as public product anchors.
- [x] 8.7 Update `make check-accessibility-policy`, `make check-policy`, and help text as needed for new strict/current-tree behavior.
- [x] 8.8 Run `make check-accessibility-policy`, `make check-automation-docs`, and `make check-policy`.

## 9. Privacy, Data Safety, And Artifact Boundaries

- [x] 9.1 Audit accessibility labels, descriptions, announcements, and value text for unbounded document, note, draft, search-result, local-history, or sidecar identifier leakage.
- [x] 9.2 Audit accessibility smoke fixtures to ensure they create isolated synthetic app data and do not depend on the user's real workspace.
- [x] 9.3 Audit generated AT-SPI tree excerpts, screenshots, logs, manual templates, and manifest fields for bounded fixture-only text.
- [x] 9.4 Add tests for UTF-8 safe announcement truncation, repeated announcement throttling, and privacy-safe message keys where gaps exist.
- [x] 9.5 Verify accessibility-triggered save, close, discard, delete, restore, Replace All, undo, migration, and format-upgrade paths use normal safety behavior.
- [x] 9.6 Update `docs/accessibility.md` with privacy and artifact-boundary guidance discovered during implementation.

## 10. Documentation And Release Integration

- [x] 10.1 Update `docs/accessibility.md` to describe the completed matrix, full proof stack, manual Orca workflow, known GTK/AT-SPI caveats, and release-grade evidence rules.
- [x] 10.2 Update `docs/end-user-coverage.md` to explain matrix ownership, current-tree guardrails, and full release validation expectations.
- [x] 10.3 Update `docs/automation.md` and `docs/automation-reference.md` for new readiness, helper flags, artifacts, stable anchors, matrix ids, and scenario manifest fields.
- [x] 10.4 Update `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, `.agents/rules/build.md`, and `.agents/rules/documentation.md` for the completed accessibility workflow.
- [x] 10.5 Update `AGENTS.md` Rules Index or nested guidance if any rule file materially changes or new accessibility docs become required context.
- [x] 10.6 Update user-facing release validation notes or checklist templates so future releases cannot claim skipped accessibility coverage as passing.

## 11. Final Verification

- [x] 11.1 Run `openspec validate complete-gtk-accessibility-spine --strict`.
- [x] 11.2 Run `openspec validate --specs --strict`.
- [x] 11.3 Run `make test-widget-headless`.
- [x] 11.4 Run `make check-accessibility-policy`.
- [x] 11.5 Run `make check-automation-docs`.
- [x] 11.6 Run `make accessibility-smoke` and review `build/smoke/accessibility/summary.json`.
- [x] 11.7 Run `make visual-smoke` and review `build/smoke/visual/summary.json`.
- [x] 11.8 Run `make visual-geometry-smoke` and review `build/smoke/visual-geometry/summary.json`.
- [x] 11.9 Run `make check-visual-proof-policy`.
- [x] 11.10 Run `make check-policy`.
- [x] 11.11 Run manual Orca validation or record the exact unsupported-host reason and alternate evidence plan.
- [x] 11.12 Confirm every matrix row is verified, caveated, or intentionally deferred with user approval before marking the OpenSpec change complete.
