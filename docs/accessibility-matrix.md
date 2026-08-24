# Accessibility Completion Matrix

This matrix is the source of truth for deciding whether LushText has complete
GTK accessibility coverage. `docs/accessibility.md` describes the user-facing
contract; this file maps that contract to surfaces, state extremes, proof
lanes, stable anchors, manual Orca expectations, and known gaps.

Rows use these state extreme labels:

- `no-context`: the backing document, workspace, query, or collection is absent.
- `representative`: normal fixture data with one or a few realistic items.
- `dense/awkward`: many rows, long labels, deep paths, mixed item kinds, or
  capped results.
- `constrained/compact`: narrow, short, bottom-sheet, or adaptive layouts.
- `hidden/dismissed`: a transient or secondary surface has closed and must not
  leave stale focus targets or stale metadata behind.
- `busy/loading`: async work, refresh, save, scan, search, or migration in
  progress.
- `error`: failed load/save/search/refresh, invalid query, durability warning,
  or unavailable dependency.
- `recovery`: session/draft/metadata/migration recovery surfaces.
- `destructive`: explicit confirmation for discard, delete, restore, replace,
  migration, or close.

## Reading The Matrix

| Column | Meaning |
| --- | --- |
| Row id | Stable id used by docs, smoke manifests, and manual checklists. |
| Surface and state | Product surface plus the applicable state extreme. |
| Semantics | Role, name, description, state, relation, and privacy expectations. |
| Keyboard path | Keyboard, menu, command-palette, or equivalent accessible path. |
| Announcement | Expected alert, status, or bounded announcement behavior. |
| Visual expectation | Focus, contrast, color-not-only, large text, constrained geometry, and visibility expectations. |
| Automated proof | Existing or required widget, accessibility smoke, visual smoke, or visual-geometry coverage. |
| Stable anchors and Orca expectation | User-facing names or regions Orca should report. |
| Owner and audit status | Owning code area plus current gap/normalization notes. |

`Automated proof` entries prefixed with `existing:` already have a smoke case
or scenario name. Entries prefixed with `needed:` are open gaps that must be
closed before the row can count as release-grade proof.

## Product Matrix

