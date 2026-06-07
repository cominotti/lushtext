---
name: rust-hex-arch
description: "Evaluate Rust code changes and architecture discussions against Hexagonal Architecture, CQS, DDD, and clean-code principles for GTK4/Libadwaita desktop applications. Use whenever Rust files are created, modified, reviewed, or refactored, or when the user asks how to modularize code, separate concerns, improve maintainability, choose between helper extraction vs new types, decide where code should live, or reason about domain purity, service boundaries, adapter splits, command/query boundaries, preview/apply splits, value objects, grouped state structs, or workflow-oriented module structure."
---

Evaluate Rust code changes against Hexagonal Architecture, Command-Query Separation (CQS), Domain-Driven Design (DDD), and clean-code principles adapted for GTK4/Libadwaita desktop applications.

The goal is to preserve and deepen architectural intent, not to preserve any one recent refactor, helper name, or file layout. Recommend durable principles first, then concrete changes that fit the current code.

Prioritize: correctness, simplicity, cohesion, testability, maintainability.

Assume developers may not know these patterns in a Rust/GTK4 context. When using a term from the Concept Glossary, include a one-line explanation the first time it appears in the report. When tradeoffs are genuinely ambiguous, present options with a strong recommendation. When the right answer is clear, state it directly.

Use `references/port-patterns.md` when deciding whether a trait is justified. Use `references/gtk-boundaries.md` when the boundary between GObject adapter code and business logic is unclear.

## CQS From First Principles

Treat Command-Query Separation as a caller-facing contract, not a naming game.

Start every review by asking:
- Is the caller trying to **learn** something or **make** something happen?
- What observable state can differ after the call: domain state, persisted files, GTK widgets, notifications, undo state, or any cache callers depend on?
- If the caller ignores the return value, is the call still meaningful? If yes, it is usually command-shaped.
- If the caller runs the function twice, memoizes it, or reorders it, does the behavior stay sane? Queries usually tolerate that better than commands.

Classify by observable behavior:
- **Query**: answers a question and returns data. It may allocate, parse, sort, validate, or build a new immutable value, but it does not change pre-existing observable state.
- **Command**: changes observable state or performs a side effect. It may mutate an aggregate, persist to disk, update GTK state, enqueue notifications, or advance workflow state.
- **Mixed shape**: both mutates and returns a rich post-mutation snapshot, presentation model, or decision bundle. This is the main CQS smell to flag.

Do not over-flag implementation detail:
- Mutating locals, builders, or freshly-created values is not a CQS violation if no pre-existing observable state changes.
- `with_*`, `normalized`, `sorted`, or similar methods that return a new value are query-shaped even if they rearrange internal data before returning.
- Tracing, debug counters, or private memoization are only worth flagging when they change caller-visible semantics, ordering guarantees, or failure behavior.

### Command Return-Shape Rules

Commands may return narrow outcomes when the caller genuinely needs them to continue the workflow.

Usually good:
- `Result<()>`
- `Result<bool>` or a small enum when the point is "did anything change?"
- `Result<Id>` or a newly-created domain value when creation is the command's purpose
- `Result<Outcome>` where `Outcome` is a small command summary such as counts, an undo token, or a next-step handle

Usually suspicious:
- returning the full mutated aggregate so the caller can immediately inspect it
- returning GTK-facing models or presentation bundles from a service command
- returning both "what changed" and "what should the UI render next" from one mutation
- commands whose main return payload exists because the code has no clean follow-up query

When in doubt, prefer one of these splits:
- **preview/apply** when the same function both calculates and mutates
- **query/command** when the caller first needs facts, then chooses an action
- **pure transition + persistence** when one step computes the next domain state and another step writes it out
- **command + follow-up query** when the caller wants a fresh read model after the mutation

## Review Posture

- Review for principles, not snapshots. Preserve architectural outcomes like "separate two workflows" or "replace repeated field shaping with a value object", not exact filenames from prior changes.
- Recommend the smallest coherent change that improves the code.
- Name the boundary or principle first, then explain the concrete effect on this codebase.
- Use exact file moves only when the current diff makes the destination obvious. Otherwise recommend the separation and describe the target responsibility.
- Favor code that is easy to navigate under pressure. A newcomer should be able to tell what a module owns, why a type exists, and where to add the next change.

## Pragmatism Guardrails

These principles override pattern-matching instinct. When in doubt, favor the simpler option.

1. **GObject subclassing IS the adapter pattern.** Every `mod.rs` + `imp.rs` widget pair is already a driving adapter. Adding another abstraction layer between GTK widgets and services is usually noise.

