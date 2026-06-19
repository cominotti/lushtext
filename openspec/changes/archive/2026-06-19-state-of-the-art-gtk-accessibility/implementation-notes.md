# Implementation Notes

## Baseline Captured 2026-06-18

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext`:

| Command | Result | Evidence |
| --- | --- | --- |
| `make check-automation-docs` | PASS | Automation docs drift check reported current docs and a passing self-test. |
| `make accessibility-smoke` | PASS | `build/smoke/accessibility/summary.txt` reports `status=passed`. |
| `./scripts/run-widget-tests.sh --headless -- accessib` | PASS | 14 focused widget tests passed, covering existing editor, shell, status, sidebar selector, preferences, search bar, search panel, and open-popover accessibility role checks. |
| `.agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py ... --wait-atspi-text accessibility-smoke.txt` | PASS | `build/smoke/accessibility/editor-spike.png` and `build/smoke/accessibility/assertions/editor-spike-atspi-tree.txt` captured an editor-open session. |
| `timeout 25s .agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py ... --wait-atspi-text "needle one"` | EXPECTED FAIL | Negative-control spike timed out waiting for buffer text, proving the current bounded AT-SPI helper sees shell/editor chrome but not fixture buffer content. |

Primary smoke artifacts:

- `build/smoke/accessibility/summary.txt`
- `build/smoke/accessibility/environment.txt`
- `build/smoke/accessibility/warnings.txt`
- `build/smoke/accessibility/accessibility-anchors.txt`
- `build/smoke/accessibility/assertions/accessibility-focus.txt`
- `build/smoke/accessibility/assertions/shell-atspi-tree.txt`
- `build/smoke/accessibility/assertions/command-palette-atspi-tree.txt`
- `build/smoke/accessibility/assertions/notes-empty-atspi-tree.txt`
- `build/smoke/accessibility/captures/`

Editor spike artifacts:

- `build/smoke/accessibility/editor-spike.png`
- `build/smoke/accessibility/assertions/editor-spike-atspi-tree.txt`
- `build/smoke/accessibility/assertions/editor-spike-atspi-focus.txt`
- `build/smoke/accessibility/captures/editor-spike/`
- `build/smoke/accessibility/captures/editor-text-negative/`

Current smoke anchors:

- Window shell: `Open document tabs`, `Toggle workspace sidebar`, `Document metadata`, `New file`, `Open recent documents`, `Notes menu`, `Main menu`, `Toggle document properties`.
- Workspace sidebar: `New Workspace`.
- Command palette: `Command palette query`, `Command palette results`, `Files`.
- Notes browser empty state: `Notes`, `No notes yet`, `Close`.

Current warning baseline:

- `build/smoke/accessibility/warnings.txt` currently contains only three `Gdk-Message: Error reading events from display: Broken pipe` shutdown messages from the headless capture lifecycle.

Known gaps from the baseline:

- AT-SPI smoke coverage is still narrow. It covers shell, command palette, and the empty Notes browser, but not the editor text surface, Open popover state extremes, search surfaces, file tree rows, workspace search, properties, preferences, Markdown preview, local history, destructive dialogs, context menus, or compact layout variants.
- Command-palette focus proof currently passes through a visible fallback: `focused_name=<unreported>` with `fallback_visible_name=Command palette query`.
- The bounded AT-SPI tree for an open editor exposes file identity labels and tab chrome, but no obvious text/editable role, buffer text, caret, selection, or focused accessible node.
- Widget tests prove existing roles on a few surfaces, but they do not yet prove descriptions, relations, dynamic states, announcement throttling, row-recycling cleanup, keyboard parity, or AT-SPI-visible behavior.
- Visual accessibility coverage for focus rings, high contrast, large text, reduced motion, color-not-only communication, opacity/readability, and constrained geometry is not yet part of the accessibility baseline.
- There is no release-level manual screen-reader runbook yet.

## GtkSourceView / AT-SPI Spike

The editor-open spike used `build/smoke/accessibility/fixtures/accessibility-smoke.txt`, which contains the visible text `needle one`, `needle two`, and `needle three`.

Automatable today:

- Open a saved file in a private headless Mutter session.
- Enable a private AT-SPI registry.
- Capture a screenshot, bounded AT-SPI tree, focus output, session logs, warning log, and environment metadata.
- Prove surrounding shell/editor chrome, active file identity labels, tab identity, workspace selector, and status controls.
- Use a negative-control wait to prove buffer text is not currently found by the bounded AT-SPI helper.

Not automatable yet with the current helper and app metadata:

- Direct editor buffer text exposure.
- Caret/insertion location.
- Text selection range or selected text.
- Focused editor accessible node.
- Read-only state transitions for loading, saving, preview-only, failed-load, and large-file policy.

Implementation implication:

- The editor work must add LushText-owned accessible metadata around the GtkSourceView region, then investigate whether GTK/GtkSourceView exposes text, caret, selection, and readonly state through AT-SPI once the region is named and focusable.
- If the platform still does not expose enough text/caret/selection detail, the final release evidence must keep automated shell/editor-region proof and add a documented manual screen-reader fallback for the missing details.

## Shell, Editor, And Status Evidence After Implementation Slice

## Search, Open Popover, Command Palette, And Workspace Tree Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the search/open/palette/sidebar slice:

| Command | Result | Evidence |
| --- | --- | --- |
| `make accessibility-smoke` | PASS | `build/smoke/accessibility/summary.txt` reports `status=passed` after expanding the AT-SPI matrix. |
| `make check-automation-docs` | PASS | Automation reference contains every stable AT-SPI anchor asserted by `scripts/run-accessibility-smoke.sh`. |
| `make check-accessibility-policy` | PASS | Policy self-test passed and checked the current UI-sensitive diff. |
| `cargo fmt --all -- --check` | PASS | Rust formatting is current. |
| `./scripts/run-widget-tests.sh --headless -- search_bar` | PASS |  Search bar accessibility metadata, state, and announcement tests passed. |
| `./scripts/run-widget-tests.sh --headless -- search_panel` | PASS |  Workspace search metadata, result-row, replace/undo state, and runtime accessibility tests passed. |
| `./scripts/run-widget-tests.sh --headless -- open_popover` | PASS |  Open popover empty, row, filtering, no-match, and keyboard flow tests passed. |
| `./scripts/run-widget-tests.sh --headless -- command_palette` | PASS |  Command palette controls, row metadata, busy state, no-results, mode, and focus tests passed. |
| `./scripts/run-widget-tests.sh --headless -- workspace_section` | PASS |  Workspace-section header, file-tree row metadata, row recycling, DnD, refresh, watch, and tree workflow tests passed. |
| `./scripts/run-widget-tests.sh --headless -- test_target_state_actions_drive_visible_surfaces_without_toggle_parity` | PASS |  Target-state automation actions drive Open popover and workspace search query setters without toggle parity issues. |
| `./scripts/run-widget-tests.sh --headless -- test_live_app_and_window_actions_match_action_catalog` | PASS |  Exported action catalog matches the live app/window action surface. |

Expanded AT-SPI smoke scenarios now cover:

- Shell controls plus in-tab search entry/navigation/close controls.
- Main editor text exposure, focus fallback, bounded text summary, caret metadata, and selection metadata.
- Workspace search representative results and no-results state through seeded workspace data.
- Workspace sidebar representative section header, Add Folder, Refresh, file tree, and top-level folder row.
- Open popover empty, dense constrained, filtered constrained, and filtered no-match states through seeded recent-document data.
- Command palette files mode and commands-mode no-results state.
- Notes browser empty state.

Notable implementation details:

- The Open popover gained `win.set-open-popover-query`, a contextual exported action enabled only while the popover is visible. The action is registered in the action catalog and documented in automation docs so accessibility smoke can drive filtered states without private widget mutation.
- The workspace search panel gained `win.set-search-panel-query`, a contextual exported action enabled only while the workspace search panel is visible.
- `scripts/run-accessibility-smoke.sh` now seeds private per-capture `workspaces.json` and `recent-documents.json` files under each capture's isolated data directory, avoiding user data while exercising real app persistence readers.
- Workspace file-tree rows now project helper-backed row metadata on bind and clear it on unbind, including role, label, description, selected state, expanded state, disabled placeholder state, and set-position relations.

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext`:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting check passed after the helper, status, and editor edits. |
| `make check-accessibility-policy` | PASS | Policy self-test passed and checked the current UI-sensitive diff. |
| `./scripts/run-widget-tests.sh --headless -- status_bar` | PASS | 43 focused status-bar and related window tests passed. |
| `./scripts/run-widget-tests.sh --headless -- editor_page` | PASS | 81 focused editor-page tests passed. |
| `./scripts/run-widget-tests.sh --headless -- accessibility` | PASS | 21 helper and accessibility-focused widget tests passed. |
| `./scripts/run-widget-tests.sh --headless -- open_popover` | PASS | 36 Open popover tests passed, including active-editor focus restoration after dismissal. |
| `./scripts/run-widget-tests.sh --headless -- keyboard_search_workflow_navigates_closes_and_restores_editor_focus` | PASS | Focus returns to the editor after in-tab search navigation and close. |
| `./scripts/run-widget-tests.sh --headless -- keyboard_command_palette_and_secondary_surfaces_restore_editor_focus` | PASS | Command palette and secondary shell surfaces restore editor focus. |
| `./scripts/run-widget-tests.sh --headless -- closing_properties` | PASS | 3 properties-pane and bottom-sheet focus restoration tests passed. |
| `./scripts/run-widget-tests.sh --headless -- new_document_exits_markdown_preview_only_mode` | PASS | Preview-only mode clears on New Document and focus returns to the active editor. |
| `make accessibility-smoke` | PASS | `build/smoke/accessibility/summary.txt` reports `status=passed` with the new editor scenario included. |

