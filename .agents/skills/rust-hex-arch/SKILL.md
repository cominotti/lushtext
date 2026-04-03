---
name: rust-hex-arch
description: "Evaluate Rust code changes against Hexagonal Architecture, CQS, and DDD principles adapted for GTK4/Libadwaita desktop applications. Auto-invoked on any .rs file changes in the LushText codebase. Pragmatically assesses domain model purity, service/UI layer boundaries, CQS compliance, dependency direction, GObject subclassing patterns, and module structure. Use whenever Rust files are created, modified, or refactored — or when the user discusses architecture, module organization, separation of concerns, layer boundaries, where code should live, or how to structure a new feature. Also trigger when reviewing code, creating pull requests, or when any agent modifies .rs files."
---

Evaluate Rust code changes against Hexagonal Architecture, Command-Query Separation (CQS), and Domain-Driven Design (DDD). The goal is to guide the codebase toward better architecture gradually — the current structure already follows hex arch naturally, so the focus is on maintaining and deepening that alignment as features grow. Prioritize correctness, then simplicity, then testability, then maintainability.

Assume developers may not know these patterns in a Rust/GTK4 context. When using a term from the Concept Glossary (at the end of this document), include a one-line explanation the first time it appears in the report. When genuinely ambiguous tradeoffs exist, present options with a strong recommendation. When the right answer is clear, state it directly.

## Pragmatism Guardrails

These principles override pattern-matching instinct. When in doubt, favor the simpler option:

1. **GObject subclassing IS your adapter pattern.** Every `mod.rs` + `imp.rs` widget pair is a driving adapter. The `glib::wrapper!` macro defines the public interface, `imp.rs` holds the private implementation. Adding another abstraction layer between the UI and services is noise.

2. **Free functions are simpler than trait objects for single-implementation services.** `workspace_manager::load(path)` is clearer than `dyn WorkspaceStore`. Only introduce a trait when there are multiple implementations, when testability demands a seam that does not exist (e.g., replacing real file I/O in tests), or when the abstraction genuinely clarifies the domain. See `references/port-patterns.md` for the decision matrix.

3. **`RefCell<T>` interior mutability in `imp` structs is the standard pattern.** GObject's single-ownership model requires interior mutability for runtime state. `RefCell<Option<PathBuf>>` for a file path, `RefCell<Vec<T>>` for a collection — these are the GTK4-rs convention, not a code smell.

4. **GtkSourceView, AdwTabView, and TreeListModel are natural ports.** These GTK4 widgets provide well-designed contracts for text editing, tab management, and hierarchical data display. Wrapping them in custom abstractions adds indirection without benefit. See the Natural Ports table in Step 3.

5. **`spawn_blocking_then` IS your async adapter.** This project uses raw `std::thread::spawn` + `glib::idle_add_once` with `ThreadGuard` for safe thread crossing. Do not recommend adding Tokio, async-std, or any async runtime for I/O tasks. The GLib main loop IS the event loop. The only reason to introduce an async runtime would be if the app needed to manage many concurrent network connections (it does not).

6. **Signal closures are adapter glue, not business logic.** A signal handler that translates a `GtkListView::activate` signal into a `window.open_document(path)` call is adapter code. Keep closures thin — they should delegate to service functions or widget methods, not contain conditional business logic. If a closure exceeds ~5 lines of non-delegation code, the logic probably belongs in a service function.

7. **A crate boundary is stronger than a module boundary.** The two-crate workspace (`lushtext` binary + `lushtext-core` library) already enforces the primary boundary. Module-level `pub` visibility is sufficient for internal layering. Do not recommend splitting into more crates (like separate `domain`/`ports`/`adapters` crates) unless the project grows to 10k+ lines in `lushtext-core`.

8. **Not every function needs a port trait.** A function signature IS a contract. `pub fn load(data_dir: &Path) -> Result<WorkspacesFile>` defines the port implicitly. Only extract a `trait` when the benefit is concrete and immediate: multiple implementations, mock injection for testing, or the abstraction genuinely names a domain concept that free functions obscure.

