---
stepsCompleted: [1, 2, 3, 4, 5, 6, 7, 8]
lastStep: 8
status: 'complete'
completedAt: '2026-04-07'
inputDocuments:
  - '_bmad-output/planning-artifacts/prd.md'
  - '_bmad-output/planning-artifacts/ux-design-specification.md'
  - '_bmad-output/planning-artifacts/research/technical-ripgrep-crate-ecosystem-research-2026-04-06.md'
  - '_bmad-output/project-context.md'
  - '_bmad-output/implementation-artifacts/deferred-work.md'
workflowType: 'architecture'
project_name: 'lushtext'
user_name: 'Danilo'
date: '2026-04-07'
---

# Architecture Decision Document

_This document builds collaboratively through step-by-step discovery. Sections are appended as we work through each architectural decision together._

## Project Context Analysis

### Requirements Overview

**Functional Requirements (36 FRs in 6 categories):**

| Category | FRs | Architectural Impact |
|----------|-----|---------------------|
| Search Execution | FR1-9 | Core service layer: WalkParallel + grep-searcher orchestration, cancellation, binary/gitignore filtering |
| Results Display | FR10-16 | GtkTreeListModel with file-grouped results, Pango markup highlighting, streaming append via ListStore::splice |
| Navigation | FR17-20 | Click-to-open (reuses open_document), F4/Shift+F4 sequential match cycling, panel persistence |
| Filtering | FR21 | File glob passed to ignore crate's WalkBuilder |
| Multi-File Replace | FR22-27 | Highest-risk subsystem: preview generation, per-match checkboxes, atomic writes, undo via in-memory backup, open-tab synchronization |
| Persistence & Lifecycle | FR28-36 | GSettings for panel state (7 keys), json_store for search history + saved searches, focus save/restore, animated reveal |

**Non-Functional Requirements (15+ NFRs):**

| NFR Category | Key Constraints | Architectural Driver |
|-------------|----------------|---------------------|
| Performance | 500ms first result on 70k files, 60fps during streaming, 50ms cancellation | Dedicated thread + bounded channel (1024), batch polling (50 results/50ms), AtomicBool cancel token |
| Reliability | Per-file error resilience, invalid regex handling, atomic replace writes | Sink error → skip + continue, RegexMatcher error → inline UI message, temp+rename per file |
| Accessibility | WCAG AA inherited, keyboard-accessible, screen reader support | Standard Adwaita widgets only, built-in AT-SPI, no custom rendering |

**Scale & Complexity:**

- Primary domain: Native desktop application (Rust + GTK4/Libadwaita)
- Complexity level: Low-to-medium (single feature, established codebase, no compliance)
- Estimated new architectural components: 3 modules (model, service, widget) + 7 GSettings keys + 2 JSON persistence files

### Technical Constraints & Dependencies

**Established patterns that MUST be followed:**
- Three-layer architecture: model (pure Rust, no GTK) → services (GTK-free business logic) → ui (GTK widgets)
- GObject subclass pattern: mod.rs + imp.rs per widget
- 1000-line hard limit per production .rs file
- ListStore::splice() for batch updates
- Generation-counter debounce (not SourceId)
- Atomic JSON writes (temp + rename) via json_store
- Focus save/restore via WeakRef for overlay widgets
- AdwTimedAnimation + EaseOutCubic + 250ms for all panel animations
- 1px minimum animation target (never 0px)

**New pattern introduced:**
- `std::thread::spawn` + `crossbeam_channel::bounded(1024)` + `glib::timeout_add_local` polling for streaming results. This is the ONLY feature in LushText that uses channel-based streaming. The existing `spawn_blocking_then` pattern is unsuitable (fire-and-forget, single result, 8-thread concurrency guard).

**New dependencies (5 direct, ~5-8 transitive):**
- `grep-regex` 0.1, `grep-searcher` 0.1, `grep-matcher` 0.1, `ignore` 0.4, `crossbeam-channel` 0.5
- All pure Rust, GPL-3.0 compatible (MIT/Unlicense/Apache-2.0)
- Post-add chain: `cargo hakari generate` → `make cargo-sources`

