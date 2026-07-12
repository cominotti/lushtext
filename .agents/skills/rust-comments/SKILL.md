---
name: rust-comments
description: "Review and guide high-signal comments in Rust and adjacent configuration for GTK4/Libadwaita applications. Auto-invoke when Rust changes introduce or alter public APIs, unsafe code, concurrency, persistence, GTK lifecycle or ownership constraints, subtle invariants, non-obvious algorithms, or durable configuration policy. Use explicitly for comment-quality, documentation, readability, onboarding, or stale-comment reviews. Do not trigger for trivial renames, formatting-only changes, obvious delegation, generated files, or changes whose intent is already clear from names and types."
---

# Rust Comments

Explain facts the code cannot express reliably: intent, invariants, ownership,
threading, side effects, safety, and the reason behind a surprising choice.
Treat comments as maintained contracts, not a coverage metric.

## Modes

### Scoped guidance (automatic)

Review only the active diff or files already in scope. Do not dispatch
subagents or broaden into a repository-wide comment audit. Add or revise a
comment only when the changed code crosses one of the risk boundaries below.
If no such boundary exists, make no comment-only edit.

### Explicit review

Use the review workflow when the user asks for comment quality, documentation,
readability, onboarding, or stale-comment review. Stay read-only unless the
request also authorizes edits.

## Decision Rule

Add a comment when all three statements are true:

1. A competent Rust developer cannot recover the important fact from names,
   types, and nearby control flow.
2. Missing the fact could cause a wrong change, misuse, safety issue, or costly
   rediscovery.
3. The fact is stable enough to maintain beside the code.

Prefer a clearer name, smaller function, named type, or explicit state machine
when that makes the code self-explanatory. Do not use comments to compensate
for avoidable complexity.

## Required Comments

- Give every public API meaningful rustdoc when its contract is not completely
  expressed by its signature. Document relevant errors, panics, side effects,
  thread requirements, durability, and lifecycle behavior.
- Put a `// SAFETY:` explanation immediately before every `unsafe` block or
  unsafe implementation. State the invariant and why it holds here.
- Explain ordering constraints in persistence, async generation guards,
  cancellation, signal disconnection, and GTK main-thread handoffs.
- When a filesystem comment is warranted, describe the invariant owned by
  `services::filesystem` (such as canonical identity, metadata preservation, or
  durability ordering) instead of narrating an obvious wrapper call.
- Explain workarounds for toolkit or platform behavior and name the observable
  symptom or upstream contract that makes the workaround necessary.
- Explain policy constants when their value encodes a meaningful product,
  resource, protocol, retry, timeout, or geometry decision.
- Give a module `//!` documentation when its architectural role, boundary, or
  split-workflow ownership is not obvious from the module path.

These are not blanket requirements. Obvious getters, private delegators,
descriptive fields, enum variants, fixtures, and conventional GObject boilerplate
do not need comments merely because they exist.

## GTK and Rust Orientation

Explain a GTK/GLib or Rust mechanism at the point where its consequence matters,
not at every first syntactic appearance. Examples include:

- why a callback must stay on the main thread;
- why a signal handler ID must be disconnected;
- why a weak reference prevents an ownership cycle;
- why `Cell` or `RefCell` is needed for GObject-owned state;
- what a generation guard prevents after background work completes;
- what an `unsafe` FFI or SIMD precondition guarantees.

Do not narrate standard `glib::wrapper!`, derives, imports, `match`, `Option`,
`Result`, or common container operations unless this use has a non-obvious
constraint. Read [references/gtk-concepts.md](references/gtk-concepts.md) only
when the changed code uses a concept whose consequence needs explanation.

## Style

- State the reason or invariant first.
- Keep most comments to one or two sentences.
- Describe current behavior, not project history or a past pull request.
- Avoid filler, jokes, first-person narration, and vague words such as “magic”
  or “edge case.”
- Do not duplicate exact values or implementation details likely to drift; link
  the explanation to the owning symbol instead.
- Delete commented-out code. Give actionable `TODO` or `FIXME` notes an owner
  condition: what remains and what makes it safe to defer.

Read [references/comment-patterns.md](references/comment-patterns.md) when a
concrete rewrite example would help.

## Explicit Review Workflow

### 1. Collect scope

Use the user-provided review file list when available. Otherwise collect all
relevant states:

```bash
git diff --name-only --diff-filter=ACMRTUXB
git diff --cached --name-only --diff-filter=ACMRTUXB
git ls-files --others --exclude-standard
```

For a branch or PR review, use its explicit base/head range instead of guessing
`HEAD~1`. Normalize and deduplicate paths. Include changed `.rs` files and
adjacent durable configuration; exclude deleted, generated, vendored, and
format-only files.

### 2. Review fixed dimensions

Check every in-scope file in this order:

1. Contract and safety: public contracts, `unsafe`, errors, side effects,
   threading, persistence, and lifecycle.
2. Rationale: algorithms, ordering, policy values, workarounds, and boundary
   crossings that need a stable “why.”
3. Signal-to-noise: stale, vague, duplicated, chatty, or implementation-narrating
   comments that should be removed or rewritten.

For a substantial explicit audit, independent leaf reviewers may cover these
three dimensions. Never reserve more than three child slots in a four-slot
runtime, never ask reviewers to spawn subagents, and use smaller deterministic
batches when fewer slots are available. If no child slots are available, run
the same dimensions locally. Automatic guidance never dispatches reviewers.

### 3. Validate each finding

Report a location only when the proposed comment communicates a concrete fact
that the code does not. Re-open the surrounding implementation to ensure an
existing comment is actually stale before flagging it. Deduplicate by location,
preferring safety/contract findings over style findings.

### 4. Report or edit

Use these labels:

- `[FLAG]`: missing or stale contract can cause misuse, unsoundness, data loss,
  deadlock, lifecycle failure, or an incorrect maintenance change.
- `[RECOMMEND]`: a stable rationale would materially reduce rediscovery.
- `[NOISE]`: comment is misleading, stale, redundant, or less clear than code.
- `[GOOD]`: unusually effective contract or rationale worth preserving.

For every finding, provide `file:line`, the hidden fact, and a concrete rewrite
or removal. Do not report density totals or require comments for every symbol.

## Verification

After edits:

1. Re-read each changed comment against the current implementation.
2. Run `cargo fmt --all -- --check` when Rust doc formatting changed.
3. Run the narrowest build, lint, or documentation check required by the owning
   project rules.
4. Use `git diff --check` and confirm no unrelated comment churn entered scope.

## Coordination

- Let `rust-hex-arch` decide where responsibilities belong; this skill explains
  only non-obvious surviving boundaries.
- Let performance and data-safety skills own technical correctness; document
  the invariant they establish without copying volatile implementation details.
- If clearer code removes the need for a proposed comment, prefer the code
  improvement and omit the comment.
