---
name: rust-comments
description: "Review and guide code commenting for Rust and configuration files in GTK4/Libadwaita desktop applications. Ensures every struct, function, and non-obvious code path has a clear, friendly comment that explains 'why' and orients developers who may not know Rust or GTK well. Maintains high signal-to-noise by skipping obvious code and focusing on architectural decisions, GTK/GLib patterns, threading constraints, and design trade-offs. Auto-invoked on any .rs file changes in the LushText codebase. Use whenever writing new code, reviewing code, modifying configuration files, adding modules or types, or when any other skill (rust-hex-arch, gtk-perf-review, gtk-responsiveness) produces code changes. Also trigger when the user discusses documentation, comments, readability, onboarding, 'what does this do', or code clarity. This is the single authority on what, where, and how to comment in this codebase."
---

# Rust Comments

Comments in this codebase serve as the primary onboarding mechanism. A developer unfamiliar with Rust, GTK4, or GLib should be able to read any file and understand not just *what* it does, but *why* it's structured that way. Without good comments, the codebase becomes a locked room that only its original authors can navigate.

At the same time, comments that restate what the code already says are noise. Every comment must teach something the code alone cannot convey.

## Who Reads This Code

Assume the reader is:
- **Smart but unfamiliar** — a competent developer who hasn't worked with Rust, GTK4, or GLib before
- **Coming in cold** — they opened this file from a search result or stack trace, without reading every other file first
- **Time-pressured** — they need to understand this code quickly to fix a bug or add a feature

This means:
- Rust-specific patterns (interior mutability, trait objects, `unsafe` justifications) need explanation at point of use
- GTK/GLib patterns (`glib::wrapper!`, `ObjectSubclass`, signal connections, `ThreadGuard`) need explanation the first time they appear in a file — see `references/gtk-concepts.md`
- Architectural decisions (why this code lives in this layer, why this approach was chosen over alternatives) need inline rationale

## Comment Categories

Each category has a **density rule** indicating when comments are required. See `references/comment-patterns.md` for detailed before/after examples of every category.

### Module-Level Docs (`//!`)

**Density:** Required on every `.rs` file, immediately after the SPDX license header.

- What this module does (1 sentence)
- Its role in the architecture — which layer, what depends on it
- Key design constraints (e.g., "no GTK dependencies — fully unit-testable")
- If the module's layer placement is non-obvious, explain why

### Type Docs — Structs, Enums, Traits (`///`)

**Density:** Required on every `pub` and `pub(crate)` type. Required on private types when the name alone doesn't convey purpose.

- What it represents in the domain, not just what it contains
- If it's a GObject wrapper: what GTK widget it wraps and its role in the hierarchy
- If it has invariants or lifecycle constraints: state them
- If it derives traits for non-obvious reasons: explain why

### Field and Variant Docs (`///`)

**Density:** Required when the field's purpose isn't obvious from its name + type. **Always required on `imp` struct fields** — `Cell`/`RefCell` wrappers obscure intent and the GObject context is unfamiliar to newcomers.

- What the field tracks and why
- If the type choice is non-obvious (e.g., `Arc<PathBuf>` for memory sharing, `Cell<u32>` as a generation counter): explain the reasoning
- If there's a lifecycle constraint (e.g., "populated during `constructed()`, never `None` after"): state it

### Function Docs (`///`)

**Density:** Required on all `pub` and `pub(crate)` functions. Required on private functions that perform non-trivial work (more than simple delegation or field access).

- What it does and why it exists (not a restatement of the function name)
- Threading model: main thread only? Spawns background work? Safe to call from any thread?
- Notable side effects: modifies shared state? Emits signals? Writes to disk?
- For signal handlers: what triggers them and the expected control flow
- If it intentionally omits something: explain why

### Inline Comments (`//`)

**Density:** Required before any line or block where the "why" isn't obvious from reading the code alone.

Always comment these situations:
- GTK/GLib patterns on first use in a file (see `references/gtk-concepts.md`)
- Algorithmic choices (e.g., "iterate in reverse so removing items doesn't shift indices")
- Intentional omissions (e.g., "we do NOT call X because...")
- Workarounds for GTK/GLib quirks
- `unsafe` blocks — safety justification is mandatory
- Performance-motivated choices (e.g., "use `splice()` for single `items-changed` signal")
- Guard clauses preventing subtle bugs
- Thread boundary crossings and data snapshots
- Non-obvious control flow (early returns, fallthrough logic)

### Constants and Statics

**Density:** Always required. Every constant must explain its value and the reasoning behind it.

- What the constant controls
- Why this specific value — cite measurements, benchmarks, or heuristics
- What happens if the value is too high or too low
- Hardware/environment assumptions (e.g., "comfortable on 8GB machines")

