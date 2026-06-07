# Comment Patterns Reference

Detailed before/after examples for each comment category. All examples use LushText's GTK4/Libadwaita/Rust domain.

---

## Module-Level Docs (`//!`)

### Good: Architecture-aware

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! Background I/O utility for GTK4 applications.
//!
//! GTK widgets are not thread-safe (they contain raw pointers), so blocking
//! work (file reads, network calls, JSON serialization) must run on a
//! background thread. This module provides `spawn_blocking_then`, which
//! handles the thread-safety dance: wraps the GTK object in a `ThreadGuard`,
//! runs blocking work on a separate thread, and dispatches the result back
//! to the main thread via `glib::idle_add_once`.
//!
//! Lives in the services layer — no GTK widget dependencies, fully testable.
```

### Good: Constraint-first

```rust
// SPDX-License-Identifier: GPL-3.0-or-later

//! File size thresholds for graceful degradation of editor features.
//!
//! These constants encode domain policy, not presentation decisions — they
//! live in the services layer because the UI simply reacts to the
//! classification returned by `FileSizeCheck`.
```

### Bad: Name-only

```rust
//! Main application window.
```

This tells the reader nothing they couldn't guess from the file path. Better:

```rust
//! Main application window — orchestrates tabs, sidebar, status bar, and
//! command palette. Owns the `AdwTabView` and coordinates between all
//! child widgets through signal connections and shared state.
//!
//! This is the "driving adapter" in hexagonal terms: it translates user
//! interactions (tab clicks, keyboard shortcuts) into service-layer calls
//! and updates the UI with the results.
```

---

## Type Docs — Structs and Enums

### Good: Domain meaning + implementation rationale

```rust
/// A file discovered during workspace directory scanning, ready for fuzzy search.
///
/// Each indexed file stores its display path (relative to workspace root) for
/// search matching, plus a shared reference to the workspace root for resolving
/// the full filesystem path when the user selects a result.
pub struct IndexedFile {
    /// Path relative to the workspace root, used as the fuzzy search haystack.
    /// Example: `"src/ui/window/mod.rs"` for a file at
    /// `/home/user/project/src/ui/window/mod.rs`.
    pub relative_path: String,

    /// The workspace root directory. Shared via `Arc` across all files in the
    /// same workspace to avoid cloning the full path per file — a workspace
    /// with 50k files saves ~2.4MB (50k x 48 bytes/PathBuf).
    pub workspace_root: Arc<PathBuf>,
}
```

### Good: GObject wrapper type

```rust
/// Main application window — the top-level container for the editor UI.
///
/// This is a GObject wrapper around `AdwApplicationWindow` (Adwaita's
/// application window widget). The actual struct fields and implementation
/// live in `imp.rs`; this file provides the public API that other parts
/// of the application use to interact with the window.
///
/// Owns the tab view, sidebar, status bar, and command palette. Coordinates
/// file opening, tab management, and session persistence.
glib::wrapper! {
    pub struct LushtextWindow(ObjectSubclass<imp::LushtextWindow>)
        // This chain declares the GTK class hierarchy: our window IS an
        // AdwApplicationWindow, which IS a GtkApplicationWindow, etc.
        // Each level adds capabilities (AdwApplicationWindow adds Adwaita
        // styling, GtkApplicationWindow adds app-level actions, etc.)
        @extends
            libadwaita::ApplicationWindow,
            gtk4::ApplicationWindow,
            gtk4::Window,
            gtk4::Widget,
        @implements
            gio::ActionGroup,
            gio::ActionMap;
}
```

### Good: Enum with behavioral docs

```rust
/// Classification of file sizes for progressive feature degradation.
///
/// As files get larger, we disable expensive features one by one to keep
/// the editor responsive. The thresholds are based on measured GtkSourceView
/// performance on mid-range hardware (8GB RAM, NVMe storage).
pub enum FileSizeCheck {
    /// Normal file — all features enabled.
    Normal,
    /// >1MB — show a toast warning about potential slowness.
    LargeWithToast,
    /// >10MB — disable syntax highlighting (GtkSourceView's regex engine
    /// takes >500ms for initial highlight pass at this size).
    DisableSyntax,
    /// >50MB — disable undo history (GtkTextBuffer's undo B-tree becomes
    /// a significant memory consumer at this scale).
    DisableUndo,
    /// >500MB — refuse to open. `buffer.set_text()` for 500MB allocates
    /// ~1GB and blocks the main thread for 5-10 seconds.
    TooLarge,
}
```

### Bad: imp fields without context

```rust
pub struct LushtextWindow {
    index_rebuild_generation: Cell<u32>,
    last_sidebar_pos: Cell<i32>,
    pending_sidebar_pos: Cell<bool>,
    saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
}
```

These `Cell`/`RefCell` fields are opaque. Better:

```rust
pub struct LushtextWindow {
    /// Generation counter for debouncing file index rebuilds. Each workspace
    /// mutation increments this; the rebuild callback no-ops if the counter
    /// has advanced (meaning a newer rebuild superseded it).
    index_rebuild_generation: Cell<u32>,

