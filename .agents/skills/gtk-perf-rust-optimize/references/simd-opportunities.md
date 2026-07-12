# Established Acceleration Patterns

Use this reference to recognize existing hot-path choices. Verify each pattern and dependency in the current checkout before reporting it; CPU features, file sizes, and speedups vary by target and workload.

## Contents

1. [Encoding-aware text decode](#encoding-aware-text-decode)
2. [Byte scanning](#byte-scanning)
3. [Fuzzy matching](#fuzzy-matching)
4. [Evidence standard](#evidence-standard)

## Encoding-aware text decode

`services/editor_io.rs` owns document decoding. Its normal load path reads bytes through `services::filesystem`, checks BOM and explicit reopen choices, uses `simdutf8` as the fast valid-UTF-8 branch, and falls back to supported legacy encodings. The current production conversion is safe:

```rust
if let Ok(utf8) = simdutf8::basic::from_utf8(bytes) {
    utf8.to_string()
} else {
    // Continue through the encoding-aware fallback policy.
}
```

Do not replace this workflow with “validate then `from_utf8_unchecked`,” and do not require SIMD validation for unrelated small metadata reads. The contract is correct decoding and surfaced confidence, not SIMD for its own sake.

When reviewing a new document-reading path, first ask whether it should reuse `editor_io` rather than duplicating decode policy.

## Byte scanning

The workspace has a direct `memchr` dependency and uses it on established byte-scanning paths. Recommend it only when all of these hold:

- the input is already bytes;
- scanning is material at realistic sizes or frequency;
- Unicode semantics are not required;
- an existing equivalent path or benchmark supports the choice.

Never infer a universal crossover size. `memchr` chooses optimized implementations at runtime where supported, but targets do not all guarantee AVX2 or NEON. Empty and tiny editor files are valid inputs.

## Fuzzy matching

Palette scoring uses `nucleo-matcher`. Preserve matcher/buffer reuse and bounded result handling when changing equivalent search paths. Do not describe `nucleo-matcher` as guaranteeing a particular SIMD instruction set or measured speedup.

New search semantics may not be equivalent to fuzzy filename scoring. Confirm ranking, Unicode, highlighting, and cancellation requirements before reusing the matcher.

## Evidence standard

- Verify versions in the workspace `Cargo.toml`.
- Locate the current call site with `rg`.
- Use checked-in Criterion groups or add representative coverage for a changed hot path.
- Compare before and after on the same machine, profile, and data set.
- Label external crate behavior as such; do not turn approximate hardware figures into project guarantees.