New app-level coverage:

- Shell/header/tab controls now expose helper-backed names, descriptions, popup states, toggle state, and shortcut metadata for the main header controls, tab list, tab view, focus-mode affordance, and menu buttons.
- Status bar metadata controls expose stable names/descriptions and current value text for line endings and encoding. Visible warning/error status messages are routed through the shared announcement throttler, while routine informational status text remains visually present without noisy screen-reader announcements.
- The main `GtkSourceView` now exposes role `TextBox` at the GTK widget-test level and role `text` through AT-SPI, named as `Editor for accessibility-smoke.txt` in smoke artifacts.
- Editor metadata tracks loading, saving, failed-load, large-file, eviction, read-only, and preview-only states through helper-backed accessible properties and states.
- The AT-SPI dump helper now uses the same depth envelope as the AT-SPI editing helper and records bounded text-interface details (`text_chars`, `caret`, `selections`, and a capped `text_sample`) when the platform exposes them.

Current editor AT-SPI proof:

- `build/smoke/accessibility/assertions/editor-atspi-tree.txt` contains `role='text' name='Editor for accessibility-smoke.txt'`.
- `build/smoke/accessibility/assertions/accessibility-text.txt` records `text_chars=215` for the editor, proving bounded text exposure for the fixture document.
- The same tree line includes caret and selection metadata (`caret=0`, `selections=0`) for the active editor.
- `build/smoke/accessibility/assertions/accessibility-focus.txt` still records editor focus through a visible fallback because the headless bridge does not report a focused accessible node for the editor in this scenario.

