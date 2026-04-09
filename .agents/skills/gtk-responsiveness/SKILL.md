---
name: gtk-responsiveness
description: "Guide and review Rust code for GTK4/Libadwaita responsiveness, performance, and memory efficiency. Uses parallel subagents for deterministic, focused reviews. Auto-invoked when writing or modifying UI code, async patterns, signal handlers, file I/O, TreeListModel usage, or any code that could block the main thread. Use whenever the user writes code in ui/, touches spawn_blocking_then or async_task, works with GLib signals, handles file operations, implements ListView/TreeListModel patterns, or discusses performance, responsiveness, threading, memory leaks, or 'app not responding' issues. Also trigger when reviewing UI code in pull requests."
---

Guide and review Rust code for keeping LushText buttery smooth — no "Waiting for application to respond" dialogs, no UI freezes, no janky scrolling, no memory leaks from signal handlers. The GTK main loop runs on a single thread; any blocking call on that thread freezes the entire UI. This skill ensures every I/O operation, heavy computation, and signal handler follows patterns that keep the main loop free and memory usage low.

## The Golden Rule

> **Never block the GTK main thread.**

The main thread runs the GLib main loop, which processes user input events, widget drawing, signal dispatch, timer callbacks, and D-Bus messages. If your code takes >16ms on the main thread (~60fps frame budget), the UI stutters. If it takes >5 seconds, the desktop environment shows "Application Not Responding." There is no exception.

For paned/revealer animations, "responsive" also includes **warning-free live geometry**. A sidebar toggle that feels smooth in a widget test but still logs `GtkBox ... needs at least ...` in the real app is not responsive enough to ship.

## Decision Matrix: Sync vs Async

| Operation | Time | Pattern | Where |
|-----------|------|---------|-------|
| Read small config file (<1KB) | <1ms | Sync on main thread | OK in `constructed()` or startup |
| Read user file (any size) | Variable | `spawn_blocking_then` | Always async |
| Write/save file | Variable | `spawn_blocking_then` | Always async |
| Scan directory listing | Variable | `spawn_blocking_then` | Always async |
| JSON parse small config | <1ms | Sync after async read | Parse in `then` callback |
| JSON parse large file | >10ms | Parse in background | Parse in `work` closure |
| Syntax highlighting | GtkSourceView | N/A | Don't reimplement |
| Tree model population | Per-directory | `spawn_blocking_then` | Return empty store, populate async |

**The 1ms Rule**: If an operation can exceed 1ms in the worst case (large file, slow disk, network mount, many entries), it must run off the main thread. The overhead of `spawn_blocking_then` is negligible compared to a UI freeze.

## Sidebar / Paned Animation Lessons

- Large restored workspace trees can make sidebar toggle stutter even when no explicit I/O runs during the animation. The problem can be per-frame relayout of the live subtree, not blocking calls.
- Snapshotting a heavy sidebar subtree with `GtkWidgetPaintable` can reduce animation cost, but only if the live child is truly removed from the paned's measurement path during the animation.
- GTK source matters here: `gtk_paned_size_allocate()` computes positions using the handle widget's natural size, and `gtk_revealer_measure()` scales and rounds child sizes during transitions. One-pixel geometry gaps are therefore common in live runs.
- Because of that, sidebar animation fixes must be validated in the real app (`make run`) against restored workspaces while watching stderr. Widget tests alone are not enough to prove geometry safety.

## Execution Model: Parallel Subagents

This skill uses **parallel subagents** for independent review concerns. Do NOT attempt to review all concerns inline — dispatch focused subagents instead.

### Workflow

1. **Identify changed files** — run `git diff --name-only` (or use the diff context if already available)
2. **Match trigger patterns** — for each subagent below, check its path globs and content patterns against the file list. A subagent triggers if any changed file matches a listed path glob OR contains a listed content pattern.
3. **Dispatch all relevant subagents in parallel** via the Agent tool — even if only one triggers, always dispatch as a subagent for consistent output format. In each prompt, replace `{changed_files}` with the actual file list from step 1.
4. **Aggregate results** — merge findings, deduplicate, produce the final report

### Memory Awareness