2. **Free functions are the default service boundary.** `workspace_manager::load(path)` is usually clearer than `dyn WorkspaceStore`. Only introduce a trait when multiple implementations, mock seams, or a named domain boundary justify it. See `references/port-patterns.md`.

3. **`services::filesystem` is the filesystem adapter.** Prefer its concrete operation-family APIs over adding a virtual filesystem trait unless a change proves multiple implementations or a named testing seam is truly needed. Use cheap status helpers for presence/kind questions and rich metadata facts only when canonical identity, byte size, or mtime are part of the workflow.

4. **`RefCell<T>` and `Cell<T>` in `imp` structs are normal.** GObject methods take `&self`; interior mutability is the standard GTK4-rs pattern, not a smell by itself.

5. **GtkSourceView, AdwTabView, TreeListModel, and ListStore are natural ports.** They already provide strong framework contracts. Do not recommend wrapping them just to satisfy an abstract architecture diagram.

6. **`spawn_blocking_then` is the async adapter.** The GLib main loop is the event loop. Do not recommend Tokio or another async runtime for ordinary editor I/O.

7. **Signal closures are adapter glue, not business logic.** Thin closures that delegate immediately are good. If a closure grows beyond a few non-delegation lines, the logic likely belongs in a widget method, helper module, or service.

8. **A crate boundary is stronger than a module boundary.** The existing two-crate workspace already enforces the major separation. Do not recommend more crates unless scale clearly demands it.

9. **Not every contract needs a trait.** A function signature is already a port. Do not invent traits for single implementations with no testing or runtime polymorphism need.

10. **Domain types in `model/` must stay framework-free.** No GTK, GLib, gio, sourceview, or UI/service imports.

11. **GLib collections belong in the UI layer.** Services should return `Vec`, `HashMap`, enums, and domain structs. Convert to `gio::ListStore` or similar at the adapter boundary.

12. **Split large driving adapters by workflow before adding abstraction.** If one widget mixes unrelated flows, extract sibling modules by workflow. Prefer `search.rs`, `drafts.rs`, `runtime.rs`, `dialogs.rs`, or similar over new service traits.

12. **Repeated value shaping belongs in the domain.** If UI or services rebuild the same field bundle in multiple places, extract a value object in `model/`.

13. **Large `imp` structs may group related state into plain Rust helper structs.** Grouping fields by workflow is normal adapter hygiene when it improves navigation.

14. **Prefer named structs and enums over anonymous tuples and parallel booleans when semantics matter.** If the reader has to remember what `(bool, bool)` means or which `Cell<u32>` pairs move together, the code wants a named type.

15. **Prefer ubiquitous language over technical bucket names.** If the domain says draft, workspace, session, replacement preview, or formatting override, use those words instead of generic names like `data`, `manager`, `helper`, or `state2`.

16. **Keep one level of abstraction per function.** A function should not bounce between raw widget manipulation, domain decisions, persistence details, and message formatting without clear internal phase boundaries.

17. **Keep one dominant reason to change per module.** A module can coordinate multiple steps in one workflow, but should not own several unrelated workflows at once.

18. **Preserve principles, not exact refactors.** A recommendation like "separate draft lifecycle from session persistence" is durable. A recommendation like "create `drafts.rs` and `session_persistence.rs`" is only appropriate when the current diff makes that the clearest concrete move.

## Clean Code / DDD Heuristics

Use these heuristics proactively when reviewing modularization and maintainability:

### Prefer Value Objects for Repeated Bundles

If the same cluster of fields is assembled in 2+ places, extract a **Value Object** — an identity-free type compared by value — into `model/`.

Examples:
- query text + toggles + glob
- preview counts + selected indices
- cursor line + column + scroll line
- file path + display name + kind

### Prefer Named Types Over Primitive Bags

Recommend a named struct, enum, or newtype when code shows:
- tuples with positional meaning
- several related booleans that define one mode
- `String`/`PathBuf`/`usize` values whose role is ambiguous across signatures
- repeated `HashMap<K, V>` or `Vec<(A, B)>` shaping with implicit semantics

### Move Invariants Toward the Domain

If several services or adapters enforce the same invariant, recommend moving that rule onto the domain type or into a small domain policy.

Good triggers:
- ensuring exactly one active thing
- deduplicating entries
- retaining valid tabs
- normalizing names or options
- validating state transitions

Do not force behavior into the domain when the rule is purely UI orchestration, lifecycle timing, or toolkit-specific.

### Make CQS Pressure Concrete

