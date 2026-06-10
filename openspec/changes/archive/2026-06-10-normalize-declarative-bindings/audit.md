# Declarative Binding Audit

## Method

- Searched `crates/lushtext-core/src/ui/` for signal, notify, settings, property-binding, direct widget projection setters, and projection refresh/update patterns with `rg`.
- Reviewed the 26 production `resources/ui/*.blp` and generated `resources/ui/*.ui` files for template binding or expression opportunities that could move into Blueprint or GtkBuilder without changing object IDs, layout roles, or accessibility/action metadata.
- Split the repo by surface and delegated independent audits:
  - Preferences, search panel, search bar, properties panel, status bar, and info bar.
  - Window shell, sidebar, command palette, automation UI glue, and shrinkable bin.
  - Editor page and Markdown preview.

## Inventory

The inventory command matched 957 UI projection-candidate patterns. Each surface below was reviewed with the same treatment rule: direct copies are converted when GTK-native bindings preserve lifecycle and behavior; pure-derived projections are converted only when a local pure transform is enough; workflow, persistence, async, model/factory, focus, readiness, and layout orchestration stay imperative.

Pattern columns are `binding / notify / signal / refresh-or-update / projection-set`.

| Surface | Pattern hits | Candidate classifications and intended treatment |
| --- | ---: | --- |
| `automation.rs` | `0 / 0 / 0 / 8 / 0` | Automation snapshot and readiness updates are workflow/observation work. No pure projection was converted; automation contract stays unchanged. |
| `command_palette/` | `0 / 1 / 9 / 10 / 9` | Search mode/result visibility includes pure-derived projections, but currently depends on query debounce, file-index rebuilds, activation, and focus restoration. Deferred until private projection properties exist; workflow stays imperative. |
| `editor_page/` | `5 / 2 / 33 / 27 / 10` | `word-wrap -> wrap-mode` and `minimap-width -> width-request` were pure settings projections and were converted. Formatting settings with EditorConfig overrides, estimated memory, style scheme, opacity/dark mode, minimap refresh/markers, bookmarks, local history, monitor, focus mode, and load/save are derived or side-effectful and stay imperative. |
| `info_bar/` | `0 / 0 / 9 / 1 / 20` | Alert presentation combines document context, severity, accessibility announcement, dismissal, and layout. No direct pure projection found; stays imperative. |
| `markdown_preview/` | `0 / 3 / 7 / 11 / 7` | Content/placeholder and opacity are possible future derived properties, but rendering, dark-mode tag repair, link activation, code-block width repair, and embedded widget lifecycle are side-effectful. Deferred or imperative. |
| `preferences/` | `17 / 3 / 0 / 0 / 0` | Existing settings-backed rows already use native settings/widget binding. The transparency percentage label was a pure-derived adjustment projection and was converted. Persistence and value normalization remain in the preferences controls. |
| `properties_panel/` | `0 / 0 / 0 / 0 / 11` | Metadata rows are tab-dependent projections without a single existing GObject source property. Deferred until wrapper/private properties exist. |
| `search_bar/` | `1 / 2 / 13 / 15 / 2` | Replace-mode revealers were direct `active -> reveal-child` projections and were converted. Query, replace, match count, option actions, focus, and navigation callbacks stay imperative. |
| `search_panel/` | `8 / 3 / 25 / 23 / 45` | Advanced-options revealer and row count bindings were already declarative. Search, replace, undo backup, result streaming, history, saved searches, and model/factory recycling remain imperative. |
| `sidebar/` | `0 / 5 / 99 / 36 / 62` | Workspace section body/tooltip/collapse projections are possible future derived-property work. File tree factories, DnD, context menus, inline rename, watcher refresh, workspace persistence, and async tree loading are side-effectful or model/factory work and stay imperative. |
| `status_bar/` | `0 / 0 / 0 / 5 / 7` | Message, severity, metadata, and pulse state are derived from notification and tab workflow state. Deferred until private properties exist; notification lifecycle stays imperative. |
| `window/` | `4 / 15 / 100 / 180 / 104` | Fullscreen action enablement and Local History/Notes back-button visibility were pure object projections and were converted. Zoom labels/sensitivity and tab pinned icon remain deferred derived projections; geometry, split views, breakpoints, preview, transient surfaces, focus restoration, session, search, notes, local history, notifications, and document workflows stay imperative. |
| `resources/ui/*.blp`, `resources/ui/*.ui` | `0 template-binding hits` | Production templates contain no existing binding/expression constructs. No template-level binding was introduced because the safe batch was fully expressible through Rust-owned GTK bindings. |

### File-Level Index

