# LushText

> **Load order:** Agents must always load [SOUL.md](./SOUL.md) first and immediately, before reading the rest of [AGENTS.md](./AGENTS.md), so it can inform how the agent should behave while applying the project guidance below.

## Critical: Subagent Approval

Subagents can and MUST ALWAYS be used whenever a skill or command asks for them. Approval to use subagents is already EXPLICITLY GRANTED by default; do not wait for additional per-turn confirmation when a skill or command requires delegation.

A minimalist, fast text editor targeting Libadwaita. Looks similar to GNOME Text Editor but with a left workspace pane, an optional right properties pane, and workspace support.

## Tech Stack

- **Language:** Rust (MSRV: 1.96.0, Edition 2024)
- **GUI:** GTK4 (0.11) + Libadwaita (0.9) + GtkSourceView 5 (0.11)
- **Config:** GSettings (`data/dev.cominotti.lushtext.gschema.xml`)
- **Build:** Cargo workspace + Makefile (dev), Meson (Flatpak/installed)
- **App ID:** `dev.cominotti.lushtext`
- **License:** GPL-3.0-or-later

## Rules Index / Sync Map

Keep this index in sync with `.agents/rules/*.md`. When a new rule file is added or an existing rule is materially changed, update this section in the same change.

- `build.md` — build, dependency, property-testing, test, and smoke command rules
- `documentation.md` — documentation maintenance requirements
- `git.md` — git workflow and commit conventions
- `preexisting-blockers.md` — mandatory no-exceptions rule: fix pre-existing blockers in the same work stream
- `rust.md` — Rust language, module-splitting, and state-grouping conventions
- `ui.md` — UI, theming, state-extreme visibility checks, grouped-row dialog/detail readability, GSettings binding rules, Libadwaita template-validation caveats, adaptive dialog navigation, file-tree DnD/TreeExpander behavior, TextView child-anchor geometry, adaptive bottom-sheet sizing, and GTK paned geometry/animation conventions
- `widget-wiring.md` — GTK widget composition, signal wiring, GTK Lush signal/settle helpers, declarative projection bindings, factory row projection refresh, state-extreme coverage, menu popup lifecycle, tab projection state, live paned-validation, and allocation-frame animation rules

## Architecture

Cargo workspace (application crates, GTK Lush staging crates, and
workspace-hack for cargo-hakari):

- `crates/lushtext-build-support` — tiny build-script helper crate, including the build-only filesystem boundary used by `build.rs` files that cannot depend on runtime services
- `crates/lushtext-core` — all application logic: data models, services, GTK widgets
- `crates/lushtext` — thin binary entry point + integration tests
- `crates/gtk-lush/` — governed `0.0.0` GTK Lush family crates for extracting reusable GTK4/Libadwaita patterns; `signals`, `settle`, `tasks`, `viewport`, `widgets`, `proof-harness`, and `proof-spine` are stable in-tree LushText platform APIs consumed through workspace path dependencies, while publication/graduation remains a dormant future track
- `crates/gtk-lush-adoption-lab` — maintained second-consumer GTK application for adoption validation; it may depend on multiple GTK Lush crates but is not a family crate and must stay outside `crates/gtk-lush/`
- `crates/cargo-gtk-proof` — workspace visual proof tool outside the GTK Lush family; owns schema validation, corpus replay, PNG/policy proof logic, and the default Rust same-session live visual runner
- `workspace-hack` — generated cargo-hakari crate for unified dependency features

### Module Layout (lushtext-core)

```
src/
├── app.rs              # LushtextApplication (AdwApplication subclass)
├── config.rs           # Compile-time constants (APP_ID, VERSION, PKGDATADIR)
├── lib.rs              # Entry point: GResource registration, CSS loading, GSettings schema dir, app.run()
├── model/              # Domain types (no GTK deps)
│   ├── action_catalog.rs # ActionCatalogEntry and automation action metadata value objects
│   ├── automation.rs   # Bounded read-only automation snapshot and readiness value objects
│   ├── workspace.rs    # WorkspaceId, WorkspaceScope, WorkspaceFolder, FolderTreeEntry, WorkspaceConfig, WorkspacesFile
│   ├── session.rs      # SessionTab, SessionData — global session for tab restore
│   ├── palette.rs      # IndexedFile, CommandDef, CommandCategory, SearchMode, ScoredResult
│   ├── draft.rs        # DraftEntry, DraftManifest — draft persistence metadata
│   ├── note.rs         # RichNoteBody, NoteViewMode — shared note-body primitives
│   ├── bookmark.rs     # BookmarkId, BookmarkRecord, BookmarkDocument — saved-file line bookmarks
│   ├── document_note.rs # DocumentNoteDocument — one rich note per saved file
│   ├── local_history.rs # LocalHistoryDocument, LocalHistorySnapshotMeta — per-file snapshot metadata
│   ├── content_search.rs # SearchMatch, ContentSearchOptions, SearchEvent, SearchHistoryEntry — content search types
│   ├── encoding.rs     # DocumentEncodingState, LineEnding, FileHealthFinding, InvisibleCharactersMode
│   ├── sidecar_identity.rs # DocumentSidecarIdentity — canonical-path sidecar keys for notes and history
│   ├── folder_note.rs # FolderNoteIdentity, FolderNoteDocument — one rich note per workspace folder
│   ├── migration_ledger.rs # MigrationLedgerDocument — retry state for post-rename sidecar/history migrations
│   └── formatting_overrides.rs  # FormattingOverrides — per-file EditorConfig overrides
├── services/           # Business logic
│   ├── action_catalog/ # GTK-free action catalog construction, audits, and developer-reference rows
│   ├── bookmark_service.rs # Bookmark sidecar load/save/move/list helpers
│   ├── bookmark_excerpt.rs # Bounded source excerpts for bookmark previews
│   ├── document_note_service.rs # Saved-file document-note load/save/move/list helpers
│   ├── local_history_service.rs # Snapshot capture/list/load/prune/move helpers
│   ├── note_storage.rs # Shared sidecar identity/load/filter helpers for note workflows
│   ├── content_search/ # Workspace-wide grep: streaming search + replace/undo helpers
│   ├── palette/        # Command registry, fuzzy matching, and file indexing
│   ├── draft_service.rs # Draft persistence: save/load/delete draft files and manifest
│   ├── durable_write.rs # Private crash-durable write state machine over the filesystem backend: safe temp perms, metadata-before-final-sync, stable target guards, copy fallback, streaming writes, before/after-rename failure classification
│   ├── editor_io.rs    # Encoding-aware text file load/save helpers, health analysis, mtimes
│   ├── editorconfig.rs # .editorconfig file discovery and parsing (pure I/O, no GTK)
│   ├── file_peek.rs    # Bounded read-only snapshots for sidebar file peek
│   ├── file_limits.rs  # File size thresholds for graceful degradation
│   ├── file_tree.rs    # Directory scanning (pure I/O, bounded/cancellable helpers for sidebar)
│   ├── format_upgrade/ # Sealed inventory, plan, backup, apply, and legacy converter workflow for app-data format upgrades
│   ├── json_store.rs   # Generic JSON load/save + data_dir()
│   ├── notifications.rs # Window-scoped status and inline notification store
│   ├── migration_ledger.rs # Durable retry ledger and startup reconciliation for sidecar/history migrations
│   ├── recovery_metadata.rs # Recovery-aware metadata integrity, quarantine, repair diagnostics, and bounded JSON loading
│   ├── saved_searches.rs # Named saved search persistence: load/save/add/remove (permanent)
│   ├── search_backup.rs # Replace All per-file undo journal persistence within the active safety window
│   ├── search_history.rs # Search history persistence: load/save/dedup (capped at 20)
│   ├── session_service.rs
│   ├── folder_note_service.rs # Workspace-folder note load/save/move/list helpers
│   ├── workspace_manager.rs
│   └── workspace_watch.rs # Materialized-scope filesystem watch service for sidebar auto-refresh
├── benches/
│   └── benchmarks.rs   # Criterion benchmarks for all performance-sensitive services
└── ui/                 # GTK4/Libadwaita widgets (each folder keeps mod.rs + imp.rs)
    ├── automation.rs    # App-owned read-only D-Bus automation adapter, readiness waits, and snapshot collection
    ├── window/          # Main window shell plus workflow modules for actions, documents, drafts, encoding, Focus Mode, local-history, notes, search, preview, startup data preflight, print, session persistence, tab management, transient-surface dismissal, workspace scope, and zoom
    ├── editor_page/     # Per-tab editor adapter plus Focus Mode presentation, local-history capture, minimap, overscroll, invisible-character rendering, bookmark projection, load/save, monitor, and in-tab search helpers
    ├── sidebar/         # Multi-workspace sidebar orchestrator plus dialogs, callbacks, and per-workspace sections
    ├── search_panel/    # Workspace-wide content search panel plus history, list factory, replace, results, and runtime flows
    ├── command_palette/ # Ctrl+P fuzzy search: files + commands
    ├── properties_panel/ # Right-side document metadata + formatting controls
    ├── markdown_preview/ # Read-only Markdown preview (pulldown-cmark → TextTags + anchored GTK widgets)
    ├── info_bar/        # Contextual editor inline alerts above editor
    ├── search_bar/      # Find/replace widget
    ├── status_bar/      # Bottom bar: feedback messages + file metadata
    └── preferences/     # AdwPreferencesDialog, including the Data page for app-data format status
```