Apply CQS based on caller confusion, not doctrine:
- If the caller has to read a return value to understand a mutation it just requested, the API may be doing two jobs.
- If the caller wants to know "what would happen?" before deciding, recommend a preview/query instead of a command that mutates and reports.
- If the same function both decides and executes, check whether that coupling is essential or just convenient.
- If a command returns a broad struct only so downstream code can avoid a second read, prefer a follow-up query unless the outcome must be atomic.

Useful refactor directions:
- `build_*_preview(...) -> Preview` and `apply_*(preview) -> Outcome`
- `next_state(...) -> DomainType` and `save_state(...) -> Result<()>`
- `active_workspace(...) -> Option<WorkspaceId>` and `set_active_workspace(...) -> Result<()>`
- `collect_matches(...) -> Vec<_>` and `replace_matches(...) -> ReplaceOutcome`

### Keep Cross-Context Coordination Out of Domain Types

When two **Bounded Contexts** — separate areas where one domain model applies — interact, coordination usually belongs in an application service or UI adapter, not in one domain type trying to own both worlds.

Examples:
- session restore coordinating with draft recovery
- search results coordinating with editor navigation
- sidebar mutations coordinating with command-palette indexing

### Prefer Policies and Specifications for Repeated Decisions

If one business rule appears across multiple call sites, recommend a small domain or service helper that names the rule.

Examples:
- "can this row be expanded?"
- "should this file be skipped?"
- "is this tab eligible for eviction?"
- "is this replacement safe to apply?"

Do not invent an extra layer for trivial one-off predicates.

### Watch for Temporal Coupling

Temporal coupling means code only works because steps happen in the right order. If a function depends on "first do A, then B, then C" with subtle state between them, recommend:
- a named helper type representing that workflow state
- a smaller function split by phase
- comments that explain the ordering constraint when it must remain inline

### Prefer Workflow Splits Over Utility Dumps

If a module is large, split by workflow or responsibility, not by arbitrary "utils" buckets.

Prefer:
- `history.rs`, `runtime.rs`, `replace.rs`
- `drafts.rs`, `session_persistence.rs`
- `dialogs.rs`, `workspaces.rs`

Avoid:
- `helpers.rs`
- `misc.rs`
- `common.rs`

unless the code is genuinely shared and cohesive.

### Avoid Boolean Blindness

If an API takes booleans whose meaning is hard to remember, recommend:
- an enum
- a named options struct
- a domain value object

Example:
- `load_roots(roots, true)` is weaker than `load_roots(roots, AutoExpand::Yes)` or a named helper method when the flag carries workflow meaning.

## Architectural Shape

The exact file tree is illustrative, not mandatory. Review against this shape of responsibilities:

```text
model/     pure domain types, invariants, value objects, aggregate helpers
services/  application logic and driven adapters, no GTK types
ui/        driving adapters, GTK widgets, signal glue, widget orchestration
app/lib/config  framework glue
```

The recommendation target is almost always one of these outcomes:
- preserve dependency direction
- separate bounded contexts
- split adapter workflows
- extract a value object or named state bundle
- reduce primitive obsession
- move repeated invariants closer to the domain

Do not treat the current folder names or recent refactor filenames as canonical law.

### Bounded Context Orientation

Use bounded contexts as a review lens. In LushText, common contexts include:

| Bounded Context | Typical Domain Concepts | Typical Application Logic | Typical UI Surface |
|----------------|-------------------------|---------------------------|--------------------|
| Workspace | Workspace id, roots, names, entries | CRUD, persistence, file-list coordination | sidebar, preferences |
| Session | open tabs, active tab, restore positions | save/restore, filtering missing tabs | window shell |
| Drafts | draft id, manifest entries, recovery state | autosave, cleanup, restore | window + editor page |
| Editing | formatting overrides, cursor state | file load/save, editorconfig resolution | editor page, search bar |
| Search | query spec, matches, replacements, saved searches | search, replace, history persistence | search panel, window actions |

The rule is not "one file per bounded context". The rule is "do not mix unrelated contexts without a good reason, and make the coordination boundary obvious when you must."

## Dependency Direction Rules

Dependencies point inward: `ui/` -> `services/` -> `model/`.

```text
ui/        depends on services/, model/, GTK4/Libadwaita
services/  depends on model/, std, serde, anyhow, pure support crates
model/     depends on std, serde, and other non-UI pure-Rust crates only
```

Exceptions:
- driven adapters in `services/` may depend on `serde_json` and other pure support crates; file I/O goes through `services::filesystem`
- `async_task.rs` may depend on `gtk4::glib` because it is infrastructure glue for thread hopping

[FLAG] anything that reverses this dependency direction.