Each regex hit belongs to one of these file rows. `Converted` rows name the direct projection converted in this change; `deferred` rows require private projection properties or wrapper state; `imperative` rows are workflow, persistence, async, layout/readiness, focus, or model/factory candidates.

| File | Hits | Classification and treatment |
| --- | ---: | --- |
| `crates/lushtext-core/src/ui/automation.rs` | 8 | Workflow observation/readiness; imperative. |
| `crates/lushtext-core/src/ui/command_palette/imp.rs` | 20 | Query, mode, and result workflow; derived projections deferred, search/focus imperative. |
| `crates/lushtext-core/src/ui/command_palette/mod.rs` | 9 | Palette open/close/result workflow; imperative. |
| `crates/lushtext-core/src/ui/editor_page/bookmarks.rs` | 6 | Bookmark projection/persistence workflow; imperative. |
| `crates/lushtext-core/src/ui/editor_page/focus_mode.rs` | 14 | Focus-mode geometry and editor presentation; imperative. |
| `crates/lushtext-core/src/ui/editor_page/imp.rs` | 19 | Converted `word-wrap -> wrap-mode` and `minimap-width -> width-request`; formatting override and style/opacity handlers remain imperative. |
| `crates/lushtext-core/src/ui/editor_page/load_save.rs` | 3 | Load/save completion and minimap refresh; imperative. |
| `crates/lushtext-core/src/ui/editor_page/local_history.rs` | 2 | Local-history editor markers; imperative. |
| `crates/lushtext-core/src/ui/editor_page/minimap.rs` | 15 | Minimap visibility, marker, geometry, and reflow workflow; imperative after width projection moved to binding. |
| `crates/lushtext-core/src/ui/editor_page/mod.rs` | 9 | Editor metadata, state, and refresh entry points; imperative. |
| `crates/lushtext-core/src/ui/editor_page/monitor.rs` | 1 | File monitor workflow; imperative. |
| `crates/lushtext-core/src/ui/editor_page/overscroll.rs` | 5 | Overscroll geometry; imperative. |
| `crates/lushtext-core/src/ui/editor_page/search.rs` | 3 | In-tab search marker refresh; imperative. |
| `crates/lushtext-core/src/ui/info_bar/imp.rs` | 6 | Info-bar template signal wiring; imperative. |
| `crates/lushtext-core/src/ui/info_bar/mod.rs` | 24 | Alert rendering/accessibility/dismissal; imperative. |
| `crates/lushtext-core/src/ui/markdown_preview/imp.rs` | 9 | Preview widget signal wiring and settings; imperative or deferred. |
| `crates/lushtext-core/src/ui/markdown_preview/mod.rs` | 19 | Rendered content, placeholder, dark-mode, and embed lifecycle; imperative or deferred. |
| `crates/lushtext-core/src/ui/preferences/imp.rs` | 20 | Converted transparency percentage label; existing settings bindings retained; persistence handlers stay imperative. |
| `crates/lushtext-core/src/ui/properties_panel/mod.rs` | 11 | Tab metadata projections need source properties; deferred. |
| `crates/lushtext-core/src/ui/search_bar/imp.rs` | 20 | Converted replace-mode revealers; query/options/focus workflow stays imperative. |
| `crates/lushtext-core/src/ui/search_bar/mod.rs` | 13 | Search bar public state and callbacks; imperative. |
| `crates/lushtext-core/src/ui/search_panel/history.rs` | 7 | Search history row state and persistence workflow; imperative. |
| `crates/lushtext-core/src/ui/search_panel/imp.rs` | 33 | Existing options revealer binding retained; search options, actions, and layout workflow imperative. |
| `crates/lushtext-core/src/ui/search_panel/item.rs` | 1 | Result item row state; model/factory candidate, imperative. |
| `crates/lushtext-core/src/ui/search_panel/list_factory.rs` | 20 | Existing row bindings retained with factory unbind; model/factory lifecycle imperative. |
| `crates/lushtext-core/src/ui/search_panel/mod.rs` | 7 | Panel show/close/search orchestration; imperative. |
| `crates/lushtext-core/src/ui/search_panel/replace.rs` | 18 | Replace, backup, undo, and notification workflow; imperative. |
| `crates/lushtext-core/src/ui/search_panel/results.rs` | 8 | Result list and empty-state workflow; imperative. |
| `crates/lushtext-core/src/ui/search_panel/runtime.rs` | 10 | Runtime search worker/progress workflow; imperative. |
| `crates/lushtext-core/src/ui/sidebar/callbacks.rs` | 17 | Sidebar callbacks and workspace scope propagation; imperative. |
| `crates/lushtext-core/src/ui/sidebar/dialogs.rs` | 4 | Dialog action workflow; imperative. |
| `crates/lushtext-core/src/ui/sidebar/file_tree_item.rs` | 1 | File-tree item state; model candidate, imperative. |
| `crates/lushtext-core/src/ui/sidebar/imp.rs` | 7 | Sidebar construction and callbacks; imperative. |
| `crates/lushtext-core/src/ui/sidebar/mod.rs` | 11 | Sidebar public state and workspace projection; deferred/imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/actions.rs` | 8 | Context-menu file actions; imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/dnd.rs` | 10 | Drag/drop state and file mutation workflow; imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/folders.rs` | 16 | Folder rows, focus state, and tree updates; imperative or deferred. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/imp.rs` | 63 | Section setup, signals, menu/factory lifecycle; body/tooltip projections deferred, workflow imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/mod.rs` | 19 | Section public state, callbacks, and refresh; imperative or deferred. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/peek.rs` | 19 | File peek preview and dismissal workflow; imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/refresh.rs` | 17 | Watcher/manual refresh reconciliation; imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspace_section/tree_loading.rs` | 1 | Tree loading model lifecycle; imperative. |
| `crates/lushtext-core/src/ui/sidebar/workspaces.rs` | 9 | Workspace selector and persistence workflow; imperative. |
| `crates/lushtext-core/src/ui/status_bar/imp.rs` | 5 | Status-bar template children setup; deferred/imperative. |
| `crates/lushtext-core/src/ui/status_bar/mod.rs` | 7 | Message/metadata projections; deferred until source properties exist. |
| `crates/lushtext-core/src/ui/window/actions.rs` | 13 | Converted fullscreen/unfullscreen enabled state; other action handlers imperative. |
| `crates/lushtext-core/src/ui/window/dialogs.rs` | 10 | Dialog response and file chooser workflow; imperative. |
| `crates/lushtext-core/src/ui/window/documents.rs` | 58 | Document stack, save/load, status, and tab projection workflow; imperative. |
| `crates/lushtext-core/src/ui/window/drafts.rs` | 4 | Draft restore/persistence workflow; imperative. |
| `crates/lushtext-core/src/ui/window/encoding.rs` | 14 | Encoding and file-health presentation workflow; imperative. |
| `crates/lushtext-core/src/ui/window/focus_indexing.rs` | 9 | Indexing/readiness feedback; imperative. |
| `crates/lushtext-core/src/ui/window/focus_mode.rs` | 18 | Focus-mode shell presentation and geometry; imperative. |
| `crates/lushtext-core/src/ui/window/imp.rs` | 59 | Split-view geometry, settings restore, and breakpoints; layout/readiness imperative. |
| `crates/lushtext-core/src/ui/window/local_history.rs` | 43 | Converted collapsed back-button visibility; snapshot selection/restore workflow imperative. |
| `crates/lushtext-core/src/ui/window/mod.rs` | 5 | Window public state projections; imperative. |
| `crates/lushtext-core/src/ui/window/notes.rs` | 73 | Converted collapsed back-button visibility; note/bookmark preview/save/search workflow imperative. |
| `crates/lushtext-core/src/ui/window/notifications.rs` | 8 | Notification publish/resolve lifecycle; imperative. |
| `crates/lushtext-core/src/ui/window/preview.rs` | 18 | Markdown preview layout and render orchestration; imperative. |
| `crates/lushtext-core/src/ui/window/print.rs` | 4 | Print dialog workflow; imperative. |
| `crates/lushtext-core/src/ui/window/search.rs` | 21 | Workspace search panel wiring and focus workflow; imperative. |
| `crates/lushtext-core/src/ui/window/session_persistence.rs` | 2 | Session persistence workflow; imperative. |
| `crates/lushtext-core/src/ui/window/tabs.rs` | 30 | Tab title/indicator/session projections; pinned icon deferred, session workflow imperative. |
| `crates/lushtext-core/src/ui/window/transient_surfaces.rs` | 2 | Transient dismissal workflow; imperative. |
| `crates/lushtext-core/src/ui/window/workspace_scope.rs` | 5 | Workspace scope propagation; imperative. |
| `crates/lushtext-core/src/ui/window/zoom.rs` | 7 | Zoom label/sensitivity derived projections deferred; zoom workflow imperative. |