| Row id | Surface and state | Semantics | Keyboard path | Announcement | Visual expectation | Automated proof | Stable anchors and Orca expectation | Owner and audit status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| A11Y-SHELL-NO-CONTEXT | Shell, header, tab strip, status bar; `no-context` | Window, header buttons, sidebar toggle, properties toggle, empty status page, and status metadata expose bounded names without document text. | `Ctrl+N`, `Ctrl+O`, `Ctrl+K`, `Ctrl+Shift+P`, `F9`, `Tab`, menus. | Startup/status messages are bounded and do not pulse endlessly. | Header/status controls remain readable with no tab, no sidebar content, large text, and short height. | existing: accessibility `shell`; needed: focused no-tab assertions. | `LushText`, `Open recent documents`, `Command palette`, `Document properties`; Orca can discover the empty editor area and chrome. | `ui/window`, `ui/status_bar`; matrix owner: UI shell. |
| A11Y-SHELL-REPRESENTATIVE | Shell with open tab, workspace, and metadata; `representative` | Active tab identity, modified state, metadata cluster, and toggles expose names/states/value text. | Tab traversal, `Ctrl+W`, `Ctrl+S`, `F9`, `Ctrl+Shift+F`, `Ctrl+Shift+M`. | Status changes and save/update results use bounded status lane. | Focus ring visible across header, tab, sidebar toggle, status controls, and editor. | existing: accessibility `shell`, `editor`; visual `main-search-minimap`, `modified-tab`; geometry minimap scenarios. | Orca reports active document display name and active controls, not full path/body unless visibly selected. | `ui/window`, `ui/editor_page`, `ui/status_bar`; verify direct metadata through helper. |
| A11Y-SHELL-DENSE-CONSTRAINED | Shell with dense side surfaces; `dense/awkward`, `constrained/compact` | Secondary surfaces retain names and visibility state when adaptive layout hides or moves them. | `F9`, sidebar toggle, `Escape`, command palette fallback. | No repeated announcements during layout settle. | Primary chrome stays visible; no overlap among sidebars, bottom sheet, status bar, tab bar, and overlay controls. | existing: accessibility `compact-properties`; visual `short-layout`, `compact-properties`, `constrained-properties`, `large-text-constrained`; geometry `minimap-sidebar-*`. | Orca focus after resize remains on the active workflow or a documented fallback. | `ui/window`; manual Orca still owns speech behavior during live resize. |
| A11Y-SHELL-ERROR-STATUS | Status bar and inline errors; `error` | Alerts and durability warnings expose alert/status semantics, bounded labels, and clear recovery action names. | `Tab`, direct dialog response, command/menu retry where available. | High-priority alert or bounded status announcement. | Error is not color-only; icon/text/message remain readable in dark/high contrast. | existing: accessibility `editor-failed-load`, `editor-too-large-policy`; visual `file-health-properties`, `recovery-startup`; needed: failed-save/durability AT-SPI warning fixtures. | Orca should hear/read a concise failure message and the next action. | `ui/info_bar`, `services/notifications`; direct alert role in `info_bar/imp.rs` must route through helper. |
| A11Y-EDITOR-REPRESENTATIVE | GtkSourceView editor; `representative` | Editable text surface exposes active document identity, editable state, caret/text behavior owned by GtkSourceView, and no unbounded document echo in app labels. | `Tab` to editor, typing, arrows, selection shortcuts, `Ctrl+S`, `Ctrl+F`, `Ctrl+H`. | Typing is not announced by app code; status changes are bounded. | Focus ring/caret visible; text remains readable under theme, opacity, and zoom. | existing: accessibility `editor`; visual `main-search-minimap`, `dark-style`, `high-contrast-style`, `transparency-readability`; needed: caret/selection AT-SPI evidence where host exposes it. | Orca reaches the editable document region and receives GtkSourceView text feedback. | `ui/editor_page`; privacy owner: editor adapter. |
| A11Y-EDITOR-BUSY | Editor loading or saving; `busy/loading` | Read-only/busy state and active document identity are exposed without losing final editable restoration. | Save/open shortcuts disabled or no-op safely; close waits or blocks while saving. | Save/load progress and completion announce through bounded status/alert lanes. | Busy state does not hide focus target; spinner/progress is not color-only. | existing: accessibility `editor-save-completion`; needed: long-running loading fixture where host timing can expose busy state. | Orca should identify loading/saving and later restored editability. | `ui/editor_page/load_save`; gap: automation needs safe fixtures for prolonged load states. |
| A11Y-EDITOR-ERROR | Failed load, failed save, durability warning; `error` | Editor/inline alert exposes failure reason and safe next action; document remains modified on uncertain durability. | Retry, Save As, close/cancel paths use normal dialogs. | Alert/status announcement for failure, not repeated heartbeats. | Inline alert visible in constrained layouts; destructive or durability state has text/icon in addition to color. | existing: accessibility `editor-failed-load`, `editor-too-large-policy`; visual partial via file-health/recovery; needed: failed-save/durability fixtures. | Orca should hear concise error and available response. | `ui/editor_page`, `ui/info_bar`; normalize alert role through helper. |
| A11Y-EDITOR-LARGE-READONLY | Large-file policy and preview-only readonly; `busy/loading`, `error` | Read-only state, reason, size policy, syntax/undo degradation are named without exposing body. | Open, Save As, search, close remain keyboard reachable; editing disabled where policy requires. | One bounded warning for policy threshold. | Read-only/policy notices visible at large text and narrow width. | existing: accessibility `editor-too-large-policy`; needed: preview-only readonly smoke. | Orca should report readonly/policy state before edit attempt. | `ui/editor_page`, `services/file_limits`; gap: preview-only proof remains separate from oversized-file policy. |
| A11Y-EDITOR-SEARCH | In-tab search bar; `representative`, `error`, `hidden/dismissed` | Query/replacement entries, result count, invalid/no-result state, next/previous/replace controls, close control, and described-by relations are set and cleared. | `Ctrl+F`, `Ctrl+H`, `Ctrl+G`, `Ctrl+Shift+G`, `Escape`, `Tab`. | Debounced match count and no-result announcement; no per-keystroke spam. | Search bar controls fit in constrained width; focus returns to editor on close. | existing: accessibility `editor-search`; widget stale-state proof owns local metadata. | `Find`, `Replace`, `Next match`, `Previous match`; Orca should hear result count. | `ui/search_bar`, `ui/editor_page`; targeted smoke now owns the AT-SPI-visible bar contract. |
| A11Y-EDITOR-MINIMAP | Minimap and invisible-character controls; `representative`, `constrained/compact` | Minimap toggle/value and invisible-character mode expose state/value text; minimap decorative regions are not misleading focus targets. | `Ctrl+Shift+M`, `Ctrl+Shift+I`, command palette. | Mode changes produce bounded status. | Rendered minimap viewport/focus remains stable during sidebar/properties transitions. | existing: accessibility `minimap-transition`; visual `main-search-minimap`; geometry `minimap-sidebar-top`, `minimap-sidebar-mid`, `minimap-sidebar-live-threshold`, `minimap-sidebar-workspace-animation`; AT-SPI proves the editor remains the semantic text target while visual lanes prove the decorative minimap. | Orca should report toggle state and mode, not minimap internals. | `ui/editor_page/minimap`, `ui/window`; visual geometry owner: proof scenarios. |
| A11Y-EDITOR-FOCUS-PREVIEW | Focus Mode and Markdown preview-only mode; `representative`, `hidden/dismissed` | Mode affordance, readonly preview state, exit control, and hidden-state cleanup have names/states. | `Ctrl+Shift+F11`, `Alt+P`, `Escape`, command palette. | Mode entered/exited status is bounded. | Focus/preview transitions keep primary close/back controls visible and restore focus. | existing: accessibility `focus-mode`, `preview-mode-transition`; visual `reduced-motion-command-palette`; widget proof owns preview/focus-mode exit focus restoration. | Orca should know whether editing is active, readonly preview is active, or focus mode is active. | `ui/window`, `ui/markdown_preview`; gap: manual Orca speech verification. |
| A11Y-MARKDOWN-REPRESENTATIVE | Markdown preview; `representative` | Preview region, read-only state, rendered text, code/table/image/link/fallback widgets, and scroll area expose useful labels/roles. Pending, limited, failed, cancelled, and complete generations expose explicit descriptions instead of silently partial content. | `Alt+P`, side-by-side action, `Tab`, scroll keys, `Escape` when transient controls are present. | Planning, projection, and image work are bounded and remain in readiness until the current terminal. | Text and embedded widgets readable under opacity, dark/high contrast, and large text. | existing: accessibility `markdown-preview`; visual `markdown-preview`, `markdown-preview-side-by-side`; widget proofs cover dense multi-slice, limited, failed, stale, flood, and oversized-image states; AT-SPI smoke asserts preview text-interface plus code, table, and image fallback anchors. | `Markdown preview`; Orca should read rendered fixture text and controls where exposed. | `ui/markdown_preview`; text-interface owner: preview widget. |
| A11Y-MARKDOWN-CONSTRAINED | Markdown preview constrained/side-by-side; `constrained/compact`, `hidden/dismissed` | Side-by-side and preview-only layouts clear hidden side metadata and keep active region named. | `Alt+P`, `Escape`, layout action, `Tab`. | No repeated announcements on layout settle. | Preview/code blocks wrap without overlap; focus target remains visible. | existing: visual `constrained-preview`, `constrained-preview-side-by-side`; geometry `minimap-sidebar-dense-markdown-top`; needed: accessibility hidden-state proof. | Orca should not land on hidden preview/editor controls after mode switch. | `ui/window/preview`, `ui/markdown_preview`; gap: smoke hidden cleanup. |
| A11Y-WORKSPACE-NO-CONTEXT | Workspace sidebar; `no-context` | Sidebar, workspace selector, New Workspace, empty section list/status expose labels and descriptions. | `Tab`, selector, add-folder/new-workspace action, command palette. | No workspace state does not announce as error. | Empty sidebar fits without horizontal scrollbar. | existing: accessibility `workspace-tree-no-workspace`, `workspace-search-no-workspace`; visual `workspace-empty`. | `Workspace scope`, `New Workspace`; Orca can discover empty workspace affordance. | `ui/sidebar`; direct labels in `sidebar/imp.rs` must route through helper. |
| A11Y-WORKSPACE-ZERO-FOLDER | Workspace section; `no-context` | Zero-folder workspace header, add-folder action, and empty folder-set state have distinct names. | `Tab`, context menu, add-folder action. | None unless action completes. | Zero-folder state remains visible in dense sidebar. | existing: accessibility `workspace-tree-zero-folder`. | Orca identifies workspace name and no folders state. | `ui/sidebar/workspace_section`; current smoke exists. |
| A11Y-WORKSPACE-REPRESENTATIVE | File tree; `representative` | Tree rows expose row label, type, expanded/selected/current state, position metadata, and safe path snippets. | Arrow keys, Enter, Space for peek, context menu key/`Shift+F10`, refresh button. | Open/refresh status bounded. | Selected row/focus visible; directory/file/icon state not color-only. | existing: accessibility `workspace-tree`; visual `workspace-representative`, `workspace-refresh`. | Orca reports folder/file rows, expansion, selection, and refresh. | `ui/sidebar/workspace_section`; row metadata apply/clear present and must stay covered. |
| A11Y-WORKSPACE-DENSE-DEEP | File tree; `dense/awkward` | Deep paths, long names, file/folder row recycling, focused folder affordance, and clipped labels have bounded descriptions. | Arrow keys, focus-folder command/context menu, refresh, open. | Dense refresh/search count announcements remain bounded. | No horizontal scrollbar; headers/actions remain visible; deep rows ellipsize or clip intentionally. | existing: accessibility `workspace-tree-dense-constrained`, `workspace-tree-deep-expanded`; visual `workspace-dense-awkward`, `workspace-constrained`; needed: focus-folder a-smoke. | Orca should receive row name/type and not stale previous row metadata after recycling. | `ui/sidebar/workspace_section`; row unbind audit required by policy. |
| A11Y-WORKSPACE-BUSY-ERROR | Refresh/watch scan; `busy/loading`, `error` | Refresh button busy/disabled state, load errors, inaccessible folder state, and watcher warnings have names and bounded status. | Refresh button, command palette/menu fallback, retry. | Refresh start/finish/error status announcement. | Busy/error not color-only; row region remains reachable. | existing: visual `workspace-refresh`; needed: accessibility smoke for busy/error. | Orca should hear refresh/error state and keep focus on stable control. | `services/file_tree`, `ui/sidebar`; gap: safe error fixture. |
| A11Y-WORKSPACE-PEEK | File peek popover; `representative`, `hidden/dismissed` | Peek region is read-only, bounded sample/fallback is named, and hidden popover clears focus target. | `Space`, `Escape`, Enter to promote/open, row navigation. | Peek open/close does not spam; fallback state concise. | Peek does not resize split layout; remains readable next to row. | existing: accessibility `workspace-tree-file-peek`; needed: preview text-interface evidence. | Orca should identify read-only file peek and selected file name. | `ui/sidebar/workspace_section/file_peek`; artifact privacy owner: file peek. |
| A11Y-WORKSPACE-CONTEXT | File and workspace context menus; `representative`, `destructive`, `hidden/dismissed` | Menu items have labels, enabled state, destructive delete confirmation, and no pointer-only paths. | Menu key/`Shift+F10`, arrow keys, Enter, Escape. | Delete/rename/create results announced through status/alerts. | Menu focus visible; hover buttons have context-menu fallback. | existing: accessibility `workspace-tree-folder-context-menu`, `workspace-header-context-menu`; visual `workspace-tree-context-menu`, `workspace-header-context-menu`; widget `workspace_section::test_file_tree_context_menu_opens_from_keyboard_for_selected_row`, `workspace_section::test_file_tree_keyboard_context_menu_exposes_workspace_folder_reorder`, `workspace_section::test_workspace_header_context_menu_opens_from_keyboard_for_focused_header_child`. | Orca should discover New File, New Folder, Rename, Delete, Rename Workspace, Remove Workspace. | `ui/sidebar/workspace_section`; app-owned popovers now expose stable menu item names in AT-SPI and visual screenshots. |
| A11Y-WORKSPACE-DRAG-DROP | Folder reorder/drop targets; `dense/awkward` | Drag convenience has menu alternate path; drop targets are not misleading AT users. | Menu key/`Shift+F10` on workspace-folder row exposes Move Up/Move Down; drag remains pointer convenience. | Reorder completion/failure bounded. | Hover/drop paint not color-only and does not steal row semantics. | existing: accessibility `workspace-tree-folder-context-menu`; visual `workspace-tree-context-menu`; widget `workspace_section::test_file_tree_keyboard_context_menu_exposes_workspace_folder_reorder`. | Orca should not be forced into drag-only workflow for required operation. | `ui/sidebar/workspace_section`; keyboard fallback is the folder context menu. |
| A11Y-OPEN-EMPTY | Open recent popover; `no-context` | Query entry, empty state, Open Another File button, and close/dismiss paths named. | `Ctrl+K`, type, `Tab`, `Enter`, `Escape`. | No recent/no match status if useful, debounced. | Popover fits large text and constrained width. | existing: accessibility `open-popover-empty`; geometry `open-popover`. | `Open recent documents`, `Open another file`; Orca can find empty state and primary action. | `ui/open_popover`; row helper already present. |
| A11Y-OPEN-DENSE-FILTERED | Open recent popover; `dense/awkward`, `representative` | Recycled rows expose file title/path metadata, selected/current state, remove-recent action, and position metadata. | `Ctrl+K`, arrows, Enter, Delete/remove button, `Escape`. | Filter count/no match bounded. | Dense list scrolls while header/search/action remain visible. | existing: accessibility `open-popover-dense`, `open-popover-filtered`, `open-popover-no-match`; visual partial via geometry `open-popover`; needed: remove-recent a-smoke. | Orca reports selected recent document and row count without stale previous item. | `ui/open_popover`; row bind/unbind coverage present, expand tests for stale selected/position. |
| A11Y-OPEN-HIDDEN | Open recent popover; `hidden/dismissed` | Dismissed popover and recycled rows clear metadata/focus. | `Escape`, click-away, open result, Open Another File. | None after dismissal except action result. | Focus returns to opener/editor; no hidden controls remain reachable. | existing: accessibility `open-popover-dismiss`; geometry `open-popover`. | Orca focus returns to documented target. | `ui/open_popover`, `ui/window`; AT-SPI focus proof covers dismissal fallback. |
| A11Y-PALETTE-FILES | Command palette files mode; `representative` | Query, mode selector, result rows, command category, selected/current state, and count expose names/descriptions. | `Ctrl+Shift+P`, type, arrows, Enter, Escape, mode shortcut/tabs where supported. | Debounced result count/no result; selected command not announced repeatedly. | Overlay is readable and focus visible above other surfaces. | existing: accessibility `command-palette`, `command-palette-dense-files`; visual `command-palette-files`, `command-palette-dense-files`; geometry `command-palette-overlay`. | `Command palette query`; Orca reports selected file result safely. | `ui/command_palette`; row helper present. |
| A11Y-PALETTE-COMMANDS | Command palette commands mode; `representative` | Command rows expose command label, category, shortcut, enabled/disabled state, and selected state. | `Ctrl+Shift+P`, mode switch, arrows, Enter, Escape. | Disabled or unavailable command result is bounded status. | Shortcut/value text remains readable. | existing: accessibility `command-palette-commands`; visual `command-palette-commands`. | Orca reports command label plus shortcut when available. | `ui/command_palette`, `services/action_catalog`; audit against action catalog. |
| A11Y-PALETTE-NOTES | Command palette notes mode; `representative` | Note/bookmark result rows expose bounded title/kind/path/date and selected state, not note body. | `Ctrl+Shift+P`, notes mode, arrows, Enter, Escape. | Result count bounded. | Long note titles/paths ellipsize. | existing: accessibility `command-palette-notes`; visual `command-palette-notes`. | Orca reports note result identity without private body. | `ui/command_palette`, notes services; privacy owner: note search. |
| A11Y-PALETTE-NO-RESULTS | Command palette; `no-context`, `error` | No-results and mode-change state expose status, row metadata clears after replacement. | Type/filter, mode switch, Escape. | Debounced no-results announcement. | Empty state fits in overlay; focus remains in query. | existing: accessibility `command-palette-no-results`, `command-palette-mode-changes`; visual `command-palette-no-results`. | Orca reports no results and active mode. | `ui/command_palette`; row replacement tests should include selected/current clear. |
| A11Y-PALETTE-DISMISS | Command palette; `hidden/dismissed`, `constrained/compact` | Close/click-away/Escape clears hidden focus target and restores saved focus. | `Escape`, click-away, activated result. | None after dismissal except action result. | Overlay not visible after close; reduced-motion state still restores focus. | existing: accessibility `command-palette-focus-restore`; visual `command-palette-dismissed`, `reduced-motion-command-palette`; geometry `command-palette-overlay`. | Orca focus returns to previous control or active editor. | `ui/window/command_palette`; focus restoration already has smoke but needs matrix manifest id. |
| A11Y-WORKSPACE-SEARCH-NO-CONTEXT | Workspace search panel; `no-context` | Panel, query, include/exclude, replace controls, and empty no-workspace state are named. | `Ctrl+Shift+F`, `Tab`, Escape/close, command palette. | No-workspace status bounded. | Panel fits above status bar in short layout. | existing: accessibility `workspace-search-no-workspace`. | `Workspace search`, `Search query`; Orca reports no workspace. | `ui/search_panel`; row helper present. |
| A11Y-WORKSPACE-SEARCH-REPRESENTATIVE | Workspace search panel/results; `representative` | Result rows expose bounded file path, line number, match summary, current/selected state, and result count. | `Ctrl+Shift+F`, arrows, Enter, `F4`, `Shift+F4`, Tab. | Debounced result count/progress completion. | Match highlights not color-only; result list scrolls only rows. | existing: accessibility `workspace-search`; needed: richer result text-interface proof. | Orca reports result count and selected result identity. | `ui/search_panel`, `services/content_search`; privacy owner: search excerpt. |
| A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS | Workspace search panel; `dense/awkward`, `constrained/compact`, `error` | Dense result rows recycle cleanly; no-results/invalid query and capped/truncated states expose a clear status. | Filter/query, arrows, `F4`, close. | No-result/invalid status bounded. | Header controls remain visible; dense rows do not overlap. | existing: accessibility `workspace-search-no-results`, `workspace-search-dense-constrained`, `workspace-search-capped`; needed: invalid-query specific proof if applicable. | Orca does not receive stale previous row names after filter/model replacement. | `ui/search_panel/list_factory`; row apply/clear present. |
| A11Y-WORKSPACE-SEARCH-REPLACE | Replace preview, Replace All, undo; `destructive`, `recovery` | Replace entry, preview rows, confirmation, completion, undo availability, saved searches/history rows named. | `Ctrl+Shift+F`, Tab to replace, Replace All, Undo, Escape/cancel. | Replace count, completion, and undo availability announce bounded counts. | Destructive/undo state not color-only; confirmation buttons clear. | existing: accessibility `workspace-search-replace-undo`; needed: destructive confirmation smoke and saved-search/history rows. | Orca reports replacement count and undo state without dumping matches. | `ui/search_panel`, `services/content_search`, `services/search_backup`; safety owner: replace workflow. |
| A11Y-PROPERTIES-NORMAL | Document properties wide pane; `representative` | Pane, grouped rows, file metadata, formatting source, line ending, encoding controls, file-health rows expose names/value text. | `F9`, Tab, arrows/combo, Escape in compact sheet. | Changed setting/value announcements through normal GTK controls/status. | Rows readable; controls and values align at large text. | existing: accessibility `properties-panel`; visual `normal-properties`, `file-health-properties`. | `Document properties`; Orca reads row labels and values. | `ui/properties_panel`; verify row metadata/value text. |
| A11Y-PROPERTIES-COMPACT | Document properties bottom sheet; `constrained/compact`, `hidden/dismissed` | Same panel retains names when presented as bottom sheet; hidden wide/sheet presentation does not leave stale focus targets. | `F9`, Escape, Tab. | None except open/close status if present. | Sheet has close/back affordance, stable focus, no overlap with status/editor. | existing: accessibility `compact-properties`; visual `compact-properties`, `constrained-properties`. | Orca reports the active sheet/pane once, not both hidden/visible instances. | `ui/window/properties`, `ui/properties_panel`; manual Orca owns live sheet speech/focus quality. |
| A11Y-NOTES-EMPTY | Browse Notes; `no-context` | Dialog/sidebar/query/empty status expose names and no private note body. | `Ctrl+Alt+A`, Tab, Escape, query. | No notes/no results bounded. | Empty state fits without useless scroll. | existing: accessibility `notes-empty`; visual `notes-empty`. | Orca reports Notes empty state and search field. | `ui/command_palette`/notes dialogs; owner: notes UI. |
| A11Y-NOTES-POPULATED | Notes rows/actions; `representative`, `dense/awkward`, `constrained/compact` | Rows expose bounded title/kind/path/date, preview identity, open/copy/edit/delete actions, selected/current state, and row recycling clear. | `Ctrl+Alt+A`, arrows, Enter, context menu, Delete action, Escape. | Open/copy/edit/delete result bounded; no body dump. | Dense/long rows scroll; action affordances have keyboard fallback. | existing: accessibility `notes-populated`, `notes-no-results`; visual `notes-few`, `notes-dense`, `notes-constrained`; needed: preview/action/context a-smoke. | Orca reports note identity and actions, not full body. | Notes/browser UI; gap: action/context parity proof. |
| A11Y-BOOKMARKS | Bookmarks rows/actions; `representative`, `dense/awkward`, `constrained/compact` | Rows expose line, label, bounded excerpt, open/copy/edit/delete, selected/current state, and row recycling clear. | `Ctrl+Alt+B`, `Ctrl+F2`, `F2`, arrows, Enter, context menu, Escape. | Bookmark changed/deleted/copy status bounded. | Bookmark state not color-only; dense rows readable. | existing: accessibility `bookmarks-populated`; visual `bookmarks-few`, `bookmarks-dense`, `bookmarks-constrained`. | Orca reports bookmark label/line safely and actions. | `ui/editor_page/bookmarks`, browse UI; populated AT-SPI proof owns the browser shell, row, and open action. |
| A11Y-LOCAL-HISTORY-EMPTY | Local History; `no-context` | Dialog/sidebar/query/empty state and empty snapshot state named. | `Ctrl+Alt+L`, Tab, Escape. | Empty state does not announce as error. | Empty snapshot surface fits. | existing: accessibility `local-history-empty`, `local-history-empty-snapshot`; visual `local-history-restore` partial. | Orca reports empty local history/snapshot state. | `ui/window/local_history`; smoke exists for empty variants. |
| A11Y-LOCAL-HISTORY-POPULATED | Local History rows/preview/restore; `representative`, `destructive`, `hidden/dismissed` | Snapshot rows, preview, copy, restore, destructive restore confirmation, selected state, and row recycling clear. | `Ctrl+Alt+L`, arrows, Enter/preview, Copy, Restore, Escape. | Restore confirmation/completion bounded. | Restore/destructive state is not color-only; preview remains read-only. | existing: accessibility `local-history`, `local-history-restore`; visual `local-history-restore`. | Orca reports snapshot identity and restore confirmation without dumping snapshot body. | `ui/window/local_history`, `services/local_history_service`; safety owner: restore workflow. |
| A11Y-PREFERENCES-PAGES | Preferences dialog; `representative`, `constrained/compact` | Every page, row, switch, combo, spin/control row, and navigation element has label/value/description. | App menu/preferences action, Tab, arrows, Escape/close. | Setting changes may use GTK control feedback; no extra spam. | Dialog remains readable at large text; page navigation works collapsed. | existing: accessibility `preferences`; needed: per-page row matrix manifest ids. | Orca reads page names and row values. | `ui/preferences`; static rows use helper metadata. |
| A11Y-PREFERENCES-DATA-SCAN | Preferences Data page scan/status; `busy/loading`, `error`, `recovery` | Scan status, app-data format/migration state, retry/repair controls, and destructive reset/upgrade warnings named. | Tab, activate scan/retry/repair, Escape/cancel. | Scan start/progress/result bounded. | Warnings and repair state not color-only. | needed: accessibility smoke for scan announcements and migration warnings. | Orca hears scan status and safe next action. | `ui/preferences`, `services/format_upgrade`; gap: smoke fixture needed. |
| A11Y-DIALOG-SAVE-CLOSE | Unsaved close/save/discard dialog; `destructive` | Dialog role/title, grouped modified documents, per-document checkboxes, response labels, suggested/destructive appearance, and focus order are explicit. | `Ctrl+W`, app close, Tab, arrows, Space, Enter, Escape. | Destructive/discard result bounded; cancellation quiet. | Buttons, checkboxes, and grouped rows fit with long names and large text. | existing: accessibility `unsaved-close-dialog`; visual `destructive-close-dialog`; helper normalization in `window/dialogs.rs` complete. | Orca reports modified document group, checkbox labels, Save/Discard/Cancel. | `ui/window/dialogs`; manual Orca owns speech ordering. |
| A11Y-DIALOG-DESTRUCTIVE | Delete, restore, Replace All, migration/format-upgrade confirmations; `destructive`, `recovery` | Alert/dialog roles, destructive/suggested response labels, affected item count, and bounded descriptions are explicit. | Context/menu action, Tab, Enter, Escape, Space on checkbox. | Confirmation/completion/failure bounded. | Destructive intent not color-only and response focus visible. | existing: accessibility `discard-confirmation`, `workspace-search-replace-undo`, `local-history-restore`; needed: delete and migration/format-upgrade AT-SPI cases where safe fixtures exist. | Orca should identify action, affected count, and Cancel before destructive response. | Dialog owners in sidebar/search/local-history/preferences; safety owner: each workflow. |
| A11Y-CONTEXT-MENUS-GENERAL | Context menus across rows/editor/search/preview; `representative`, `hidden/dismissed` | Menu labels/enabled states are product-facing; hidden menus restore focus and do not require pointer coordinates. | Menu key/`Shift+F10`, arrows, Enter, Escape, command-palette fallback. | Action results bounded. | Menu focus and disabled state visible. | existing: accessibility `workspace-tree-folder-context-menu`, `workspace-header-context-menu`; visual `workspace-tree-context-menu`, `workspace-header-context-menu`; widget proof for workspace, tab, and editor menus; needed: non-workspace AT-SPI menu anchors as surfaces grow. | Orca reads menu items for file rows, workspace headers, notes/bookmarks, local history, editor/tab actions, search rows, preview surfaces. | Surface owners; global gap now tracks remaining non-workspace menus instead of the workspace sidebar. |
| A11Y-RECOVERY-STARTUP | Draft/session/recovery startup surfaces; `recovery`, `error` | Recovery diagnostics, restored tabs, failed metadata, quarantine/repair actions, and startup alerts expose bounded names. | Startup, Tab through alerts, repair/skip actions, Escape when safe. | Recovery/repair status bounded. | Recovery warning not color-only and does not block main chrome invisibly. | existing: visual `recovery-startup`; needed: accessibility smoke/manual Orca for recovery workflow. | Orca reports recovery status and available actions without private draft body. | `ui/window/startup`, recovery services; privacy owner: recovery metadata. |
| A11Y-ERROR-SURFACES | Inline alerts, unavailable host lanes, failed smoke/runtime errors; `error` | User-facing error controls are named; test artifacts distinguish unsupported host from passing coverage. | Retry/cancel/close paths keyboard reachable. | High-priority alerts bounded and throttled. | Errors remain visible and non-color-only across themes. | existing: accessibility `editor-failed-load`, `editor-too-large-policy`; policy current-tree checks and smoke manifests with matrix ids. | Orca reads the concise error and control names. | Cross-cutting; policy owner: scripts and docs. |