9. **Domain types in `model/` must have zero GTK/GLib dependencies.** This is non-negotiable. The `model/` module contains pure Rust types with `serde` derives. If a type needs a GObject wrapper (like `FileTreeItem`), that wrapper belongs in the UI layer, not in `model/`. The domain types are the core that everything else depends on — they must remain portable and testable without a display server.

10. **`gio::ListStore` and other GLib collection types belong in the UI layer, not services.** Services should return standard Rust types (`Vec<T>`, `HashMap<K,V>`). The conversion to GLib types (`gio::ListStore`, `glib::BoxedAnyObject`) should happen at the adapter boundary — in the UI code that consumes the service result.

## Target Module Structure

Feature-first with layer separation within `lushtext-core/src/`. The structure below reflects the current layout plus the target direction for new features:

```
crates/lushtext-core/src/
├── app.rs                       # Framework Glue: AdwApplication subclass, app-level actions
├── config.rs                    # Framework Glue: compile-time constants (APP_ID, VERSION)
├── lib.rs                       # Framework Glue: GResource registration, CSS loading, run()
├── model/                       # Domain: pure Rust types, serde derives, zero GTK deps
│   ├── workspace.rs             #   WorkspaceId, WorkspaceEntry, WorkspaceConfig, WorkspacesFile
│   ├── document.rs              #   DocumentId
│   └── session.rs               #   SessionTab, SessionData
├── services/                    # Application + Driven Adapters
│   ├── async_task.rs            #   Infrastructure: spawn_blocking_then utility
│   ├── json_store.rs            #   Driven Adapter: JSON file persistence
│   ├── workspace_manager.rs     #   Application: workspace CRUD operations
│   ├── session_service.rs       #   Application: session save/restore
│   └── file_tree.rs             #   Application: directory scanning logic
│                                #   (NOTE: GTK type construction should move to UI)
└── ui/                          # Driving Adapters: GTK4/Libadwaita widgets
    ├── window/                  #   Main window: tab management, file opening, actions
    ├── editor_page/             #   GtkSourceView wrapper: file loading, editing, search
    ├── sidebar/                 #   File tree: ListView + TreeListModel + TreeExpander
    │   └── file_tree_item.rs    #   GObject data wrapper for tree entries
    ├── search_bar/              #   Find/replace widget
    └── preferences/             #   AdwPreferencesDialog
```

### Bounded Context Mapping

Each bounded context corresponds to a coherent domain area:

| Bounded Context | Domain (`model/`) | Application (`services/`) | UI (`ui/`) |
|----------------|-------------------|--------------------------|------------|
| **Workspace** | `WorkspaceId`, `WorkspaceEntry`, `WorkspaceConfig`, `WorkspacesFile` | `workspace_manager` | `sidebar` (workspace display), `preferences` (workspace settings) |
| **Session** | `SessionTab`, `SessionData` | `session_service` | `window` (tab state capture/restore) |
| **Editing** | `DocumentId` | _(GtkSourceView is the natural port)_ | `editor_page`, `search_bar` |
| **File Browsing** | _(implicit: paths)_ | `file_tree` (scan logic) | `sidebar` (tree display), `file_tree_item` |

## Dependency Direction Rules

Dependencies must point inward: **UI → Services → Model**. Never the reverse.

```
  ┌─────────────────────────────────────────┐
  │            ui/ (Driving Adapters)        │
  │  Depends on: services/, model/, GTK4    │
  └──────────────────┬──────────────────────┘
                     │ calls
  ┌──────────────────▼──────────────────────┐
  │          services/ (Application)         │
  │  Depends on: model/, std, serde, anyhow │
  │  Must NOT depend on: ui/, GTK4, GLib    │
  └──────────────────┬──────────────────────┘
                     │ uses
  ┌──────────────────▼──────────────────────┐
  │           model/ (Domain)                │
  │  Depends on: std, serde ONLY            │
  │  Must NOT depend on: services/, ui/,    │
  │    GTK4, GLib, gio, any I/O crate       │
  └─────────────────────────────────────────┘
```