## Converted Batch

The converted batch is intentionally limited to direct or pure-derived projections where GTK-native bindings express the existing behavior without changing ownership, action semantics, or workflow order.

| Surface | Treatment | Coverage |
| --- | --- | --- |
| Preferences transparency row | `Adjustment:value` now binds to the percentage `Label:label` through a pure transform helper. The setting write remains imperative because it persists user intent. | `test_transparency_row_updates_setting_and_label` |
| Search bar replace mode | `ToggleButton:active` now binds to the three replace revealers' `reveal-child` properties. Search and replace callbacks remain imperative. | `test_replace_mode_toggle_shows_replace_row` |
| Editor word wrap | `gio::Settings::bind` maps the `word-wrap` setting to `GtkSourceView:wrap-mode`. The minimap refresh side effect remains in the changed handler. | Existing minimap wrap-mode test |
| Editor minimap width | `gio::Settings::bind` maps `minimap-width` to the minimap overlay `width-request` with the existing clamp. The minimap redraw side effect remains in the changed handler. | `test_settings_minimap_width_bounds_overlay_width_request` |
| Window fullscreen actions | `LushtextWindow:fullscreened` now binds to the fullscreen and unfullscreen action enablement, with inversion only for the fullscreen action. | `test_fullscreen_actions_follow_fullscreened_state` |
| Local History browser | `AdwNavigationSplitView:collapsed` now binds to the back button `visible` property. Navigation selection and restore actions remain imperative. | `test_local_history_browser_controls_expose_accessibility_roles` |
| Notes browser | `AdwNavigationSplitView:collapsed` now binds to the back button `visible` property. Note selection, save, and preview behavior remain imperative. | `test_notes_browser_controls_expose_accessibility_roles` |