## Nested AGENTS.md Files

Use nested `AGENTS.md` files only when a subtree has stable local contracts that would otherwise make this root file churn constantly.

Current nested files:

- `crates/lushtext-core/AGENTS.md` — crate-level layering and ownership for core app logic
- `crates/lushtext/AGENTS.md` — binary crate and test-harness boundaries
- `crates/lushtext-core/src/ui/AGENTS.md` — common GTK driving-adapter rules
- `crates/lushtext-core/src/ui/window/AGENTS.md` — shell/window workflow contracts
- `crates/lushtext-core/src/ui/sidebar/AGENTS.md` — workspace sidebar and section contracts
- `crates/lushtext-core/src/ui/editor_page/AGENTS.md` — per-tab editor, save/load, monitor, and draft-sensitive rules
- `crates/lushtext-core/src/ui/search_panel/AGENTS.md` — workspace search/replace panel contracts

If you add another nested `AGENTS.md`, keep it local, non-duplicative, and worth its maintenance cost. Update this list in the same change.

### Key Design Decisions

- **GtkSourceView owns editing**: language detection, syntax highlighting, style schemes, and undo/redo are all delegated to GtkSourceView, not reimplemented.
- **GSettings for preferences**: Editor settings (word wrap, tab width, line numbers, etc.) are stored in GSettings (`dev.cominotti.lushtext` schema). Preferences rows use two-way `gio::Settings::bind()` where the row value maps directly to the setting. Each `EditorPage` binds simple and pure mapped projections in `constructed()` so setting changes update all editors without manual iteration; formatting values that can be overridden by EditorConfig and workflow side effects such as minimap refresh stay in explicit handlers.
- **Dark mode**: GtkSourceView has its own style scheme system separate from GTK CSS. The base scheme ID is read from GSettings (`style-scheme` key) and the dark variant (e.g., `"Adwaita-dark"`) is selected automatically based on `libadwaita::StyleManager::is_dark()`, with live switching via `connect_dark_notify()`.
- **Tab content transparency**: `Preferences > Editor > Appearance` includes an always-visible `Transparency` control patterned after modern Fedora terminal settings. The selected opacity applies to the main editor document surface and Markdown preview background, while the header bar, tab bar chrome, side panels, status/search chrome, and minimap remain explicitly opaque.
- **Automation spine**: same-user agents and smoke tools discover user operations through the GTK-free `services::action_catalog` contract, mutate state only through normal app/window `org.gtk.Actions`, and observe readiness plus bounded state through the read-only `/dev/cominotti/lushtext/Automation` object implementing `dev.cominotti.lushtext.Automation1`. The reusable client `scripts/lushtext-automation.py` wraps introspection, catalog/snapshot/event reads, readiness waits, catalog-checked action activation, and smoke artifact summaries; keep its commands, statuses, exits, and result fields documented. Use `WaitForReady` with the narrowest named predicate before falling back to broad `WaitForIdle`. Snapshots may expose visible metadata such as paths, titles, counts, and surface state, but never document text, note bodies, draft bodies, local-history contents, complete search result text, or private persistence identifiers. LushText keeps full filesystem permission; portal and sandbox checks remain diagnostics, not a portals-only migration. Any action, D-Bus member, snapshot field, readiness predicate/blocker, automation-client contract, or scenario-helper flag change must update `docs/automation.md` and `docs/automation-reference.md`, then pass `make check-automation-docs` and `make automation-client-self-test`.
- **Multi-workspace sidebar**: The sidebar is a two-level orchestrator. `LushtextSidebar` keeps a fixed workspace-selector row at the top and a scrollable list of `LushtextWorkspaceSection` widgets below it. The selector row uses a `GtkDropDown` with the aggregate scope `All workspaces` plus one item per workspace, alongside the existing add-folder button for creating a new workspace. Selecting `All workspaces` shows every workspace section and acts as the app-wide aggregate workspace scope; selecting a specific workspace narrows the shared workspace scope to that single workspace and hides the others in the sidebar. Each persisted workspace owns an ordered set of zero or more folders and therefore one workspace section; each configured folder renders as one top-level folder tree in stored order. The top selector row stays visible while the workspace list scrolls, and the sidebar no longer exposes a horizontal scrollbar when long workspace or file names exceed the visible width. Workspace sidebar width now lives in `Preferences > Workspace` as `Small`, `Comfy`, and `Large` presets that keep their `20% / 30% / 40%` identity while clamping to a comfortable desktop width on large windows. Clicking the top add-folder button creates a new workspace (auto-named from folder). Each section's header keeps a right-aligned `Refresh` control, and right-click on the workspace header shows Rename/Remove context menu. Zero-folder workspaces render as real sections with an explicit empty folder-set state, while no-workspace startup keeps the fixed top row and an intentionally empty section list.
- **Inner ScrolledWindow pattern**: Each `WorkspaceSection` wraps its `GtkListView` in a `GtkScrolledWindow(propagate-natural-height=true, propagate-natural-width=false, vscrollbar-policy=never, hscrollbar-policy=never)`. `propagate-natural-width` is set to false (and tree labels use `EllipsizeMode::End`) so that deep tree indentation yields to the sidebar width instead of expanding the paned handle horizontally. A drill-down "Focus Folder" affordance lets users temporarily focus the tree on deeply nested paths that become clipped. When focused, other workspaces can be automatically collapsed (controllable via GSettings `workspace-auto-collapse`) and the sidebar scrolls to bring the focused header to the top. Without the inner scroller, GtkListView may crash or emit warnings.
- **Empty folder detection**: `scan_directory_bounded` peeks one level deep into subdirectories (up to a configurable `lookahead_cap`) to detect emptiness. Folders confirmed empty show an `(Empty)` label, hide their expansion arrow, and disable the "Focus Folder" action. This provides immediate visual feedback without the N+1 lookahead bottleneck on large trees.
- **Workspace persistence**: `WorkspacesFile` (model) is loaded from `workspaces.json` via `workspace_manager::load()` on window construction. Sidebar mutations mark persistence dirty, debounce for 150ms, and save via `spawn_blocking_then`, with in-flight serialization so older snapshots cannot overwrite newer ones. `WorkspacesFile` derives `Clone` to enable cloning out of `RefCell` for background save work. The sidebar owns the `WorkspacesFile` state.
- **Workspace auto-refresh**: Each `WorkspaceSection` watches the directories the sidebar has actually materialized, using non-recursive debounced watches for top-level workspace folder rows plus expanded directories via `services/workspace_watch.rs`. Broad configured folders no longer force a recursive startup watch across every descendant; collapsed or not-yet-loaded areas still refresh on demand through the manual `Refresh` button or when their parent directory is expanded. Access-only watcher noise is filtered before it reaches the UI, refreshed child stores are reconciled with bounded `splice()` updates instead of blanking the whole subtree, and manual refresh now keeps the existing `TreeListModel` mounted whenever the visible folder-row set is still reconcilable.
- **File tree uses modern GTK4 model**: `GtkListView` + `GtkTreeListModel` + `GtkTreeExpander` (NOT the deprecated `GtkTreeView`). File tree labels use the `.monospace` CSS class, sharing the editor's font customization provider.
- **File context menu**: Per-`WorkspaceSection`. Right-click on a file or directory shows a `GtkPopoverMenu` with New File, New Folder, Rename, and Delete actions. Uses `Widget::pick()` + ancestor traversal to find the `TreeExpander` → `TreeListRow` → `FileTreeItem`. Actions are in a `section` action group on each section widget.
- **File peek**: Per-`WorkspaceSection`. Press `Space` on a selected sidebar file row to open a `GtkPopover` anchored beside that row without resizing the split layout. The popover renders file name, absolute file path, size, modified time, and either a bounded read-only text sample or an explicit fallback state. Peek stays read-only, updates in place as sidebar selection changes, dismisses on repeated `Space`, `Escape`, click-away, non-file selection, section rebuild, or workspace-filter hide, and promotes through the existing sidebar `file_activated` callback so `open_document()` remains the single duplicate-tab and editor-focus authority.
- **Workspace header context menu**: Per-`WorkspaceSection`. Right-click on the workspace header shows a `GtkPopoverMenu` with Rename Workspace and Remove Workspace. Rename shows an `AdwAlertDialog` with text entry. Remove shows a confirmation dialog. Actions are in a `ws-header` action group.
- **Filesystem boundary**: Production code must use `services::filesystem` for reads, cheap path status, rich metadata, canonical identity, traversal, mutation, sidecar handling, and durable writes. Use `metadata::exists` or `metadata::path_status` for presence/kind probes, and reserve `metadata::file_facts` for callers that need canonical path, size, or mtime. Raw backend APIs belong only in the private filesystem backend or fixture helpers; the private durable-write state machine delegates platform operations through that backend. Specialized read-only engines must be documented and allowlisted, with content search as the approved `ignore`/ripgrep-style traversal adapter while Replace All, undo backup, cleanup, and persistence stay on `services::filesystem`. Tests and benches should use `services::filesystem::fixture` so examples stay readable without teaching bypasses.
- **Inline rename**: Rename swaps the row's `GtkLabel` for a `GtkEntry` dynamically. Enter confirms (removes entry immediately, then the filesystem boundary's durable rename helper runs on a background thread via `spawn_blocking_then`; on success the `FileTreeItem` path and label are updated on the main thread), Escape and focus-out cancel. A guard (`entry.parent().is_none()`) prevents double-fire from focus-out after confirm/cancel removes the entry. The window is notified via `connect_file_renamed` callback for tab path updates.
- **Delete with confirmation**: Delete shows an `AdwAlertDialog` with a destructive "Delete" response. On confirm, filesystem mutation helpers remove the file or directory tree on a background thread via `spawn_blocking_then`; on success the item is removed from the parent `ListStore` and the window is notified via `connect_file_deleted` callback to close affected tabs. Directory operations use `Path::starts_with` for prefix matching (closing all tabs inside deleted/renamed directories).
- **New File / New Folder**: Context menu "New File" and "New Folder" actions use `spawn_blocking_then` for the `create_unique` filesystem call (avoids blocking the UI on slow filesystems or name collision retries), then add a `FileTreeItem` with `pending_rename = true` to the target `ListStore`. `connect_bind` detects the flag to automatically trigger inline rename. On confirm, the temp file is renamed to the user's chosen name and `connect_file_created` opens files in tabs. On cancel, the temp file is deleted via fire-and-forget `std::thread::spawn` and removed from the model. The `workspace_section/tree_loading.rs` helper deduplicates items already in the store to prevent duplicates when an expanded directory's async scan finds the temp file.
- **Workspace concept**: a workspace is one named, ordered folder set persisted in `$XDG_DATA_HOME/lushtext/workspaces.json`, plus a shared current workspace scope that can target one workspace or the aggregate `All workspaces` view. The folder set may be empty. Creating a workspace selects it immediately; removing the selected workspace falls back to `All workspaces` instead of silently rebasing to another concrete workspace.
- **Session persistence**: All open tabs (with cursor position and scroll offset) are saved to a single global `$XDG_DATA_HOME/lushtext/session.json`. Tabs are not workspace-scoped in the UI — they all share one `AdwTabView` — so a single session file captures everything. Session save is debounced at 500ms with `gtk_lush_settle::Debounce` and triggered by tab open, close, switch, and detach. Async and synchronous saves use ordered generations so an older background snapshot cannot overwrite a newer accepted one. A synchronous save runs on `close_request` as a safety net. On startup, `load_session_and_drafts` combines draft manifest + session loading in one background task (via `spawn_blocking_then`), then `restore_tabs` opens file-backed tabs via `open_document` and untitled tabs via `new_tab` with draft recovery. Cursor/scroll positions are deferred via `set_restore_position` → `apply_restore_position` (called in `load_file_async`'s success callback after content is loaded). The `restoring_session` flag suppresses redundant session saves during restore. CLI file arguments (`ApplicationImpl::open`) take priority over session's active tab selection.
- **Status bar**: per-window bottom bar below the shrinkable editor/sidebar shell, always visible. The flexible center shell is wrapped in `gtk_lush_widgets::ClipBin` (`GtkLushClipBin` in templates) so it can clip before pushing the status bar below the visible window at tiny heights. Three sections: workspace sidebar toggle button (far left), a full-width `message_area_box` wrapping `message_label` for feedback messages (left, hexpand, with a small non-flashing start margin after the toggle), and a compact metadata cluster containing the terse `EditorConfig` badge plus the active document's line-ending and encoding entry points. Repeated visible notification updates briefly pulse the full message area wrapper, not just the text, using severity-specific styling scoped so the workspace toggle and metadata controls do not flash. Slower document-inspection details such as file size, formatting source, statistics, and file-health review live in document properties instead of the bottom bar. The window orchestrates document-state updates via `refresh_status_bar()`, which also refreshes the document-properties rows, and notification updates via the window-scoped notification bus.
- **Adaptive document-properties shell**: The outer window shell keeps `workspace_split_view` on the left, while `properties_layout_view` uses Libadwaita layout slots to present the same `LushtextPropertiesPanel` either as the wide `properties_split_view` right pane or the compact `properties_bottom_sheet`. The panel is not manually rehosted between containers. The left pane restores one of three preset identities (`Small=20%`, `Comfy=30%`, `Large=40%`) from `Preferences > Workspace`, then clamps the visible sidebar width to a comfortable desktop range before turning that width back into the effective split fraction. The document-properties toggle lives in the header bar with `info-outline-symbolic` and owns `F9`; the status bar keeps only the workspace toggle. Compact layouts render only one secondary surface at a time, but requested visibility for the workspace sidebar and document properties is preserved so wider layouts can restore both surfaces when appropriate. The properties breakpoint is recalculated from the active left pane's effective visible width whenever the workspace pane actually consumes width so the center editor column stays wide enough for restored-document inline alerts and other editor chrome before the document-properties surface becomes a bottom sheet. Split-view allocation sync is runtime-only: it caches the last allocated width and derived breakpoint threshold, does not rewrite GSettings from animation-frame allocation or notify paths, and only reparses/reinstalls `AdwBreakpoint` conditions when the threshold actually changes.
- **Status bar notification lifecycle**: transient and progress messages live in `services::notifications::NotificationBus` with a 10-second expiry. Generic renders, expiry sweeps, resolves, and search-progress heartbeats do not pulse the bar; publish/update paths pulse only when the expected status message is actually the visible status-bar view. The message-area pulse uses `gtk_lush_settle::SupersedingTimer` so rapid repeated messages restart the CSS animation and older cleanup timers cannot clear a newer pulse.
- **File metadata on EditorPage**: `file_size: Cell<Option<u64>>` is populated during async load through `services::filesystem::metadata` and updated on save from the written byte count. The window pulls this on tab switch via `editor.file_size()`.
- **Window state persistence**: Window width, height, maximized state, workspace sidebar visibility, workspace sidebar width fraction, document-properties visibility, and properties sidebar width fraction are persisted via GSettings. Width/height/maximized still use `connect_notify_local` on their respective properties for incremental persistence. The workspace sidebar width key stores the selected preset hint fraction (`20%`, `30%`, `40%`), which is snapped to the nearest supported preset on restore before adaptive clamping is applied. Visibility keys now store requested desktop intent, while compact layouts may temporarily render only one secondary surface at a time without overwriting that intent. Legacy `sidebar-position` and `sidebar-visible` keys remain only as one-shot migration inputs.
- **CLI file opening**: `ApplicationImpl::open()` is overridden in `app.rs` to handle `HANDLES_OPEN`. File arguments open as tabs via `open_document()`, with window reuse for single-instance behavior.
- **Save As dialog**: `show_save_as_dialog()` in `window/dialogs.rs` uses `FileDialog::save()`. After saving, `set_file_path()` updates the path and re-detects syntax language via `reapply_language()` (gated on `size_check.syntax_enabled()` to avoid re-highlighting large files), then the tab title and status bar are refreshed.
- **Large file handling**: `load_file_async` checks size through the filesystem metadata boundary before reading. Thresholds in `services/file_limits.rs`: >1MB toast, >10MB disable syntax, >50MB disable undo, >500MB refuse. `FileSizeCheck` enum classifies sizes and provides `syntax_enabled()` / `undo_enabled()` queries. Files >50MB keep `begin_irreversible_action()` permanently open (no `end_irreversible_action()`). File reads use the filesystem read boundary plus `simdutf8` for accelerated UTF-8 validation where editor loading needs it, avoiding redundant scalar validation on large documents.
- **Async save**: `save_file_async` uses `services::filesystem::write` on a background thread via `spawn_blocking_then`: probe metadata, create the temp file with safe permissions, write/flush content, apply required metadata, `sync_all()` the temp file after metadata, `rename`, then parent-directory sync. All persistence callers (editor save, streaming `json_store`, drafts, style-scheme writes, Replace All) route through the filesystem write boundary, which preserves identity metadata on overwrite, coordinates in-app writes by stable canonical target rather than replaceable destination inode, keeps symlink-backed saves writing the resolved target instead of replacing the link, and classifies failures as before-rename (previous bytes intact → `SaveError::WriteTemp`, document stays modified) vs after-rename (bytes on disk but directory `fsync` failed → `SaveError::DurabilityUnconfirmed`, surfaced as a distinct durability warning that still keeps the document modified). Save snapshot strategy uses the live buffer size: small/normal buffers snapshot synchronously; large, unknown, untitled, or grown-in-memory buffers snapshot in 64k-char GTK main-loop slices. The view stays read-only for the whole save, duplicate save requests return `SaveInProgress`, and close flows are blocked while any editor is saving. The modified dot is cleared only after the durable write succeeds; on failure the previous modified state is restored. Callers pass a callback for success/error handling. This prevents UI freezes on slow filesystems (NFS, USB) and data loss on crash.
- **GSettings handler disconnect**: `EditorPage` stores application-global `gio::Settings` and `StyleManager` registrations in `gtk_lush_signals::SignalBag`. The bag is cleared during editor disposal so tab open/close cycles do not accumulate stale closures that retain editor state.
- **Tab duplicate detection**: `LushtextWindow` maintains an `open_paths: RefCell<HashSet<PathBuf>>` for O(1) duplicate file detection in `open_document()`. Updated on tab open, close, rename, and save-as. Eliminates O(n²) during session restore with many tabs.
- **GTK4 window resizing in tests**: In GTK4, `set_default_size` does not shrink a window that has already been presented. For widget tests that verify collapse/overlay logic at specific widths, instantiate the window at the target size directly or create a fresh window for the narrow state instead of attempting to resize an existing one.
- **Load cancellation**: `EditorPage` stores an `Arc<AtomicBool>` cancel token. `cancel_load()` sets it; the background work closure checks it before and after the filesystem-boundary read. Both `close-tab` and `close_tab_for_path` call `cancel_load()` before `close_page()`.
- **Focus restoration on overlay close**: When an overlay widget steals focus (command palette, search bar), focus must be explicitly saved before opening and restored after closing. The command palette saves `window.focus()` into a `RefCell<Option<glib::WeakRef<gtk4::Widget>>>` on `LushtextWindow` before `open()`, and `close_command_palette()` calls `restore_saved_focus()` which tries: (1) the saved widget via `WeakRef::upgrade()`, (2) the active editor's `source_view`, (3) `set_focus(None)` (empty state). The search bar always restores to its own editor's `source_view` (no saved state needed). Without explicit restoration, GTK4's default focus traversal after `GtkRevealer.set_reveal_child(false)` walks the widget tree to the first focusable widget, which is typically a sidebar button.
- **Transient surface dismissal**: Window-level transient surfaces share one shell-owned dismissal path in `window/transient_surfaces.rs`. Escape is handled at the window in `PropagationPhase::Bubble` so child popups, dialogs, dropdowns, and focused entries get first chance; if it reaches the shell, one press closes exactly the topmost visible dismissible surface before falling through to Focus Mode exit. Command-palette click-away is also owned by the shell: primary presses outside the palette bounds call `close_command_palette()` and claim the sequence, while inside palette controls, result rows, scrollbars, and child popup roots keep their own interaction.
- **Search debounce**: Command palette uses a 150ms `gtk_lush_settle::Debounce` in `setup_search`; empty queries clear immediately, while pending non-empty searches no-op when superseded. `rebuild_file_index` uses the same helper at 300ms to coalesce rapid workspace mutations before spawning the background rebuild.
- **SIMD fuzzy matching**: `fuzzy_score` and `search_items` use `nucleo-matcher` (SIMD-accelerated via AVX2/NEON) instead of hand-rolled scalar scoring. `search_items` reuses a single `Matcher` and char buffer across all candidates. Top-N results use `collect` + `sort_unstable_by` + `truncate(max)` for simplicity (max=50 is fixed and small).
- **Inline alerts**: Per-`EditorPage` `LushtextInfoBar` widget containing one `GtkRevealer` alert row built from supported GTK widgets. It renders editor-scoped error and warning alerts above the editor overlay for access errors, restored drafts, external changes, mixed line-ending normalization, and local-history restore undo. The row starts hidden with `reveal-child=false` and `visible=false`, becomes visible only while an inline notification is active, and uses Adwaita semantic warning/error CSS variables rather than deprecated `GtkInfoBar` message-type styling. Communication uses direct rendering plus callback connectors (`connect_save()` / `connect_discard()` / `connect_retry()` / `connect_dismissed()`), following the StatusBar pattern. Titles, subtitles, and action labels must remain narrow-safe by wrapping instead of disappearing, and Save/Discard widths stay balanced so restored-document banners remain usable during horizontal resize. Workflow buttons and dismiss share one trailing horizontal action row, dismiss is ordered last, and button contrast is scoped through `.editor-inline-alert .inline-alert-button` rather than a broad nested-button selector. The message (`message_box`) and that action row (`actions_box`) are the two children of an `AdwWrapBox` (`content_wrap`): they share one line with the actions pinned trailing (`justify=spread`) when the editor column is wide, and the action row wraps as one atomic unit onto its own row beneath the message when the column is narrow. `AdwWrapBox` is `ensure_type()`-registered in `class_init` before `bind_template()`.
- **File monitoring**: Each `EditorPage` with a file path gets a `gio::FileMonitor` (started in `open_document`, cancelled on tab close/dispose). The `changed` signal is debounced with a 500ms `gtk_lush_settle::Debounce`, and the same token guards the async mtime completion. After debounce, the file's mtime (stored as `Cell<Option<u64>>` epoch seconds) is compared against `last_known_mtime` — only shows the "File Has Changed on Disk" bar when mtime actually differs. Mtime is updated on load success and save success to prevent our own writes from triggering the bar.
- **Draft persistence**: Unsaved buffer content is periodically written to `$XDG_DATA_HOME/lushtext/drafts/` as plain UTF-8 text files. A JSON manifest (`manifest.json`) maps draft IDs to original paths and metadata (mtime, saved_at). Draft IDs for path-backed files use `std::hash::DefaultHasher` (SipHash) → 16-char hex string; untitled tabs use `"untitled-{counter}"`. A global 5-second `glib::timeout_add_local` timer on the window scans all tabs, writing drafts only for those where `is_modified() && draft_dirty && !is_evicted()`. The `draft_dirty` flag is set by `connect_changed` (fires on every text mutation) and cleared after each draft write. On file open, the manifest is checked for an existing draft — if found, draft content replaces the buffer and the "Draft Changes Restored" bar is shown. Draft cleanup: deleted on explicit save, on discard, and on clean tab close; preserved on crash for recovery. Orphan cleanup runs at startup.
- **Save-changes dialog**: `AdwAlertDialog` matching GNOME Text Editor's design, shown on tab close (via `AdwTabView::connect_close_page`) and window close (via `WindowImpl::close_request`). Heading: "Save Changes?", body explains permanent loss. Buttons: Cancel (neutral), Discard/Discard All (destructive), Save (suggested). For multiple unsaved files, an `AdwPreferencesGroup` with `AdwActionRow` + `GtkCheckButton` per file lets the user select which to save. Unchecked files are not saved but their drafts are still deleted (same as discard). On Save, all checked files are saved via `save_file_async` with a pending-count to fire the completion callback after the last save finishes. On Discard, all drafts are deleted immediately. Tab close uses `close_page_finish(confirmed)` to complete or cancel the Adwaita close-page flow, and close requests are cancelled while a save is already in progress. Window close returns `Propagation::Stop` to inhibit and calls `window.destroy()` after confirmation. The `close-tab` action delegates entirely to `tab_view.close_page()` — cleanup (open_paths, monitor, memory) is handled by the `page_detached` handler.
- **Buffer eviction**: When total estimated buffer memory exceeds 256MB (`BUFFER_MEMORY_BUDGET`), unmodified background tabs are evicted on tab switch. Memory estimation uses `file_size * 2` to account for GtkTextBuffer overhead (B-tree + line index + undo stack). `EditorPage::evict()` sets `evicted=true` first (to prevent `modified-changed` signal flash), then clears buffer text via irreversible action. `reload_if_evicted()` transparently reloads evicted tabs when re-focused. The eviction loop computes a running total inline (O(n)) instead of rescanning all tabs after each eviction.
- **ListStore splice**: Both command palette results and file tree children use `gio::ListStore::splice()` for batch updates (single `items-changed` signal) instead of per-item `append()` loops.
- **Directory entry cap**: `workspace_section/tree_loading.rs::build_children_model` caps entries at 10,000 per directory, appends rows in 256-item batches, and shows a placeholder row when truncation occurs so very large folders do not stall the GTK thread or silently clip results.
- **File index cap**: `FileIndex::rebuild` skips well-known build/dependency directories during scanning (`IGNORED_INDEX_DIRS`: `node_modules`, `target`, `__pycache__`, `venv`, `vendor`) and truncates at 100,000 files with a warning log as a safety net. The skip list applies only to the palette file index, not the sidebar file tree.
- **Arc workspace_folder**: `IndexedFile.workspace_folder` uses `Arc<PathBuf>` — files in the same indexed workspace folder share one allocation instead of cloning per file.
- **EditorConfig support**: Per-file formatting overrides via `.editorconfig` files. The service (`services/editorconfig.rs`) walks the directory tree from the file's parent upward, parses each `.editorconfig` with the `editorconfig-parser` crate (pure Rust, zero deps), and returns a `FormattingOverrides` struct (model layer). Resolution runs on a background thread via `spawn_blocking_then`. The `EditorPage` stores overrides in `Cell<FormattingOverrides>` and uses `apply_formatting_settings()` to resolve EditorConfig vs GSettings: override wins when `Some`, GSettings fallback when `None`. This replaces the previous `Settings::bind(GET)` for `tab-width` and `insert-spaces-instead-of-tabs` with manual `connect_changed` handlers. A `use-editorconfig` GSettings toggle (default: `true`) enables/disables the feature. The status bar shows an "EditorConfig" label when overrides are active. Supported properties: `indent_style`, `tab_width`, `indent_size`. Deferred properties documented in `docs/next/editorconfig-future.md`.
- **Bookmarks and rich notes**: Notes are available only for saved files or explicit workspace folders and persist as sidecar JSON under the app data directory. `BookmarkDocument` and `DocumentNoteDocument` use `DocumentSidecarIdentity` with a hash of the canonical path, so Save As starts a new file-backed note identity while in-app sidebar renames migrate existing sidecars. Folder notes use a canonical-folder identity instead of a transient workspace slot ID, so removing and re-adding the same folder restores the same folder note, while adding a different folder uses that folder's own note identity. Open editors project bookmarks as `GtkSourceMark` gutter icons. Document and folder notes share markdown-capable edit/render surfaces, and the unified notes browser uses `AdwSidebar` sections for workspace-scoped bookmarks, folder notes, document notes, plus a supplemental `Open Tabs` section for saved open files outside the current workspace scope. Untitled tabs surface explicit feedback instead of silently creating note state.
- **Local history**: Saved files keep a separate local-history lineage under `$XDG_DATA_HOME/lushtext/local-history/`, keyed by the same canonical-path identity pattern used by note sidecars. Automatic capture records the pre-edit baseline, periodic modified-session snapshots every five minutes, and successful saves on background threads; files above 10 MB fall back to save-boundary capture only, and files above 50 MB disable local history entirely. File-backed draft restore is treated as continuity of unsaved work, so reopening a file with restored draft content does not mint a fresh baseline row for stale on-disk contents. The browser lives in `window/local_history.rs` as an adaptive `AdwDialog` + `AdwNavigationSplitView` with an `AdwSidebar` snapshot rail, opens from the main menu, command palette, `Ctrl+Alt+L`, the sidebar file context menu, and the editor content context menu, and sizes itself from the parent window so wide layouts feel like a large viewer while the preview keeps the majority of the side-by-side width. Empty-state browsing stays compact, valid empty snapshots get their own explanatory preview state instead of a blank pane, and legacy stale-disk empty baseline rows from older history may be hidden from the browser while their stored data remains on disk. Explicit inner preview padding keeps text from rendering flush against the scroll frame. Save As starts a fresh lineage, sidebar renames migrate lineages, and restore always captures a safety snapshot before replacing the buffer plus surfacing an `Undo Restore` info-bar action.
- **Benchmark framework**: Criterion.rs benchmarks in `crates/lushtext-core/benches/benchmarks.rs` cover all performance-sensitive service code (fuzzy search, file indexing, directory scanning, JSON persistence). All benchmarked functions are GTK-free. `FileIndex::from(Vec<IndexedFile>)` enables synthetic index construction without filesystem I/O. `scripts/bench-report.sh` parses Criterion JSON output into markdown for GitHub release assets. CI compile-checks benchmarks on every PR; full benchmark runs happen on release tags.
- **Content search panel**: Ctrl+Shift+F toggles `LushtextSearchPanel` (open if closed, close if open), a workspace-wide grep panel below the content stack. It searches the shared current workspace scope: the selected workspace's folder set or the aggregate `All workspaces` folder coverage. Uses `GtkRevealer(slide-up, 250ms)` for animated show/hide. A top `GtkSeparator` (not CSS `border-top`) provides the visual divider so the separator animates cleanly with the revealer transition. The panel is **compact when empty**: `results_scroll` starts `visible=false` (no `vexpand`) and is shown when the first match arrives, hidden again on `clear_results()`. This keeps the panel a thin strip (header + footer only) until results populate it. A close button (`window-close-symbolic`, flat + circular) sits to the right of the save button in the header for explicit panel dismissal; Escape on the search entry also closes. Ctrl+F (begin-search) and Ctrl+H (begin-replace) close the search panel first (with 260ms delay for animation completion) before opening the in-editor Find/Replace bar. The service (`services/content_search/search.rs`) spawns a background thread with `ignore::WalkParallel` (same crate powering ripgrep) + `grep-searcher`/`grep-regex` for fast parallel file searching, while `services/content_search/replace.rs` owns replace/undo flows. Results stream to the UI via `crossbeam_channel` (bounded, 1024 items), polled by a 50ms `glib::timeout_add_local`. Results are grouped by file in a two-level `GtkTreeListModel` (file → matches). Search options: case-sensitive, regex, whole-word (toggle buttons in the header), plus .gitignore toggle and glob filter in an expandable options revealer. GSettings keys: `search-panel-visible` (b), `search-panel-position` (i), `search-case-sensitive` (b), `search-regex` (b), `search-whole-word` (b), `search-panel-options-expanded` (b), `search-gitignore` (b). Match highlighting uses Pango markup (`<b>` tags) with proper escaping. Line content is truncated at 500 chars with ellipsis. Result cap: 10,000 matches (approximate under parallel walkers). Search panel integration logic extracted to `window/search.rs`.
- **Match navigation**: F4 (`win.search-next-match`) and Shift+F4 (`win.search-prev-match`) cycle through search results across files. `match_positions: RefCell<Vec<(PathBuf, u32)>>` maintains a flat navigation index in match arrival order, separate from the hierarchical `TreeListModel` display model. `current_match_index: Cell<Option<usize>>` tracks position. Navigation triggers `navigate_callback` to open the file at the matching line (shared `open_file_at_line` helper in `window/search.rs`), and visually selects the corresponding row in the `SingleSelection` model via O(n) scan + `ListView::scroll_to`. Actions are disabled when: no tabs open, search panel not visible, or no results — controlled by `update_search_navigation_actions()`. Navigation resets on new search via `clear_results()`.
- **Search progress reporting**: `SearchEvent::Progress(usize)` is emitted every 100 files by the search service via an `Arc<AtomicUsize>` file counter shared across `WalkParallel` threads. Best-effort via `try_send` to avoid blocking match delivery. In `window/search.rs`, progress display uses a 500ms delay before the notification bus starts rendering `"Searching X files…"` messages in the status bar, and a 1-second heartbeat renews the progress lease until the search completes or is cancelled.
- **Search history**: Recent searches (capped at 20) are persisted to `$XDG_DATA_HOME/lushtext/search-history.json` via `json_store` atomic write. Each `SearchHistoryEntry` captures query, case_sensitive, regex, whole_word, gitignore, and glob. History is saved on `SearchEvent::Done` when `total_matches > 0` (no-result searches are not recorded). Deduplication moves identical entries to the top. A `GtkPopover` + `GtkListBox` dropdown appears on search entry focus (created programmatically, not via template, because `GtkPopover` needs `set_parent()` not box child semantics). Each `AdwActionRow` shows the query and a compact toggle summary (`"Aa .* *.rs"`). Selecting an entry restores all state (query, toggles, glob) and triggers immediate search, using a `restoring_history: Cell<bool>` guard to suppress the redundant searches that `set_text()` and toggle `set_active()` would otherwise trigger. History is loaded at startup via `spawn_blocking_then` in `window/search.rs::setup_search_panel()`. Missing/corrupt files gracefully return empty history.
- **Saved searches**: Named saved searches are persisted permanently to `$XDG_DATA_HOME/lushtext/saved-searches.json` via `json_store` atomic write (separate file from history per architecture Decision 6). Each `SavedSearch` captures a user-given name, query, and all toggle states (case, regex, word, gitignore, glob). No cap (permanent until explicitly deleted), no dedup (user-named entries may duplicate). Service: `services/saved_searches.rs` (load, save, add, remove). UI: "Save Search" button (bookmark icon) appears in the search panel header when results exist (hidden during preview mode). Save dialog: `AdwAlertDialog` with `GtkEntry` pre-filled with query text. Dropdown popover restructured from flat `ListBox` to two-section layout: "Saved Searches" section (with delete buttons) above "Recent" history section. Selection restores all state and triggers immediate search, reusing the `restoring_history` guard. Saved searches loaded at startup via parallel `spawn_blocking_then` alongside history.
- **Multi-file Replace All**: Replace UI lives inside the `options_revealer` (behind "More" toggle). Two-phase flow: (1) user clicks "Replace All" → `enter_preview_mode()` generates `Replacement` previews via `generate_replacement_preview()` (pure function in `model/content_search.rs`), results list switches to show before/after with per-match `GtkCheckButton`; (2) user clicks "Confirm Replace" → checked replacements sent via `replace_callback` to window, which filters out modified open tabs (`skip_paths`), then calls `apply_replacements()` via `spawn_blocking_then`. Service function groups by file, acquires the same stable target write guard as editor save before reading/writing each target, skips files above `10 * 1024 * 1024` bytes, skips later files when undo payload would exceed `64 * 1024 * 1024` bytes, validates UTF-8 with `simdutf8`, builds replacement output from byte ranges without a full owned-line vector, durably writes one per-file undo journal entry before mutating that file, and writes atomically through the shared durable-write helper. After replace: status bar shows summary, open non-modified tabs auto-reload via `load_file_async()` with `last_known_mtime` updated to suppress file monitor. Undo: `undo_replacements()` restores files under the same stable target guard and keeps skipped/failed entries retryable. Journal state is cleared on search-panel close, successful undo, and startup so stale undo data cannot outlive the active safety window. Regex mode: `regex::RegexBuilder` compiles query, `Captures::expand()` handles `$1`/`$2` backreferences.
- **Markdown preview**: `LushtextMarkdownPreview` widget uses `pulldown-cmark` (CommonMark parser) → `GtkTextTag` rendering on a read-only `GtkTextView`, with `GtkTextChildAnchor` widgets for native tables, local image blocks, and embedded GtkSourceView code blocks. Supported preview behavior includes activatable links, tight and loose ordered/unordered list row flow with nested hanging indents, task lists, generic blockquote rails with depth-aware indentation, GitHub alert callouts, footnotes, Markdown tables, syntax-highlighted fenced code blocks with plain fallback for unresolved languages, and explicit fallback states for missing or remote Markdown images. Generic blockquote rails are rendered as preview glyphs with depth-specific text tags because `GtkTextTag` does not provide a clean per-paragraph left-border primitive; typed GitHub alert callouts stay on their distinct alert tag path. The preview shell is an Adwaita-native `AdwMultiLayoutView` in the "tabs" stack page: the normal editor layout hosts `editor_box` as the content slot of `preview_split_view` and the same Markdown preview widget as an end sidebar slot, while preview-only layout places the preview slot as full content. Three states are state-driven through actions: editor-only (default, preview hidden), side-by-side (`toggle-preview-pane` / `set-preview-pane-visible`, `preview_split_view.show-sidebar=true`, clamped to max 1/3 of the current content width), and preview-only (Alt+P `toggle-preview-mode` / `set-preview-mode`, editor hidden). The legacy `preview-pane-position` key is retained as preferred side-by-side preview width, not a live divider coordinate, and side-by-side preview still resets hidden on startup. The compatibility readiness blocker `preview-animation` now means preview layout switching or embedded code-block width repair is still settling; no custom paned animation owns preview transitions. Markdown detection uses GtkSourceView language ID (`"markdown"`). Preview refreshes on tab switch and on buffer changes with a 300ms `gtk_lush_settle::Debounce`. TextTags use Adwaita-matching color constants (`#1c71d8`/`#78aeed` accent, `#f6f5f4`/`#3d3846` code bg, `#5e5c64`/`#9a9996` dim), switched by `StyleManager::connect_dark_notify()`; embedded code blocks use the active GtkSourceView style scheme and share one resolved background across their outer block and inner source text area. GSettings keys: `preview-pane-position` (i, legacy preferred width), `preview-pane-visible` (b). Preview logic lives in `window/preview.rs` so the main window module stays focused on shell orchestration. Repo-owned sample content lives under `samples/`; `samples/markdown-test.md` is the canonical showcase for the Markdown preview features currently supported and should be updated when shipped preview behavior changes.

## Build Commands

```
make dev-tools   # Flatpak deps + GTK debug input/screenshot helpers
make build       # Release build
make build-debug # Debug build
make run         # Debug build + force a fresh run with temporary GNOME desktop staging
make run-format-upgrade-newer-manual-test # Launch with isolated future-version app data
make run-format-upgrade-older-manual-test # Launch with isolated upgradeable old-version app data
make refresh-dock-icon # Regenerate app icon assets + force a fresh GNOME Shell dock icon reload
make test        # All tests (unit + integration + widget)
make test-unit   # Unit tests only
make test-int    # Integration tests only
make test-widget # Widget tests with the private headless runner
make test-widget-headless # Widget tests with the CI mutter/dbus path
make visual-smoke # Real-session screenshot smoke with artifacts
make crash-recovery-smoke # Real-process SIGKILL/relaunch recovery smoke with artifacts
make portal-sandbox-smoke # Available Flatpak/Snap confinement diagnostics
make accessibility-smoke # AT-SPI-enabled accessibility smoke
make performance-smoke # Lightweight Criterion performance smoke
make automation-client-self-test # Reusable D-Bus automation client self-test
make end-user-smoke # Run all host-supported end-user smoke lanes
make check       # fmt + all-feature Clippy + fast policy audits
make blueprint-generate # Regenerate generated GtkBuilder .ui files from Blueprint sources
make check-blueprint # Validate Blueprint drift and generated UI template contract
make lint-blueprint # Advisory grouped Blueprint lint triage
make lint-advisory # grouped advisory Rust lint discovery
make pre-commit  # repo pre-commit gate (fmt + all-feature Clippy + policy audits)
make install-git-hooks
make clean       # Remove build artifacts

# Benchmarks
make bench             # Run Criterion benchmarks
make bench-report      # Run + generate markdown report (short)
make bench-report-full # Run + generate markdown report (full sampling)
make bench-baseline    # Save current results as baseline
make bench-compare     # Compare against saved baseline

# Packaging
make meson-build     # Meson release build (installed layout)
make flatpak-deps    # Install Flatpak runtime/SDK deps into the user installation
make flatpak         # Build Flatpak (sets up missing runtime/SDK deps)
make flatpak-install # Build and install Flatpak into the user installation
make cargo-sources   # Regenerate cargo-sources.json
make cominotti-flatpak-repo VERSION=v0.2.0 # Generate Cominotti Flatpak repo artifacts
make verify-cominotti-flatpak-repo # Verify Cominotti Flatpak repo metadata
make test-cominotti-flatpak-repo # Test Cominotti Flatpak repo tooling
make flathub-manifest VERSION=v0.2.0 # Generate Flathub tag-based manifest
make verify-flathub-manifest # Verify generated Flathub manifest invariants
make verify-flathub-domain   # Verify cominotti.dev Flathub verification endpoint
make release-bump TYPE=patch DRY_RUN=1 # Preview next release version
make snap            # Build the Snap (LXD); GATED on the GNOME 50 platform snap
make snap-smoke      # Confined smoke test of the built Snap (skips if unavailable)
make verify-snap-identity # Verify Snap confinement, plugs, and common-id
make snap-store-readiness # Check Snap Store/platform gates without mutating them
```

LushText ships as a Flatpak and is being prepared as an Ubuntu Snap
(`snap/snapcraft.yaml`). The Snap reuses the Meson/Cargo build, uses strict
confinement + portals, and is gated on the `core26` / GNOME 50 platform snap
(GTK 4.22) — see `.agents/rules/build.md` (Snap section). It will release
Unlisted on the `edge` channel.

## Build Optimizations

Replicated from invowk-rust:

- `[profile.dev] debug = "line-tables-only"` — smaller debug info, faster linking
- `[profile.dev.package."*"] opt-level = 2` — deps compiled at O2, cached
- `[profile.dev.build-override] opt-level = 3` — build scripts at full optimization
- `[profile.release] lto = "thin", strip = true, codegen-units = 1`
- **rust-lld linker** — default on x86_64-linux since Rust 1.90, ~10x faster than BFD, zero configuration
- **cargo-hakari** workspace-hack for unified dependency features
- **cargo-nextest** auto-detected for parallel non-widget execution across the workspace; `.config/nextest.toml` excludes the `widget` binary from the default nextest filter, and `make test` drives widget coverage through the shared headless runner for deterministic CI parity. Widget tests must never run against the developer's live desktop session.

## Testing

- Unit tests: `#[cfg(test)]` modules across models, services, and selected GTK-free UI helper modules
- Integration tests: `crates/lushtext/tests/integration.rs` with `#[path]` split binary pattern
- Widget tests: `crates/lushtext/tests/widget.rs` uses a custom single-threaded harness so GTK tests stay on one stable thread for the life of the process. The harness is Cargo-visible by default, but non-list executions self-supervise into a private `mutter --headless` session before GTK initializes; `scripts/run-widget-tests.sh`, `make test-widget`, and `make test-widget-headless` are all headless-only. The runner defaults to `GSK_RENDERER=cairo` to avoid headless Mesa/EGL device-probe warnings in CI while still allowing explicit renderer overrides. Presented widget tests do not reliably advance `AdwTimedAnimation` frame clocks, so animation-dependent assertions should use deterministic end-state checks or a narrow `LUSHTEXT_WIDGET_CHILD` immediate-completion path.
- End-user smoke lanes: `docs/end-user-coverage.md` maps which behavior belongs in unit, integration, property, fuzz, widget, automation client, automation smoke, visual, crash-recovery, portal/sandbox, accessibility, performance, and mutation lanes. Use `make automation-client-self-test` for the reusable D-Bus client/parser contract, and use `make visual-smoke`, `make crash-recovery-smoke`, `make portal-sandbox-smoke`, `make accessibility-smoke`, and `make performance-smoke` when the widget harness cannot prove rendered desktop, real-process recovery, confinement, AT-SPI, or user-visible latency behavior; those scripts must preserve artifacts and skip explicitly when host support is unavailable.
- `TestContext` struct: isolated tempdir with simulated XDG data directory
- Non-widget tests exercise models, services, and GTK-free helper code with no display server required; widget tests cover real GTK flows and require a display server plus the custom harness

## GObject Subclassing Pattern

Every custom widget follows the gtk4-rs two-module pattern:
- `mod.rs` — public wrapper type via `glib::wrapper!`, public API methods
- `imp.rs` — private struct with `#[derive(CompositeTemplate)]`, trait impls

Required trait chain for `AdwApplicationWindow`:
`ObjectSubclass` → `ObjectImpl` → `WidgetImpl` → `WindowImpl` → `ApplicationWindowImpl` → `AdwApplicationWindowImpl`

Child widget types must be registered via `ensure_type()` in `class_init()` before template parsing.

## Dependencies

All versions centralized in the repository-root `Cargo.toml` under `[workspace.dependencies]`.

**Critical version alignment:** All gtk-rs crates must be from the same release series. For the 0.11 cycle: `gtk4 = 0.11`, `libadwaita = 0.9`, `sourceview5 = 0.11`, `glib/gio/pango = 0.22`, `glib-build-tools = 0.22`.

## Rust Linting Policy

The blocking Rust lint gate is curated and all-featured:
`cargo clippy --workspace --all-targets --all-features -- -D warnings`.
Local `make check`, `.githooks/pre-commit`, and CI must stay aligned with that
command, and `make check` also runs fast filesystem-boundary and Blueprint
template drift/contract audits. Broad
Clippy groups (`restriction`, `pedantic`, `nursery`, and `cargo`) remain
advisory discovery inputs, not blanket blocking policy; run `make
lint-advisory` when reviewing lint drift after a toolchain or dependency update.
Dependency policy is `cargo deny check advisories bans sources licenses`.
Use narrow `#[expect(..., reason = "...")]` exceptions only after reviewing the
local GTK, generated-code, test, or benchmark invariant.

## Meson / Flatpak Build

Meson wraps Cargo for installed/Flatpak builds. `build-aux/cargo.sh` bridges Meson → Cargo.

- **GResource dual-path**: Meson compiles and installs `.gresource` to `$(pkgdatadir)/`. `cargo.sh` exports `LUSHTEXT_PKGDATADIR` env var. `config.rs` reads it via `option_env!()`. `lib.rs` loads from installed path first, falls back to `include_bytes!` (dev). UI templates are authored in `resources/ui/*.blp`, but committed generated `resources/ui/*.ui` files remain the resource inputs for Cargo, Meson, Flatpak, and Snap; `blueprint-compiler` is a contributor/CI regeneration and drift-check tool only. Unknown Blueprint compile warnings are blocking, while `make lint-blueprint` is a curated advisory gate that keeps promoted diagnostics clean and bounds accepted advisory findings by rule and file.
- **GSettings**: `data/meson.build` installs schema to system path. `gnome.post_install()` compiles schemas. `build.rs` skips schema compilation when `LUSHTEXT_PKGDATADIR` is set.
- **Flatpak manifest**: `build-aux/dev.cominotti.lushtext.Flatpak.json` for local builds. `cargo-sources.json` (same dir) vendors all Cargo dependencies for offline builds.
- **Flatpak release lane**: `scripts/release.sh` owns release version computation, AppStream release-note insertion, metadata validation, release commit creation, and signed tag creation. Real releases require `RELEASE_NOTES_FILE` and a clean `main`; use `make release-bump TYPE=patch DRY_RUN=1` before mutating anything. The primary Flatpak publication channel is the Cominotti-owned remote at `https://flatpak.cominotti.dev/`: `scripts/generate-cominotti-flatpak-repo.sh` produces a signed repository plus `cominotti.flatpakrepo` and `lushtext.flatpakref`, while `scripts/verify-cominotti-flatpak-repo.sh` checks metadata, app refs, and manifest invariants. `scripts/verify-cominotti-pages-limits.sh` enforces Cloudflare Pages static asset size and file-count limits before the default Pages deployment; `COMINOTTI_FLATPAK_DEPLOY_COMMAND` remains an override, and Cloudflare R2 is the first fallback when Pages limits are exceeded. Public Cominotti install docs must not use `--no-gpg-verify`. The local Flatpak manifest remains checkout-based. `scripts/generate-flathub-manifest.sh`, `scripts/verify-flathub-manifest.sh`, and `scripts/verify-flathub-domain.sh` remain for optional Flathub handoff; linked GitHub accounts do not verify this custom-domain app ID, and Flathub publication defaults to a reviewable PR, not automerge.
- **Snap manifest**: `snap/snapcraft.yaml` reuses the same Meson/Cargo build via the `meson` plugin. A `layout:` bind-mounts the baked `LUSHTEXT_PKGDATADIR` into `$SNAP`, so the GResource/GSettings dual-path needs no Rust changes. Strict confinement + portals. GATED on the GNOME 50 platform snap (GTK 4.22); not buildable until `base:` → `core26`. No `cargo-sources.json` needed (snap builds are online). `make snap-store-readiness` checks the external store/platform gates without mutating Snap Store state and exits nonzero while those gates remain pending. See `.agents/rules/build.md`.
- **CI**: `.github/workflows/ci.yml` now covers rustfmt, all-targets/all-features Clippy, the filesystem-boundary audit, Blueprint template drift/contract validation, the GTK Lush family policy, the rustdoc lint gate, non-widget tests, GTK Lush doctests/examples/MSRV/API snapshots, property tests, fuzz corpus replay, widget tests, benchmark compilation, and `cargo deny check advisories bans sources licenses`; `.github/workflows/flatpak.yml` owns Flatpak build validation; `.github/workflows/release-dry-run.yml` exercises release helper behavior, current release metadata validation, Flathub manifest generation, and Cominotti repository metadata generation; `.github/workflows/release.yml` validates `v*` tag releases, builds the Flatpak, prepares/deploys Cominotti Flatpak repository artifacts when signing/deploy configuration exists, creates/updates the GitHub Release, and opens an optional Flathub PR when configured; `.github/workflows/snap.yml` validates `snapcraft.yaml` (always) and builds/publishes to `edge` (gated on the `SNAP_PLATFORM_AVAILABLE` variable).

## GTK Initialization Order

CSS and Display access require GTK to be initialized. Initialization happens during `app.run()` → `startup()`. GResource registration can happen before (in `run()`), but CSS loading must happen in the `startup()` callback.

## Async I/O Pattern

Background I/O uses `gtk_lush_tasks::spawn_blocking_then(state, work, then)`:
1. `state` — non-Send GTK object, wrapped in `ThreadGuard` automatically
2. `work` — `FnOnce() -> T + Send`, runs on a background thread
3. `then` — `FnOnce(state, T)`, runs on the main thread via `glib::idle_add_once`

Both `state` and `then` are wrapped in `glib::thread_guard::ThreadGuard` by `gtk-lush-tasks` to safely cross thread boundaries. `ThreadGuard` implements `Send` and asserts same-thread access on `.into_inner()`.

**Concurrency guard:** `spawn_blocking_then` limits outstanding work to `DEFAULT_MAX_CONCURRENT_SPAWNS = 8` via a global `AtomicUsize` counter. The slot stays held until the GLib idle completion consumes the result, so large completed results cannot pile up outside the cap. When at limit, work waits in a GTK-main-thread FIFO and starts when a slot releases.

**Fire-and-forget pattern:** For tiny non-critical cleanup with no main-thread callback (for example temp file cleanup after a failed inline-create flow), raw `std::thread::spawn` is acceptable. Persistent app state writes should still go through `spawn_blocking_then` so they respect the concurrency guard and serialize correctly.

**Atomic JSON writes:** `json_store::save` streams pretty JSON into the shared durable-write helper: safe temp creation, metadata application, final temp-file sync, `rename`, and parent-directory sync. The file is either fully old or fully new, never partially written, and the renamed directory entry is durable before success is reported.

**Key constraint:** GTK objects are NOT `Send`/`Sync` (raw pointers inside). Never pass them directly across threads. Always use `ThreadGuard` or `SendWeakRef`.

**TreeListModel caveat:** Never set `autoexpand = true` on `TreeListModel` — it recursively calls the child-model callback for every directory, which with background I/O spawns unbounded threads, and with synchronous I/O freezes the UI.

## Critical Rule: Pre-existing Blockers

If implementation or verification reveals a pre-existing blocker, fix it in the same work stream instead of deferring around it or treating it as out of scope.

This rule is mandatory and has no exceptions.

## Active Technologies
- Rust 1.96.0 (Edition 2024) + GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22, existing `spawn_blocking_then` background executor (001-file-peek)
- Local workspace files for read-only snapshot reads; transient in-memory peek state only; no new XDG, draft, session, or GSettings persistence (001-file-peek)

## Recent Changes
- automate-flathub-releases: Added an Invowk-style release command surface, release metadata synchronization, generated Flathub manifest updates, domain-verification checks for `cominotti.dev`, and release workflows that keep Flathub publication as a reviewable PR by default.
- add-snap-packaging: Scaffolded the Ubuntu Snap (`snap/snapcraft.yaml`, `scripts/run-snap-smoke.sh`, `scripts/verify-snap-identity.sh`, `.github/workflows/snap.yml`, Snap Makefile targets). Strict confinement + portals, reuses the Meson/Cargo build via a `layout:` bind-mount, Unlisted + edge-only release. Build is gated on the unpublished GNOME 50 platform snap (`core26`).
- 001-file-peek: Added Rust 1.96.0 (Edition 2024) + GTK4 0.11, Libadwaita 0.9, GtkSourceView 5 0.11, gio/glib/pango 0.22, existing `spawn_blocking_then` background executor