## Baseline Audit Notes

### Direct GTK Accessibility Calls Outside `ui::accessibility`

Current tree audit command:

```sh
rg -n "set_accessible_role|update_property|update_state|update_relation|announce" crates/lushtext-core/src/ui -g '*.rs'
```

Calls inside `crates/lushtext-core/src/ui/accessibility.rs` are the helper
implementation and are allowed. The remaining direct-call baseline is:

| Location | Current direct call | Decision |
| --- | --- | --- |
| `crates/lushtext-core/src/ui/sidebar/imp.rs` | Workspace selector and New Workspace labels/descriptions use `update_property`. | Normalize through `ui::accessibility::set_labelled_description`. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/folders.rs` | Workspace row/folder description uses `update_property`. | Normalize through `ui::accessibility::set_labelled_description`. |
| `crates/lushtext-core/src/ui/window/dialogs.rs` | Modified-document group role and checkbox labels/descriptions use direct role/property calls. | Normalize through `ui::accessibility::set_role` and `set_labelled_description`. |
| `crates/lushtext-core/src/ui/info_bar/imp.rs` | Inline alert role uses `set_accessible_role` directly. | Normalize through `ui::accessibility::set_role`. |

No permanent exception is currently justified. If a future GTK contract requires
one, add an explicit allowlist entry with owner, GTK reason, proof lane, and
expiry review date.

### Row Factory Apply/Clear Coverage

Current row factory audit command:

```sh
rg -n "SignalListItemFactory|connect_bind|connect_unbind|RowAccessibility|clear_row_accessibility|apply_row_accessibility" crates/lushtext-core/src/ui crates/lushtext/tests/widget -g '*.rs'
```

| Surface | Current state | Required follow-up |
| --- | --- | --- |
| Open popover | Uses `RowAccessibility` on bind and `clear_row_accessibility()` on unbind. | Extend tests for stale selected, set-position, description, and label clear after filtering/model replacement. |
| Workspace search results | Uses row helper apply/clear. | Extend tests for no-results/filter replacement and dense constrained rows. |
| Command palette | Uses palette row helper apply/clear. | Extend tests for mode changes, selected/current row clear, and no-results replacement. |
| Workspace file tree | Uses row helper apply/clear. | Extend tests for deep/dense row recycling, selected/expanded state, and file/folder descriptions. |
| Preferences Data page rows | Uses helper metadata on static/preference rows. | Keep static-row audit in widget tests; no recycled factory clear path needed unless a factory is introduced. |
| Notes/bookmarks/local history browsers | Matrix requires row apply/clear proof where recycled list/sidebar rows exist. | Audit implementation during the surface completion phase and add missing tests. |

### Transient Surface Audit

| Surface | Names/roles | Focus restoration | Dismissal | Hidden cleanup | Proof gap |
| --- | --- | --- | --- | --- | --- |
| Command palette | Existing names and row metadata. | Existing saved-focus path. | `Escape` and click-away. | Existing focused smoke. | Manifests need row ids; reduced-motion and constrained focus should stay covered. |
| Open popover | Existing query/action/row names. | Needs explicit focused proof. | `Escape`, activation, click-away. | Needs row/focus cleanup assertions. | Add a-smoke dismissal and remove-recent coverage. |
| In-tab search bar | Needs full matrix coverage. | Should restore editor focus. | `Escape`/close. | Needs stale state proof. | Add widget and a-smoke. |
| Workspace search panel | Existing panel/row proof. | Needs close/focus proof in constrained layout. | `Escape`/close. | Needs hidden metadata cleanup. | Add a-smoke. |
| File peek | Existing a-smoke. | Should return to selected row/editor on promote. | `Space`, `Escape`, click-away, row change. | Needs preview text/focus proof. | Add text-interface evidence. |
| Context menus | Product labels exist per surface. | Needs keyboard popup target proof. | `Escape`, action. | Needs hidden focus cleanup. | Add widget and a-smoke for Menu/`Shift+F10`. |
| Dialogs | Some direct metadata exists. | GTK dialog focus order, needs proof. | Response buttons/`Escape`. | Dialog destruction should return focus. | Normalize helper calls and add a-smoke. |
| Bottom sheet/properties | Visual proof exists. | Needs compact focus proof. | `Escape`, `F9`. | Hidden wide/sheet target cleanup. | Add a-smoke. |

### Keyboard And Pointer-Only Audit

Pointer convenience is acceptable only when an equivalent keyboard path exists
or the operation is explicitly non-essential and documented. The current
parity ledger is:

| Surface or operation | Keyboard/context path | Proof and remaining gap |
| --- | --- | --- |
| File-tree row actions | `Menu` or `Shift+F10` on the selected row opens New File, New Folder, Rename, Delete, Focus Folder, document note, and local-history actions. | Widget proof exists for file rows; AT-SPI smoke now covers the seeded workspace-folder popup and stable menu item labels. |
| Workspace header actions | `Menu` or `Shift+F10` on the header opens Add Folder, Open Folder Note, Rename Workspace, and Remove Workspace. | Widget proof exists for the keyboard popup; AT-SPI smoke now covers the seeded workspace header popup. |
| Folder reorder | Drag/drop is pointer convenience; the workspace-folder context menu exposes Move Up/Move Down. | Widget proof and AT-SPI smoke both cover the keyboard menu reorder fallback. |
| Notes and bookmark rows | Browse Notes and Browse Bookmarks use dialog/sidebar selection, Open buttons, menu-scoped Notes actions, command-palette note results, and action-backed `select-notes-browser-row`/`open-notes-browser-selection`; row activation previews instead of surprising the user with destructive or navigation side effects. | Widget proof exists for empty, populated, dense, filtering, preview, and menu-scoped actions; add AT-SPI smoke for row action discovery. |
| Local-history rows | `Ctrl+Alt+L` opens the browser; keyboard selection reaches Copy, Restore, Back, and Escape dismissal. Restore keeps the normal confirmation/undo path. | Widget proof exists for empty, populated, empty snapshot, restore, and undo; AT-SPI smoke now covers the restore completion alert and undo/dismiss controls. |
| Search-result rows | Workspace search results activate with list keyboard activation; file groups expand/collapse, match rows open the file, replace preview rows use checkboxes plus Replace All/Undo buttons. | Widget proof exists for result row metadata, activation callbacks, replace preview, undo, and safety state; add AT-SPI smoke for row activation/focus. |
| Editor and tab context actions | GtkSourceView exposes the editor extra menu with note/local-history actions; Adwaita tab setup-menu exposes Pin, Close Other Tabs, Close Tabs Right, and Move actions. | Widget proof exists for menu labels, tab target behavior, and confirmation on bulk close; native GTK/Adwaita popup keyboard behavior is delegated to toolkit, with command/menu actions as fallback. |
| Markdown preview and preview modes | Primary menu action, `Alt+P`, `set-preview-*` target actions, and focus-mode escape paths cover preview entry/exit; preview content itself is read-only and scrollable. | Widget proof exists for menu action, target actions, hidden-state cleanup, and read-only preview metadata; AT-SPI smoke now covers preview text-interface evidence and editor focus after preview dismissal. |
| Hover-only affordances | File-tree focus-folder and folder reorder affordances have context-menu alternatives; nonessential hover paint stays decorative. | Keep policy checks watching for new hover-only actions without fallback evidence. |

## Existing Accessibility Smoke Crosswalk

| Smoke case | Matrix rows |
| --- | --- |
| `shell` | A11Y-SHELL-NO-CONTEXT, A11Y-SHELL-REPRESENTATIVE |
| `preferences` | A11Y-PREFERENCES-PAGES |
| `properties-panel` | A11Y-PROPERTIES-NORMAL |
| `compact-properties` | A11Y-PROPERTIES-COMPACT, A11Y-SHELL-DENSE-CONSTRAINED |
| `markdown-preview` | A11Y-MARKDOWN-REPRESENTATIVE |
| `preview-mode-transition` | A11Y-EDITOR-FOCUS-PREVIEW, A11Y-MARKDOWN-CONSTRAINED |
| `editor` | A11Y-EDITOR-REPRESENTATIVE |
| `editor-search` | A11Y-EDITOR-SEARCH |
| `editor-save-completion` | A11Y-EDITOR-BUSY, A11Y-SHELL-REPRESENTATIVE |
| `editor-failed-load` | A11Y-EDITOR-ERROR, A11Y-SHELL-ERROR-STATUS, A11Y-ERROR-SURFACES |
| `editor-too-large-policy` | A11Y-EDITOR-LARGE-READONLY, A11Y-EDITOR-ERROR, A11Y-ERROR-SURFACES |
| `focus-mode` | A11Y-EDITOR-FOCUS-PREVIEW |
| `minimap-transition` | A11Y-EDITOR-MINIMAP |
| `workspace-search-no-workspace` | A11Y-WORKSPACE-SEARCH-NO-CONTEXT, A11Y-WORKSPACE-NO-CONTEXT |
| `workspace-search` | A11Y-WORKSPACE-SEARCH-REPRESENTATIVE |
| `workspace-search-no-results` | A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS |
| `workspace-search-dense-constrained` | A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS |
| `workspace-search-capped` | A11Y-WORKSPACE-SEARCH-DENSE-NORESULTS |
| `workspace-search-replace-undo` | A11Y-WORKSPACE-SEARCH-REPLACE |
| `workspace-tree-no-workspace` | A11Y-WORKSPACE-NO-CONTEXT |
| `workspace-tree` | A11Y-WORKSPACE-REPRESENTATIVE |
| `workspace-tree-zero-folder` | A11Y-WORKSPACE-ZERO-FOLDER |
| `workspace-tree-dense-constrained` | A11Y-WORKSPACE-DENSE-DEEP |
| `workspace-tree-deep-expanded` | A11Y-WORKSPACE-DENSE-DEEP |
| `workspace-tree-file-peek` | A11Y-WORKSPACE-PEEK |
| `workspace-tree-folder-context-menu` | A11Y-WORKSPACE-CONTEXT, A11Y-WORKSPACE-DRAG-DROP, A11Y-CONTEXT-MENUS-GENERAL |
| `workspace-header-context-menu` | A11Y-WORKSPACE-CONTEXT, A11Y-CONTEXT-MENUS-GENERAL |
| `open-popover-empty` | A11Y-OPEN-EMPTY |
| `open-popover-dense` | A11Y-OPEN-DENSE-FILTERED |
| `open-popover-filtered` | A11Y-OPEN-DENSE-FILTERED |
| `open-popover-no-match` | A11Y-OPEN-DENSE-FILTERED |
| `open-popover-dismiss` | A11Y-OPEN-HIDDEN |
| `command-palette` | A11Y-PALETTE-FILES |
| `command-palette-commands` | A11Y-PALETTE-COMMANDS |
| `command-palette-notes` | A11Y-PALETTE-NOTES |
| `command-palette-dense-files` | A11Y-PALETTE-FILES |
| `command-palette-mode-changes` | A11Y-PALETTE-NO-RESULTS |
| `command-palette-focus-restore` | A11Y-PALETTE-DISMISS |
| `command-palette-no-results` | A11Y-PALETTE-NO-RESULTS |
| `notes-empty` | A11Y-NOTES-EMPTY |
| `notes-populated` | A11Y-NOTES-POPULATED |
| `notes-no-results` | A11Y-NOTES-POPULATED |
| `bookmarks-populated` | A11Y-BOOKMARKS |
| `local-history-empty` | A11Y-LOCAL-HISTORY-EMPTY |
| `local-history` | A11Y-LOCAL-HISTORY-POPULATED |
| `local-history-restore` | A11Y-LOCAL-HISTORY-POPULATED, A11Y-DIALOG-DESTRUCTIVE |
| `local-history-empty-snapshot` | A11Y-LOCAL-HISTORY-EMPTY |
| `unsaved-close-dialog` | A11Y-DIALOG-SAVE-CLOSE |
| `discard-confirmation` | A11Y-DIALOG-DESTRUCTIVE |

Accessibility rows with remaining partial or uncovered smoke work include:
A11Y-SHELL-ERROR-STATUS for failed-save/durability warnings,
A11Y-EDITOR-BUSY for prolonged loading, A11Y-EDITOR-ERROR for failed-save
fixtures, A11Y-EDITOR-LARGE-READONLY for preview-only readonly,
A11Y-WORKSPACE-BUSY-ERROR, A11Y-PREFERENCES-DATA-SCAN,
A11Y-CONTEXT-MENUS-GENERAL for non-workspace context menus, and
A11Y-RECOVERY-STARTUP.

## Existing Visual And Geometry Crosswalk

| Visual or geometry case | Matrix rows |
| --- | --- |
| `main-search-minimap` | A11Y-SHELL-REPRESENTATIVE, A11Y-EDITOR-MINIMAP |
| `modified-tab` | A11Y-SHELL-REPRESENTATIVE |
| `destructive-close-dialog` | A11Y-DIALOG-SAVE-CLOSE |
| `file-health-properties` | A11Y-SHELL-ERROR-STATUS, A11Y-PROPERTIES-NORMAL |
| `local-history-restore` | A11Y-LOCAL-HISTORY-POPULATED |
| `normal-properties` | A11Y-PROPERTIES-NORMAL |
| `compact-properties` | A11Y-PROPERTIES-COMPACT |
| `constrained-properties` | A11Y-PROPERTIES-COMPACT |
| `short-layout` | A11Y-SHELL-DENSE-CONSTRAINED |
| `markdown-preview` | A11Y-MARKDOWN-REPRESENTATIVE |
| `constrained-preview` | A11Y-MARKDOWN-CONSTRAINED |
| `markdown-preview-side-by-side` | A11Y-MARKDOWN-REPRESENTATIVE |
| `constrained-preview-side-by-side` | A11Y-MARKDOWN-CONSTRAINED |
| `workspace-empty` | A11Y-WORKSPACE-NO-CONTEXT |
| `workspace-representative` | A11Y-WORKSPACE-REPRESENTATIVE |
| `workspace-dense-awkward` | A11Y-WORKSPACE-DENSE-DEEP |
| `workspace-constrained` | A11Y-WORKSPACE-DENSE-DEEP |
| `workspace-refresh` | A11Y-WORKSPACE-BUSY-ERROR |
| `workspace-tree-context-menu` | A11Y-WORKSPACE-CONTEXT, A11Y-WORKSPACE-DRAG-DROP, A11Y-CONTEXT-MENUS-GENERAL |
| `workspace-header-context-menu` | A11Y-WORKSPACE-CONTEXT, A11Y-CONTEXT-MENUS-GENERAL |
| `notes-empty` | A11Y-NOTES-EMPTY |
| `notes-few` | A11Y-NOTES-POPULATED |
| `bookmarks-few` | A11Y-BOOKMARKS |
| `notes-dense` | A11Y-NOTES-POPULATED |
| `bookmarks-dense` | A11Y-BOOKMARKS |
| `notes-constrained` | A11Y-NOTES-POPULATED |
| `bookmarks-constrained` | A11Y-BOOKMARKS |
| `command-palette-files` | A11Y-PALETTE-FILES |
| `command-palette-commands` | A11Y-PALETTE-COMMANDS |
| `command-palette-notes` | A11Y-PALETTE-NOTES |
| `command-palette-no-results` | A11Y-PALETTE-NO-RESULTS |
| `command-palette-dense-files` | A11Y-PALETTE-FILES |
| `command-palette-dismissed` | A11Y-PALETTE-DISMISS |
| `dark-style` | A11Y-EDITOR-REPRESENTATIVE |
| `high-contrast-style` | A11Y-EDITOR-REPRESENTATIVE |
| `large-text-constrained` | A11Y-SHELL-DENSE-CONSTRAINED, A11Y-EDITOR-REPRESENTATIVE |
| `reduced-motion-command-palette` | A11Y-PALETTE-DISMISS, A11Y-EDITOR-FOCUS-PREVIEW |
| `transparency-readability` | A11Y-EDITOR-REPRESENTATIVE, A11Y-MARKDOWN-REPRESENTATIVE |
| `recovery-startup` | A11Y-RECOVERY-STARTUP |
| `command-palette-overlay.json` | A11Y-PALETTE-DISMISS, A11Y-PALETTE-FILES |
| `minimap-sidebar-dense-markdown-top.json` | A11Y-EDITOR-MINIMAP, A11Y-MARKDOWN-CONSTRAINED |
| `minimap-sidebar-live-threshold.json` | A11Y-EDITOR-MINIMAP, A11Y-SHELL-DENSE-CONSTRAINED |
| `minimap-sidebar-mid.json` | A11Y-EDITOR-MINIMAP |
| `minimap-sidebar-top.json` | A11Y-EDITOR-MINIMAP |
| `minimap-sidebar-workspace-animation.json` | A11Y-EDITOR-MINIMAP, A11Y-WORKSPACE-DENSE-DEEP |
| `open-popover.json` | A11Y-OPEN-EMPTY, A11Y-OPEN-DENSE-FILTERED, A11Y-OPEN-HIDDEN |

Visual rows not yet covered by current visual/geometry smoke include:
A11Y-EDITOR-BUSY, A11Y-EDITOR-ERROR, A11Y-EDITOR-LARGE-READONLY,
A11Y-EDITOR-SEARCH, A11Y-OPEN-HIDDEN visual focus after dismissal,
A11Y-PREFERENCES-PAGES at large text, A11Y-PREFERENCES-DATA-SCAN, and
A11Y-DIALOG-DESTRUCTIVE.

## Release Completion Rule

A row is complete only when all applicable proof lanes are current for the
relevant tree:

1. Widget tests cover helper state, stale metadata cleanup, focus restoration,
   or keyboard behavior that can be proven without AT-SPI.
2. Accessibility smoke covers AT-SPI-visible names, roles, focus, tree shape,
   text-interface evidence, or an explicit unsupported-host caveat.
3. Visual or visual-geometry smoke covers focus visibility, large text,
   constrained geometry, color-not-only state, and pixel-sensitive invariants.
4. Manual Orca validation covers host-sensitive speech, caret, selection, and
   announcement behavior that automation cannot honestly prove.
5. Policy checks confirm matrix rows, smoke manifests, stable anchors, helper
   use, and current-tree freshness have not drifted.

Focused smoke runs are debugging evidence. A release-grade accessibility claim
requires unfiltered current-tree summaries or an explicit scoped release note
that names the rows intentionally deferred.