**Exception for driven adapters**: Service modules that perform I/O (like `json_store.rs`) inherently depend on `std::fs`. This is acceptable — they ARE the driven adapters. The key rule is that they must not depend on GTK/GLib types or on the `ui/` module.

**`async_task.rs` exception**: This module imports `gtk4::glib` because its entire purpose is bridging background threads with the GLib main loop. It is infrastructure glue, not application logic.

## Severity Levels

- **[FLAG]** — Architectural violation that will cause real problems (dependency direction wrong, business logic in signal closures, domain types with GTK deps). Recommend fixing in the current change.
- **[RECOMMEND]** — Meaningful improvement. When genuinely ambiguous, includes a tradeoff discussion. Developer decides.
- **[CONSIDER]** — Minor observation. Current approach is acceptable. Brief mention, no action required.
- **[GOOD]** — Existing pattern that already follows Hex Arch/CQS/DDD well. Reinforces good habits and teaches by example. Include when code genuinely follows the patterns — do not fabricate praise.

## Step 0: Identify Changed Files and Classify by Zone

Determine which files changed using git (try `git diff --name-only`, then `--cached`, then `HEAD~1`, then `git status --porcelain`). Filter to `.rs` files in `crates/lushtext-core/src/`. Skip deleted files, test modules (`#[cfg(test)]`), and generated code.

**Classify each file into a zone** based on its module path and responsibilities:

| Zone | Path Pattern | Characteristics | Scrutiny |
|------|-------------|-----------------|----------|
| **Domain** | `model/*.rs` | Pure Rust types, serde derives, no I/O, no GTK | Full |
| **Application** | `services/*.rs` (logic) | Business rules, data transformations, orchestration. No GTK, no GLib. | Full |
| **Driven Adapter** | `services/*.rs` (I/O) | File I/O, JSON persistence. Uses `std::fs`, `serde_json`. No GTK. | Moderate |
| **Driving Adapter** | `ui/**/*.rs` | GTK4 widgets, signal handlers, template bindings. Delegates to services. | Light |
| **Framework Glue** | `app.rs`, `lib.rs`, `config.rs` | Application lifecycle, GResource init, CSS loading, app-wide actions | Minimal |

For files that span zones (e.g., a service that imports GTK types), classify by primary responsibility and flag the zone-crossing code for extraction.

State the zone classification and reasoning at the top of each file's review.

## Step 1: Domain Zone Review (`model/`) — Full Scrutiny

### 1a. Type Design

Domain types should be data-focused Rust structs with `#[derive(Serialize, Deserialize, Debug, Clone)]`. They should:
- Use newtypes for identity (`WorkspaceId(String)`, `DocumentId(PathBuf)`) rather than bare primitives when the type appears in 3+ signatures or could be confused with another.
- Use enums for variants (`WorkspaceEntry::Directory` vs `WorkspaceEntry::File`) — Rust enums are the natural encoding for DDD value objects with behavior variants.
- Implement validation in constructors or `TryFrom` when invariants exist.

**Do NOT flag**: Simple wrapper structs without validation — not every ID needs constructor validation.

### 1b. Domain Model Richness

- **Rich model (good)**: Business rules live on the type. `WorkspacesFile` could have an `active_workspace()` method that enforces the "always has an active workspace" invariant.
- **Anemic model (flag when behavior belongs to the type)**: If multiple service functions perform the same transformation on a domain type's fields, that logic belongs as a method on the type.
- **Do NOT flag**: DTOs that are intentionally data-only (e.g., `SessionTab` whose fields are just cursor positions).

### 1c. CQS on Domain Types

- **`&self` methods** should be queries: return data, no mutation. Good: `fn active_workspace(&self) -> Option<&WorkspaceConfig>`.
- **`&mut self` methods** should be commands: mutate state, return `()` or `Result<()>`. Good: `fn add_entry(&mut self, entry: WorkspaceEntry)`.
- Flag methods that take `&mut self` AND return meaningful data (beyond `Result<()>`).

### 1d. Dependency Purity

