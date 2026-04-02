---
name: gtk-perf-rust-optimize
description: "Review Rust code for language-level optimization opportunities — SIMD coverage gaps, allocation waste, zero-copy patterns, modern Rust idioms, and benchmark completeness. Uses parallel subagents for focused reviews. Auto-invoked when writing or modifying Rust code in services/, ui/editor_page/, model/, benches/, or Cargo.toml. Trigger whenever code touches file loading/saving, string conversions (to_string_lossy, to_string, format!), fuzzy search, file indexing, byte processing, error handling patterns, or benchmark configuration. Also trigger when the user mentions SIMD, memchr, simdutf8, allocations, zero-copy, Cow, thiserror, benchmarks, or 'could this be faster'. Use this skill proactively after writing any performance-sensitive Rust code — even if the user doesn't mention optimization."
---

Review Rust code in LushText for language-level optimization opportunities that the GTK-specific performance skills (`gtk-responsiveness` and `gtk-perf-scale`) don't cover. While those skills answer "does this block the main thread?" and "does this scale to large inputs?", this skill answers: **"is this Rust code as efficient as the language allows?"**

## Relationship to Other Performance Skills

| Concern | gtk-responsiveness | gtk-perf-scale | gtk-perf-rust-optimize (this) |
|---------|-------------------|----------------|-------------------------------|
| Core question | "Does this block the main thread?" | "Does this scale to large inputs?" | "Is the Rust code maximally efficient?" |
| Primary focus | Thread safety, frame budget | Throughput, RAM at 10x–100x | SIMD coverage, allocations, idioms |
| Example finding | `fs::write` on main thread | Missing file size check | `to_string_lossy().to_string()` instead of `.into_owned()` |

If you find a GTK threading issue, flag it but reference `gtk-responsiveness`. If you find a scaling issue, reference `gtk-perf-scale`.

## Execution Model: Parallel Subagents

This skill uses **parallel subagents** for independent review concerns. Do NOT review all concerns inline — dispatch focused subagents.

### Workflow

1. **Identify changed files** — run `git diff --name-only` (or use the diff context if already available)
2. **Match trigger patterns** — for each subagent, check if any changed files match its triggers
3. **Always include allocation-audit** — allocation waste is cross-cutting
4. **Dispatch threshold** — if fewer than 2 subagents are relevant, run inline using only the relevant criteria
5. **Dispatch relevant subagents in parallel** via the Agent tool
6. **Aggregate results** — merge findings, deduplicate, produce the final report

### Optimization Preamble (inject into every subagent prompt)

> While reviewing, also check for: unnecessary type conversions (OsStr → String when &str suffices), redundant allocations in loops, missing `Cow<str>` for conditionally-owned strings, `format!()` where a static `&str` would work, and `clone()` where a reference or `Arc` sharing would suffice.

## Severity Levels

- **[FLAG]** — Measurable performance waste. SIMD crate available but not used on a hot path, or allocation scales with input size unnecessarily. Must fix.
- **[RECOMMEND]** — Optimization opportunity. Works today but leaves performance on the table. Fix proactively.
- **[CONSIDER]** — Minor improvement. Not measurable at current scale but good practice.
- **[GOOD]** — Existing optimized pattern. Reinforce and protect from regression.

## Subagent Definitions

### 1. simd-coverage-audit

**Triggers**: Changes to `editor_page/`, `services/`, `Cargo.toml`, or any file containing `read_to_string`, `fs::read`, `from_utf8`, `memchr`, `simdutf8`, byte processing loops.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for SIMD acceleration opportunities.

Read the reference file at: .claude/skills/gtk-perf-rust-optimize/references/simd-opportunities.md

Changed files to review:
{changed_files}

{optimization_preamble}

The project targets x86-64-v3 (AVX2 guaranteed) and Apple Silicon (NEON guaranteed). SIMD crates provide free performance gains on every target machine.