### Configuration Files (TOML, XML, Meson, Makefile)

**Density:** Required for section groupings and non-obvious values.

- Group related entries with section headers
- For dependencies: explain *why* each exists, not just its name
- For build flags and profile settings: explain what each flag does and why this value
- For GSettings schema keys: use `<summary>` and `<description>` tags meaningfully

## Style: Friendly and Explanatory

Write as if explaining to a colleague who's smart but new to this corner of the codebase.

**Rules:**
- Lead with what the reader needs to know, then add context
- Use concrete examples in comments when they clarify (e.g., `"Adwaita-dark"`, `"src/ui/window/mod.rs"`)
- Prefer plain language over jargon — say "background thread" not "off-main-thread execution context"
- When naming a GTK/GLib concept, briefly explain what it does — don't assume the reader has the GLib API docs memorized
- Keep comments concise: 1-3 lines for most. Longer (up to a short paragraph) for architectural decisions or complex GTK patterns
- When a comment explains a workaround, name the underlying issue so a future developer can check if it's still needed

**What "friendly" does NOT mean:**
- Don't be chatty or use filler ("As you can see...", "Note that...")
- Don't use editorial commentary or jokes
- Don't use first person ("I chose...", "We decided...") — state the decision and reasoning directly
- Don't use emojis

## GTK/GLib Newcomer Orientation

GTK4 and GLib patterns that are invisible to experienced GTK developers are bewildering to newcomers. When any of these patterns appears in a file, the **first use** must have an explanatory comment. Subsequent uses in the same file can go uncommented.

Read `references/gtk-concepts.md` for the complete glossary with expected comment templates. Key concepts requiring explanation:

- `glib::wrapper!` — what it generates, what the `@extends`/`@implements` chains mean
- `imp.rs` / `mod.rs` split — why this separation exists (GObject type system requirement)
- `RefCell`/`Cell` on imp structs — why GObject forces `&self` and what interior mutability solves
- Signal connections (`connect_*`) — GObject's observer pattern and handler lifecycle
- `ThreadGuard` — what it does, why GTK objects can't cross threads
- Main loop scheduling (`idle_add_once`, `timeout_add_local_once`) — what the main loop is and how callbacks reach it
- `downcast_ref` — GObject's dynamic type casting
- `GtkTreeListModel` + `GtkListView` + `GtkTreeExpander` — the three-piece tree pattern
- `gio::ListStore::splice()` — why batch updates matter for performance
- `GSettings::bind()` — what two-way property binding means
- `ensure_type()` — why registration order matters before template parsing

## Rust Idiom Orientation

Some Rust patterns that experienced Rustaceans take for granted can confuse newcomers. Explain at point of use:

- **Interior mutability** (`Cell`, `RefCell`, `Arc<AtomicBool>`) — why `&self` prevents `&mut self` in the GObject context
- **Trait objects** (`Box<dyn Fn(...)>`) — what dynamic dispatch means here
- **Closure captures** — when `move` appears at a thread boundary, explain what's being moved and why
- **`unsafe` blocks** — always explain the safety invariant
- **Lifetime annotations** — when non-trivial, explain what the lifetime represents
- **Non-standard derives** — explain what `#[derive(CompositeTemplate)]` does, etc.

Do NOT explain: `let`, `match`, `if let`, `.map()`, `.unwrap_or()`, `Vec::new()`, `Option`, `Result`, or other Rust basics. The reader has basic language literacy.

## What NOT to Comment

High signal-to-noise means knowing what to skip:

- **Code restating:** `// create a new vector` before `Vec::new()` — the code IS the explanation
- **Obvious getters/setters:** `/// Returns the file path` on `fn file_path(&self) -> &Path` — the signature says it all
- **Standard library basics:** don't explain `String`, `Vec`, `HashMap`, `Option`, `Result`
- **Import statements:** imports are self-documenting
- **Descriptive test names:** if the test is named `test_save_creates_temp_then_renames`, no doc needed. If the name is cryptic, rename the test
- **Trailing brace comments:** `} // end if` — indicates the function is too long, not that comments are needed
- **Changelog entries:** don't write "Added in PR #42" — that's what `git blame` is for
- **Dead code:** delete it, don't comment it out. Git has history
- **`TODO`/`FIXME` without context:** if you must leave one, include *what* and *why not now*

## Severity Levels

| Level | Meaning | Example |
|---|---|---|
| **[FLAG]** | Missing comment causes real confusion risk | Public function with non-obvious side effects has no doc |
| **[RECOMMEND]** | Comment would meaningfully improve understanding | GTK pattern used without explanation |
| **[CONSIDER]** | Minor improvement, low urgency | Private helper could use a one-liner |
| **[GOOD]** | Well-written comment worth preserving | Explains a workaround with removal context |
| **[NOISE]** | Comment should be removed or rewritten | Restates what the code already says |