Each subagent prompt below includes inline memory-leak review criteria. This covers genuine leaks (strong reference cycles, missing `@weak`, signal handler accumulation) but explicitly excludes trivial allocation patterns.

## Severity Levels

- **[FLAG]** — Responsiveness hazard. Will cause user-visible UI freeze, ANR dialog, or memory leak that grows unbounded. Must fix.
- **[RECOMMEND]** — Performance improvement. Current code works but degrades under specific conditions. Fix proactively.
- **[CONSIDER]** — Future improvement. Not a problem today.
- **[GOOD]** — Existing correct pattern. Reinforce and protect from regression.

## Subagent Definitions

### 1. blocking-io-audit

**Triggers**:
- paths: `ui/**/*.rs`, `services/**/*.rs`
- content: `fs::read|fs::write|fs::read_to_string|fs::read_dir|fs::metadata|Command::new|std::process`

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for main-thread blocking.

Read the reference file at: .agents/skills/gtk-responsiveness/references/async-patterns.md
Focus on patterns 1-3 (Fire-and-Forget, Background Work with UI Update, Cancellable Background Work).

Changed files to review:
{changed_files}

The project uses a custom async primitive: crate::services::async_task::spawn_blocking_then(state, work, then)
- state: non-Send GTK object (auto-wrapped in ThreadGuard)
- work: FnOnce() -> T + Send, runs on background thread via std::thread::spawn
- then: FnOnce(S, T), runs on main thread via glib::idle_add_once
Do NOT recommend Tokio. This pattern is sufficient for file I/O.

While reviewing, also check for genuine memory leaks: strong reference cycles that prevent widget cleanup, missing `@weak` references in long-lived closures, signal handlers that accumulate without cleanup. Do NOT flag trivial clones, missing `Vec::with_capacity()`, or other micro allocation patterns — those are not responsiveness concerns.