## Review Workflow

## Step 0: Identify Changed Files and Classify by Zone

Determine changed files using git (`git diff --name-only`, `--cached`, `HEAD~1`, or `git status --porcelain`). Filter to `.rs` files in `crates/lushtext-core/src/`. Skip deleted files, generated code, and `#[cfg(test)]` modules.

Classify each file by primary responsibility:

| Zone | Path Pattern | Characteristics | Scrutiny |
|------|-------------|-----------------|----------|
| Domain | `model/*.rs` | Pure Rust types, no GTK, no I/O | Full |
| Application | `services/*.rs` logic | Business rules, orchestration, transformations | Full |
| Driven Adapter | `services/*.rs` I/O | File or JSON I/O, persistence, infrastructure access | Moderate |
| Driving Adapter | `ui/**/*.rs` | GTK widgets, signal wiring, presentation orchestration | Light |
| Framework Glue | `app.rs`, `lib.rs`, `config.rs` | lifecycle, registration, constants | Minimal |

State the zone and why at the top of each file review.

## Step 1: Domain Review (`model/`) — Full Scrutiny

Check:
- dependency purity
- type design and naming
- invariants and constructors
- value-object extraction opportunities
- aggregate responsibilities
- whether methods are clearly command-shaped or query-shaped
- whether pure transitions want `with_*`, `next_*`, or other value-returning APIs
- whether command methods return only narrow outcomes instead of broad read models
- repeated business rules that belong on the type
- primitive obsession that wants a named type

[FLAG]:
- GTK or service imports
- domain logic scattered outside the domain in multiple places
- ambiguous primitive bags across many signatures
- command/query hybrids on aggregates that mutate and then hand back broad post-state data

[GOOD]:
- pure value objects
- aggregate helpers enforcing invariants
- clear newtypes and enums

## Step 2: Application Review (`services/`) — Full Scrutiny

Check:
- free-function service design by default
- CQS compliance from the caller's point of view
- no GTK dependencies
- plain Rust inputs/outputs
- orchestration across bounded contexts
- whether repeated decision logic wants a named policy/specification helper
- whether parameter/result tuples want named structs

[FLAG]:
- GTK/GLib types in application services
- services constructing widget-facing models
- services that both perform a mutation and return a rich query result when that should be split
- query-like names on functions that persist, notify, or otherwise mutate state

[RECOMMEND]:
- extract a named command/result object
- move repeated invariants toward the domain
- split mixed workflows inside one large service file
- split preview/apply or query/command phases when one function both answers and acts

## Step 3: Driving Adapter Review (`ui/`) — Light Scrutiny

Check:
- thin signal handlers
- `mod.rs` as small public API
- `imp.rs` limited to template/state/signal glue
- large adapter splits by workflow
- grouped state structs inside `imp` when state clusters move together
- natural ports used directly, not wrapped gratuitously
- query-like widget helpers do not hide durable business-state changes

[FLAG]:
- business logic living in signal closures
- application logic buried in `imp.rs`
- services pulled upward into GTK-only helpers because it was convenient

[RECOMMEND]:
- split a large widget by workflow
- replace tuples and scattered `Cell`s with named helper structs
- move repeated adapter-only predicates into widget-local helper modules

## Step 4: Driven Adapter Review (`services/` I/O) — Moderate Scrutiny

Check:
- I/O isolation behind clear function boundaries
- filesystem access routed through `services::filesystem` rather than raw calls or private durable-write implementation details
- atomic or otherwise deliberate persistence patterns
- no upward dependency on `ui`
- whether a trait is actually justified per `references/port-patterns.md`
- whether read helpers and write helpers stay distinct unless one atomic workflow truly needs both

## Step 5: Framework Glue Review — Minimal Scrutiny

Only check that `app.rs`, `lib.rs`, and `config.rs` do not accumulate business rules.

## Step 6: Module and Recommendation Quality Review

Recommendations must be principle-first.

Good:
- "Separate draft lifecycle from session snapshot persistence; the current module mixes two workflows."
- "Extract a query value object because UI and services rebuild the same fields."
- "Group these `Cell`/`RefCell` fields into one named state bundle because they move together."
- "Split `preview_replacements` from `apply_replacements`; the current function both computes impact and mutates files."
- "Keep `set_active_workspace` command-shaped and move the read model into a follow-up query."

Bad:
- "Create `drafts.rs` because that was done before."
- "Use helper struct `SearchRuntimeState` because that exact name exists elsewhere."
- "Mirror the current module tree."
- "Return the full updated aggregate from the command so callers do not need a second function."