## Review Mode

When reviewing code for comment quality, dispatch **4 parallel subagents**, each focused on one dimension. This ensures deterministic, consistent reviews — each subagent has a narrow checklist and can't skip concerns or conflate priorities.

### Execution Model

1. **Identify changed files** — run `git diff --name-only` to get changed `.rs` files. If reviewing a PR, use the PR's file list. Also include any modified config files (`.toml`, `.xml`, `Makefile`, `meson.build`).

2. **Dispatch four subagents in parallel** via the Agent tool:

   **Subagent A: Structural Coverage Audit**
   ```
   You are auditing Rust code for structural comment coverage in the LushText text editor.

   Read the skill file at: .claude/skills/rust-comments/SKILL.md (sections: "Comment Categories" and "Style: Friendly and Explanatory")

   For each changed file, check:
   1. Module-level `//!` doc exists immediately after SPDX header and explains architectural role — not just the module name
   2. Every `pub` and `pub(crate)` struct, enum, and trait has `///` doc explaining domain meaning, not just structure
   3. Every `pub` and `pub(crate)` function has `///` doc covering: what it does, why it exists, threading model, and notable side effects
   4. ALL `imp` struct fields have `///` docs explaining purpose, lifecycle, and why that Cell/RefCell type was chosen
   5. Every constant and static has a doc comment explaining the value AND the reasoning behind it (measurements, heuristics, hardware assumptions)
   6. Field/variant docs exist where name+type combination alone doesn't convey purpose
   7. Non-trivial private functions (>5 lines, not simple delegation) have `///` docs

   For each gap, provide a concrete draft of what the doc comment should say.

   Tag findings: [FLAG] (missing doc causes confusion), [RECOMMEND] (would improve understanding), [CONSIDER] (minor), [GOOD] (well-written doc).

   Changed files:
   {changed_files}
   ```

   **Subagent B: GTK/GLib Orientation Audit**
   ```
   You are auditing Rust code for GTK/GLib newcomer orientation in the LushText text editor.

   Read the reference file at: .claude/skills/rust-comments/references/gtk-concepts.md

   For each changed file, check whether GTK/GLib patterns are explained at their FIRST use in the file. Patterns to look for:
   - `glib::wrapper!` and @extends/@implements chains
   - `ObjectSubclass` / `ObjectImpl` / `CompositeTemplate` / `TemplateChild`
   - The imp.rs/mod.rs split explanation (in mod.rs where `mod imp;` appears)
   - `Cell`/`RefCell` on imp structs (explain GObject's &self constraint)
   - `connect_*` signal handlers and `connect_notify_local` vs `connect_notify`
   - `ThreadGuard` and why GTK objects can't cross threads
   - `idle_add_once` / `timeout_add_local_once` (main loop scheduling)
   - `downcast_ref` (GObject dynamic casting)
   - `GtkTreeListModel` + `GtkListView` + `GtkTreeExpander` composition
   - `gio::ListStore` and `splice()` for batch updates
   - `GSettings::bind()` and two-way property binding
   - `ensure_type()` in `class_init()` — registration order
   - `SignalHandlerId` and handler disconnection in Drop

   For each unexplained pattern, provide a concrete draft comment using gtk-concepts.md as the template.

   Tag findings: [FLAG] (complex GTK pattern completely unexplained), [RECOMMEND] (pattern should have explanation), [GOOD] (well-explained GTK pattern worth preserving).

   Changed files:
   {changed_files}
   ```

   **Subagent C: Inline Density Audit**
   ```
   You are auditing Rust code for inline comment density in the LushText text editor.

   Read the skill file at: .claude/skills/rust-comments/SKILL.md (sections: "Inline Comments" under Comment Categories, and "Rust Idiom Orientation")

   For each changed file, scan function bodies for code that needs but lacks inline `//` comments:
   1. Algorithmic choices — iteration order, data structure selection, early exits
   2. Intentional omissions — code that deliberately does NOT do something, and why
   3. Workarounds for GTK/GLib quirks — with enough context to revisit later
   4. `unsafe` blocks — safety invariant justification is mandatory
   5. Performance-motivated choices — splice vs append, SIMD paths, batch sizes, capacity hints
   6. Guard clauses preventing subtle bugs — explain what goes wrong without the guard
   7. Thread boundary crossings — what data is being snapshotted and why it can't cross directly
   8. Non-obvious control flow — early returns, reverse iteration, fallthrough logic
   9. Rust idioms needing context — interior mutability motivation, `move` closures at thread boundaries, trait object usage, lifetime annotations
   10. Magic numbers or non-obvious literals — what does `50` mean in `Duration::from_millis(50)`?

   For each gap, provide a draft inline comment.

   Tag findings: [FLAG] (unsafe without safety comment, critical workaround unexplained), [RECOMMEND] (non-obvious code needs explanation), [CONSIDER] (minor clarity improvement).

   Changed files:
   {changed_files}
   ```

   **Subagent D: Signal-to-Noise Audit**
   ```
   You are auditing Rust code for comment signal-to-noise quality in the LushText text editor.

   Read the skill file at: .claude/skills/rust-comments/SKILL.md (sections: "What NOT to Comment" and "Style: Friendly and Explanatory")

   For each changed file, flag comments that are:
   1. **Noise** — restate what the code says (e.g., `// create a new vector` before `Vec::new()`)
   2. **Stale** — don't match current behavior (comment says X, code does Y)
   3. **Vague** — could be specific but aren't (e.g., `// handle the edge case`)
   4. **Chatty** — use filler words ("Note that...", "As you can see..."), first person, or editorial commentary
   5. **Misplaced** — explain standard library basics or trivial Rust that doesn't need explanation
   6. **Dead code** — commented-out code that should be deleted (Git has history)
   7. **Dangling TODOs** — TODO/FIXME without what, why, or context

   For noise/stale/vague comments: provide a rewritten version or recommend removal.
   For good signal-to-noise balance: highlight as [GOOD] to reinforce.

   Tag findings: [NOISE] (should be removed or rewritten), [CONSIDER] (could be tightened), [GOOD] (high-signal comment worth preserving).

   Changed files:
   {changed_files}
   ```

3. **Aggregate reports** — when all four subagents return:
   - Combine all findings, grouped by file
   - Sort by severity within each file: FLAG > RECOMMEND > NOISE > CONSIDER > GOOD
   - Deduplicate if multiple subagents flag the same location
   - Produce the unified report

### Report Format

```
## Comment Quality Review

### Summary
- **Files reviewed**: N
- **Structural coverage**: X flag, Y recommend, Z consider, W good
- **GTK/GLib orientation**: X flag, Y recommend, W good
- **Inline density**: X flag, Y recommend, Z consider
- **Signal-to-noise**: X noise, Y consider, W good

### File: path/to/file.rs

#### [FLAG] Missing doc on public function — line N
`fn open_document(&self, path: &Path)` has no `///` doc. This function has
non-obvious behavior (async loading, duplicate detection). Suggested doc:
/// Opens a file in a new tab, or focuses the existing tab if already open.
/// The tab appears immediately; file content loads asynchronously.

#### [RECOMMEND] GTK pattern unexplained — line N
`glib::wrapper!` used without explaining what it generates or what the
@extends chain means. See gtk-concepts.md template.

#### [NOISE] Comment restates code — line N
`// Check if path exists` before `if path.exists()` — remove.

#### [GOOD] Excellent workaround documentation — line N
Clear explanation of GtkTreeExpander gesture interception with removal context.
```

### When NOT to Dispatch

For trivially small changes (renaming a variable, fixing a typo), skip subagent dispatch: "No comment-relevant code changed — no review needed."

## Guidance Mode

When writing new code (invoked proactively by `rust-hex-arch`, `gtk-perf-review`, or directly):

1. **Before writing a type:** Draft the `///` doc comment first. This clarifies your thinking about what the type represents and its invariants.
2. **Before writing a function:** Draft the `///` doc comment first. Include threading model and side effects.
3. **After writing a function body:** Re-read each line as a newcomer. Add inline comments where the "why" isn't obvious.
4. **When using a GTK/GLib pattern:** Check `references/gtk-concepts.md` — is this the first use in this file? If so, add the explanatory comment.
5. **When making a design choice:** Write the reasoning in a comment *now*. "Why" fades from memory; "what" is always visible in the code.
6. **When adding constants:** Write the doc comment with value justification immediately.
7. **Before finishing:** Noise check — re-read your comments and remove any that just restate code.

## Integration with Other Skills

This skill is referenced by:

- **`rust-hex-arch`** — after architectural review, verify comment density and GTK concept explanations on all new/modified code
- **`gtk-perf-review`** — flag performance-sensitive code paths lacking explanatory comments (SIMD usage, threading choices, cache-conscious patterns)
- **`gtk-responsiveness`** — ensure async patterns and main-thread constraints are commented at point of use
- **`gtk-perf-rust-optimize`** — ensure `unsafe` blocks, SIMD intrinsics, and zero-copy patterns have safety and rationale comments

When another skill produces code changes, this skill's Guidance Mode applies to the result before finalizing.