    /// Last persisted sidebar position in pixels. Compared against current
    /// position to avoid redundant GSettings writes on every `size_allocate`.
    last_sidebar_pos: Cell<i32>,

    /// Whether a sidebar position change is pending GSettings persistence.
    /// Guards against writing stale values during rapid resize sequences.
    pending_sidebar_pos: Cell<bool>,

    /// Saved focus widget before opening a modal overlay (command palette).
    /// Uses `WeakRef` (not a strong ref) to avoid preventing widget
    /// finalization if the focused widget is destroyed while the overlay
    /// is open.
    saved_focus: RefCell<Option<glib::WeakRef<gtk4::Widget>>>,
}
```

---

## Function Docs

### Good: Threading model + side effects

```rust
/// Runs `work` on a background thread, then calls `then` on the GTK main
/// thread with the result.
///
/// `state` is a GTK object (not thread-safe) that the `then` callback needs.
/// It's wrapped in a `ThreadGuard` automatically — this makes it `Send` by
/// enforcing same-thread access at runtime.
///
/// Back-pressure: when all spawn slots are occupied (8 concurrent tasks),
/// the call is deferred by 50ms and retried. This prevents RAM spikes
/// during burst operations like session restore with many tabs.
pub fn spawn_blocking_then<S, T, W, F>(state: S, work: W, then: F)
```

### Good: UX sequencing explained

```rust
/// Opens a file in a new tab, or focuses the existing tab if already open.
///
/// The tab appears immediately with a loading indicator; file content loads
/// asynchronously on a background thread to keep the UI responsive on slow
/// filesystems (NFS, USB). Duplicate detection uses an O(1) `open_paths`
/// HashSet lookup — no linear tab scan needed.
pub fn open_document(&self, path: &Path) {
```

### Good: Signal handler with trigger context

```rust
/// Called when the active tab changes in the tab bar.
///
/// GTK emits `notify::selected-page` on the `AdwTabView` whenever the
/// active tab changes — including during creation, reordering, and closing.
/// This handler refreshes all tab-dependent UI: header bar title/subtitle,
/// status bar metadata, and triggers evicted-tab reload if the newly
/// focused tab was evicted from memory.
fn on_selected_page_changed(&self) {
```

### Bad: Name restatement

```rust
/// Refreshes the header bar.
fn refresh_header_bar(&self) {
```

Better:

```rust
/// Updates the header bar title and subtitle to reflect the active tab.
///
/// Title shows the filename (or "Untitled" for new files). Subtitle shows
/// the parent directory path. Both are cleared when no tabs are open.
fn refresh_header_bar(&self) {
```

---

## Inline Comments

### Good: GTK quirk explanation

```rust
// GtkTreeExpander installs an internal GestureClick (at BUBBLE phase) that
// claims mouse events for ALL rows, even non-expandable file rows. This
// prevents GtkListView's built-in double-click activation from ever firing
// on files. Fix: disable the gesture entirely for file rows so double-click
// reaches the ListView. Directory rows keep it for expand/collapse.
if !item.is_directory() {
    if let Some(gesture) = find_gesture_click(&expander) {
        gesture.set_propagation_phase(gtk4::PropagationPhase::None);
    }
}
```

### Good: Intentional omission

```rust
// If undo is disabled (file >50MB), we intentionally do NOT call
// end_irreversible_action(). This keeps the buffer permanently in
// "irreversible" mode, preventing GtkTextBuffer from recording any
// undo history — which would consume significant memory at this scale.
if size_check.undo_enabled() {
    buffer.end_irreversible_action();
}
```

### Good: Algorithmic choice

```rust
// Iterate tabs in reverse so that removing a page doesn't shift the
// indices of pages we haven't visited yet. Without this, closing tab 2
// would shift tab 3 to index 2, causing us to skip it.
for i in (0..tab_view.n_pages()).rev() {
```

### Good: Performance rationale

```rust
// Use splice() to replace all items in a single operation. This emits
// one items-changed signal instead of N individual signals from append()
// calls, which avoids N separate ListView relayout passes.
list_store.splice(0, list_store.n_items(), &new_items);
```

### Good: Thread boundary

```rust
// Snapshot the buffer text on the main thread before spawning. GtkTextBuffer
// is not Send (contains raw pointers), so we must extract the text as a
// String here. For a 10MB buffer this copies ~20MB (UTF-8 + GLib's internal
// representation), but it's the only safe approach.
let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
```

### Good: Guard clause preventing subtle bug

```rust
// Guard: if the entry was already removed (confirm removed it, then
// focus-out fires for the now-detached widget), bail out. Without this
// check, we'd try to remove a widget that no longer has a parent, which
// panics in GTK.
if entry.parent().is_none() {
    return;
}
```

### Bad: Code restating

```rust
// Check if the path is a directory
if path.is_dir() {

// Create a new HashMap
let map = HashMap::new();

// Return early if None
let Some(page) = page else { return };
```

### Bad: Vague

```rust
// Handle the edge case
if count > MAX_ENTRIES {
```

Better:

```rust
// Cap directory entries at 10,000 to prevent GtkListView from stalling
// on huge directories (e.g., node_modules with 100k+ entries). A
// placeholder row is shown when truncation occurs.
if count > MAX_ENTRIES {
```

---

## Constants

### Good: Value justification with measurement

```rust
/// Maximum estimated buffer memory before evicting background tabs.
///
/// 256MB is comfortable on 8GB machines (leaves room for OS, GTK overhead,
/// and other apps). Memory estimation uses `file_size * 2` to approximate
/// GtkTextBuffer's internal overhead (B-tree structure, line index, and
/// undo stack).
const BUFFER_MEMORY_BUDGET: u64 = 256 * 1024 * 1024;
```

### Good: Threshold with consequence

```rust
/// Files larger than this have syntax highlighting disabled.
///
/// GtkSourceView's regex-based syntax engine performs a full-buffer scan
/// for context on load. Above 10MB, the initial highlight pass exceeds
/// 500ms on mid-range hardware, causing a visible UI freeze.
const DISABLE_SYNTAX_HIGHLIGHTING: u64 = 10 * 1024 * 1024;
```

### Bad: No justification

```rust
const MAX_CONCURRENT_SPAWNS: usize = 8;
```

Better:

```rust
/// Maximum concurrent background threads for `spawn_blocking_then`.
///
/// 8 balances parallelism with RAM usage — each thread may hold a file
/// buffer snapshot (up to 50MB). During session restore with many tabs,
/// this cap prevents spawning dozens of threads simultaneously.
const MAX_CONCURRENT_SPAWNS: usize = 8;
```

---

## Configuration Files

### Good: Dependency groups with rationale (Cargo.toml)

```toml
# --- Centralized dependencies (union of features across all crates) ---

# GTK / GNOME stack — all must be from the same 0.11/0.9/0.22 release series
gtk4 = "0.11"
libadwaita = "0.9"

# Fuzzy matching engine — SIMD-accelerated (AVX2/NEON) for sub-millisecond
# scoring of 100k+ file candidates in the command palette
nucleo-matcher = "0.3"

# SIMD-accelerated UTF-8 validation for large files (>10MB) — keeps editor
# loading on the filesystem byte-read boundary instead of scalar string reads
simdutf8 = "0.1"
```

### Good: Build profile with flag explanations (Cargo.toml)

```toml
[profile.dev]
# Line-tables-only debug info: enough for backtraces and basic debugging,
# but ~60% smaller than full debug info — cuts link time significantly
debug = "line-tables-only"

[profile.dev.package."*"]
# Compile all dependencies at O2 even in debug builds. Dependencies rarely
# change (cached by cargo), so the one-time compile cost pays for itself
# in faster runtime during development
opt-level = 2

[profile.release]
# Thin LTO: cross-crate optimization with ~3x faster link than full LTO.
# Combined with codegen-units=1, produces binaries within ~2% of full LTO.
lto = "thin"
# Strip debug symbols from release binary — saves ~30MB
strip = true
# Single codegen unit enables maximum optimization across the crate
codegen-units = 1
```

### Bad: No context

```toml
simdutf8 = "0.1"
nucleo-matcher = "0.3"
```

Why are these here? What do they do? A new contributor has to leave the file and look them up.
