---
stepsCompleted: ['step-01-validate-prerequisites', 'step-02-design-epics', 'step-03-create-stories', 'step-04-final-validation']
inputDocuments:
  - '_bmad-output/planning-artifacts/prd.md'
  - '_bmad-output/planning-artifacts/architecture.md'
  - '_bmad-output/planning-artifacts/ux-design-specification.md'
---

# lushtext - Epic Breakdown

## Overview

This document provides the complete epic and story breakdown for lushtext, decomposing the requirements from the PRD, UX Design, and Architecture into implementable stories. The feature being planned is **workspace-wide content search** — ripgrep-speed search across all workspace roots with streaming results, match highlighting, click-to-open-at-line, Replace All with preview, and search history/saved searches.

## Requirements Inventory

### Functional Requirements

**Content Search Execution (FR1-FR9)**

FR1: User can search file contents across all workspace roots using a text query
FR2: User can search using regular expressions
FR3: User can toggle case-sensitive matching
FR4: User can toggle whole-word matching
FR5: User can cancel an in-progress search by typing a new query, pressing Escape, or closing the search panel
FR6: System skips binary files during search automatically
FR7: System respects .gitignore / .ignore / .rgignore patterns during search by default
FR8: User can toggle .gitignore filtering on or off
FR9: System caps search results at 10,000 matches and indicates truncation to the user

**Search Results Display (FR10-FR16)**

FR10: System displays search results grouped by file, with each file as an expandable group
FR11: System shows line number and matching line content for each result row
FR12: System highlights the matching text within each result line
FR13: System shows a total result count (number of matches and number of files)
FR14: System displays a truncation indicator with guidance to narrow the query when the result cap is reached
FR15: System shows an empty state ("No results found") when a search yields zero matches
FR16: System shows an inline error message when the user enters an invalid regex pattern, without executing a search

**Search Results Navigation (FR17-FR20)**

FR17: User can open a file at the matching line by activating a search result
FR18: User can navigate to the next match across files via keyboard shortcut (F4)
FR19: User can navigate to the previous match across files via keyboard shortcut (Shift+F4)
FR20: Search panel remains visible after the user navigates to a result

**Search Filtering (FR21)**

FR21: User can filter searched files by glob pattern (e.g., *.rs, *.md, *.toml)

**Multi-File Replace (FR22-FR27)**

FR22: User can enter a replacement string for the current search query
FR23: System shows a preview list of all proposed replacements before execution, displaying file path, line number, original line, and resulting line
FR24: User can select or deselect individual replacements via checkboxes (all selected by default)
FR25: User can execute Replace All for the selected replacements
FR26: System confirms replacement results after execution (number of matches replaced, number of files affected)
FR27: User can undo all replacements made in the most recent Replace All operation

**Search Persistence (FR28-FR31)**

FR28: System maintains a history of recent search queries with their associated toggle settings and file glob
FR29: User can select a previous search from the history to re-execute it with its saved settings
FR30: User can save a search as a named entry for permanent access
FR31: User can select a saved search to execute it with all its saved options pre-configured

**Search Panel Lifecycle (FR32-FR36)**

FR32: User can toggle the search panel open/closed via Ctrl+Shift+F
FR33: System animates the search panel reveal and hide transitions
FR34: System saves focus before opening the panel and restores focus after closing it
FR35: System persists search panel visibility and last-used search options across application sessions
FR36: System displays search progress in the status bar during active searches (files searched / total estimate)

### NonFunctional Requirements

**Performance (NFR1-NFR8)**

NFR1: First search results appear within 500ms of query submission on a workspace with 70,000 files (NVMe storage)
NFR2: Full search completes within 5 seconds on a workspace with 70,000 files and 30 million lines (NVMe storage)
NFR3: Search cancellation (new query, Escape, panel close) halts background work within 50ms with no visible UI lag
NFR4: The GTK main thread maintains 60fps (no dropped frames) during active search result streaming
NFR5: Result batching delivers up to 50 results per 50ms polling tick — sufficient for perceived streaming without overloading the GTK main loop
NFR6: Channel back-pressure (bounded at 1024 items) prevents unbounded memory growth during searches that produce results faster than the UI can consume
NFR7: Replace All execution writes files atomically (temp file + rename) — a crash mid-operation leaves each file either fully old or fully new, never partially written
NFR8: Search panel open/close animation completes in 250ms (matching sidebar and preview pane transitions)