### Cross-Cutting Concerns Identified

1. **Streaming threading model** — New pattern that doesn't fit `spawn_blocking_then`. Must be carefully documented as the canonical pattern for any future streaming features.
2. **GSettings schema expansion** — 7 new keys in the existing schema file. All `kebab-case`, consistent with existing conventions.
3. **Widget hierarchy modification** — The content area's end-child must change from a direct GtkStack to a vertical GtkBox containing the GtkStack + a GtkRevealer for the search panel. This is a structural change to the window template.
4. **Status bar integration** — Progress reporting ("Searching X / Y files...") uses the existing status bar message infrastructure but introduces a new pattern: non-auto-dismissing progress messages that are cleared programmatically on completion.
5. **Open-tab synchronization for Replace All** — When Replace All modifies a file that's already open in a tab, the tab buffer must be updated. This creates a coupling between the search panel and EditorPage that doesn't exist elsewhere.
6. **Search cancellation coordination** — A single AtomicBool must coordinate 3 cancellation points (WalkParallel, Sink::matched, GTK timer). The cancel token lifecycle must be robust against rapid re-search (new query before old search fully drains).

## Starter Template Evaluation

### Primary Technology Domain

Native desktop application (Rust + GTK4/Libadwaita) — brownfield project with established architecture.

### Starter Template Assessment

**Not applicable.** LushText is a brownfield project with a fully established tech stack, build system, dependency management, and architectural patterns. No starter template evaluation is needed.

The content search feature adds to the existing architecture rather than establishing a new foundation. All new modules follow the existing three-layer pattern (model → services → ui) and GObject subclass conventions.

### Technology Stack Baseline (Existing)

| Layer | Technology | Version | Role |
|-------|-----------|---------|------|
| Language | Rust | 1.94.1 (Edition 2024) | All application code |
| GUI Framework | GTK4 | 0.11 (GTK 4.20+) | Widget toolkit |
| App Framework | Libadwaita | 0.9 (1.8+) | GNOME HIG compliance |
| Editor Engine | GtkSourceView 5 | 0.11 | Syntax highlighting, undo/redo |
| Preferences | GSettings | — | Widget state, editor settings |
| Persistence | serde_json + json_store | — | Session, workspace, draft data |
| Build (dev) | Cargo + Makefile | — | Development workflow |
| Build (dist) | Meson + Flatpak | — | Installed/packaged builds |
| Benchmarks | Criterion | 0.5 | Performance regression detection |
| Tests | cargo-nextest | — | Parallel test execution |

### New Dependencies for Content Search

| Crate | Version | Purpose |
|-------|---------|---------|
| `grep-regex` | 0.1 | Regex engine adapter for grep-searcher |
| `grep-searcher` | 0.1 | Line-oriented search with mmap, binary detection |
| `grep-matcher` | 0.1 | Matcher trait abstraction |
| `ignore` | 0.4 | Parallel directory walker with .gitignore support |
| `crossbeam-channel` | 0.5 | Bounded channel for streaming results to GTK main thread |

## Core Architectural Decisions

### Decision Priority Analysis

**Critical Decisions (Block Implementation):**

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| 1 | Replace All undo mechanism | In-memory file backup (`HashMap<PathBuf, Vec<u8>>`) | Simple, bounded by 10k result cap. Lost on panel close or next search — acceptable per PRD. No disk I/O overhead. |
| 2 | Open-tab synchronization on Replace | Skip modified tabs | Zero risk of buffer/disk divergence. User saves first, re-runs Replace All. Simplest implementation — `is_modified()` check per file. Reports "N files skipped (unsaved changes)." |
| 3 | Search panel height | GtkRevealer, auto-sized | Panel height follows content naturally. `max-content-height` on ScrolledWindow caps maximum. No drag handle, no GtkPaned complexity. Compact when few results, scrollbar when many. |
| 4 | Result ordering | Arrival order (non-deterministic) | Preserves the streaming "results appear as they arrive" perception — the primary delight moment. Simplest implementation. Non-reproducible order between runs is acceptable. |
| 5 | Walker thread count | `std::thread::available_parallelism().min(8)` | Scales with hardware. Capped at 8 to prevent thrashing on slow filesystems. Better than fixed-4 on NVMe where parallelism helps. |
| 6 | Search history vs saved searches storage | Two separate files | `search-history.json` (capped at 20, rotated) + `saved-searches.json` (permanent). Separate lifecycles, cleaner semantics. Both use `json_store` atomic writes. |