Mention exact file moves only when the current diff makes them clearly appropriate.

Do not recommend moving untouched files unless there is a high-severity boundary violation.

## Step 7: Produce the Report

Use this structure:

```markdown
## Hex Arch / CQS / DDD Review

### Summary
- Files reviewed: N
- Zone classification: N domain, N application, N driven adapter, N driving adapter, N glue
- Findings: N (X flag, Y recommend, Z consider, W good)

### File: path/to/file.rs
**Zone**: Driving Adapter
**Why**: GTK widget orchestration and signal handling.

#### [GOOD] Workflow-oriented split preserves adapter clarity
The widget stays the adapter, but search runtime and history flows are separated into sibling modules.
This preserves the same architectural boundary while improving navigation.

#### [RECOMMEND] Replace repeated field shaping with a value object
This file and one service rebuild the same query-plus-options bundle.
**Principle**: repeated value shaping belongs in the domain.
**Fix**: extract or reuse one domain value object and make both call sites convert through it.

#### [RECOMMEND] Separate two bounded contexts in one module
This module mixes session persistence and draft recovery.
**Principle**: keep cross-context coordination explicit and keep modules cohesive.
**Fix**: split by workflow or introduce named helper structs that make the two flows obvious. Exact filenames are secondary.

#### [RECOMMEND] Split preview from apply for a CQS-clean workflow
This function calculates replacement impact and mutates files in one pass.
**Principle**: if the caller needs to inspect consequences before deciding, that wants a query or preview first, then a command.
**Fix**: return a preview/query object from one function and keep file mutation in a separate apply command or explicit second step.

#### [FLAG] Upward dependency on GTK in application logic
This service constructs `gio::ListStore`.
**Fix**: return plain Rust data from the service and construct GTK models in the UI adapter.
```

Explain why each finding matters. Present findings as a thinking partner, not a linter.

## Comment Quality Cross-Check

After the architectural review, verify that new and modified code also follows the `rust-comments` skill:
- module-level `//!` docs explain architectural role
- public types/functions have `///` docs with side effects and threading model when relevant
- GTK/GLib concepts are explained at first use
- `imp` struct fields are documented with purpose and lifecycle
- inline comments explain non-obvious ordering constraints and boundary crossings

## What NOT to Flag

- `RefCell<T>` or `Cell<T>` in `imp.rs`
- `#[derive(CompositeTemplate)]`, `#[template_child]`, `glib::wrapper!`
- `ensure_type()` calls in `class_init()`
- short delegating `connect_*_local()` closures
- `tracing::*` in services
- test modules
- `config.rs` constants
- `PathBuf` in domain types
- grouped helper structs inside `imp.rs`
- exact filename differences that still preserve the same principle

## Concept Glossary

| Term | Explanation |
|------|------------|
| **Hexagonal Architecture** | Business logic stays independent of frameworks and I/O. In Rust/GTK4 this usually means `model/` pure, `services/` GTK-free, `ui/` framework-facing. |
| **Port** | A boundary contract. In Rust this can be a function signature or a trait, depending on the level of indirection actually needed. |
| **Driving Adapter** | Inbound adapter that translates UI or external events into application calls. In GTK4 this is usually a widget module. |
| **Driven Adapter** | Outbound adapter that reaches infrastructure like files, JSON, or external systems. |
| **Natural Port** | A framework type whose contract is already strong enough that wrapping it adds no value. |
| **Value Object** | Identity-free data compared by value. Good for repeated field bundles and option sets. |
| **Entity** | Data with stable identity even when other fields change. |
| **Aggregate** | A cluster of domain objects treated as one consistency boundary. |
| **CQS** | Separate "tell" from "ask": a command changes observable state, a query returns information. Small command acknowledgements are fine; rich post-mutation read models are the smell. |
| **Bounded Context** | A coherent part of the domain with its own vocabulary and model. |
| **Ubiquitous Language** | The shared domain vocabulary used consistently in types, functions, and module names. |
| **Temporal Coupling** | Logic that only works because operations happen in a specific order. If important, make that order explicit. |
| **Interior Mutability** | Rust pattern using `RefCell<T>` or `Cell<T>` to mutate data through `&self`, required frequently in GObject adapters. |
| **Framework Glue** | Lifecycle, registration, CSS/resources, and other setup code with no business rules. |

## Tone

Be direct, pragmatic, and principle-first.
- Reinforce what is already working.
- Recommend the smallest coherent improvement.
- Explain the why behind architecture advice.
- Prefer durable guidance over copying prior refactors.
- When calling out a CQS problem, explain the caller confusion or temporal coupling it creates.