**Reliability (NFR9-NFR12)**

NFR9: Per-file I/O errors (permission denied, file locked, encoding failure) are logged and skipped without aborting the overall search
NFR10: Invalid regex input produces a user-facing error message — the application never panics on user-provided search patterns
NFR11: Undo All reliably reverts all replacements from the most recent Replace All operation, even if the user has navigated away from the search panel
NFR12: Search history and saved searches persist across application restarts via atomic JSON writes (same crash-safe pattern as session and workspace persistence)

**Accessibility (NFR13-NFR15)**

NFR13: Search panel uses standard GTK4/Libadwaita widgets throughout, inheriting built-in AT-SPI accessibility (keyboard navigation, screen reader support, focus management)
NFR14: All search panel controls are keyboard-accessible — no mouse-only interactions
NFR15: Search results list supports keyboard navigation (arrow keys for row selection, Enter for activation)

### Additional Requirements

**From Architecture:**

- New threading pattern: `std::thread::spawn` + `crossbeam_channel::bounded(1024)` + `glib::timeout_add_local` polling for streaming results. This is the ONLY feature in LushText that uses channel-based streaming — distinct from the existing `spawn_blocking_then` pattern
- 5 new direct dependencies: `grep-regex` 0.1, `grep-searcher` 0.1, `grep-matcher` 0.1, `ignore` 0.4, `crossbeam-channel` 0.5 (all pure Rust, GPL-3.0 compatible)
- Post-dependency chain: `cargo hakari generate` + `make cargo-sources` for Flatpak
- 3 new modules: `model/content_search.rs` (~80 lines), `services/content_search.rs` (~300 lines), `ui/search_panel/` (mod.rs ~350, imp.rs ~450, item.rs ~100)
- Window template modification: existing GtkStack wrapped in vertical GtkBox + GtkRevealer for search panel
- 6 new GSettings keys: `search-panel-visible`, `search-panel-options-expanded`, `search-case-sensitive`, `search-regex`, `search-whole-word`, `search-gitignore`
- Channel ownership: UI creates channel, service receives sender (maximally testable)
- New `Arc<AtomicBool>` cancel token per search — never reuse (races with old search's drain loop)
- Service sends flat `SearchMatch` items; UI groups into file hierarchy via `HashMap<PathBuf, ListStore>`
- Model carries raw data only; UI generates Pango markup in `connect_bind` (no GTK deps in model/services)
- Replace All uses `spawn_blocking_then` (single result, not streaming) — same domain module, different execution pattern
- Action namespace: `search.*` for panel-internal actions, `win.*` for window-level (toggle-search-panel, search-next-match, search-prev-match)
- Skip modified tabs during Replace All: `is_modified()` check per file, report "N files skipped (unsaved changes)"
- In-memory `HashMap<PathBuf, Vec<u8>>` for undo backup — cleared on next search, panel close, or app exit
- 2 new JSON data files: `search-history.json` (capped 20, oldest rotated), `saved-searches.json` (permanent)
- Walker thread count: `std::thread::available_parallelism().min(8)`
- Benchmark additions: literal search 10k files, regex search, large file, gitignore filter
- F4 navigation state: `current_match: Cell<Option<(usize, usize)>>` on panel, `SingleSelection::set_selected()` for visual highlight
- Replace All preview: same ListView, conditional `connect_bind` via `preview_mode: Cell<bool>` — no second widget
- Progress total: use FileIndex count as estimate when available, `SearchEvent::Progress(searched, Option<total>)`
- For non-modified open tabs after Replace All: write to disk atomically, trigger file monitor reload via existing `changed` signal path

### UX Design Requirements

UX-DR1: Panel uses "Progressive Minimal" layout — default state shows only search input, 3 core toggles (Aa, .*, W), and a "More" button. Advanced options (gitignore toggle, glob filter, replace controls) revealed behind "More" toggle. State remembered via GSettings
UX-DR2: Results grouped by file using GtkTreeListModel + GtkTreeExpander — file header rows show filename (.heading class) + match count (.caption class), match rows show line number (.monospace + .dim-label) + highlighted content (.monospace)
UX-DR3: Match highlighting uses Pango markup with @accent_color bold on matching substrings. One GtkLabel per result row with inline markup — no separate label widgets per segment
UX-DR4: Panel container is GtkRevealer with slide-up transition (250ms, EaseOutCubic). Options/replace revealers use slide-down (150ms). 1px minimum animation target rule applies
UX-DR5: Pre-fill search from editor selection — if text is selected when Ctrl+Shift+F is pressed, populate the search field. Matches in-editor Ctrl+F behavior
UX-DR6: Re-invocation behavior — if panel already visible, Ctrl+Shift+F refocuses search input and selects all text for easy replacement
UX-DR7: Result count label updates in real-time during streaming: "3 results in 2 files" -> "47 results in 12 files (done)". Uses .caption CSS class
UX-DR8: Viewport stability during streaming — results append at bottom of list, user scroll position preserved, no viewport jumping during rapid result arrival
UX-DR9: Replace All preview mode — same ListView switches to before/after display. Original line with dimmed matching text, replacement line with highlighted new text. GtkCheckButton per match row (all checked by default). Confirm button executes checked replacements
UX-DR10: Search history dropdown on search input focus — recent (auto, capped 20) + saved (explicit, permanent) searches. Full state restoration (query + toggles + glob). Search runs immediately on selection
UX-DR11: Inline error/warning labels below search input — @error_color for invalid regex, @warning_color for truncation ("10,000+ results -- narrow your search"). No modal dialogs for feedback, no toast notifications
UX-DR12: Toggle button changes (case, regex, word) trigger immediate re-search (no debounce). Toggles use .linked CSS class for grouped appearance
UX-DR13: F4/Shift+F4 works from both results list AND editor — does not require panel focus. Current match highlighted in results list via SingleSelection
UX-DR14: Panel sizing: auto-sized from content via GtkRevealer. `max-content-height` on ScrolledWindow caps at ~1/3 content area (dynamic). Min ~150px. No GtkPaned, no `search-panel-height` GSettings key
UX-DR15: State preservation on panel close — query, toggles, glob, results, scroll position all preserved. Reopening restores everything
UX-DR16: All colors via Adwaita semantic tokens (@accent_color, @error_color, @warning_color, @window_bg_color, @dim_label). No hardcoded colors. Automatic light/dark/high-contrast support
UX-DR17: Result line content uses .monospace CSS class — shares editor's font customization provider (use-system-font / custom-font GSettings)
UX-DR18: Escape respects overlay priority — closes topmost overlay first when multiple open. Action enabled state manages priority
UX-DR19: No toast notifications in search panel. All feedback either inline (within panel) or in status bar
UX-DR20: Empty state: "No results found" centered in results area. Query visible in search input for typo correction
UX-DR21: LushtextSearchPanel GObject subclass following mod.rs + imp.rs pattern with CompositeTemplate (search-panel.ui)
UX-DR22: SearchResultItem GObject wrapper following PaletteItem/FileTreeItem pattern — properties: item-type (File/Match), file-path, display-path, line-number, line-content, match-markup, match-count, checked, replace-markup

### FR Coverage Map

FR1: Epic 1 — Search file contents across all workspace roots
FR2: Epic 1 — Regular expression search support
FR3: Epic 1 — Case-sensitive matching toggle
FR4: Epic 1 — Whole-word matching toggle
FR5: Epic 1 — Cancel in-progress search (new query, Escape, panel close)
FR6: Epic 1 — Automatic binary file skip
FR7: Epic 1 — .gitignore/.ignore/.rgignore respected by default
FR8: Epic 1 — Toggle .gitignore filtering on/off
FR9: Epic 1 — 10,000 result cap with truncation indicator
FR10: Epic 1 — Results grouped by file (expandable groups)
FR11: Epic 1 — Line number and content per result row
FR12: Epic 1 — Match text highlighting within result lines
FR13: Epic 1 — Total result count (matches and files)
FR14: Epic 1 — Truncation indicator with guidance to narrow query
FR15: Epic 1 — Empty state ("No results found")
FR16: Epic 1 — Inline error for invalid regex
FR17: Epic 1 — Open file at matching line on result activation
FR18: Epic 1 — F4 next match navigation across files
FR19: Epic 1 — Shift+F4 previous match navigation across files
FR20: Epic 1 — Panel remains visible after navigation
FR21: Epic 1 — File glob pattern filter
FR22: Epic 2 — Replace input field for search query
FR23: Epic 2 — Preview list of all proposed replacements
FR24: Epic 2 — Per-match checkboxes (all selected by default)
FR25: Epic 2 — Execute Replace All for selected replacements
FR26: Epic 2 — Replacement results confirmation
FR27: Epic 2 — Undo All for most recent Replace All
FR28: Epic 3 — History of recent search queries with settings
FR29: Epic 3 — Select from history to re-execute with saved settings
FR30: Epic 3 — Save search as named permanent entry
FR31: Epic 3 — Select saved search with pre-configured options
FR32: Epic 1 — Toggle search panel via Ctrl+Shift+F
FR33: Epic 1 — Animated panel reveal/hide transitions
FR34: Epic 1 — Focus save/restore on panel open/close
FR35: Epic 3 — Persist panel visibility and search options across sessions
FR36: Epic 1 — Search progress in status bar

## Epic List

### Epic 1: Workspace Content Search & Navigation
Users can search file contents across all workspace roots, see streaming results grouped by file with match highlighting, and navigate to any match with one click — the complete search-navigate loop. Ctrl+Shift+F opens the panel, users type a query, results stream in with file grouping and line numbers, and clicking a result opens the file at the matching line. F4/Shift+F4 cycles matches across files. Includes regex/case/word toggles, gitignore filtering, glob filter, error/empty/truncation states, and progress reporting.
**FRs covered:** FR1-FR21, FR32-FR34, FR36
**NFRs addressed:** NFR1-NFR6, NFR8-NFR10, NFR13-NFR15
**UX-DRs addressed:** UX-DR1-DR8, UX-DR11-DR14, UX-DR16-DR22

## Epic 1: Workspace Content Search & Navigation

Users can search file contents across all workspace roots, see streaming results grouped by file with match highlighting, and navigate to any match with one click — the complete search-navigate loop.

### Story 1.1: Content Search Service & Types

As a developer,
I want a content search service that searches file contents across workspace roots with streaming results via a bounded channel,
So that the search panel has a fast, testable, cancellable engine to build on.

**Acceptance Criteria:**

**Given** the `grep-regex`, `grep-searcher`, `grep-matcher`, `ignore`, and `crossbeam-channel` crates are added to workspace dependencies
**When** `cargo hakari generate` and `make cargo-sources` are run
**Then** all dependencies compile and the workspace-hack crate is updated

**Given** `model/content_search.rs` exists with `SearchMatch`, `ContentSearchOptions`, `SearchEvent`, `Replacement`, `ReplaceResult`, `SearchHistoryEntry`, and `SavedSearch` types
**When** the types are used by services and UI code
**Then** all types are GTK-free (no glib/gtk4/libadwaita imports) and derive appropriate traits (Clone, Debug, Serialize, Deserialize where needed)

**Given** `services/content_search.rs` exists with a public `search(query, roots, options, tx, cancel)` function
**When** a literal search query is submitted with one workspace root containing 3 text files (2 with matches, 1 without)
**Then** `SearchEvent::Match` items are sent through the channel for the 2 matching files with correct file paths, line numbers, line content, and match byte ranges
**And** `SearchEvent::Done` is sent when the search completes

**Given** a search is in progress
**When** the `Arc<AtomicBool>` cancel token is set to `true`
**Then** the search stops within 50ms and sends `SearchEvent::Done`
**And** no further `SearchEvent::Match` items are sent after cancellation

**Given** a directory containing a binary file (e.g., a PNG image) and a text file with a match
**When** a search is executed
**Then** the binary file is silently skipped and only the text file match is returned

**Given** a directory with a `.gitignore` file listing `target/` and a `target/` subdirectory containing matching files
**When** a search is executed with gitignore filtering enabled (default)
**Then** files inside `target/` are not searched and no matches from `target/` appear

**Given** a search that would produce more than 10,000 matches
**When** the result count reaches 10,000
**Then** the search stops and sends `SearchEvent::ResultCap` before `SearchEvent::Done`

**Given** a search query with regex mode enabled and a valid regex pattern `fn\s+\w+`
**When** the search executes across files
**Then** only lines matching the regex pattern are returned as matches

**Given** a search query with case-sensitive mode enabled
**When** the search executes with query "Error"
**Then** lines containing "Error" match but lines containing only "error" do not

**Given** a search query with whole-word mode enabled and query "port"
**When** the search executes
**Then** lines containing "port" as a standalone word match but lines containing only "report" or "export" do not

**Given** a search with a file glob filter `*.rs`
**When** the search executes across a workspace with `.rs`, `.toml`, and `.md` files
**Then** only `.rs` files are searched and matches from other file types are excluded

**Given** multiple workspace roots are provided
**When** a search executes
**Then** all roots are searched and matches from every root are returned

**Given** an empty search query
**When** search is called
**Then** no search executes and `SearchEvent::Done` is sent immediately

**Given** regex mode is enabled and the pattern is invalid (e.g., `fn\s+[`)
**When** search is called
**Then** `SearchEvent::Error` is sent with a descriptive error message and no file traversal occurs

**Given** Criterion benchmarks exist for literal search (10k files), regex search, large file search, and gitignore-filtered search
**When** `cargo bench` is run
**Then** all benchmarks execute successfully and produce timing results

### Story 1.2: Search Panel with Streaming Results

As a user,
I want to open a search panel with Ctrl+Shift+F, type a query, and see results streaming in grouped by file with line numbers, then click a result to open the file at that line,
So that I can find text across my entire workspace without leaving the editor.

**Acceptance Criteria:**

**Given** the user is in the main window with at least one workspace root
**When** the user presses `Ctrl+Shift+F`
**Then** the search panel slides up from below the content stack with a 250ms EaseOutCubic animation (GtkRevealer, slide-up)
**And** the cursor is placed in the search input field
**And** the previous focus widget is saved for restoration

**Given** the search panel is already visible
**When** the user presses `Ctrl+Shift+F` again
**Then** the search input is refocused and all text in the input is selected

**Given** the user has text selected in the editor
**When** the user presses `Ctrl+Shift+F`
**Then** the selected text pre-fills the search input field

**Given** the search panel is open with the cursor in the search input
**When** the user types a query and waits 300ms (debounce, generation-counter pattern)
**Then** a new search is started across all workspace roots via `std::thread::spawn` + `crossbeam_channel::bounded(1024)`
**And** the previous search (if any) is cancelled via a new `Arc<AtomicBool>` (old token set to true, new token created)

**Given** search results are streaming in via the channel
**When** the `glib::timeout_add_local(50ms)` polling timer fires
**Then** up to 50 results are drained from the channel per tick
**And** results are grouped by file in a `GtkTreeListModel` — file header rows (expandable) with match rows as children
**And** new results are inserted via `ListStore::splice()` for batch updates
**And** the result count label updates in real-time ("N results in M files")

**Given** results are streaming and the user has scrolled to a specific position in the results list
**When** new results arrive and are appended
**Then** the user's scroll position is preserved — no viewport jumping

**Given** the search completes with zero matches
**When** all results have been processed
**Then** "No results found" is displayed centered in the results area
**And** the query remains visible in the search input for correction

**Given** search results are displayed with file-grouped rows
**When** the user double-clicks or presses Enter on a match row
**Then** the file opens at the matching line number (reusing an existing tab if the file is already open, or creating a new tab)
**And** the search panel remains visible with results intact

**Given** the search panel is open
**When** the user presses Escape
**Then** the panel slides down with 250ms animation
**And** focus restores to the previously saved widget (or active editor source_view, or window default)
**And** search state (query, results, scroll position) is preserved for next open

**Given** a search is in progress and the user types a new query
**When** the 300ms debounce expires
**Then** the in-flight search is cancelled (old cancel token set true), previous results are cleared, and a new search starts

**Given** the `LushtextSearchPanel` widget
**When** inspected
**Then** it follows the mod.rs + imp.rs GObject subclass pattern with a CompositeTemplate (search-panel.ui)
**And** `SearchResultItem` GObject wrapper follows the PaletteItem/FileTreeItem pattern
**And** the widget is registered via `ensure_type()` in the window's `class_init()`

**Given** the search panel's widget tree
**When** the panel is in its default (collapsed options) state
**Then** only the search input and the results area are visible (the "More" options area is hidden) — the Progressive Minimal layout header row structure is in place for Story 1.3 to add toggle buttons

### Story 1.3: Search Toggles & Match Highlighting

As a user,
I want to toggle regex, case-sensitive, and whole-word matching, and see the matching text highlighted in results,
So that I can search precisely and identify matches at a glance.

**Acceptance Criteria:**

**Given** the search panel header row
**When** the panel is visible
**Then** three toggle buttons are displayed in a `.linked` GtkBox group: "Aa" (case), ".*" (regex), "W" (word)
**And** a "More" button is displayed to the right of the toggles (non-functional until Story 1.4)

**Given** the regex toggle is off (default)
**When** the user clicks the ".*" toggle button to enable regex
**Then** the toggle visually activates
**And** the current search re-runs immediately with regex matching enabled (no debounce)

**Given** the case-sensitive toggle is off (default)
**When** the user clicks the "Aa" toggle button to enable case sensitivity
**Then** the current search re-runs immediately with case-sensitive matching

**Given** the whole-word toggle is off (default)
**When** the user clicks the "W" toggle button to enable whole-word matching
**Then** the current search re-runs immediately with whole-word matching

**Given** search results are displayed
**When** a match row is rendered in `connect_bind`
**Then** the matching substring within the line content is highlighted using Pango markup with `@accent_color` bold
**And** the non-matching portions of the line are rendered in normal weight
**And** special characters in the line content are escaped via `glib::markup_escape_text`

**Given** result line content labels
**When** rendered
**Then** they use the `.monospace` CSS class, sharing the editor's font customization provider

**Given** a file header row in the results tree
**When** rendered
**Then** it displays the filename (`.heading` style) and match count (`.caption` style, e.g., "— 3 matches")

**Given** regex mode is enabled and the user enters an invalid pattern (e.g., `fn\s+[`)
**When** the debounce timer fires
**Then** an inline error label appears below the search input in `@error_color` with a descriptive message (e.g., "Invalid pattern: unclosed character class")
**And** no search is executed
**And** the error label disappears when the user corrects the pattern

### Story 1.4: Gitignore Toggle, Glob Filter & Options Panel

As a user,
I want to toggle .gitignore filtering and filter by file glob patterns via an expandable options area,
So that I can narrow search scope to relevant files without cluttering the default panel view.

**Acceptance Criteria:**

**Given** the search panel header row has a "More" button (gear icon toggle)
**When** the user clicks the "More" button
**Then** an options revealer slides down (150ms, EaseOutCubic) showing:
- A `.gitignore` toggle button (enabled by default, matching FR7)
- A file glob filter `GtkEntry` with placeholder text "File filter (e.g., *.rs, *.toml)"
**And** the "More" button visually shows its active/pressed state

**Given** the options area is expanded
**When** the user clicks the "More" button again
**Then** the options revealer slides up (150ms) and hides
**And** the expanded/collapsed state is persisted via GSettings `search-panel-options-expanded`

**Given** the .gitignore toggle is enabled (default)
**When** the user clicks it to disable gitignore filtering
**Then** the current search re-runs immediately including files that would normally be filtered by .gitignore
**And** the toggle state is persisted via GSettings `search-gitignore`

**Given** the glob filter entry is empty
**When** the user types `*.rs` and waits 300ms (debounce)
**Then** the current search re-runs filtering to only `.rs` files

**Given** a search produces more than 10,000 matches
**When** the result cap is reached
**Then** the search stops
**And** the result count label changes to "10,000+ results (truncated) — narrow your search" styled with `@warning_color`
**And** the glob filter is available for immediate refinement (visible if "More" is expanded, discoverable via "More" if collapsed)

**Given** the GSettings keys `search-case-sensitive`, `search-regex`, `search-whole-word`, and `search-gitignore`
**When** the panel is opened on a subsequent application launch
**Then** all toggle buttons reflect their persisted GSettings state

### Story 1.5: Match Navigation & Progress Reporting

As a user,
I want to cycle through matches across files with F4/Shift+F4 and see search progress in the status bar,
So that I can navigate results efficiently with the keyboard and know when a search is still running on large workspaces.

**Acceptance Criteria:**

**Given** search results are displayed in the panel and the user is in the editor (focus not on the results list)
**When** the user presses F4
**Then** the next match in the results list is selected (via `SingleSelection::set_selected()`)
**And** the file containing that match opens at the matching line (switching to existing tab or opening a new tab)
**And** the search panel remains visible

**Given** the user has navigated to a match via F4
**When** the user presses F4 again
**Then** the next match is selected, cycling to the next file's matches when the current file's matches are exhausted

**Given** the user is on the first match in the results list
**When** the user presses Shift+F4
**Then** navigation wraps to the last match in the results list

**Given** the user presses F4 from the results list (focus on the list)
**When** a match is activated
**Then** focus moves to the editor at the matching line
**And** the panel stays visible with the current match highlighted in the results list

**Given** a search is in progress on a large workspace
**When** results are streaming
**Then** the status bar displays progress: "Searching X / Y files..." (where Y is estimated from FileIndex count when available, or "Searching X files..." without denominator)
**And** the progress message does not auto-dismiss (unlike normal status bar messages)

**Given** a search completes or is cancelled
**When** the final `SearchEvent::Done` is processed
**Then** the status bar progress message is cleared programmatically

**Given** `win.search-next-match` (F4) and `win.search-prev-match` (Shift+F4) actions are registered on the window
**When** the search panel is not visible or has no results
**Then** both actions are disabled (greyed out, shortcuts are no-ops)

**Given** the current match tracker `current_match: Cell<Option<(usize, usize)>>` on the panel
**When** a new search starts
**Then** the tracker is reset to `None`

---

## Epic 2: Multi-File Replace

Users can perform search-and-replace across multiple files with full preview, per-match checkboxes, confirmation, and Undo All — safe multi-file refactoring.

### Story 2.1: Replace All with Preview, Execution & Undo

As a user,
I want to enter a replacement string, preview all proposed changes with per-match checkboxes, execute Replace All, and undo if needed,
So that I can safely refactor across multiple files without fear of unintended changes.

**Acceptance Criteria:**

**Given** the search panel options area is expanded (via "More")
**When** the panel is visible
**Then** a replace input `GtkEntry` is displayed below the glob filter row
**And** a "Replace All" `GtkButton` is displayed next to the replace entry
**And** an "Undo" `GtkButton` is displayed next to "Replace All" (initially hidden)

**Given** search results are displayed and the user has typed replacement text in the replace entry
**When** the user clicks "Replace All"
**Then** the results list switches to preview mode: each match row displays the original line (matching text dimmed) and the resulting line (replacement text highlighted)
**And** each match row has a `GtkCheckButton` (all checked by default)
**And** a "Confirm Replace" button appears (replacing the "Replace All" button)

**Given** the preview is displayed with all checkboxes checked
**When** the user unchecks specific match rows
**Then** those matches are excluded from the replacement set
**And** the confirm button label reflects the count (e.g., "Replace 8 of 9")

**Given** all checkboxes in the preview are unchecked
**When** the user reviews the preview
**Then** the "Confirm Replace" button is disabled

**Given** the user clicks "Confirm Replace" with checked matches
**When** replacement executes
**Then** `services/content_search::replace_all()` runs via `spawn_blocking_then` (not the streaming channel pattern)
**And** each affected file is written atomically (temp file + rename) — a crash mid-operation leaves each file either fully old or fully new
**And** pre-replacement file content is backed up in an in-memory `HashMap<PathBuf, Vec<u8>>` for undo

**Given** a file targeted for replacement is already open in a tab with unsaved modifications (`is_modified() == true`)
**When** Replace All executes
**Then** that file is skipped (not replaced)
**And** the skip is included in the confirmation count

**Given** a file targeted for replacement is already open in a tab without modifications
**When** Replace All writes the file to disk atomically
**Then** the tab's file monitor detects the change via the existing `changed` signal path
**And** the tab reloads the updated content from disk

**Given** Replace All completes
**When** all files are written
**Then** the status bar displays a transient message: "Replaced N of M matches in K files" (and "L files skipped (unsaved changes)" if applicable)
**And** the results list exits preview mode and returns to normal result display
**And** the "Undo" button becomes visible next to "Replace All"

**Given** the "Undo" button is visible after a Replace All
**When** the user clicks "Undo"
**Then** all replaced files are restored to their pre-replacement content from the in-memory backup via atomic writes
**And** the status bar displays "Reverted K files"
**And** the "Undo" button is hidden
**And** open tabs for reverted files reload via file monitor

**Given** the in-memory undo backup exists
**When** the user starts a new search, closes the search panel, or exits the application
**Then** the undo backup is cleared (memory freed)
**And** the "Undo" button is hidden

**Given** the `replace_all()` service function
**When** inspected
**Then** it is in `services/content_search.rs` alongside the `search()` function (same domain, same file)
**And** it takes a list of `Replacement` structs and a cancel token
**And** it returns a `ReplaceResult` summary (files written, matches replaced, files skipped)
**And** it contains no GTK/GLib imports

---

## Epic 3: Search History & Session Persistence

Users' search workflow persists across sessions. Recent searches with toggle/glob settings are remembered automatically, and frequently-used searches can be saved as named entries for permanent one-click access.

### Story 3.1: Search History

As a user,
I want my recent searches automatically remembered with their toggle settings and glob filter, and accessible via a dropdown on the search input,
So that I can quickly re-run searches I've done before without retyping or reconfiguring.

**Acceptance Criteria:**

**Given** a search completes (SearchEvent::Done received, at least one result or explicit user submission)
**When** the search is added to history
**Then** a `SearchHistoryEntry` is created containing the query text, toggle states (regex, case, word, gitignore), and glob filter value
**And** the entry is prepended to `$XDG_DATA_HOME/lushtext/search-history.json` via `json_store` atomic write (temp file + rename)

**Given** the search history contains 20 entries
**When** a new search completes
**Then** the oldest entry is removed before the new entry is prepended (FIFO, capped at 20)

**Given** the search history contains duplicate queries with identical settings
**When** a matching search completes
**Then** the existing entry is moved to the top of the history (not duplicated)

**Given** the search input receives focus and search history entries exist
**When** the input is focused
**Then** a dropdown appears below the search input showing recent searches
**And** each entry displays the query text and a summary of active toggles (e.g., "Aa" if case-sensitive was on, "*.rs" if glob was set)

**Given** the history dropdown is visible
**When** the user selects an entry
**Then** the search input is populated with the saved query
**And** all toggle buttons are restored to the saved states (regex, case, word, gitignore)
**And** the glob filter entry is restored to the saved value
**And** a search runs immediately with the restored settings (no debounce)

**Given** the history dropdown is visible
**When** the user starts typing in the search input
**Then** the dropdown closes and normal search-as-you-type behavior resumes

**Given** the application is restarted
**When** the search panel is opened and the search input receives focus
**Then** the history dropdown shows entries from `search-history.json` loaded at startup

**Given** `search-history.json` is corrupted or missing
**When** history is loaded
**Then** an empty history is used (no error, no crash) and the file is recreated on next save

### Story 3.2: Saved Searches & Panel State Persistence

As a user,
I want to save frequently-used searches by name for permanent access and have the panel remember its state across restarts,
So that my search workflow is ready exactly where I left it when I reopen the application.

**Acceptance Criteria:**

**Given** search results are displayed
**When** the user triggers a "Save Search" action (button or keyboard shortcut)
**Then** a dialog prompts for a search name (pre-filled with the query text)
**And** on confirm, a `SavedSearch` entry is written to `$XDG_DATA_HOME/lushtext/saved-searches.json` via `json_store` atomic write
**And** the entry contains the name, query, all toggle states, and glob filter

**Given** saved searches exist
**When** the search input receives focus and the dropdown appears
**Then** saved searches are displayed in a separate section below the recent history section
**And** each saved search shows its user-given name and query

**Given** the dropdown is visible
**When** the user selects a saved search
**Then** the search input, toggles, and glob filter are all restored from the saved entry
**And** a search runs immediately with the restored settings

**Given** a saved search exists
**When** the user wants to delete it
**Then** a delete action is available (e.g., right-click or swipe in the dropdown)
**And** the entry is removed from `saved-searches.json` via atomic write

**Given** the search panel is visible with a query, toggle states, and expanded options
**When** the application is closed and reopened
**Then** the panel visibility is restored from GSettings `search-panel-visible`
**And** toggle states are restored from GSettings (`search-case-sensitive`, `search-regex`, `search-whole-word`, `search-gitignore`)
**And** the options expanded state is restored from GSettings `search-panel-options-expanded`

**Given** the panel was hidden when the application was closed
**When** the application reopens
**Then** the panel is hidden (GSettings `search-panel-visible` = false)
**And** pressing `Ctrl+Shift+F` opens the panel with the last-used toggle states restored

**Given** `saved-searches.json` is corrupted or missing
**When** saved searches are loaded
**Then** an empty saved searches list is used (no error, no crash) and the file is recreated on next save
