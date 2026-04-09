---
name: gtk-perf-rust-optimize
description: "Review Rust code for idiomatic patterns, correctness improvements, and SIMD coverage on established hot paths. Uses parallel subagents for focused reviews. Auto-invoked when writing or modifying Rust code in services/, ui/editor_page/, model/, or Cargo.toml. Trigger whenever code touches file loading/saving, fuzzy search, file indexing, byte processing, or error handling patterns. Also trigger when the user mentions SIMD, error handling, thiserror, or code quality. Use this skill proactively after writing any performance-sensitive Rust code."
---

Review Rust code in LushText for idiomatic patterns, correctness improvements, and SIMD coverage on established hot paths. While `gtk-responsiveness` answers "does this block the main thread?" and `gtk-perf-scale` answers "does this scale to large inputs?", this skill answers: **"is this Rust code idiomatic, correct, and following established project patterns?"** Benchmark coverage is handled by `gtk-perf-scale`'s benchmark-audit subagent.

## Philosophy: Readability First

**Code readability is the top priority.** Every recommendation from this skill must make the code easier to understand, not harder. If an optimization requires an unfamiliar abstraction, obscure trait bounds, or indirection that a new contributor wouldn't immediately grasp — it is a net negative.

The purpose of this skill is to catch:
1. **Correctness issues** that look like optimizations but are really about robustness (thiserror vs string errors, proper SIMD validation)
2. **Established pattern consistency** where the codebase already uses a pattern (nucleo for search, simdutf8 for validation) and new code should follow suit
3. **Benchmark coverage** for genuinely hot paths

### What We Do NOT Flag

These are explicitly out of scope:

- **Allocation micro-counting**: `.to_string_lossy().to_string()` vs `.into_owned()`, `format!()` vs `push_str()`, intermediate collections, `Cow<str>` opportunities
- **Speculative SIMD adoption**: Don't recommend SIMD crates for code that hasn't been profiled. Only flag when an established project pattern (simdutf8, nucleo, memchr) is missing on a similar code path.
- **Clone avoidance in non-hot paths**: `.clone()` is fine in setup code, signal handlers, error paths, and any code that runs fewer than ~1000 times
- **`Vec::with_capacity()` suggestions**: The doubling strategy is fine for most collections
- **Drop ordering**: Scoping intermediates to free memory earlier is not worth the nested blocks

The threshold: **if the suggestion makes the code harder to read and you need a benchmark to prove the difference, don't suggest it.**

## Relationship to Other Performance Skills

| Concern | gtk-responsiveness | gtk-perf-scale | gtk-perf-rust-optimize (this) |
|---------|-------------------|----------------|-------------------------------|
| Core question | "Does this block the main thread?" | "Does this scale to large inputs?" | "Is this idiomatic and following project patterns?" |
| Primary focus | Thread safety, frame budget | Throughput at 10x-100x, RAM budgets | Correctness, SIMD consistency, benchmarks |
| Benchmarks | Defer to gtk-perf-scale | Owns benchmark-audit | Defer to gtk-perf-scale |
| Example finding | `fs::write` on main thread | Missing file size check | `Result<T, String>` instead of thiserror enum |

If you find a GTK threading issue, flag it but reference `gtk-responsiveness`. If you find a scaling issue, reference `gtk-perf-scale`.

## Execution Model: Parallel Subagents

This skill uses **parallel subagents** for independent review concerns. Do NOT review all concerns inline — dispatch focused subagents.

### Workflow

1. **Identify changed files** — run `git diff --name-only` (or use the diff context if already available)
2. **Match trigger patterns** — for each subagent below, check its path globs and content patterns against the file list. A subagent triggers if any changed file matches a listed path glob OR contains a listed content pattern.
3. **Dispatch all relevant subagents in parallel** via the Agent tool — even if only one triggers, always dispatch as a subagent for consistent output format. In each prompt, replace `{changed_files}` with the actual file list from step 1.
4. **Aggregate results** — merge findings, deduplicate, produce the final report

## Severity Levels

- **[FLAG]** — Correctness or consistency issue. A proven hot path missing an established SIMD pattern, or an error handling approach that enables silent bugs. Must fix.
- **[RECOMMEND]** — Idiomatic improvement that also improves readability or correctness. Fix proactively.
- **[CONSIDER]** — Minor improvement. Only if the change is also a readability win.
- **[GOOD]** — Existing correct pattern. Reinforce and protect from regression.

## Subagent Definitions

### 1. simd-coverage-audit

**Triggers**:
- paths: `ui/editor_page/**/*.rs`, `services/palette.rs`
- content: `read_to_string|fs::read|from_utf8|memchr|simdutf8`

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for consistency with established SIMD patterns.

Read the reference file at: .agents/skills/gtk-perf-rust-optimize/references/simd-opportunities.md

Changed files to review:
{changed_files}

The project has established SIMD patterns that new code on similar paths should follow:
- simdutf8 for ALL file loads (UTF-8 validation)
- nucleo-matcher for ALL fuzzy search scoring
- memchr for byte scanning on large buffers (if applicable)

IMPORTANT: Only flag SIMD issues where an ESTABLISHED project pattern is missing on a similar code path. Do NOT recommend SIMD adoption for new code paths that haven't been profiled. Do NOT flag micro allocation patterns.