**Deferred Decisions (Post-MVP):**

- Context lines (before/after match) in results display
- Multi-line / cross-line regex search
- Search scope refinement (search within results, exclude paths)
- Incremental file index for near-instant repeated searches
- Walker thread count as a user-tunable GSettings key

### Data Architecture

**Persistence model:** Two-tier, matching existing LushText conventions.

| Data | Storage | Lifecycle |
|------|---------|-----------|
| Panel visibility | GSettings (`search-panel-visible`) | Session-persistent, instant read |
| Toggle states (case, regex, word, gitignore) | GSettings (4 keys) | Session-persistent |
| Options expanded state | GSettings (`search-panel-options-expanded`) | Session-persistent |
| Search history | `$XDG_DATA_HOME/lushtext/search-history.json` | Capped at 20 entries, oldest rotated out, atomic writes |
| Saved searches | `$XDG_DATA_HOME/lushtext/saved-searches.json` | Permanent until user deletes, atomic writes |
| Replace All undo backup | In-memory `HashMap<PathBuf, Vec<u8>>` | Cleared on next search, panel close, or app exit |

**No new GSettings keys for panel height.** The auto-sized GtkRevealer determines its own height from content — no user-controlled position to persist.

### Search Service Architecture

**Stateless function module** (not a struct), matching `services::palette` and `services::file_tree`:

```
content_search::search(query, roots, options, tx, cancel) -> ()
```

- Blocks until search completes or is cancelled — call from a dedicated `std::thread::spawn`
- Per-thread `Searcher` + `Matcher` created inside `WalkParallel::run()` closure factory
- Results sent through `crossbeam_channel::bounded(1024)` as `SearchEvent` enum variants
- Cancellation via shared `Arc<AtomicBool>` checked at 3 points: walker callback, Sink::matched, GTK timer

**Thread model:**

| Thread | Role | Lifetime |
|--------|------|----------|
| GTK main thread | Polls channel via `timeout_add_local(50ms)`, splices into ListStore | App lifetime |
| Search coordinator | Calls `content_search::search()`, owns walker thread pool | Per-search (spawned, joins on cancel/complete) |
| Walker pool (up to 8) | Parallel file traversal + per-file grep-searcher | Managed by `WalkParallel` internally |

### Widget Integration

**Panel placement:** GtkRevealer inside a vertical GtkBox that wraps the existing content stack.

```
[end-child of horizontal GtkPaned]
GtkBox (vertical)
├── GtkStack (existing: "tabs" + "empty")
└── GtkRevealer [search_panel_revealer]     ← NEW
    └── LushtextSearchPanel                  ← NEW
```

