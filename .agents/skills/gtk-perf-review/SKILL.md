---
name: gtk-perf-review
description: Unified performance review entry point for LushText GTK4/Libadwaita Rust changes. Use automatically for Rust changes in ui, services, model, benches, GTK Lush signals, tasks, settle, viewport, or widgets code, related Cargo dependency or feature changes, pull-request reviews involving Rust, and requests about responsiveness, memory, large files, search, indexing, SIMD, benchmarks, or application hangs. Coordinates bounded leaf reviews for responsiveness, scale, and Rust hot-path correctness; do not invoke the three leaf performance skills separately for an ordinary review.
---

# GTK Performance Review

Use this skill as the sole automatic entry point for performance work. It coordinates three fixed review domains without nested delegation:

1. responsiveness and GTK main-loop safety;
2. scale, bounds, and memory;
3. Rust hot-path correctness and established acceleration patterns.

The leaf skills are detailed checklists. An assigned leaf reviewer must read its leaf `SKILL.md`, inspect the supplied scope, and return findings directly. A leaf reviewer must never spawn another agent.

## Contents

1. [Establish the review scope](#establish-the-review-scope)
2. [Read project contracts](#read-project-contracts)
3. [Dispatch with a four-slot ceiling](#dispatch-with-a-four-slot-ceiling)
4. [Review standard](#review-standard)
5. [Merge and cross-review](#merge-and-cross-review)

## Establish the review scope

Prefer an explicit file list supplied by the user, a PR tool, or the parent task. Otherwise collect scope deterministically.

For an explicit base and head, use:

```bash
git diff --find-renames --name-only --diff-filter=ACDMRTUXB <base>...<head>
```

For local work, take the sorted union of unstaged, staged, and untracked paths:

```bash
{
  git diff --find-renames --name-only --diff-filter=ACDMRTUXB
  git diff --cached --find-renames --name-only --diff-filter=ACDMRTUXB
  git ls-files --others --exclude-standard
} | LC_ALL=C sort -u
```

Do not silently substitute `HEAD` for an unknown PR base. Record the exact scope source in the report. Normalize paths to repository-relative form, use Git's rename detection, review deleted Rust through its deletion diff, and review an untracked Rust file from its full contents because it has no diff yet. Exclude generated/vendor output (`target/`, `vendor/`, generated Flatpak sources) unless the user explicitly puts it in scope.

Resolve performance scope through Cargo metadata and package-local role metadata instead of
assuming crate names or directories:

```bash
{
  git diff --find-renames --name-only --diff-filter=ACDMRTUXB
  git diff --cached --find-renames --name-only --diff-filter=ACDMRTUXB
  git ls-files --others --exclude-standard
} | LC_ALL=C sort -u | scripts/agent-topology.py performance-scope
```

The helper reads `cargo metadata --no-deps --format-version=1`, honors each package's
`package.metadata.lushtext-agent.performance-roots`, and retains normalized
`src/ui`, `src/services`, `src/model`, and `benches` suffixes as a move-safe fallback. It also
includes an owning package manifest and `Cargo.lock` when they can affect a registered
performance package. Record the helper output as the review path set; a crate rename or move
must not remove registered code from scope.

If no relevant path changed, report that no performance-sensitive Rust is in scope and stop.

## Read project contracts

Read the applicable nested `AGENTS.md` files before reviewing code. Treat current code and tests as the source of truth for constants and behavior. Do not copy thresholds or benchmark numbers from a skill into a finding without verifying them in the current checkout.

Production filesystem access must stay behind `services::filesystem`; review both boundary compliance and whether the boundary call belongs off the GTK thread. Specialized read-only engines are allowed only where the repository documents them.

When behavior depends on GTK measurement, allocation, list-factory lifecycle, focus, parenting, or Adwaita containers, read `gtk4-libadwaita-internals` before judging the performance symptom. Use `gtk-lush-stewardship` only if a change proposes a GTK Lush API boundary.

## Dispatch with a four-slot ceiling

There are at most four concurrent slots including the coordinating agent. Dispatch at most three leaf reviewers, one per domain:

| Domain | Leaf skill | Output focus |
|---|---|---|
| Responsiveness | `.agents/skills/gtk-responsiveness/SKILL.md` | Main-thread work, async freshness, signals, timers, list factories |
| Scale | `.agents/skills/gtk-perf-scale/SKILL.md` | Input bounds, memory budgets, traversal/search scale, benchmarks |
| Rust hot paths | `.agents/skills/gtk-perf-rust-optimize/SKILL.md` | Correctness, idioms, established SIMD/search patterns |

Use this prompt shape for each leaf:

```text
Read <leaf-skill-path> completely. Review only the supplied repository-relative paths and their diff in <scope-source>. Follow the leaf checklist and return evidence-backed findings with file:line, severity, impact, and fix. Verify volatile facts against current code. Do not spawn subagents and do not edit files.

Paths:
<sorted paths>
```

If fewer than three child slots are available, dispatch in deterministic domain order (responsiveness, scale, Rust hot paths) in batches, or run the undispatched checklist inline. Never ask a leaf to delegate. Never leave a required domain unreviewed merely because concurrency is constrained.

## Review standard

Only report evidence-backed issues. A finding must identify a concrete path and line (or diff hunk for deleted code), the triggering input or lifecycle, user impact, and a scoped fix. Do not invent timing or memory figures; measure them or label a reasoned bound as an estimate.

Use these severities:

- **FLAG**: correctness, data-safety, unbounded-resource, main-thread blocking, or demonstrated regression risk that must be fixed;
- **RECOMMEND**: clear, material improvement supported by current evidence;
- **CONSIDER**: optional improvement with a stated tradeoff;
- **GOOD**: a relevant pattern worth preserving.

Do not flag allocation trivia, speculative SIMD, harmless clones, capacity hints for small/unknown collections, or readability-reducing micro-optimizations. Existing GTK Lush primitives are preferred when their contracts fit: `gtk-lush-tasks` for bounded worker-to-main dispatch, `gtk-lush-settle` for timer semantics, `gtk-lush-viewport` for adjustment observation, and `gtk-lush-widgets` for governed geometry helpers.

## Merge and cross-review

After all three domains are covered:

1. Validate every finding against the diff and current code.
2. Deduplicate by root cause, keeping each domain's distinct consequence.
3. Drop findings outside the skill boundaries or without evidence.
4. Sort by severity, then repository path and line for deterministic output.
5. Cross-check proposed fixes against data safety, filesystem-boundary, GTK lifecycle, and readability contracts.
6. Report reviewed paths, scope source, checks performed, and any unverified assumption.

Use this compact report shape:

```markdown
## Performance Review

- Scope: <source and path count>
- Verdict: <pass or findings summary>

### Cross-cutting findings
### Responsiveness
### Scale and memory
### Rust hot paths
### Good patterns
### Verification
```
