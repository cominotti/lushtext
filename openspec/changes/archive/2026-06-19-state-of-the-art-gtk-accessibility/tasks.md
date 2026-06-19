## 1. Baseline And Inventory

- [x] 1.1 Capture the current baseline with `make accessibility-smoke`, `make check-automation-docs`, and a focused widget role/metadata test run; record artifact paths and known gaps in the change notes.
- [x] 1.2 Create a reviewable accessibility surface inventory covering shell, editor, Markdown preview, workspace sidebar/file tree, Open popover, command palette, in-tab search, workspace search, properties, notes/bookmarks, local history, preferences, save/close dialogs, context menus, focus mode, preview mode, minimap, and compact layouts.
- [x] 1.3 For each inventory row, classify applicable state extremes: no required context, representative populated data, dense or awkward data, and constrained geometry.
- [x] 1.4 For each inventory row, record required accessible role, name, description, relations, dynamic states, keyboard path, announcement behavior, and proof lane.
- [x] 1.5 Run a focused GtkSourceView/AT-SPI spike for editor text, caret, selection, and read-only exposure; document which details are automatable and which require manual fallback.
- [x] 1.6 Decide the release-level manual screen-reader reference path, including whether Orca in a normal GNOME session is required in addition to the headless AT-SPI helper.

## 2. Accessibility Helper Spine

- [x] 2.1 Add an internal UI accessibility helper module for setting labels, descriptions, roles, relations, states, and announcements consistently.
- [x] 2.2 Add helper tests for label/description assignment, state update/reset behavior, relation assignment where gtk4-rs exposes it, and bounded announcements.
- [x] 2.3 Add row-factory helper patterns for refreshing and clearing item-specific accessible metadata during bind/unbind.
- [x] 2.4 Add announcement throttling helpers for debounced results, progress milestones, repeated status updates, and high-priority alerts.
- [x] 2.5 Add test-only accessibility audit utilities that widget tests can use without requiring AT-SPI.
- [x] 2.6 Add a focused policy check for new icon-only controls, custom rows, transient surfaces, hover-only affordances, and stable AT-SPI anchors.
- [x] 2.7 Update `.agents/rules/ui.md`, `.agents/rules/widget-wiring.md`, and `.agents/rules/build.md` with the helper and policy expectations.

## 3. Shell, Editor, And Status Surfaces

- [x] 3.1 Normalize shell/header/tab/status accessible labels, descriptions, toggle states, menu states, and shortcut descriptions through the helper layer.
- [x] 3.2 Add explicit accessible metadata for the main editor surface, including bounded active-document identity and editing-region description.
- [x] 3.3 Track editor editable, readonly, loading, saving, failed-load, large-file, and preview-only states in accessible metadata.
- [x] 3.4 Add AT-SPI smoke proof for active editor discovery, focus path, text exposure, caret or insertion context, and selection behavior where supported.
- [x] 3.5 Add widget tests for editor metadata and focus restoration after search, command palette, Open popover, dialogs, and preview-mode transitions.
- [x] 3.6 Normalize status-bar metadata controls, status message semantics, line-ending and encoding controls, and non-noisy announcement behavior.
- [x] 3.7 Verify shell/editor/status work with focused widget tests plus `make accessibility-smoke` filtered or scoped to the updated scenarios.

## 4. Search, Open Popover, And Command Palette

- [x] 4.1 Normalize in-tab search controls, option toggles, replace controls, match count state, no-result state, invalid state, and close behavior.
- [x] 4.2 Add debounced search-result announcements for in-tab search without announcing every keystroke.
- [x] 4.3 Normalize workspace search entries, filters, result lists, replace controls, saved-search controls, undo controls, busy/error states, and result-row metadata.
- [x] 4.4 Add workspace search AT-SPI scenarios for no workspace, representative matches, dense/awkward results, replacement completion, undo availability, and constrained geometry.
- [x] 4.5 Normalize Open popover search, chooser, recent rows, remove row action, no-recent state, no-match state, long-path tooltips, and row-recycling metadata.
- [x] 4.6 Add Open popover keyboard-only and AT-SPI scenarios for empty, representative, filtered, dense/awkward, and constrained states.
- [x] 4.7 Normalize command palette search, mode selector, result groups, result rows, no-results state, dense files, notes, commands, and selected-row metadata.
- [x] 4.8 Add command palette AT-SPI and keyboard-only scenarios for files, commands, notes, no results, dense files, mode changes, and focus restoration.

## 5. Workspace Sidebar And File Tree

- [x] 5.1 Normalize workspace selector, New Workspace button, workspace section headers, add-folder, refresh, collapse/expand, rename/remove, and zero-folder states.
- [x] 5.2 Normalize file-tree row metadata for files, folders, empty folders, expanded/collapsed folders, selected rows, focused folders, deep indentation, and long names.
- [x] 5.3 Add accessible state updates for folder expansion, workspace collapse, focused-folder drill-down, loading, empty, and error states.
- [x] 5.4 Ensure hover-only Focus Folder, reorder, and row overlay affordances have keyboard or context-menu alternatives with visible focus indication.
- [x] 5.5 Normalize inline rename, New File/New Folder, Delete, Rename, context menus, file peek, and DnD reorder accessibility semantics.
- [x] 5.6 Add widget tests for row recycling, stale metadata clearing, keyboard navigation, context-menu reachability, and focus restoration in sidebar/file-tree workflows.
- [x] 5.7 Add accessibility smoke scenarios for no workspace, zero-folder workspace, representative tree, dense/awkward paths, deep focused folder, file peek, and constrained sidebar geometry.

## 6. Notes, Bookmarks, Local History, Properties, Preferences, And Preview