**Animation:** `GtkRevealer` with `slide-up` transition type, 250ms duration (matching sidebar and preview pane via Adwaita's built-in revealer animation).

**Replace All — open-tab handling:**
1. Before writing each file, check all open tabs for matching path with `is_modified() == true`
2. If modified tab found → skip that file, increment skip counter
3. After all replacements, report: "Replaced N matches in K files. M files skipped (unsaved changes)."
4. For non-modified open tabs: write to disk atomically, then trigger file monitor reload (existing `changed` signal path)

### Decision Impact Analysis

**Implementation sequence (respects dependencies):**

1. Dependencies added (grep-*, ignore, crossbeam-channel) + hakari + cargo-sources
2. `model/content_search.rs` — types used by everything else
3. `services/content_search.rs` — testable independently, no UI
4. `ui/search_panel/` — widget shell with placeholder data
5. Window template modification (GtkBox wrapper + GtkRevealer)
6. End-to-end wiring (service → channel → timer → ListStore)
7. Replace All with skip-modified-tabs logic
8. Persistence (GSettings keys, search-history.json, saved-searches.json)

**Cross-component dependencies:**

- Search panel → window (open_document callback for click-to-open)
- Search panel → status bar (progress reporting)
- Search panel → sidebar (workspace roots for multi-root search)
- Replace All → EditorPage (is_modified check, file monitor reload)
- Replace All → open_paths HashSet (path lookup for open-tab detection)

## Implementation Patterns & Consistency Rules

### Critical Conflict Points

7 areas identified where AI agents implementing different phases could make incompatible choices.

### Pattern 1: Channel Ownership

**Rule:** The UI layer creates the channel; the service receives the sender.

```
// UI creates channel
let (tx, rx) = crossbeam_channel::bounded(1024);

// UI passes sender to service
std::thread::spawn(move || {
    content_search::search(&query, &roots, &options, tx, cancel);
});

// UI owns receiver and polling timer
let source_id = glib::timeout_add_local(Duration::from_millis(50), move || { ... rx.try_recv() ... });
```

**Why:** The caller (UI) owns the lifecycle. The service is a stateless function that writes to whatever sender it's given — maximally testable (unit tests create their own channel).

### Pattern 2: Action Namespace

| Action | Prefix | Registered On | Rationale |
|--------|--------|---------------|-----------|
| `win.toggle-search-panel` | `win.` | Window | Window-level visibility control |
| `win.search-next-match` | `win.` | Window | F4 works globally, even with panel unfocused |
| `win.search-prev-match` | `win.` | Window | Shift+F4 works globally |
| `search.toggle-regex` | `search.` | SearchPanel | Panel-internal toggle |
| `search.toggle-case` | `search.` | SearchPanel | Panel-internal toggle |
| `search.toggle-word` | `search.` | SearchPanel | Panel-internal toggle |
| `search.toggle-gitignore` | `search.` | SearchPanel | Panel-internal toggle |
| `search.replace-all` | `search.` | SearchPanel | Panel-internal action |
| `search.undo-replace` | `search.` | SearchPanel | Panel-internal action |

**Why:** Matches the established pattern. `section.*` for WorkspaceSection, `ws-header.*` for workspace header. `search.*` for search panel internals.

### Pattern 3: Replace All Placement

**Rule:** `services/content_search.rs` contains both `search()` and `replace_all()` as separate public functions. Replace All runs via `spawn_blocking_then` (single result, not streaming).

```rust
// services/content_search.rs
pub fn search(query, roots, options, tx, cancel) -> ()     // streaming, dedicated thread
pub fn replace_all(replacements: &[Replacement], cancel) -> ReplaceResult  // one-shot, spawn_blocking_then
```

**Why:** Same domain, same file. Replace is not streaming — it reads files, applies replacements, writes atomically, and returns a summary. `spawn_blocking_then` is the correct pattern for fire-and-forget with callback.

### Pattern 4: Workspace Roots Communication

**Rule:** Window mediates. `set_workspace_roots(Vec<PathBuf>)` on the search panel, called by the window when the sidebar's workspace state changes.

**Anti-pattern:** The search panel directly accessing the sidebar widget or its data.

**Why:** The window is the only widget that knows about both the sidebar and the search panel. Direct widget-to-widget coupling violates the existing mediation pattern.

### Pattern 5: Cancel Token Lifecycle

**Rule:** New `Arc<AtomicBool>` per search. Old token cancelled, new one created.

```rust
// In search panel, on new search:
if let Some(old_cancel) = self.cancel_token.replace(None) {
    old_cancel.store(true, Ordering::Relaxed);
}
let cancel = Arc::new(AtomicBool::new(false));
self.cancel_token.replace(Some(Arc::clone(&cancel)));
// Pass cancel to search thread
```

**Anti-pattern:** Reusing a single AtomicBool by resetting it to `false`. This races with the old search's drain loop.

### Pattern 6: Result Grouping

**Rule:** Service sends flat `SearchMatch` items. UI groups them into file hierarchy.

```
Service layer:  SearchMatch { path, line_number, line_content, match_range }
                  ↓ channel (flat stream)
UI layer:       HashMap<PathBuf, (FileGroupItem, ListStore)>
                  → root ListStore (file groups)
                    → child ListStore per file (match rows)
```

**Anti-pattern:** Service pre-grouping results by file (couples service to UI's display model and complicates streaming — service would need to buffer until a file is fully searched).

### Pattern 7: Pango Markup Generation

**Rule:** Model carries raw data. UI generates markup in `connect_bind`.

```rust
// model/content_search.rs — raw data only
pub struct SearchMatch {
    pub line_content: String,
    pub match_range: Range<usize>,
    // ...
}

// ui/search_panel — markup in connect_bind
fn bind_match_row(item: &SearchResultItem, label: &gtk4::Label) {
    let markup = format!(
        "{}<b>{}</b>{}",
        glib::markup_escape_text(&content[..range.start]),
        glib::markup_escape_text(&content[range.start..range.end]),
        glib::markup_escape_text(&content[range.end..]),
    );
    label.set_markup(&markup);
}
```

**Anti-pattern:** Service or model generating Pango markup strings (introduces GTK dependency into GTK-free layers).

### Enforcement Guidelines

**All AI agents implementing content search MUST:**

1. Keep `model/content_search.rs` and `services/content_search.rs` free of any GTK/GLib imports
2. Use `crossbeam_channel::bounded(1024)` — never unbounded, never a different bound
3. Create a new `Arc<AtomicBool>` per search — never reuse
4. Use `ListStore::splice()` for batch result insertion — never per-item `append()`
5. Use generation-counter debounce for search input (300ms) — never `SourceId` cancellation
6. Register panel-internal actions under the `search` action group — never under `win`
7. Route all inter-widget communication through the window — never direct widget-to-widget

## Project Structure & Boundaries

### Complete Project Structure — New Files

New files marked with `← NEW`. Existing files that need modification marked with `← MODIFY`.

```
crates/lushtext-core/src/
├── model/
│   ├── mod.rs                          ← MODIFY (add pub mod content_search)
│   └── content_search.rs              ← NEW (~80 lines)
│       # SearchMatch, ContentSearchOptions, SearchEvent,
│       # Replacement, ReplaceResult, SearchHistoryEntry, SavedSearch
├── services/
│   ├── mod.rs                          ← MODIFY (add pub mod content_search)
│   └── content_search.rs              ← NEW (~300 lines)
│       # pub fn search(query, roots, options, tx, cancel)
│       # pub fn replace_all(replacements, cancel) -> ReplaceResult
│       # struct LushtextSink (Sink impl)
│       # #[cfg(test)] mod tests (~200 lines, excluded from 1000-line limit)
└── ui/
    ├── mod.rs                          ← MODIFY (add pub mod search_panel)
    ├── search_panel/                   ← NEW directory
    │   ├── mod.rs                      ← NEW (~350 lines)
    │   │   # LushtextSearchPanel public API
    │   │   # open(), close(), set_query(), set_workspace_roots()
    │   │   # connect_open_file(), connect_replace_completed()
    │   │   # setup_search(), setup_channel_polling(), setup_actions()
    │   ├── imp.rs                      ← NEW (~450 lines)
    │   │   # CompositeTemplate, internal state (RefCells, Cells)
    │   │   # constructed(), dispose()
    │   │   # connect_setup(), connect_bind() for results ListView
    │   └── item.rs                     ← NEW (~100 lines)
    │       # SearchResultItem GObject wrapper for ListStore
    └── window/
        ├── mod.rs                      ← MODIFY (wire search panel, Ctrl+Shift+F action)
        └── imp.rs                      ← MODIFY (add search_panel_revealer to template)

resources/
├── ui/
│   └── search-panel.ui                ← NEW (composite template)
├── dev.cominotti.lushtext.gresource.xml ← MODIFY (add search-panel.ui entry)

data/
└── dev.cominotti.lushtext.gschema.xml  ← MODIFY (add 6 new keys)

crates/lushtext-core/benches/
└── benchmarks.rs                       ← MODIFY (add content search benchmarks)

crates/lushtext/tests/
├── integration.rs                      ← MODIFY (add content search service tests)
└── widget.rs                           ← MODIFY (add search panel widget tests)
```

### Runtime Data Files (NEW)

```
$XDG_DATA_HOME/lushtext/
├── workspaces.json          (existing)
├── session.json             (existing)
├── drafts/                  (existing)
├── search-history.json      ← NEW (capped at 20, json_store atomic writes)
└── saved-searches.json      ← NEW (permanent, json_store atomic writes)
```

### Architectural Boundaries

**Layer boundaries (strict, enforced by import rules):**

```
model/content_search.rs    ── No GTK/GLib imports. Pure Rust types.
                              Used by: services, ui, tests, benchmarks
                                 │
                                 ▼
services/content_search.rs ── No GTK/GLib imports. Uses: grep-*, ignore, crossbeam-channel.
                              Depends on: model/content_search.rs
                              Used by: ui (search panel), tests, benchmarks
                                 │
                                 ▼
ui/search_panel/           ── GTK4/Libadwaita widgets. Uses: glib, gio, gtk4, libadwaita.
                              Depends on: model + services
                              Communicates with window via callbacks.
```

**Widget communication boundaries:**

```
LushtextWindow
├── mediates ALL inter-widget communication
├── connects sidebar workspace_changed → search_panel.set_workspace_roots()
├── connects search_panel.connect_open_file() → window.open_document()
├── connects search_panel.connect_replace_completed() → status_bar.push_message()
├── registers win.toggle-search-panel action + Ctrl+Shift+F shortcut
├── registers win.search-next-match (F4) + win.search-prev-match (Shift+F4)
└── Replace All: panel asks window for is_tab_modified(path) check
```

### Requirements to Structure Mapping

| FR Category | Primary File(s) | Supporting File(s) |
|-------------|----------------|-------------------|
| **Search Execution** (FR1-9) | `services/content_search.rs` | `model/content_search.rs` |
| **Results Display** (FR10-16) | `ui/search_panel/imp.rs` (connect_bind, ListStore) | `ui/search_panel/item.rs` |
| **Navigation** (FR17-20) | `ui/window/mod.rs` (F4 actions, open_document) | `ui/search_panel/mod.rs` |
| **Filtering** (FR21) | `services/content_search.rs` (WalkBuilder glob) | `ui/search_panel/imp.rs` (glob entry) |
| **Multi-File Replace** (FR22-27) | `services/content_search.rs` (replace_all fn) | `ui/search_panel/mod.rs` (preview, checkboxes) |
| **Persistence** (FR28-31) | `services/json_store.rs` (reused) | `model/content_search.rs` (SearchHistoryEntry, SavedSearch) |
| **Panel Lifecycle** (FR32-36) | `ui/search_panel/mod.rs` + `imp.rs` | `ui/window/imp.rs` (revealer, GSettings) |

### GSettings Schema Additions

6 new keys added to `data/dev.cominotti.lushtext.gschema.xml`:

| Key | Type | Default | FR |
|-----|------|---------|-----|
| `search-panel-visible` | `b` | `false` | FR35 |
| `search-panel-options-expanded` | `b` | `false` | FR35 |
| `search-case-sensitive` | `b` | `false` | FR35 |
| `search-regex` | `b` | `false` | FR35 |
| `search-whole-word` | `b` | `false` | FR35 |
| `search-gitignore` | `b` | `true` | FR35 |

### Data Flow

```
User types query
    │
    ▼
GtkSearchEntry (300ms debounce, generation counter)
    │
    ▼
Search panel creates: channel(1024) + Arc<AtomicBool> + std::thread::spawn
    │                                                          │
    │  ┌───────────────────────────────────────────────────────┘
    │  │
    │  ▼
    │  content_search::search()
    │  ├── WalkBuilder::new(roots).build_parallel()
    │  ├── Per-thread: Searcher + RegexMatcher + LushtextSink
    │  └── Sink::matched() → tx.send(SearchEvent::Matches(...))
    │                              │
    ▼                              ▼
timeout_add_local(50ms)     crossbeam rx
    │                              │
    ├── rx.try_recv() ◄────────────┘
    ├── Group by file (HashMap<PathBuf, ListStore>)
    ├── ListStore::splice() (batch insert)
    ├── Update result count label
    └── Check cancel token / generation → self-remove timer

User activates result
    │
    ▼
connect_open_file callback → window.open_document(path) + scroll_to_line()
```

### Test Organization

| Test Tier | File | Content Search Tests |
|-----------|------|---------------------|
| **Unit** | `services/content_search.rs` `#[cfg(test)]` | literal, regex, case, word, cancel, binary skip, gitignore, result cap, multi-root, glob, empty query, invalid regex |
| **Integration** | `crates/lushtext/tests/integration.rs` | search + replace_all end-to-end with TestContext |
| **Widget** | `crates/lushtext/tests/widget.rs` | panel toggle, focus save/restore, toggle buttons, result activation |
| **Benchmarks** | `crates/lushtext-core/benches/benchmarks.rs` | literal search 10k files, regex search, large file, gitignore filter |

## Architecture Validation Results

### Coherence Validation ✅

**Decision Compatibility:**
- GtkRevealer (auto-sized) + slide-up animation: compatible — GtkRevealer natively supports `slide-up` transition type
- Channel-based streaming + generation-counter debounce: compatible — debounce on the input side, channel on the output side, no interference
- Skip modified tabs + in-memory undo backup: compatible — skip means those files aren't in the backup HashMap, no conflict
- Arrival order + UI-side file grouping: compatible — flat stream grouped into HashMap on arrival, no ordering dependency
- `search.*` action group + `win.*` global shortcuts: compatible — follows the established `section.*` / `ws-header.*` pattern

**Pattern Consistency:**
- All 7 implementation patterns align with established LushText conventions (layer separation, ListStore::splice, generation counters, WeakRef focus, atomic JSON writes)
- The one new pattern (channel-based streaming) is clearly distinguished from `spawn_blocking_then` with explicit rationale

**Structure Alignment:**
- New files follow the existing directory structure exactly (model/ → services/ → ui/)
- File line estimates (80, 300, 350, 450, 100) all within the 1000-line limit
- UI template follows existing naming convention (`search-panel.ui`, kebab-case)

### Requirements Coverage Validation ✅

**Functional Requirements (36/36 covered):**

| FR Range | Category | Architectural Support | Status |
|----------|----------|----------------------|--------|
| FR1-9 | Search Execution | `services/content_search.rs` — WalkParallel + grep-searcher + Sink | ✅ |
| FR10-16 | Results Display | `ui/search_panel/` — GtkTreeListModel, Pango markup, ListStore::splice | ✅ |
| FR17-20 | Navigation | `window/mod.rs` — F4/Shift+F4 actions, open_document, panel persistence | ✅ |
| FR21 | Filtering | `services/content_search.rs` — WalkBuilder glob | ✅ |
| FR22-27 | Multi-File Replace | `services/content_search.rs` replace_all + UI preview with checkboxes | ✅ |
| FR28-31 | Persistence | json_store (search-history.json, saved-searches.json) | ✅ |
| FR32-36 | Panel Lifecycle | GtkRevealer + GSettings (6 keys) + focus save/restore | ✅ |

**Non-Functional Requirements (15/15 covered):**

| NFR | Requirement | Architectural Support |
|-----|------------|----------------------|
| NFR1 | 500ms first result | Dedicated thread + WalkParallel + bounded channel |
| NFR2 | 5s full search on 70k files | Parallel walker (up to 8 threads) + mmap |
| NFR3 | 50ms cancellation | Arc<AtomicBool> at 3 points |
| NFR4 | 60fps during streaming | Batch polling (50 results/50ms), ListStore::splice |
| NFR5 | 50 results per tick | Timer callback batch drain |
| NFR6 | Back-pressure | crossbeam_channel::bounded(1024) |
| NFR7 | Atomic replace writes | temp file + rename per file |
| NFR8 | 250ms animation | GtkRevealer slide-up transition |
| NFR9 | Per-file error resilience | Sink skips errors, continues search |
| NFR10 | Invalid regex handling | RegexMatcher error → inline UI message |
| NFR11 | Undo All reliability | In-memory HashMap backup |
| NFR12 | Crash-safe persistence | json_store atomic writes |
| NFR13-15 | Accessibility | Standard Adwaita widgets, AT-SPI built-in |

### Implementation Readiness Validation ✅

**Decision Completeness:** All 6 critical decisions documented with rationale and anti-patterns.

**Structure Completeness:** Every new file specified with estimated line count, purpose, and which FRs it covers. Modification points on existing files identified.

**Pattern Completeness:** 7 consistency patterns with code examples and anti-patterns. Enforcement guidelines list 7 mandatory rules.

### Gap Analysis Results

4 important gaps identified and resolved:

| # | Gap | Resolution | Priority |
|---|-----|------------|----------|
| 1 | ScrolledWindow max-content-height value | Dynamic: `content_area_height / 3`, updated in parent `size_allocate`. Min 150px. | Important |
| 2 | F4 navigation state tracking | `current_match: Cell<Option<(usize, usize)>>` on panel. `SingleSelection::set_selected()` for visual highlight. | Important |
| 3 | Replace All preview mode UI | Same ListView, conditional `connect_bind` via `preview_mode: Cell<bool>`. No second widget. | Important |
| 4 | Progress total file count | Use FileIndex count as estimate when available. `SearchEvent::Progress(searched, Option<total>)` — `None` when no estimate. Show "Searching X files..." without denominator. | Important |

No critical gaps found. All gaps have proposed resolutions that follow existing LushText patterns.

### Architecture Completeness Checklist

**✅ Requirements Analysis**
- [x] Project context thoroughly analyzed (79 rules, 36 FRs, 15+ NFRs)
- [x] Scale and complexity assessed (low-to-medium, brownfield)
- [x] Technical constraints identified (layer separation, 1000-line limit, GTK threading)
- [x] Cross-cutting concerns mapped (6 concerns)

**✅ Architectural Decisions**
- [x] 6 critical decisions documented with rationale
- [x] Technology stack fully specified (5 new deps + existing)
- [x] Integration patterns defined (channel streaming, spawn_blocking_then for replace)
- [x] Performance considerations addressed (bounded channel, batch polling, cancellation)

**✅ Implementation Patterns**
- [x] 7 consistency patterns with code examples
- [x] Anti-patterns documented for each
- [x] Enforcement guidelines (7 mandatory rules)
- [x] Action namespace convention established

**✅ Project Structure**
- [x] Complete file listing with line estimates
- [x] Architectural layer boundaries defined
- [x] Widget communication boundaries mapped
- [x] Requirements to structure mapping complete
- [x] Data flow diagram provided
- [x] Test organization specified (unit, integration, widget, benchmarks)

### Architecture Readiness Assessment

**Overall Status:** READY FOR IMPLEMENTATION

**Confidence Level:** High — brownfield project with established patterns, all decisions consistent with existing codebase conventions.

**Key Strengths:**
- Strict layer separation (model/services/ui) prevents GTK leakage into testable code
- Channel-based streaming pattern is cleanly isolated as the one new pattern
- Skip-modified-tabs for Replace All eliminates the highest-risk coupling
- All 7 consistency patterns have concrete code examples agents can follow

**Areas for Future Enhancement:**
- Context lines (before/after match) in results — post-MVP
- Multi-line regex search — post-MVP
- Walker thread count as user-tunable GSettings key — post-MVP
- Incremental file index for near-instant repeated searches — Vision

### Implementation Handoff

**AI Agent Guidelines:**
- Follow all architectural decisions exactly as documented
- Use implementation patterns consistently across all components
- Respect project structure and boundaries
- Refer to this document for all architectural questions
- When in doubt, follow the existing LushText pattern from AGENTS.md

**First Implementation Priority:**
Phase 1 — Service layer: add dependencies, create `model/content_search.rs` and `services/content_search.rs` with unit tests and benchmarks. No UI, no GTK.