## Release-Level Screen-Reader Reference Decision

Use both proof paths:

- Headless AT-SPI smoke is the repeatable release artifact path. It should run for every release candidate on a supported host and preserve scenario manifests, AT-SPI tree excerpts, focus artifacts, warning scans, screenshots, environment metadata, and unsupported-host skip reasons.
- Orca in a normal GNOME session is the manual reference path for release candidates that touch UI or accessibility, and it is required for editor text/caret/selection/read-only claims until automated AT-SPI evidence proves those details directly.

If Orca or a normal GNOME session is unavailable, the release notes must record an explicit unsupported-host skip. A headless AT-SPI pass alone is not enough to claim editor text/caret/selection screen-reader behavior until that behavior is directly observed in artifacts.

## Properties, Preferences, Markdown Preview, And App-Action Smoke Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the properties/preferences/preview slice:

| Command | Result | Evidence |
| --- | --- | --- |
| `./scripts/run-widget-tests.sh --headless -- properties_panel` | PASS | Document-properties panel role, labels, descriptions, value text, and file-health button hidden/disabled states passed. |
| `./scripts/run-widget-tests.sh --headless -- preferences` | PASS | Preferences dialog rows, grouped spin controls, opacity controls, data-page status/details, and dynamic hidden/disabled/busy states passed. |
| `./scripts/run-widget-tests.sh --headless -- markdown_preview` | PASS | Markdown preview document region, placeholder, rendered text, code blocks, tables, images, and fallback metadata passed. |
| `make check-automation-docs` | PASS | Automation docs include the new `--app-action` helper flag and stable AT-SPI anchors. |
| `make accessibility-smoke` | PASS | The smoke matrix now includes preferences, document properties, and Markdown preview scenarios. |

New proofable coverage:

- The properties panel exposes a named grouping, named/value-backed metadata rows, file-health status details, and a disabled/hidden review action when no review content exists.
- Preferences expose row-level and grouped-control metadata for editor, workspace, and data-format settings, including current value text for opacity and data-format state.
- Markdown preview exposes read-only document/text semantics, placeholder status metadata, code-block read-only text boxes, table structure, and image/fallback descriptions.
- The Mutter capture helper can now activate application actions before window actions through documented `--app-action` flags, which lets the smoke runner open Preferences without private widget mutation.

Remaining gap:

- This slice does not finish notes/bookmarks AT-SPI scenarios, local history, announcement policy expansion, high-contrast/large-text visual accessibility variants, or the full per-scenario manifest work described by the remaining OpenSpec tasks.

## Notes And Bookmark Accessibility Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the notes/bookmarks metadata slice:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all --check` | PASS | Formatting is current after notes/bookmark accessibility edits. |
| `cargo check -p lushtext-core` | PASS | Core UI bookmark and notes changes compile. |
| `scripts/run-widget-tests.sh --headless -- test_notes_browser_controls_expose_accessibility_roles test_document_note_dialog_supports_edit_and_render_modes test_notes_browser_renders_raw_bookmark_excerpt_with_target_marker test_notes_browser_uses_sectioned_adw_sidebar_and_filters_note_body test_empty_notes_browser_close_button_and_escape_dismiss` | PASS | Notes browser controls, edit/render mode, raw bookmark preview, no-match status, and empty state metadata passed. |
| `scripts/run-widget-tests.sh --headless -- test_notes_browser_caps_large_result_sets_with_refine_notice` | PASS | Dense notes result-limit notice exposes status metadata. |
| `scripts/run-widget-tests.sh --headless -- test_bookmark_gutter_edit_dialog_validates_moves_and_persists test_bookmark_commands_report_saved_file_and_empty_bookmark_context` | PASS | Bookmark edit dialog metadata, validation invalid states, persistence, disabled no-tab actions, untitled-file guard, edit-no-bookmark guard, and next/previous empty-bookmark feedback passed. |
| `scripts/run-widget-tests.sh --headless -- test_bookmark_toggle_and_navigation test_bookmark_edit_moves_existing_id_across_lines test_bookmark_edit_rejects_invalid_lines_without_mutating test_bookmark_activation_callback_only_fires_for_bookmark_lines` | PASS | Editor-page bookmark command semantics and live gutter activation still pass after tooltip changes. |

New proofable coverage:

- The unified Notes browser exposes named search, list, preview, open/back/close controls, read-only raw bookmark previews, edit/render note body surfaces, no-match status, empty status, and dense-result status metadata.
- Bookmark gutter tooltips now identify the target as a bookmark and include the current one-based line, so labeled gutter icons remain meaningful outside the visual icon.
- The bookmark edit dialog exposes a named field group, named/described label and line rows, named/described close/cancel/save controls, and status/invalid metadata for validation feedback.
- Bookmark commands keep no-tab actions disabled, report the saved-file requirement for untitled editors, report missing cursor bookmark context for Edit Bookmark, and report empty bookmark state for next/previous navigation.