No `.blp` or generated `.ui` template changed. No action catalog row, D-Bus member, automation snapshot field, readiness predicate, automation client behavior, or scenario helper flag changed.

## Already Declarative

- Search panel advanced options already bind `more_toggle.active` to `options_revealer.reveal-child`.
- Search panel list rows already bind match counts through list-factory setup and unbind them during recycling.
- Existing GSettings-backed preferences rows continue to use the widget or settings machinery appropriate to their Adwaita control.

## Left Imperative

These handlers are intentionally not converted because they are not pure projections:

- GSettings persistence writes in preferences rows, where the handler records user intent or normalizes values before persistence.
- Search panel and search bar query, replace, navigation, result, history, and saved-search behavior, which trigger search work, debounce state, selection, model updates, undo backups, or notifications.
- Status bar notification bus rendering, expiry timers, progress heartbeats, and pulse cleanup, which are ordered workflow effects rather than state mirroring.
- Info bar rendering, announcement, resize, and dismissal logic, which combine severity state, document context, accessibility, and layout work.
- Window geometry, split-view layout, breakpoint readiness, session persistence, tab selection, focus restoration, transient-surface dismissal, and preview orchestration.
- Command palette search mode, query debounce, result activation, file-index rebuilds, and focus restoration.
- Sidebar workspace scope, tree loading, factory lifecycle, context menus, file mutations, watcher refreshes, and workspace persistence.
- Editor style-scheme, dark-mode, opacity, language, monitor, bookmark, local-history, load/save, focus-mode, and minimap marker workflows where handlers invoke redraws, async work, or side effects.
- Markdown preview rendering, dark-mode tag repair, placeholder/content swaps, link activation, code-block width repair, and embedded widget lifecycle.
- `LushtextShrinkableBin` measurement and allocation behavior, which is custom GTK layout code.

## Deferred Candidates

These may become declarative in later phases, but doing so safely requires private derived properties, wrapper objects, or a broader projection model. They were not converted in this change because that scaffolding would add new abstraction before GTK Lush has a real API shape:

- Status bar message, severity, metadata, and tab-dependent properties.
- Command palette mode/result projections.
- Sidebar workspace section collapse/body/tooltip projections.
- Zoom action labels and sensitivity.
- Markdown preview content/placeholder visibility and opacity state.
- Effective EditorConfig formatting projections, where settings and per-file overrides combine.
- Editor estimated memory projections.
- Tab pinned-state indicator icon projection, because the nullable `gio::Icon` transform shares a handler with session persistence.

## Template Notes

No template-level bindings were introduced. The safe conversions all live in Rust-created bindings so current `TemplateChild` contracts, object IDs, widget hierarchy, accessibility metadata, and action wiring remain unchanged. Scratch templates under `.claude/worktrees/` and build outputs were intentionally excluded from the production inventory.

## Review Batches

- Batch 1: preferences, search bar, editor page settings, fullscreen action enablement, Local History back button, Notes back button.
- Batch 2: deferred property-wrapper work after the GTK Lush extraction has a concrete projection API.
- Batch 3: template-level bindings only when a future UI-template change already needs Blueprint or generated `.ui` churn.