Review criteria:
- Is any blocking I/O (fs::read_to_string, fs::write, fs::read_dir, fs::metadata, Command::new) called on the main thread outside spawn_blocking_then?
- Is heavy work done in the `then` callback? (Large JSON parsing, file processing should be in the `work` closure, not `then`)
- For file operations: is the path cloned/moved into the closure correctly? (Borrowed paths can't cross thread boundaries)
- Cancel tokens: for large file loads, does EditorPage store an Arc<AtomicBool> checked before AND after the I/O call?
- ThreadGuard vs SendWeakRef: is the correct cross-thread reference type used? ThreadGuard (used by spawn_blocking_then automatically) is for short-lived cross-thread references where the object is guaranteed to exist. SendWeakRef is for long-lived references (periodic timers, callbacks that may outlive the widget) — it returns None on upgrade if the widget was destroyed instead of panicking. Common mistake: using ThreadGuard in a periodic timer closure — if the widget is destroyed, into_inner() panics.
- For animated sidebars/panes: if the code avoids I/O but still resizes a heavy tree or list every frame, flag that as a responsiveness hazard anyway. A frozen snapshot or lighter animation surface may be required.

Anti-patterns to flag:
- [FLAG] std::fs::read_to_string, fs::write, fs::read_dir, fs::metadata, or Command::new in ui/ code outside spawn_blocking_then
- [FLAG] Large data parsing (serde_json::from_str on >10KB) in the `then` callback
- [RECOMMEND] Missing cancel token for file loads that may become stale (tab closed during load)
- [FLAG] ThreadGuard used in a periodic timer or long-lived callback — panics if widget is destroyed; use SendWeakRef instead
- [RECOMMEND] Paned animation keeps a large live tree/list widget in the measurement path for every frame even though a snapshot/clipping strategy is possible
- [CONSIDER] Synchronous small config reads at startup (<1KB) — acceptable but note it

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. What blocks. Duration estimate. Fix pattern.
```

### 2. signal-handler-audit

**Triggers**:
- content: `connect_notify|connect_changed|connect_clicked|connect_activate|connect_close`

**Subagent prompt** (self-contained — no reference file needed):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for signal handler correctness and memory efficiency.

Changed files to review:
{changed_files}

While reviewing, also check for genuine memory leaks: strong reference cycles that prevent widget cleanup, missing `@weak` references in long-lived closures, signal handlers that accumulate without cleanup. Do NOT flag trivial clones, missing `Vec::with_capacity()`, or other micro allocation patterns — those are not responsiveness concerns.

Review criteria:
- connect_notify_local vs connect_notify: closures that capture non-Send GTK objects MUST use connect_notify_local (or connect_*_local variants). The non-local variants require Send, which GTK objects are not.
- Signal handler weight: handlers should complete in <1ms. No I/O, no heavy computation.
- Weak references: long-lived closures capturing widget references should use @weak to prevent circular references and memory leaks. Strong references keep widgets alive forever.
- Handler cleanup: signals connected in loops or conditionally should store SignalHandlerId and disconnect on cleanup. Otherwise handlers accumulate and slow signal emission.
- freeze_notify/thaw_notify: when updating multiple GObject properties at once, batch with freeze_notify/thaw_notify to prevent intermediate signal emissions.
- Large data captures: signal closures live for the widget's entire lifetime. Flag closures that capture large indexes (FileIndex, Vec<IndexedFile>) by value — these should use @weak ref and access through imp() instead.

Anti-patterns to flag:
- [FLAG] connect_notify (not _local) when closure captures non-Send GTK objects — compile error or undefined behavior
- [FLAG] Strong reference to parent widget in child signal closure — memory leak (widget never freed)
- [RECOMMEND] Signal connections in a loop without cleanup strategy — handlers accumulate
- [RECOMMEND] Closure captures large index/collection by value instead of using @weak ref + imp() access
- [CONSIDER] Missing freeze_notify for multi-property updates

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Memory impact if applicable. Fix.
```

### 3. tree-factory-audit

**Triggers**:
- paths: `services/file_tree.rs`, `ui/sidebar/workspace_section/**/*.rs`
- content: `TreeListModel|SignalListItemFactory|connect_bind|connect_setup|connect_unbind`

**Subagent prompt** (self-contained — no reference file needed):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for TreeListModel and ListView factory correctness.

Changed files to review:
{changed_files}

While reviewing, also check for genuine memory leaks: strong reference cycles that prevent widget cleanup, missing `@weak` references in long-lived closures, signal handlers that accumulate without cleanup. Do NOT flag trivial clones, missing `Vec::with_capacity()`, or other micro allocation patterns — those are not responsiveness concerns.

Review criteria:
- autoexpand = true: NEVER set autoexpand to true on TreeListModel. It recursively calls create_model_func for EVERY directory, spawning unbounded threads (with spawn_blocking_then) or freezing the UI (with sync I/O).
- Lazy population: build_children_model should return an EMPTY ListStore immediately, then populate async via spawn_blocking_then. The then callback appends FileTreeItems; TreeListModel reacts to items-changed automatically.
- Factory bind performance: connect_bind is called on every scroll. It must be <1ms. No I/O, no new widget creation, no signal connections (disconnect in unbind if you must).
- Factory setup vs bind: connect_setup creates the widget structure once (recycled). connect_bind only sets properties. Never allocate widgets in bind.
- ListStore memory from recycling: GtkListView recycles list items. connect_unbind should reset bindings. connect_bind cleanup should remove any lingering widgets from previous binds (e.g., rename GtkEntry from row recycling).

Anti-patterns to flag:
- [FLAG] autoexpand = true on TreeListModel — catastrophic: unbounded thread spawns or UI freeze
- [FLAG] I/O (fs::read, network) in connect_bind — freezes UI on every scroll
- [FLAG] Widget creation (Label::new, Box::new) in connect_bind — leaks memory, breaks recycling
- [RECOMMEND] Signal connections in connect_bind without disconnect in connect_unbind
- [GOOD] Lazy population with empty ListStore + spawn_blocking_then

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Impact on scrolling/memory. Fix.
```

### 4. debounce-timer-audit

**Triggers**:
- content: `timeout_add|timeout_add_local|timeout_add_local_once|SourceId|search_changed`

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for debounce and timer lifecycle correctness.

Read the reference file at: .agents/skills/gtk-responsiveness/references/async-patterns.md
Focus on patterns 4-5 (Periodic Background Check, Debounced User Input).

Changed files to review:
{changed_files}

While reviewing, also check for genuine memory leaks: strong reference cycles that prevent widget cleanup, missing `@weak` references in long-lived closures, signal handlers that accumulate without cleanup. Do NOT flag trivial clones, missing `Vec::with_capacity()`, or other micro allocation patterns — those are not responsiveness concerns.

Review criteria:
- Debounce on rapid input: search entries, filter inputs, and similar rapid-fire text inputs should debounce (typically 150ms) to avoid redundant work. Empty queries should bypass debounce for instant clear.
- Generation counter pattern: for operations where results can become stale, use a Cell<u32> counter. Increment on each new operation, capture the value in the timer/callback, and no-op if the counter has advanced. This avoids SourceId lifecycle bugs entirely.
- SourceId lifecycle: if using Rc<Cell<Option<glib::SourceId>>> for debounce, ensure:
  - Previous SourceId is removed before scheduling a new one
  - SourceId is set to None after the callback fires
  - The Rc is not leaked (weak references where appropriate)
- Periodic timers: use SendWeakRef for the widget reference. Return ControlFlow::Break when the widget is destroyed. Never keep a widget alive solely through a timer closure.
- Timer memory: timer closures capture state for their entire lifetime. Avoid capturing large collections. Use @weak references for GTK objects.

Anti-patterns to flag:
- [FLAG] Timer closure with strong reference to widget — keeps widget alive after destruction, memory leak
- [RECOMMEND] Missing debounce on search/filter input — every keystroke triggers full processing
- [RECOMMEND] SourceId stored without cancellation logic — stale timers fire unexpectedly
- [CONSIDER] Generation counter vs SourceId — generation counter is simpler and avoids SourceId lifecycle bugs; prefer it for new code

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Timer lifecycle issue. Fix.
```

## Aggregation

After all subagents return, produce the unified report:

1. **Merge findings** — combine all [FLAG], [RECOMMEND], [CONSIDER], [GOOD] items from all subagents verbatim. Do not add new findings beyond what was reported.
2. **Deduplicate** — if two subagents flag the same line (e.g., blocking-io-audit and signal-handler-audit both flag a handler doing I/O), keep the more specific finding
3. **Memory summary** — add a "Memory Impact" subsection if any subagent found closure capture issues, signal handler leaks, or timer lifecycle problems
4. **Sort by severity** — FLAG first, then RECOMMEND, CONSIDER, GOOD
5. **Constrain** — only include findings that match an anti-pattern listed in the subagent definitions above. Do not flag patterns outside those checklists. Large file data-path concerns (streaming reads, buffer size limits, syntax gate thresholds) are outside this skill's scope — reviewed by `gtk-perf-scale`.

## Report Format

```
## Responsiveness Audit

### Summary
- **Files reviewed**: N
- **Findings**: X flag, Y recommend, Z consider, W good
- **Main-thread risk**: Brief assessment of blocking I/O exposure

### [FLAG] Title — file:line
Description of the issue.
**Blocks for**: Estimated duration on main thread.
**Impact**: What the user experiences (freeze, stutter, ANR).
**Memory**: RAM impact if applicable (closure capture, leak).
**Fix**: Concrete recommendation with code pattern reference.

### [RECOMMEND] Title — file:line
...

### [CONSIDER] Title — file:line
...

### [GOOD] Title — file:line
Why this pattern is correct.
```

## Guidance Mode

When implementing new features (not reviewing existing code), check:

1. Does this code path do ANY I/O? → Must use `spawn_blocking_then`
2. Does it connect signals? → Use `_local` variant if closure captures GTK objects; use `@weak`
3. Does it respond to rapid user input? → Add debounce (150ms typical)
4. Does it touch TreeListModel? → Ensure `autoexpand = false` and lazy population
5. Does the closure capture large state? → Move it to the imp struct, access via `&self`
6. Does a timer reference a widget? → Use `SendWeakRef` and `ControlFlow::Break`
7. Does a paned animation touch a large sidebar/tree subtree? → validate with `make run` on restored workspaces and inspect stderr for geometry warnings, not just widget tests

## Tone

Performance advice should be specific and measurable. Instead of "this might be slow," say "this blocks the main thread for ~50ms on a directory with 1000 entries." Instead of "this might leak," say "this closure captures a strong reference to the window — it will never be freed." Acknowledge existing good patterns before suggesting improvements.