## Local History Accessibility Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the local-history metadata slice:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all` | PASS | Formatting was applied after local-history edits. |
| `cargo check -p lushtext-core` | PASS | Core local-history accessibility changes compile. |
| `scripts/run-widget-tests.sh --headless -- test_local_history_browser_controls_expose_accessibility_roles test_local_history_dialog_shows_empty_state_without_snapshots test_local_history_browser_explains_empty_snapshot_and_disables_copy test_local_history_browser_warns_and_shows_repaired_snapshot test_local_history_browser_hides_legacy_empty_baseline_noise test_local_history_dialog_scales_from_parent_and_keeps_preview_dominant test_local_history_browser_collapses_and_restore_can_be_undone` | PASS | Local-history list, preview, empty state, valid empty snapshot, recovery warning, legacy-noise filtering, constrained viewer geometry, and adaptive restore workflows passed. |

New proofable coverage:

- The local-history sidebar exposes list role, label, and description metadata for snapshot selection.
- The preview stack exposes a named read-only preview region with value text, busy state while loading, invalid state for missing/error previews, and read-only/multiline text metadata for loaded snapshot text.
- Empty local-history and valid empty-snapshot states expose semantic status-page metadata, and valid empty snapshots keep Restore enabled while Copy exposes disabled state.
- Restore/Copy/Back controls expose label and description metadata, with GTK sensitivity and explicit accessible disabled state kept in sync during loading and restore preparation.

## Validation After Final Accessibility Slice

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the final preferences cleanup:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting is current. |
| `openspec validate state-of-the-art-gtk-accessibility --strict` | PASS | OpenSpec change artifacts are valid. |
| `git diff --check` | PASS | No whitespace errors in the current diff. |
| `make check-accessibility-policy` | PASS | Accessibility policy self-test passed and checked the current UI-sensitive diff. |
| `make check-automation-docs` | PASS | Automation documentation drift check passed. |
| `./scripts/run-widget-tests.sh --headless -- preferences` | PASS | Preferences accessibility and data-page state tests passed after the Clippy cleanup. |
| `make accessibility-smoke` | PASS | AT-SPI anchors and focus artifacts passed after the final cleanup. |
| `make visual-geometry-smoke` | PASS | Refreshed `build/smoke/visual-geometry/summary.json` for the current visual-sensitive diff. |
| `make pre-commit` | PASS | Formatting, Clippy, filesystem boundary, Blueprint, docs, Flatpak, end-user smoke workflow, accessibility policy, visual proof policy, GTK Lush policy/adoption, and automation CLI self-test all passed. |
| `make check-agent-docs` | PASS | Agent guidance and filesystem-boundary documentation checks passed after updating accessibility rules. |

One `make pre-commit` run failed before the final pass because Clippy requested replacing an unnecessary `Option::map_or_else` in preferences with `unwrap_or_else`; that cleanup is included in the final diff.

## Accessibility Smoke Manifest And Artifact Summary Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after adding accessibility smoke manifests and summary support:

| Command | Result | Evidence |
| --- | --- | --- |
| `bash -n scripts/run-accessibility-smoke.sh` | PASS | Shell syntax for the manifest/summary helpers is valid. |
| `python3 -m py_compile scripts/lushtext-automation.py` | PASS | Automation client changes compile. |
| `./scripts/lushtext-automation.py self-test` | PASS | Artifact-summary parser self-test still passes. |
| `make check-automation-docs` | PASS | Automation docs remain synchronized after documenting accessibility artifacts. |
| `make accessibility-smoke` | PASS | Live AT-SPI lane passed and generated JSON evidence. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json` | PASS | Reported `status=passed`, 15 per-scenario manifests, 15 scenario rows, warning status `allowlisted`, and 0 unexpected warnings. |
| `make visual-geometry-smoke` | PASS | Refreshed `build/smoke/visual-geometry/summary.json` after automation-client artifact-summary changes affected the visual-sensitive diff fingerprint. |
| `make pre-commit` | PASS | The full pre-commit lane passed after the visual proof refresh. |

New proofable artifacts:

- `build/smoke/accessibility/summary.json` records schema version, lane, status, scenario manifest count, screenshot paths, warning status, assertion artifact paths, and unsupported-host capability fields.
- `build/smoke/accessibility/assertions/*-manifest.json` now records each accessibility scenario's fixture, capture arguments, normal UI actions, waits, screenshot, AT-SPI tree/focus artifacts, warning scan, and session log.
- `build/smoke/accessibility/assertions/accessibility-assertions.jsonl` records passed anchor, focus, focus-fallback, and text-interface assertions as bounded JSON rows.
- Known headless `Gdk-Message: Error reading events from display: Broken pipe` shutdown messages are classified as allowlisted; any other accessibility warning now fails the lane and writes `assertions/unexpected-warnings.txt`.

One `make pre-commit` run failed before the final pass because visual proof policy detected that `scripts/lushtext-automation.py` changed the visual-sensitive diff fingerprint; refreshing `make visual-geometry-smoke` resolved it.

## Focused Accessibility Smoke Filter Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after adding focused accessibility smoke filters:

| Command | Result | Evidence |
| --- | --- | --- |
| `bash -n scripts/run-accessibility-smoke.sh` | PASS | Filter parser and scenario wrappers are valid shell syntax. |
| `python3 -m py_compile scripts/lushtext-automation.py scripts/check-automation-docs.py` | PASS | Automation client and docs checker compile. |
| `scripts/run-accessibility-smoke.sh --list-cases` | PASS | Printed 15 known accessibility smoke scenario names. |
| `make check-automation-docs` | PASS | Helper flag marker and table rows include `--case` and `--list-cases`. |
| `scripts/run-accessibility-smoke.sh --artifact-dir build/smoke/accessibility-filter --case command-palette-no-results` | PASS | Filtered live AT-SPI run produced exactly one manifest and three assertion rows. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility-filter --json` | PASS | Reported `case_filters=command-palette-no-results`, one manifest, and one scenario row. |
| `make accessibility-smoke` | PASS | Full roll-up still produced `case_filters=all`, 15 manifests, 75 assertion rows, and 0 unexpected warnings. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json` | PASS | Reported `case_filters=all`, 15 manifests, 15 scenario rows, and 0 unexpected warnings. |
| `make visual-geometry-smoke` | PASS | Refreshed `build/smoke/visual-geometry/summary.json` after automation-client field changes affected the visual-sensitive diff fingerprint. |
| `make pre-commit` | PASS | The full pre-commit lane passed after the visual proof refresh. |

New proofable behavior:

- `scripts/run-accessibility-smoke.sh --list-cases` lists stable scenario names without launching LushText.
- `scripts/run-accessibility-smoke.sh --case PATTERN` runs matching scenarios only; shell-style globs are accepted and the flag can be repeated.
- Filtered summaries carry `case_filters` so artifact reviewers can tell whether a run is full coverage (`all`) or a targeted debug subset.
- A filter that matches no scenarios writes a failed summary and exits nonzero instead of silently passing with an empty artifact root.

Remaining gaps:

- Automation readiness/snapshot fields are not yet complete for every accessibility-sensitive transition.
- The scenario matrix is broader and reviewable, but it does not yet satisfy every state extreme for destructive dialogs and visual accessibility variants.

## Notes, Local History, And Preview State-Extreme AT-SPI Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 6.7:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo build -p lushtext` | PASS | Rebuilt the debug binary used by live AT-SPI smoke captures. |
| `scripts/run-accessibility-smoke.sh --artifact-dir build/smoke/accessibility-6-7 --case 'notes-*' --case 'local-history*'` | PASS | Six focused live AT-SPI manifests passed: `notes-empty`, `notes-populated`, `notes-no-results`, `local-history-empty`, `local-history`, and `local-history-empty-snapshot`. |
| `bash -n scripts/run-accessibility-smoke.sh` | PASS | Scenario-matrix shell syntax is valid. |
| `scripts/run-accessibility-smoke.sh --list-cases` | PASS | Printed 33 known accessibility smoke scenario names after adding notes/local-history state extremes. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility-6-7 --json` | PASS | Reported `status=passed`, 6 manifests, 31 accessibility assertion rows, warning status `allowlisted`, and 0 unexpected warnings. |
| `python3 -m py_compile scripts/check-automation-docs.py scripts/lushtext-automation.py` | PASS | Automation docs checker and client compile. |
| `make check-automation-docs` | PASS | Stable AT-SPI anchor documentation matches the smoke script after adding notes and local-history anchors. |
| `make accessibility-smoke` | PASS | Full accessibility roll-up passed after matrix expansion. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json` | PASS | Reported `case_filters=all`, 33 manifests, 159 accessibility assertion rows, warning status `allowlisted`, and 0 unexpected warnings. |

New proofable coverage:

- Notes browser AT-SPI coverage now includes empty, populated bookmark, and filtered no-results states with stable dialog, search entry, results list, status, close/open, result-row, preview, and raw bookmark text-interface anchors.
- Local-history AT-SPI coverage now includes no-history, populated snapshot, and valid empty-snapshot states with stable dialog, snapshot list, preview, read-only text-interface, status, Copy, and Restore anchors.
- The smoke fixture now seeds local-history sidecars through the same v1 JSON envelope and stable FNV path/content hashes used by runtime persistence.
- Properties, preferences, and Markdown preview state extremes already had widget and AT-SPI anchors in the existing task 6.4 through 6.6 proof; task 6.7 closes the missing notes/bookmark and local-history AT-SPI matrix gaps around those already-normalized surfaces.

## Announcement Policy Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 7.1:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all` | PASS | Formatting was applied after routing inline-alert and destructive-confirmation announcements. |
| `cargo check -p lushtext-core` | PASS | Core UI announcement helpers and dialog/inline-alert call sites compile. |
| `scripts/run-widget-tests.sh --headless -- test_inline_alert_announcements_use_shared_throttling_policy test_save_changes_dialog_controls_expose_accessibility_roles test_window_close_request_cancel_keeps_modified_file_tab test_keyboard_save_changes_cancel_preserves_modified_tab test_bookmark_gutter_edit_dialog_validates_moves_and_persists` | PASS | Inline-alert throttling, save/close dialog accessibility roles, close-cancel preservation, keyboard cancel behavior, and invalid bookmark validation passed. |

New proofable coverage:

- `AnnouncementThrottler` now emits through the shared lane-to-priority helper, so all throttled announcements use the same bounded text and priority policy.
- Editor inline alerts use the shared policy: warning alerts are repeated-status announcements, while failed-load/error alerts use the high-priority alert lane and bypass throttling.
- Save/discard close dialogs, sidebar delete/remove confirmations, and workspace removal confirmations announce high-priority destructive context before presenting their dialogs without changing the underlying save/close decision flow.
- Bookmark edit validation errors now announce through the high-priority alert lane in addition to exposing invalid field/status metadata.

## Workflow Announcement Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 7.2:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting remained current after workflow announcement and list-factory changes. |
| `cargo check -p lushtext-core` | PASS | Core announcement, search progress, Preferences, and list-factory changes compile. |
| `scripts/run-widget-tests.sh --headless -- test_workflow_announcements_use_status_bar_throttling_policy test_data_page_reports_current_format_hides_actions_and_shows_verified_current test_connect_search_progress_callback_stored test_search_panel_accessibility_tracks_replace_preview_and_undo_state test_enter_preview_mode_sets_flag test_enter_preview_mode_uses_cached_search_matches_without_gtk_rows test_exit_preview_mode_clears_state` | PASS | Workflow announcement throttling, Data-page scan announcements, search progress enum coverage, and Replace preview state passed. |

New proofable coverage:

- The status bar now exposes an explicit workflow-announcement API for informational completions and milestones, keeping routine info status messages visually quiet while allowing save/load, search, refresh, and replace outcomes to be spoken through the shared throttler.
- User-initiated document save and non-session document load completions announce bounded status updates.
- Workspace search emits start, cancellation, and completion updates through the existing debounced progress callback; cancellation is only spoken once the progress surface was visible, so rapid typing does not create announcement floods.
- Replace All completion, undo availability, and successful undo completion announce through status-update lanes at the same boundaries that update visible status and durable undo state.
- Workspace file indexing announces a bounded progress milestone after a fresh index is accepted, and manual workspace refresh announces start/completion while automatic watcher refreshes remain quiet unless they surface an error.
- Preferences > Data announces app-data format scan/apply outcomes from the compact status row, using alert priority for failed updates and status-update priority for current/convertible outcomes.
- The search-results factory now creates the Replace preview checkbox in `setup` and only updates state during `bind`, avoiding widget/signal churn on recycled rows; search accessibility refreshes are also coalesced at the poll-batch level instead of per appended match.

## Mode And Surface State Announcement Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 7.3:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting remained current after mode/surface state updates. |
| `cargo check -p lushtext-core` | PASS | Core mode-change announcement and accessible pressed-state changes compile. |
| `scripts/run-widget-tests.sh --headless -- test_secondary_surface_toggles_sync_accessible_pressed_and_announcements test_mode_toggles_record_state_specific_workflow_announcements test_toggle_minimap_updates_setting_and_action_state test_toggle_minimap_action_state_tracks_external_setting_changes test_preview_mode_toggle_uses_full_content_layout test_preview_pane_toggle_uses_adwaita_side_by_side_shell test_preview_target_actions_keep_adwaita_shell_modes_mutually_exclusive` | PASS | Workspace sidebar, document properties, focus mode, preview pane, preview-only mode, and minimap transition coverage passed. |

New proofable coverage:

- Workspace sidebar and document-properties visibility changes now update both action state and explicit accessible pressed state immediately when the user requests the surface, then resync after adaptive layout chooses the rendered surface.
- Focus Mode, preview pane, preview-only mode, and minimap toggles announce state-specific status updates through the shared workflow announcement policy.
- Preview-only mode turning on from an open side-by-side preview records the preview-pane-hidden transition before announcing preview-mode-on, so assistive technology receives the same state sequence the visual shell applies.

## Announcement Flood Guard Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 7.4:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting remained current after announcement-throttler test seam and flood-guard tests. |
| `cargo check -p lushtext-core` | PASS | Core announcement helper changes compile without test-only hooks leaking into normal builds. |
| `scripts/run-widget-tests.sh --headless -- test_announcement_throttler_suppresses_repeated_status_but_not_alerts test_announcement_throttler_suppresses_typing_progress_and_status_floods test_workflow_announcements_use_status_bar_throttling_policy test_visible_status_rendering_does_not_announce_info_or_flood_repeated_warnings test_generic_progress_heartbeat_and_resolve_renders_do_not_pulse test_visible_search_progress_update_pulses_message_area test_hidden_search_progress_update_does_not_pulse_over_transient` | PASS | Debounced results, progress milestones, repeated status warnings, info status text, and progress heartbeat paths passed. |

New proofable coverage:

- `AnnouncementThrottler` now has a test-only non-mutating probe so tests can verify silent paths without asking the throttler to accept a new event.
- Rapid same-key result announcements, such as in-editor typing updates, stay inside the debounced-results cooldown instead of announcing every intermediate count.
- Progress milestones stay throttled inside their milestone cooldown, while routine progress heartbeat renders do not record status-update announcements.
- Routine visible info status text stays silent for accessibility announcements, and repeated visible warning text is accepted once then suppressed inside the status-update cooldown.

## Announcement Verification Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after closing task 7.5:

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo fmt --all -- --check` | PASS | Formatting remained current after documenting the announcement verification boundary. |
| `cargo check -p lushtext-core` | PASS | Core accessibility helper and announcement code compile. |
| `scripts/run-widget-tests.sh --headless -- test_workflow_announcements_use_status_bar_throttling_policy test_visible_status_rendering_does_not_announce_info_or_flood_repeated_warnings test_secondary_surface_toggles_sync_accessible_pressed_and_announcements test_mode_toggles_record_state_specific_workflow_announcements test_inline_alert_announcements_use_shared_throttling_policy test_announcement_throttler_suppresses_typing_progress_and_status_floods` | PASS | Widget-level announcement hooks passed for workflow, surface, inline-alert, and flood-guard paths. |
| `cargo build -p lushtext` | PASS | Rebuilt the debug binary used by live AT-SPI smoke. |
| `scripts/run-accessibility-smoke.sh --artifact-dir build/smoke/accessibility-7-5 --case shell --case workspace-search-replace-undo --case command-palette-no-results` | PASS | Focused AT-SPI smoke passed for shell/state anchors, command-palette no-result state, and workspace search replace/undo state. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility-7-5 --json` | PASS | Reported `status=passed`, 3 manifests, 20 accessibility assertion rows, warning status `allowlisted`, and 0 unexpected warnings. |

New proofable coverage:

- Automated announcement verification now uses widget-level hooks for GTK announcement emission, priorities, throttling, and negative no-flood behavior.
- Live AT-SPI smoke verifies the observable tree/focus/text/state artifacts for representative announcement-sensitive surfaces: shell controls, command-palette no-results, and workspace search replace/undo.
- `docs/accessibility.md` now states the platform boundary clearly: the headless AT-SPI helper does not capture Orca speech output, so changed announcement behavior still needs a manual Orca check in a normal GNOME session before release claims.

## Visual Accessibility Proof Evidence

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the visual accessibility proof slice and final task 9.3 color-not-only closeout:

| Command | Result | Evidence |
| --- | --- | --- |
| `bash -n scripts/run-visual-smoke.sh` | PASS | Visual smoke case filtering and summary generation are valid shell syntax. |
| `python3 -m py_compile .agents/skills/gtk-agentic-debugging/scripts/capture-lushtext-mutter.py` | PASS | The Mutter capture helper's visual accessibility flags and ordered AT-SPI editor-text step compile. |
| `scripts/run-visual-smoke.sh --list-cases` | PASS | The runner lists 37 visual smoke cases, including high contrast, large text, reduced motion, transparency/readability, modified-tab, destructive-close-dialog, file-health-properties, and local-history-restore variants. |
| `scripts/run-visual-smoke.sh --artifact-dir build/smoke/visual-9-variants --case high-contrast-style --case large-text-constrained --case reduced-motion-command-palette --case transparency-readability` | PASS | Four focused accessibility-variant screenshots and manifests passed. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/visual-9-variants --json` | PASS | Reported `status=passed`, 4 manifests, 4 screenshots, 0 unexpected warnings, and variant coverage for high contrast, large text, reduced motion, and transparency/readability. |
| `scripts/run-visual-smoke.sh --artifact-dir build/smoke/visual-9-3-focused --case modified-tab --case destructive-close-dialog --case file-health-properties --case local-history-restore` | PASS | Four focused color-not-only screenshots and manifests passed for the task 9.3 edge states. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/visual-9-3-focused --json` | PASS | Reported `status=passed`, 4 manifests, 4 screenshots, 0 unexpected warnings, and color-not-only coverage for modified-tab, destructive-close-dialog, file-health-properties, and local-history-restore. |
| `make visual-smoke` | PASS | Full visual smoke passed with 37 screenshots/manifests and `build/smoke/visual/summary.json`. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/visual --json` | PASS | Reported `status=passed`, 37 manifests, 37 screenshots, 0 unexpected warnings, and populated visual accessibility coverage groups. |
| `make visual-geometry-smoke` | PASS | Rust same-session geometry proof passed and refreshed `build/smoke/visual-geometry/summary.json`. |
| `make check-visual-proof-policy` | PASS | Visual proof policy self-tests passed and the current summary matched the visual-sensitive diff, including required minimap pixel and animation invariants. |
| `cargo fmt --all -- --check` | PASS | Formatting remained current after visual proof changes. |
| `cargo check -p lushtext-core` | PASS | Core UI changes still compile after visual tooling changes. |
| `make check-automation-docs` | PASS | Automation documentation remained synchronized after smoke/docs updates. |

New proofable coverage:

- `scripts/run-visual-smoke.sh` now supports `--list-cases` and repeated `--case PATTERN` filters, matching the accessibility smoke debugging model.
- Visual smoke now captures dark style, high contrast, large text under constrained geometry, reduced-motion command-palette behavior, and document-surface transparency/readability.
- Visual smoke now captures task 9.3's previously missing color-not-only states: a modified file tab with an AT-SPI-driven editor edit, a destructive unsaved-close dialog, mixed-line-ending file-health properties, and local-history restore state with `Undo Restore` visible.
- The capture helper applies real GNOME/LushText isolated settings for high contrast, status shapes, reduced motion, text scale, and tab-content opacity; schema/key/range failures are classified as unsupported host support rather than silent passes.
- Visual smoke writes `summary.json` with scenario sources, screenshots, warning status, and `visual_accessibility_coverage` groups for focus indication, variants, color-not-only cues, constrained geometry, and unsupported variants.
- The full visual summary's `color_not_only_cases` now includes bookmarks, destructive close, file health, high contrast/status shapes, local-history restore, search/minimap, modified tab, recovery startup, and transparency/readability coverage.
- Existing and refreshed visual geometry manifests protect persistent chrome, item-region scroll bounds, minimap/sidebar geometry, open-popover row geometry, command-palette overlays, and animation-sensitive workspace sidebar transitions across light/dark and constrained cases.
- `docs/accessibility.md` and `docs/end-user-coverage.md` document visual smoke filters, variant coverage, artifact summaries, and release validation expectations.

Task 9.3 closeout:

- Alerts: `recovery-startup` captures visible recovery text, iconography, and non-color severity context.
- Destructive states: `destructive-close-dialog` captures unsaved-close text, explicit `Cancel`, `Discard`, and `Save` actions, and role-backed dialog controls.
- Modified tabs: `modified-tab` uses the ordered AT-SPI editor-text step to create a real modified file-backed buffer, then verifies the modified state through the automation snapshot and screenshot.
- Search matches, selections, and disabled/no-action states: `main-search-minimap`, `command-palette-no-results`, and focused companion accessibility/widget tests cover visible result state, selected rows, and disabled/no-result actions.
- Bookmarks: `bookmarks-few` and `bookmarks-dense` capture bookmark visual states and bounded preview context.
- File-health states: `file-health-properties` captures mixed-line-ending health text and the review action inside document properties.
- Local-history restore state: `local-history-restore` seeds real local-history sidecar data, restores through the normal dialog path, and captures the status text plus `Undo Restore` affordance.

## Final Surface Inventory And Requirement Trace Review

Review result: every OpenSpec requirement in this change has a direct implementation surface and at least one verification artifact. The release caveat is intentionally narrow: manual Orca remains required before making speech-output claims that the headless AT-SPI helper cannot observe directly, as documented in `docs/accessibility.md`.

| Requirement family | Implementation surface | Verification artifacts |
| --- | --- | --- |
| App-wide GTK semantics and inventory | `accessibility-surface-inventory.md`, `crates/lushtext-core/src/ui/accessibility.rs`, per-surface accessibility refreshes across editor, shell, status, search, sidebar, properties, preferences, notes, bookmarks, local history, and preview modules. | Widget accessibility suites, `make accessibility-smoke`, `build/smoke/accessibility/summary.json`, and this inventory trace. |
| Interactive controls, custom rows, and transient identity | Helper-backed labels/descriptions/states, row bind/unbind cleanup, focus restoration paths, dialog/popover/menu metadata, and policy guidance in `.agents/rules/ui.md` and `.agents/rules/widget-wiring.md`. | Focused widget tests listed above, `scripts/run-accessibility-smoke.sh` scenario manifests, `make check-accessibility-policy`, and `make check-automation-docs`. |
| Editor, read-only previews, and workflow announcements | LushText-owned GtkSourceView metadata, text-interface AT-SPI helpers, read-only preview metadata, announcement throttling, status workflow announcements, and manual Orca release guidance. | Editor AT-SPI artifacts, widget announcement tests, accessibility smoke manifests, and `docs/accessibility.md` caveats. |
| Keyboard parity and destructive safety | Normal GIO/app/window actions, context-menu and command alternatives, destructive confirmation roles, unsaved-close preservation, Replace All undo, local-history restore undo, and no private safety bypasses. | Keyboard/focus widget tests, destructive-dialog widget tests, `destructive-close-dialog` visual smoke, accessibility smoke action paths, and automation action-catalog self-checks. |
| Accessibility smoke and automation readiness | Expanded Automation1 snapshot/readiness fields, documented helper flags, scenario filters, manifests, warning scans, bounded fixtures, and artifact-summary support. | `make accessibility-smoke`, `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json`, `make check-automation-docs`, and `docs/automation*.md`. |
| Visual accessibility variants and color-not-only communication | `scripts/run-visual-smoke.sh`, Mutter capture variant support, high contrast/status-shape/reduced-motion/text-scale/opacity settings, AT-SPI editor-text step, seeded file-health and local-history visual states. | `make visual-smoke`, `build/smoke/visual/summary.json`, `build/smoke/visual-9-3-focused`, and `./scripts/lushtext-automation.py artifact-summary build/smoke/visual --json`. |
| Visual geometry invariants and proof policy | Same-session visual geometry manifests, protected-region policy, current visual-sensitive diff fingerprinting, and developer guidance for future UI/CSS/template/tooling changes. | `make visual-geometry-smoke`, `build/smoke/visual-geometry/summary.json`, `make check-visual-proof-policy`, and visual proof policy self-tests. |
| Documentation, release guidance, privacy, and data safety | `docs/accessibility.md`, `docs/end-user-coverage.md`, automation docs, bounded fixture data, redacted automation snapshots, and explicit unsupported-host/manual-screen-reader language. | `make check-automation-docs`, accessibility/visual artifact summaries, warning scans, and OpenSpec strict validation. |

## Final Current-Tree Validation

Commands run from `/var/home/danilo/Workspace/github/cominotti/lushtext` after the final generated-UI, Clippy, visual-proof, and OpenSpec task closeout:

| Command | Result | Evidence |
| --- | --- | --- |
| `make accessibility-smoke` | PASS | Current-tree AT-SPI lane passed under `build/smoke/accessibility`. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/accessibility --json` | PASS | Reported `status=passed`, 33 manifests, 33 assertion rows, allowlisted warnings, and 0 unexpected warnings. |
| `make visual-smoke` | PASS | Current-tree visual lane passed with 37 screenshots/manifests under `build/smoke/visual`. |
| `./scripts/lushtext-automation.py artifact-summary build/smoke/visual --json` | PASS | Reported `status=passed`, 37 manifests, 37 screenshots, 0 unexpected warnings, and the complete color-not-only case set: bookmarks, destructive close, file health, high contrast/status shapes, local-history restore, search/minimap, modified tab, recovery, and transparency/readability. |
| `make visual-geometry-smoke` | PASS | Refreshed `build/smoke/visual-geometry/summary.json` for the final visual-sensitive diff; 78 visual geometry cases passed with required minimap pixel and animation invariant ids. |
| `make pre-commit` | PASS | Formatting, Clippy, filesystem boundary, Blueprint drift/contract, automation docs, Flatpak permission, end-user smoke workflow, accessibility policy, visual proof policy, GTK Lush policy/adoption, and automation CLI self-test all passed. |
| `openspec validate state-of-the-art-gtk-accessibility --strict` | PASS | OpenSpec change artifacts are valid after marking tasks 9.3 and 10.9 complete. |
| `git diff --check` | PASS | No whitespace errors in the final diff. |