Domain types must NOT import:
- `gtk4::*`, `libadwaita::*`, `glib::*`, `gio::*`, `sourceview5::*`
- `std::fs`, `std::net`, or any I/O
- Anything from `crate::ui` or `crate::services`

Domain types MAY import: `serde`, `std::path::{Path, PathBuf}` (paths are data, not I/O), `std::collections::*`, `anyhow`/`thiserror` for error types.

## Step 2: Application Zone Review (`services/` logic) — Full Scrutiny

### 2a. Service Function Design

Service functions should be stateless free functions (no `struct` with `impl`). They take their dependencies as parameters: paths, config values, or domain types. This makes them trivially testable — pass a temp directory path instead of a real one.

```rust
// Good: free function, takes path parameter
pub fn load(data_dir: &Path) -> Result<WorkspacesFile>

// Avoid: struct with state (unless managing a connection pool or cache)
pub struct WorkspaceManager { data_dir: PathBuf }
impl WorkspaceManager { pub fn load(&self) -> Result<WorkspacesFile> }
```

**Exception**: A struct is justified when it manages a long-lived resource (connection, cache, channel) or when multiple operations share expensive initialization.

### 2b. CQS Compliance

- **Queries** return `T` or `Result<T>` and do not write to disk, mutate shared state, or produce side effects (logging is acceptable).
- **Commands** return `()` or `Result<()>` and perform a mutation (write file, update state).
- Flag functions that save data AND return the loaded result in the same call — split into `save()` then `load()`.
- **Do NOT flag**: `active_workspace()` which creates a default workspace if none exists — this is a domain rule (ensure-exists), not a CQS violation.

### 2c. No GTK Dependencies

Application-layer services must NOT import `gtk4`, `libadwaita`, `glib`, `gio`, or `sourceview5`. They must not construct GObject types (`gio::ListStore`, `glib::Object`, any `glib::wrapper!` type).

**Current violation to be aware of**: `file_tree.rs` imports `crate::ui::sidebar::file_tree_item::FileTreeItem` and constructs `gio::ListStore`. The pure scanning logic (`scan_directory`) is correctly separated, but the public functions `build_root_model` and `build_children_model` return GTK types. The scan logic belongs in services; the GTK model construction belongs in the UI layer.

### 2d. Error Handling

Services return `anyhow::Result`. They should NOT panic, unwrap without justification, or silently swallow errors. Use `tracing::warn!` or `tracing::error!` for recoverable failures (like a missing directory), and propagate `Result` for failures the caller must handle.

## Step 3: Driving Adapter Zone Review (`ui/`) — Light Scrutiny

### 3a. Thin Signal Handlers

