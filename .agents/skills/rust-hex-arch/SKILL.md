---
name: rust-hex-arch
description: "Evaluate Rust changes and architecture decisions against Hexagonal Architecture, command-query separation, domain-driven design, and clean-code principles for GTK4/Libadwaita desktop applications. Use whenever Rust files are created, modified, reviewed, or refactored, or when deciding module ownership, domain purity, service boundaries, GTK adapter splits, GTK Lush platform boundaries, preview/apply workflows, value objects, grouped state, or maintainability tradeoffs."
---

# Rust Hexagonal Architecture Review

Keep application decisions independent of GTK and infrastructure while avoiding
ceremonial abstraction. Review the boundary and caller contract before judging
file size or naming.

## Core Model

Use this dependency direction:

```text
ui/        -> services/ -> model/
GTK                    plain Rust
driving adapters       application/domain
```

- `model/` owns pure domain values, invariants, policies, and transitions. It
  must not depend on GTK or perform I/O.
- `services/` owns application workflows and driven adapters. LushText services
  remain GTK-free and route production filesystem access through
  `services::filesystem`.
- `ui/` owns GTK widgets, signals, focus, actions, projection, and lifecycle
  sequencing. It delegates durable business decisions inward.
- GTK Lush crates are an internal platform boundary and may depend on GTK/GLib
  when their governed crate contract requires it. Do not treat them as ordinary
  LushText services.

Read [references/gtk-boundaries.md](references/gtk-boundaries.md) when a change
touches GObject subclasses, signals, templates, or adapter ownership. Read
[references/port-patterns.md](references/port-patterns.md) only when deciding
whether a free function, generic trait parameter, or trait object is justified.

## Review Principles

### Preserve dependency direction

Flag GTK types or widget-facing models in domain/application logic. Return plain
Rust data from services and construct GTK projections in the driving adapter.
Keep framework glue such as registration and resource loading free of business
rules.

### Model real domain concepts

Prefer a named value object, enum, options struct, or policy when primitives are
repeated and carry rules. Do not extract a type for a one-off field bundle or
mechanical parameter grouping with no semantic identity.

Move a rule toward the domain when multiple callers must enforce the same
invariant. Keep toolkit timing, focus, animation, and widget lifecycle rules in
the adapter.

### Apply CQS from the caller's perspective

A command changes observable state; a query returns information without doing
so. Small command acknowledgements and atomic outcomes are fine. Review APIs
that mutate and return a broad read model, hide persistence behind a query-like
name, or force the caller to inspect a mutation result to understand what it
requested.

Use preview/apply splits when a caller must inspect consequences before
committing:

```text
build_preview(input) -> Preview
apply(preview) -> Outcome
```

Do not split a single atomic operation merely to satisfy terminology.

### Keep cross-context coordination explicit

Session restore plus draft recovery, search plus editor navigation, and sidebar
mutation plus indexing are cross-context workflows. Coordinate them in an
application service or driving adapter; do not make one domain type own another
context's vocabulary.

### Split by workflow, not size alone

Split a module when it mixes responsibilities, churns in unrelated features, or
forces readers to hold several lifecycles at once. Prefer workflow names such as
`history`, `runtime`, or `replace` over `helpers`, `misc`, or arbitrary line-count
splits. A cohesive large module can be better than several coupled fragments.

### Prefer direct ports until a seam pays for itself

Free functions and concrete module APIs are valid ports. Introduce a trait when
multiple implementations, long-lived dependency injection, or a valuable test
seam justifies it. Keep traits narrow and bounded-context specific.

### Make temporal coupling visible

When correctness depends on phase order, prefer a named workflow state, guard,
or phase-specific function. Add a concise comment only when the order must
remain inline and cannot be encoded structurally.

## Review Workflow

### 1. Collect and normalize scope

Use the user's explicit file list or PR base/head range when available.
Otherwise combine working-tree, staged, and untracked paths:

```bash
git diff --name-only --diff-filter=ACMRTUXB
git diff --cached --name-only --diff-filter=ACMRTUXB
git ls-files --others --exclude-standard
```

Normalize and deduplicate paths. Exclude deleted, generated, vendored, and
format-only files. Do not guess `HEAD~1` for a PR review.

### 2. Classify each Rust file

| Zone | Typical role | Scrutiny |
|---|---|---|
| Domain | values, invariants, pure policies | dependency purity and modeling |
| Application | workflows and decisions | CQS, context coordination, plain data |
| Driven adapter | persistence and infrastructure | isolation, durability, inward dependencies |
| Driving adapter | GTK widgets and orchestration | thin handlers, lifecycle, delegation |
| Framework glue | startup, registration, constants | prevent business-rule accumulation |

Classify by responsibility, not path alone. State the classification when it
affects a finding.

### 3. Trace callers and dependencies

For each changed boundary, inspect enough callers and callees to answer:

- Who owns the invariant?
- Is data crossing inward as plain Rust values?
- Does the API name reveal whether it mutates?
- Is a trait solving a current problem?
- Would a split reduce coupling or only move code?
- Is GTK Lush reuse supported by a genuine repeated platform contract?

Do not broad-refactor untouched files unless a confirmed high-severity boundary
violation makes the change necessary.

### 4. Report findings

Use these labels:

- `[FLAG]`: dependency inversion, domain impurity, hidden mutation, or mixed
  ownership creates a concrete correctness or maintenance hazard.
- `[RECOMMEND]`: a coherent boundary, value object, workflow split, or CQS
  change materially improves the design.
- `[CONSIDER]`: a plausible improvement whose benefit depends on future churn.
- `[GOOD]`: an effective boundary or modeling choice worth preserving.

For every non-good finding, include `file:line`, the caller-visible problem,
the violated principle, and the smallest coherent fix. Avoid prescribing exact
filenames unless the diff makes the ownership unambiguous.

## What Not to Flag

- `Cell` and `RefCell` inside a GObject implementation;
- standard `CompositeTemplate`, `TemplateChild`, `glib::wrapper!`, or type
  registration boilerplate;
- short signal closures that delegate immediately;
- `PathBuf` or serialization derives in pure domain values;
- grouped adapter state with clear workflow cohesion;
- a free-function service API merely because a trait could exist;
- exact module naming differences that preserve the boundary;
- tests, fixture-only shortcuts, and generated code outside production paths.

## Verification and Coordination

After architecture edits, run the repository-prescribed formatter, compile or
lint surface, affected tests, and `git diff --check`. Reinspect dependency edges
and call sites rather than assuming compilation proves ownership.

Use `rust-comments` only for changed code that retains non-obvious contracts or
rationale; do not demand comment density. Use data-safety and performance skills
when the boundary also touches persistence, threading, file I/O, search, or GTK
responsiveness.

## Report Shape

```markdown
## Hex Architecture Review

### Summary
- Files reviewed: N
- Zones: ...
- Findings: N

### `path/to/file.rs`
**Zone:** Application

#### [RECOMMEND] Separate preview from apply
The caller must inspect impact before mutation, but this function performs both.
**Principle:** Make the decision query explicit before the command.
**Fix:** Return a plain preview value, then pass it to an apply operation.
```
