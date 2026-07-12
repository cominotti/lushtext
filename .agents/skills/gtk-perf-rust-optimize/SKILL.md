---
name: gtk-perf-rust-optimize
description: Explicit deep-dive checklist for Rust hot-path correctness, idioms, and established acceleration patterns in LushText. Use when explicitly invoked or when assigned as the Rust hot-path leaf by gtk-perf-review; do not auto-invoke alongside the umbrella performance skill. Applies to byte decoding, search scoring, indexing, file processing, error models, and benchmark-sensitive Rust. Leaf reviewers must return findings directly and must not delegate.
---

# Rust Hot-Path Deep Dive

Review whether performance-sensitive Rust is correct, readable, and consistent with patterns proven in the current checkout. This is a leaf of `gtk-perf-review`; never spawn subagents from this skill.

## Load references selectively

- Read [references/simd-opportunities.md](references/simd-opportunities.md) for byte validation, scanning, and fuzzy matching.
- Read [references/allocation-patterns.md](references/allocation-patterns.md) for owned handoffs, saves, typed errors, and model batching.
- Read the applicable repository `AGENTS.md` files before judging ownership or architecture.

## Checklist

1. Inspect only the supplied scope and its diff.
2. Verify dependency versions, edition, MSRV, and established patterns from current `Cargo.toml` and code; never recommend a migration because a skill reference says a version is current.
3. Preserve `services::filesystem` as the production filesystem boundary; an optimization must not bypass it for direct standard-library I/O.
4. For text loading, preserve the encoding-aware `services::editor_io` pipeline. `simdutf8` is a fast validation branch for valid UTF-8, not proof that every file is UTF-8. Do not prescribe unsafe conversion; current production code converts the validated `&str` with safe APIs and falls back to BOM, UTF-16, or Windows-1252 handling.
5. Recommend `nucleo-matcher`, `memchr`, or another optimized primitive only when the repository already uses it for a materially equivalent hot path or profiling/benchmarks justify it.
6. Check typed errors and exhaustive handling where callers need to distinguish failure modes. Do not demand `thiserror` for a small private helper whose single error shape is already clear.
7. Check unsafe code only when present in scope. Require a local safety argument and a measurable reason; prefer a safe equivalent when its cost is immaterial.
8. Check benchmarks for changed, established hot paths. Defer scale/budget calibration to `gtk-perf-scale`.
9. Reject suggestions that trade readability for speculative throughput.

## Finding rules

- **FLAG** correctness bugs, unsafe precondition gaps, fragile error discrimination, or bypasses of an established hot-path contract.
- **RECOMMEND** a measured or strongly evidenced improvement that also preserves clarity.
- **CONSIDER** an optional profiling or benchmark follow-up.
- **GOOD** relevant correct patterns.

Do not use arbitrary item-count cutoffs to excuse or condemn an allocation. Judge frequency, input size, ownership, and evidence. Do not quote throughput figures unless reproduced in the current environment or sourced from a checked-in benchmark artifact.

Return findings with `file:line`, evidence, user/correctness impact, and a concrete fix. If no issue survives verification, say so and list the inspected hot paths.