Signal closures should translate UI events into service calls or widget method calls. They should NOT contain:
- Business logic (conditional rules, data transformations, validation beyond basic null-checks)
- Direct file I/O (use `spawn_blocking_then` via the service layer)
- Multi-step orchestration (extract to a method on the widget's `mod.rs`)

**Good pattern** — signal handler delegates immediately:
```rust
self.sidebar.connect_file_activated(move |path| {
    window.open_document(path);
});
```

**Flag** — business logic in a signal closure:
```rust
self.sidebar.connect_file_activated(move |path| {
    if path.extension().map_or(false, |e| e == "md") {
        // ... 15 lines of markdown-specific logic
    }
});
```

### 3b. Natural Ports

These GTK4 types serve as hexagonal ports — well-designed contracts that the UI layer depends on. Do NOT recommend wrapping them:

| GTK4 Type | Hex Arch Role | Why It's a Natural Port |
|-----------|---------------|------------------------|
| `sourceview5::Buffer` | **Editing Port** | Owns document text, undo/redo, syntax highlighting. The domain's text-editing contract. |
| `sourceview5::View` | **Display Port** | Renders the buffer with line numbers, margins, word wrap. |
| `libadwaita::TabView` | **Tab Management Port** | Manages tab lifecycle, ordering, drag-and-drop. |
| `gtk4::TreeListModel` | **Hierarchical Data Port** | Lazy tree expansion with `create_model_func` callback. |
| `gio::ListStore` | **Observable Collection** | Signals `items-changed` for reactive UI updates. |
| `libadwaita::StyleManager` | **Theme Port** | `is_dark()` + `connect_dark_notify()` for theme reactivity. |

### 3c. No Business Logic in `imp.rs`

The `imp.rs` file should contain only: `ObjectSubclass` trait implementations, `CompositeTemplate` bindings, `constructed()` for signal wiring, and property definitions. Business decisions belong in `mod.rs` methods (which delegate to services) or in the service layer.

### 3d. `mod.rs` as Public API

The widget's `mod.rs` defines its public API — the methods other widgets call. This is the widget's "port" from the perspective of other UI code. Keep the API small and intention-revealing:
```rust
// Good: clear intent
impl LushtextWindow {
    pub fn open_document(&self, path: &Path) { ... }
    pub fn new_tab(&self) { ... }
    pub fn save_current(&self) { ... }
}
```

## Step 4: Driven Adapter Zone Review (`services/` I/O) — Moderate Scrutiny

### 4a. I/O Isolation

Driven adapters (`json_store.rs`, the I/O parts of `file_tree.rs`) should isolate I/O behind a clean function boundary. The caller passes a path; the function does the I/O and returns a domain type.

```rust
// Good: clean boundary — takes path, returns domain type
pub fn load(data_dir: &Path) -> Result<WorkspacesFile>

// Less good: I/O scattered inline in application logic
let contents = std::fs::read_to_string(&path)?;
let data: WorkspacesFile = serde_json::from_str(&contents)?;
```

### 4b. Port Trait Decision

Read `references/port-patterns.md` for the full decision matrix. The short version:

| Situation | Pattern | Example |
|-----------|---------|---------|
| Single implementation, simple I/O | Free function | `json_store::load::<T>(path)` |
| Need to mock in tests | Trait parameter | `fn process(store: &impl WorkspaceStore)` |
| Multiple implementations | Trait object | `Box<dyn FileScanner>` for local vs remote |
| Cross-cutting infrastructure | Utility function | `async_task::spawn_blocking_then` |

### 4c. No Upward Dependencies

Driven adapters must NOT import from `crate::ui`. They may import from `crate::model` (they return domain types) and from `std::fs`, `serde_json`, etc. (they perform I/O).

## Step 5: Framework Glue Zone Review — Minimal Scrutiny

Only check: `app.rs`, `lib.rs`, and `config.rs` must not contain business rules. Application-level actions (`quit`, `about`, `preferences`) are routing, not domain logic. CSS loading, GResource registration, and `tracing` setup are infrastructure.

## Step 6: Module Structure Review

**For files IN the current diff** that are not in their target module: recommend moving to the correct layer as a [RECOMMEND] or [CONSIDER].

**For NEW files**: Guide to the correct module path directly. Ask: "Does this type need GTK? → `ui/`. Does it need I/O? → `services/`. Is it pure data? → `model/`."

**Do NOT recommend moves for files NOT in the current diff.**

**Shared code rule**: If a utility in `services/` is only used by one UI widget, it may actually be UI-layer code misplaced in services. If it doesn't need I/O, it can live in the widget's module.

## Step 7: Produce the Report

```
## Hex Arch / CQS / DDD Review

### Summary
- Files reviewed: N
- Zone classification: N domain, N application, N driven adapter, N driving adapter, N glue
- Findings: N (X flag, Y recommend, Z consider, W good)

### File: path/to/file.rs
**Zone**: Application

#### [GOOD] Clean service function design
`workspace_manager::load` is a free function taking `&Path`, returning `Result<WorkspacesFile>`.
Pure application logic with no GTK dependencies. Easily testable with a temp directory.

#### [FLAG] Upward dependency on UI layer
`file_tree.rs` imports `crate::ui::sidebar::file_tree_item::FileTreeItem` — a GObject
wrapper from the driving adapter zone. This couples the service layer to the UI.
**Fix**: Move `build_root_model` and `build_children_model` (the functions that construct
`gio::ListStore`) to the sidebar UI module. Keep `scan_directory` in services — it returns
`Vec<(PathBuf, bool)>` which is a clean, framework-free result.

#### [RECOMMEND] Extract domain method (with tradeoff)
`session_service::filter_existing_tabs` mutates `SessionData` in place. This validation
logic could live as `SessionData::retain_existing_tabs(&mut self)` — making the domain type
richer and the service thinner.
| Criteria | Keep in service | Move to domain type |
|----------|----------------|-------------------|
| Testability | Same | Same (both easy) |
| Cohesion | Logic near I/O | Logic near data |
| **Recommendation** | **Fine for now** | **Better as domain grows** |
```

## What NOT to Flag

- `RefCell<T>` or `Cell<T>` in `imp.rs` — standard GObject interior mutability
- `#[derive(CompositeTemplate)]`, `#[template_child]` — framework metadata, not coupling
- `glib::wrapper!` macro invocations — the GObject type system bridge
- `ensure_type()` calls in `class_init()` — required for template parsing
- `connect_*_local()` closures that are ≤5 lines and only delegate
- `tracing::info!` / `tracing::warn!` in services — observability is not a side effect for CQS
- Test modules (`#[cfg(test)]`) — different rules apply
- `config.rs` constants — compile-time configuration is framework glue
- `PathBuf` in domain types — paths are data, not I/O

## Concept Glossary

| Term | Explanation |
|------|------------|
| **Hexagonal Architecture** | Business logic has zero dependencies on frameworks or I/O — external access goes through ports implemented by adapters. In Rust/GTK4: `model/` depends on nothing; `services/` depends on `model/`; `ui/` depends on both. |
| **Port** | A boundary contract. In Rust: either a function signature (implicit port) or a `trait` definition (explicit port). Driving ports are called by adapters to enter the application. Driven ports are called by the application to reach infrastructure. |
| **Driving Adapter** | Inbound adapter — translates external events into application calls. In GTK4: the `ui/` widgets that handle user input and call service functions. |
| **Driven Adapter** | Outbound adapter — implements infrastructure access. In this project: `json_store.rs` (file persistence), `file_tree.rs::scan_directory` (directory listing). |
| **Natural Port** | A framework type that serves as a well-designed contract without needing a custom wrapper. GtkSourceView's `Buffer` is a natural port for text editing — it provides undo/redo, syntax highlighting, and modification tracking. |
| **Value Object** | Immutable, identity-free, equality by value. In Rust: a `struct` or `enum` with `#[derive(PartialEq, Eq, Clone)]`. Examples: `WorkspaceEntry`, `SessionTab`. |
| **Entity** | Object with unique identity. Two entities with same data but different IDs are different. Example: `WorkspaceConfig` (identified by `WorkspaceId`). |
| **Aggregate** | Cluster of domain objects treated as a unit. The aggregate root controls access. Example: `WorkspacesFile` is the aggregate root — you modify workspaces through it, not directly. |
| **CQS** | Every function either changes state (command, returns `()`) or returns data (query, no side effects) — never both. Applied to service functions and domain type methods. |
| **Bounded Context** | A boundary within which a domain model applies. In LushText: Workspace, Session, Editing, File Browsing are separate contexts. |
| **Dependency Direction** | Dependencies point inward: `ui/` → `services/` → `model/`. The domain never imports infrastructure or UI code. |
| **Interior Mutability** | Rust pattern using `RefCell<T>` or `Cell<T>` to mutate data behind a shared reference. Required in GObject `imp` structs because GTK holds shared references to widgets. |
| **Framework Glue** | Code that wires the application together: `AdwApplication` setup, GResource registration, CSS loading, app-level actions. Contains no business logic. |

## Tone

Present findings as a thinking partner, not a linter. Explain the "why" behind each finding — architecture rules exist to enable testability, maintainability, and team scalability. Acknowledge what works before suggesting improvements. Draft concrete code when recommending changes. When reviewing existing code (not just new changes), focus [GOOD] findings on patterns worth reinforcing and [RECOMMEND] on the highest-value improvements.