Review criteria:
- Does new file-loading code use the established simdutf8 pattern (std::fs::read + simdutf8::basic::from_utf8 + from_utf8_unchecked)?
- Does new search/scoring code use nucleo-matcher (the established fuzzy search pattern)?
- For new byte-scanning code on buffers that could be >1KB: is there an existing memchr pattern it should follow?

Anti-patterns to flag:
- [FLAG] New file-loading path uses std::fs::read_to_string instead of the established simdutf8 pattern
- [FLAG] New fuzzy search code uses hand-rolled scoring instead of nucleo-matcher
- [GOOD] nucleo-matcher used for fuzzy search with Matcher reuse across candidates
- [GOOD] simdutf8 + from_utf8_unchecked with correct safety comment

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Which established pattern is missing. Fix.
```

### 2. modern-rust-audit

**Triggers**: Any changed `.rs` file. This is the lightest subagent — always runs.

**Subagent prompt** (self-contained — no reference file):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for modern Rust idioms and correctness improvements that also improve readability.

Changed files to review:
{changed_files}

The project uses Rust edition 2024 with MSRV 1.94.1. thiserror = "2.0" is in workspace dependencies.

IMPORTANT: Only flag improvements that make the code BOTH more correct AND more readable. Do NOT flag micro allocation patterns, Cow<str> opportunities, format!() vs push_str, or clone avoidance.

Review criteria:
1. thiserror adoption: flag Result<T, String> return types in service functions. thiserror enums improve BOTH correctness (exhaustive pattern matching, no fragile string comparison) AND readability (the error variants document what can go wrong). The crate is already a workspace dependency.
2. Error string comparison: flag any `if err == "some string"` or `err.contains("...")` patterns. These are fragile (typo = silent bug) and hard to review. Enum pattern matching is clearer.
3. let-else usage: only flag when let-else would significantly improve readability over a match arm that just extracts or early-returns. Do not flag cases where the match is already clear.
4. Edition 2024 readiness: if the crate still uses edition = "2021", note that Edition 2024 is available. Key change: `unsafe_op_in_unsafe_fn` becomes deny-by-default — any existing `unsafe` blocks inside `unsafe fn` will need explicit `unsafe {}` wrappers. Relevant because the codebase uses `unsafe { String::from_utf8_unchecked(bytes) }`.
5. #[expect] vs #[allow]: flag `#[allow(lint)]` when `#[expect(lint)]` would be more appropriate. `#[expect]` is self-policing — it causes a compile error if the lint no longer fires, catching stale suppressions automatically. Available since Rust 1.81, well within MSRV 1.94.1. Reserve `#[allow]` only for cases where the lint may or may not fire depending on configuration.

Anti-patterns to flag:
- [RECOMMEND] Result<T, String> with format!() in service functions — thiserror enum is both more correct and more readable
- [RECOMMEND] Error string equality comparison — fragile and hard to review, use enum pattern match
- [CONSIDER] #[allow(lint)] where #[expect(lint)] would be self-policing — stale suppressions are bugs waiting to happen
- [CONSIDER] match { Some(x) => x, None => return } where let-else would be noticeably cleaner
- [CONSIDER] edition = "2021" when 2024 is available — note the unsafe_op_in_unsafe_fn impact
- [GOOD] Correct use of let-else for early returns
- [GOOD] anyhow::Result with .context() in service error propagation
- [GOOD] thiserror-derived error enums with exhaustive matching
- [GOOD] #[expect(deprecated)] for APIs with no current replacement

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Why the modern pattern improves both correctness and readability. Fix.
```

## Aggregation

After all subagents return, produce the unified report:

1. **Merge findings** — combine all [FLAG], [RECOMMEND], [CONSIDER], [GOOD] items verbatim from subagents. Do not add new findings beyond what was reported.
2. **Deduplicate** — if two subagents flag the same line, keep the more specific finding
3. **Drop excluded items** — remove any finding that falls under the "What We Do NOT Flag" list above
4. **Sort by severity** — FLAG first, then RECOMMEND, CONSIDER, GOOD

## Audit Report Format

```
## Rust Code Quality Audit

### Summary
- **Files reviewed**: N
- **Findings**: X flag, Y recommend, Z consider, W good
- **SIMD consistency**: Brief status (e.g., "all file loads use simdutf8; search uses nucleo")

### [FLAG] Title — file:line
Description of the issue.
**Why it matters**: Correctness or consistency impact (not micro-throughput).
**Fix**: Concrete recommendation.

### [RECOMMEND] Title — file:line
...

### [CONSIDER] Title — file:line
...

### [GOOD] Title — file:line
Why this pattern is correct and idiomatic.
```

## Guidance Mode

When implementing new features (not reviewing existing code), check:

1. Does this code load files? → Follow the established simdutf8 pattern
2. Does this code do fuzzy search? → Use nucleo-matcher (the established pattern)
3. Does this return `Result<T, String>`? → Consider a `thiserror` enum (improves both correctness and readability)
4. Is this a hot path? → Benchmark coverage is checked by `gtk-perf-scale`

## Tone

Focus on correctness and consistency over raw throughput. Instead of "this allocates unnecessarily," say "this error handling uses string comparison, which is fragile — a typo would silently break the match arm." Acknowledge existing good patterns before suggesting improvements. Never recommend changes that trade readability for marginal performance.