- [x] 6.1 Normalize notes browser metadata for empty state, search, sidebar list, note/bookmark/folder-note rows, render/edit mode, close/back/open controls, and dense/awkward rows.
- [x] 6.2 Normalize bookmark gutter/action metadata, next/previous bookmark commands, edit-label dialog, and saved-file/no-context states.
- [x] 6.3 Normalize local-history browser metadata for empty state, snapshot list, preview, restore/copy/back controls, valid empty snapshots, dense histories, and constrained geometry.
- [x] 6.4 Normalize document-properties panel metadata for file metadata, encoding, line endings, formatting controls, file-health controls, compact bottom-sheet presentation, and requested-vs-rendered visibility.
- [x] 6.5 Normalize preferences metadata for all rows, internal spin/buttons, grouped rows, data-format actions, background opacity controls, and large-text/constrained layouts.
- [x] 6.6 Normalize Markdown preview and read-only text surfaces, including preview-only, side-by-side preview, embedded code blocks, tables, links, images, missing/remote image fallbacks, and read-only semantics.
- [x] 6.7 Add widget and AT-SPI scenarios for notes/bookmarks, local history, properties, preferences, and Markdown preview state extremes.

## 7. Announcements And Dynamic State

- [x] 7.1 Route inline alerts, failed loads, save/durability warnings, recovery warnings, invalid states, and destructive confirmations through the shared announcement policy.
- [x] 7.2 Add announcements for user-initiated save/load completion, workspace refresh/indexing, content search start/completion/cancellation, Replace All completion, undo availability, and format-upgrade scan outcomes.
- [x] 7.3 Add mode-change announcements or state updates for focus mode, preview mode, preview pane, document properties, minimap, and workspace sidebar visibility.
- [x] 7.4 Add negative tests or smoke assertions proving high-frequency typing, progress heartbeats, and repeated visible status updates do not produce announcement floods.
- [x] 7.5 Verify announcement behavior with widget-level announcement hooks where possible and AT-SPI or manual screen-reader evidence where the platform exposes it.

## 8. Automation And Accessibility Smoke Infrastructure

- [x] 8.1 Add or refine Automation1 readiness predicates so accessibility scenarios can wait for editor, row rebinding, search results, dialogs, preview rendering, and announcement-sensitive state to settle.
- [x] 8.2 Add bounded automation snapshot fields needed for accessibility smoke, avoiding unbounded document contents, note bodies, complete search results, and private persistence identifiers.
- [x] 8.3 Extend `scripts/run-accessibility-smoke.sh` into a scenario matrix with per-scenario manifests, focus artifacts, AT-SPI tree excerpts, warning scans, environment reports, and clear skip reasons.
- [x] 8.4 Add focused accessibility smoke filters or subcommands so large coverage can be debugged surface by surface while still rolling up into `make accessibility-smoke`.
- [x] 8.5 Extend `.agents/skills/gtk-agentic-debugging/scripts/*` only as needed for bounded AT-SPI text/caret/focus/state proof and keep helper flags documented.
- [x] 8.6 Update `scripts/lushtext-automation.py artifact-summary` to summarize accessibility scenario statuses, assertions, manifests, warnings, and unsupported-host reasons.
- [x] 8.7 Update `docs/automation.md`, `docs/automation-reference.md`, and `docs/end-user-coverage.md`; run `make check-automation-docs`.

## 9. Visual Accessibility And Geometry Proof

- [x] 9.1 Add visual smoke scenarios for keyboard focus indication across shell, editor, rows, dialogs, popovers, bottom sheets, context menus, search surfaces, and constrained layouts.
- [x] 9.2 Add high-contrast, dark, large-text, reduced-motion, and transparency/readability variants where host support exists, with explicit unsupported-host reporting.
- [x] 9.3 Add visual smoke scenarios proving color-not-only communication for alerts, destructive states, modified tabs, search matches, selections, disabled actions, bookmarks, file-health states, and local-history restore state.
- [x] 9.4 Add visual geometry manifests for accessibility-sensitive regions: focus rings, close/back buttons, primary actions, visible labels, item-region scroll bounds, and persistent chrome.
- [x] 9.5 Extend visual proof policy so accessibility-sensitive UI/CSS/template/tooling changes require current visual accessibility evidence unless narrowly exempted as decorative-only.
- [x] 9.6 Update visual smoke and geometry docs, manifests, artifact summaries, and policy self-tests.
- [x] 9.7 Run `make visual-smoke`, `make visual-geometry-smoke`, and `make check-visual-proof-policy` or record explicit unsupported-host skips.

## 10. Documentation, Release Guidance, And Final Verification

- [x] 10.1 Add or expand a user-facing accessibility guide covering keyboard operation, screen-reader expectations, visual accessibility features, smoke-test coverage, known platform caveats, and bug-report guidance.
- [x] 10.2 Update developer docs and AGENTS/rules so future UI changes know the required accessibility metadata, keyboard parity, smoke artifacts, docs, and policy checks.
- [x] 10.3 Update release/end-user validation guidance so accessibility smoke, visual accessibility evidence, and manual screen-reader checks are part of release readiness.
- [x] 10.4 Run `openspec validate state-of-the-art-gtk-accessibility --strict` and fix all spec issues.
- [x] 10.5 Run `git diff --check`.
- [x] 10.6 Run focused unit/widget checks for helper, surface, keyboard, row-recycling, and announcement coverage.
- [x] 10.7 Run `make check-automation-docs`, `make accessibility-smoke`, and available visual/automation smoke gates; preserve or document artifacts.
- [x] 10.8 Run the repo's broader pre-commit/check lane required for the final implementation scope.
- [x] 10.9 Review the completed surface inventory against the specs and confirm every requirement has a direct implementation and verification artifact.