Review criteria:
- simdutf8 coverage: is simdutf8 used for ALL file loads, or only gated behind a size threshold? The SIMD path is faster even for small files (0.08ms vs 0.7ms for 1MB). Gate syntax highlighting on size, not the UTF-8 validation method.
- memchr usage: is memchr used for newline counting, byte scanning, and line-offset computation? Or does the code use scalar .chars().filter(), .find(), or manual loops? memchr provides 32x throughput on AVX2.
- Byte-level operations: any code iterating bytes in a buffer (newline counting for status bar, line position lookup) should use memchr, not scalar iteration.
- Cargo.toml: is memchr listed as a direct dependency? (It's a transitive dep through nucleo but should be explicit for direct use.)
- SIMD crate versions: are simdutf8 and nucleo-matcher on their latest stable versions?

Anti-patterns to flag:
- [FLAG] std::fs::read_to_string used where std::fs::read + simdutf8 + from_utf8_unchecked would be faster (applies to ALL file sizes, not just >10MB)
- [FLAG] Scalar newline counting (iter().filter(|&&b| b == b'\n').count() or similar) on buffers >1KB without memchr
- [RECOMMEND] memchr not in Cargo.toml as direct dependency despite byte-scanning code existing
- [RECOMMEND] simdutf8 gated on file size threshold when it should apply universally
- [GOOD] nucleo-matcher used for fuzzy search with Matcher reuse across candidates
- [GOOD] simdutf8 + from_utf8_unchecked with correct safety comment

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Throughput numbers (SIMD vs scalar). Fix.
```

### 2. allocation-audit

**Triggers**: **Always runs** — allocation waste is cross-cutting.

**Subagent prompt**:
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for unnecessary allocations and zero-copy opportunities.

Read the reference file at: .claude/skills/gtk-perf-rust-optimize/references/allocation-patterns.md

Changed files to review:
{changed_files}

{optimization_preamble}

Review criteria — check every changed file for:
1. to_string_lossy().to_string(): this double-converts OsStr → Cow<str> → String. Replace with .into_owned() when the String is consumed, or keep the Cow<str> if only a &str is needed. Most impactful in loops (IndexedFile::new at up to 100k iterations).
2. Save-path memory doubling: buffer.text().to_string() copies the entire file content from GString to String. Flag if this pattern appears without a comment acknowledging the unavoidable copy.
3. Cow<str> / Cow<Path> opportunities: functions that accept String or PathBuf but only read the data should accept &str or &Path. Functions that sometimes borrow, sometimes own should use Cow.
4. Error string allocation: Result<T, String> with format!() where a thiserror enum would be zero-allocation and enable pattern matching instead of string comparison.
5. Collection intermediaries: heap.into_vec().into_iter().map().collect() creates two Vecs. Look for ways to eliminate intermediate collections.
6. format!() for simple concatenation: format!("{}{}", a, b) allocates where push_str on a pre-allocated String would be cheaper in hot loops.

Anti-patterns to flag:
- [FLAG] to_string_lossy().to_string() in a loop running >100 iterations (e.g., file index rebuild)
- [FLAG] Error strings compared via == instead of enum pattern matching (fragile + allocates)
- [RECOMMEND] to_string_lossy().to_string() anywhere — use .into_owned()
- [RECOMMEND] Result<T, String> in service functions — use thiserror enum
- [RECOMMEND] Intermediate collection in result extraction (double Vec allocation)
- [CONSIDER] Cow<str> opportunity where a function always clones its &str parameter
- [GOOD] Arc<PathBuf> sharing for workspace roots instead of per-file PathBuf clone
- [GOOD] Vec::with_capacity for known-size collections
- [GOOD] BinaryHeap with bounded capacity for top-N selection

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Allocation count/size quantified. Fix.
```

### 3. modern-rust-audit

**Triggers**: Any `.rs` file change. This is the lightest subagent — runs inline when it's the only applicable concern.

**Subagent prompt** (self-contained — no reference file):
```
You are reviewing Rust code in a GTK4/Libadwaita text editor for modern Rust idiom adoption.

Changed files to review:
{changed_files}

{optimization_preamble}

The project uses Rust edition 2021 with MSRV 1.83+. thiserror = "2.0" is in workspace dependencies but currently unused.

Review criteria:
1. thiserror adoption: flag Result<T, String> return types in service functions. thiserror enums eliminate allocation waste and enable pattern matching. The crate is already a workspace dependency — zero adoption cost.
2. Error string comparison: flag any `if err == "some string"` or `if err != "some string"` patterns. These are fragile (typo = silent bug) and allocate unnecessarily.
3. let-else usage: flag match arms that just extract or early-return where let-else (stable since 1.65) would be cleaner. This is readability, not performance — use [CONSIDER].
4. Edition 2024 readiness: if you spot patterns that would need changes for edition 2024 (stable since Rust 1.85), note them as [CONSIDER].
5. Unused workspace dependencies: flag crates in [workspace.dependencies] that have no use/import anywhere.

Anti-patterns to flag:
- [RECOMMEND] Result<T, String> with format!() in service functions — use #[derive(thiserror::Error)]
- [RECOMMEND] Error string equality comparison — fragile, use enum pattern match
- [CONSIDER] match { Some(x) => x, None => return } where let-else would be cleaner
- [CONSIDER] Edition 2024 migration notes
- [GOOD] Correct use of let-else for early returns
- [GOOD] anyhow::Result with .context() in service error propagation

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. Why the modern pattern is better. Fix.
```

### 4. benchmark-gap-audit

**Triggers**: Changes to `benches/`, `services/`, `ui/editor_page/`, `Cargo.toml` benchmark config, or when another subagent finds a hot path lacking benchmarks.

**Subagent prompt**:
```
You are reviewing benchmark coverage for a GTK4/Libadwaita text editor's performance-sensitive code.

Read the reference file at: .claude/skills/gtk-perf-rust-optimize/references/benchmark-gaps.md

Changed files to review:
{changed_files}

The existing benchmarks in crates/lushtext-core/benches/benchmarks.rs cover:
- fuzzy_score (7 cases), file_index_search (3 queries × 5 sizes up to 100k), file_index_rebuild (3 sizes up to 5k), search_all (3 modes), scan_directory (4 sizes up to 5k), json_persistence (3 scenarios), file_size_classify (5 buckets)

Review criteria:
- Is any new performance-sensitive function missing a Criterion benchmark?
- Do changed functions have benchmarks covering their input size range?
- Are there hot paths identified by other subagents (simd-coverage, allocation) that lack benchmarks to validate optimization impact?

Known gaps (flag if changed code touches these):
- File load/save roundtrip: the actual I/O + UTF-8 validation path is unbenchmarked
- scan_directory at 10k entries: current max is 5k but the cap is 10k
- Sort key allocation: to_string_lossy().to_lowercase() in scan_directory sort
- SearchHit collection: the heap extraction + Vec conversion path

Anti-patterns to flag:
- [RECOMMEND] New hot-path function without a Criterion benchmark
- [RECOMMEND] Existing benchmark missing the boundary input size (e.g., max cap value)
- [CONSIDER] Benchmark exists but doesn't cover the SIMD vs scalar comparison
- [GOOD] Benchmark covers multiple input sizes with appropriate sample_size tuning

Output format — return findings as:
**[SEVERITY] Title** — file:line
Description. What to benchmark. Expected baseline. Criterion setup hint.
```

## Aggregation

After all subagents return, produce the unified report:

1. **Merge findings** — combine all [FLAG], [RECOMMEND], [CONSIDER], [GOOD] items
2. **Deduplicate** — if two subagents flag the same line (e.g., allocation-audit and simd-coverage both flag `to_string_lossy` in a SIMD path), keep the more specific finding
3. **Sort by severity** — FLAG first, then RECOMMEND, CONSIDER, GOOD
4. **Cross-references** — if a finding relates to `gtk-perf-scale` (e.g., an allocation that affects RAM budget) or `gtk-responsiveness` (e.g., a hot allocation on the main thread), note the cross-reference

## Audit Report Format

```
## Rust Optimization Audit

### Summary
- **Files reviewed**: N
- **Findings**: X flag, Y recommend, Z consider, W good
- **SIMD coverage**: Brief status (e.g., "simdutf8 applied to all loads; memchr missing for line counting")
- **Allocation impact**: Brief summary (e.g., "3 hot-path patterns save ~100k allocations per index rebuild")

### [FLAG] Title — file:line
Description of the issue.
**Throughput**: SIMD vs scalar numbers, or allocation count/size.
**Impact**: Measurable effect (ns/op, MB saved, allocations eliminated).
**Fix**: Concrete recommendation with code sketch or reference.

### [RECOMMEND] Title — file:line
...

### [CONSIDER] Title — file:line
...

### [GOOD] Title — file:line
Why this pattern is correct and efficient.
```

Always quantify impact ("saves 100k allocations", "32x throughput", "eliminates 50MB transient copy") rather than vague terms.

## Guidance Mode

When implementing new features (not reviewing existing code), check:

1. Does this code process bytes or strings from files? → Use SIMD crates (simdutf8, memchr)
2. Does this convert OsStr to String? → Use `.into_owned()` not `.to_string()`
3. Does this return `Result<T, String>`? → Use a `thiserror` enum
4. Does this allocate in a loop? → Can the allocation be hoisted or eliminated with `Cow`?
5. Is this a hot path? → Does it have a Criterion benchmark?
6. Does this create intermediate collections? → Can they be eliminated with iterator chains?

## Tone

Optimization advice must be grounded in numbers. Instead of "this allocates unnecessarily," say "this allocates a String per file during index rebuild — at 100k files, that's 100k allocations (~3MB) eliminated by switching to `.into_owned()`." Acknowledge existing good patterns before suggesting improvements.
