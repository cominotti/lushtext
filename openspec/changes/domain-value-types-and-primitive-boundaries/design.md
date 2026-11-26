# Design: Domain Value Types and Primitive Boundaries

## Context
Across `lushtext-model`, `lushtext-services`, and `lushtext-ui`, 204 integer fields represent distinct monotonic generations (`load_generation`, `edit_generation`, `save_generation`, `access_generation`, `policy_generation`), 5 fields represent `editor_id: usize`, 48 functions take 2–5 positional `bool` arguments, and pure policies mix `u64` bytes with `u64` characters or `i32` whole pixels (`sp`) with integer divisors. Because every newtype here is `#[repr(transparent)]` + `Copy` + `const fn`, we obtain compile-time type safety with zero runtime, stack, or heap overhead.

## Goals / Non-Goals
**Goals:**
- Provide `define_generation!`, `define_id!`, `Bytes`, `Sp`, `Micros`, `CharCount`, and `LineIndex` in `lushtext-model::values`.
- Replace raw boundary generations, `editor_id`s, unit-ambiguous policy parameters, and the 48 multi-bool signatures.
- Teach `scripts/check-threshold-kinds.py` to verify typed unit constants (`Bytes::kib(64)`, `Sp::new(280)`) alongside integer constants.
- Enforce compliance mechanically via Clippy (`fn_params_excessive_bools`) and `scripts/check-domain-value-types.py`.

**Non-Goals:**
- Wrapping local loop counters or private arithmetic temporaries ("no newtype soup").
- Consolidating `bool` + `Option` widget state bags into enums (`idiomatic-rust-state-machines-and-c-ism-removal`).

## Decisions

### D1. `#[repr(transparent)]` + `Copy` + `const assert!` layout contract
- **Decision**: Every wrapper in `lushtext-model::values` uses `#[repr(transparent)]`, derives `Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash`, implements `const fn` constructors/accessors, adds `#[cfg_attr(kani, derive(kani::Arbitrary))]` (or explicit `kani::Arbitrary` impl), and carries `const _: () = assert!(std::mem::size_of::<T>() == std::mem::size_of::<Inner>());`.
- **Rationale**: `Cell<T>` requires `Copy`; Kani/CBMC verifies `#[repr(transparent)]` wrappers identically to the underlying primitive without heap/pointer explosion; zero ABI/size difference preserves `EDITOR_MEMORY` and `B_INTERACT_US` / `B_SETTLE_US` bounds.
- **Alternatives rejected**: Using a single generic `Generation<Tag>` phantom type everywhere (adds `<Tag>` noise and `PhantomData` boilerplate across `ui/` signatures, directly violating matklad's principle of readable concrete types); using `#[serde(transparent)]` only on ad-hoc newtypes without shared macros (already caused inconsistent APIs across the 20 existing newtypes).

### D2. Boundary-only rule ("no newtype soup")
- **Decision**: Apply unit/ID/generation value types strictly to (a) module/workflow boundaries (`seams.rs`, `policy.rs`, public service structs), (b) `*State`/`*Session` fields, and (c) constants in `docs/thresholds.md` where units can collide (`Bytes`, `Sp`, `Micros`, `CharCount`). Keep `usize` for local loop indexes, slice indexing, and single-function counters.
- **Rationale**: Wrapping every local integer trades one form of syntactic noise (`.0`, `.as_usize()`) for another. Boundary-scoped newtypes give maximum bug prevention with minimal call-site friction.
- **Alternatives rejected**: Blanket ban on raw `u64`/`usize` in function signatures (would make slice/index utilities unreadable).

### D3. Replacing multi-boolean signatures with `fn_params_excessive_bools = "deny"`
- **Decision**: Set `max-fn-params-bools = 1` in `clippy.toml` and `fn_params_excessive_bools = "deny"` in `[workspace.lints.clippy]`. Migrate the 48 existing `>= 2`-bool functions to 1-byte `Copy` two-variant enums (e.g., `BufferTrust::Trusted | Untrusted`) or named `Copy` fact structs (e.g., `DraftCandidateFacts`). Keep `#[expect(clippy::fn_params_excessive_bools, reason = "...")]` unavailable on workflow boundaries and allowed only for documented FFI/catalog bindings if any exist.
- **Rationale**: `max-fn-params-bools = 1` means a single `bool` parameter (e.g., `set_visible(bool)`) remains zero-ceremony, while any two positional booleans (which can be transposed silently) become a compile-time error.
- **Alternatives rejected**: `max-fn-params-bools = 2` (still allows 2-bool transpositions like `is_dir, is_new`, which already caused a documented bug in `ui/sidebar/seams.rs`).

### D4. Extending `scripts/check-threshold-kinds.py` for typed unit constants
- **Decision**: Update `scripts/check-threshold-kinds.py` so `pub const NAME: Bytes = Bytes::from_bytes(...)` / `Bytes::kib(...)` / `Bytes::mib(...)`, `Sp::new(...)`, `Micros::from_us(...)`, `Micros::from_ms(...)`, and `CharCount::new(...)` extract the underlying integer value and apply the exact same `docs/thresholds.md` power-of-two, presentation-cap, and `Kind:` checks.
- **Rationale**: Preserves 100% of `threshold-constants` safety guarantees while letting constants carry their domain unit in the type system.
- **Alternatives rejected**: Keeping all threshold constants as raw `u64`/`i32` and wrapping at every call site (creates `.into()` noise everywhere and forfeits type safety at constant definition).

## Budget Exceptions
None. `#[repr(transparent)]` wrappers preserve identical values and power-of-two rounding.

## GTK Axiom Ledger
None (pure Rust value representations; GTK boundaries convert `Sp -> i32/f64` and `Micros -> Duration/u64` at the adapter edge).

## Timing
Verdict class: non-inferiority (changes signatures/types on timed paths in `editor_memory`, `protected_fold`, `protected_install`, `protected_view`, `geometry`, `load`, and `responsiveness_budget`, though runtime representation is `#[repr(transparent)]`).
- **Pre-registered protocol** (per `.agents/rules/verification-runs.md`): run `scripts/measure.sh` / `make responsiveness-smoke` and `cargo bench` comparing `main` vs branch on literal smoke cases `eviction/reselect`, `eviction/switch-hidden-dense`, `recovery/protected-32mib`, `save/during-validation`, `baseline/small/steady-populated` and Criterion groups `editor_memory_policy`, `transient_file_load`, `document_scale`, `save_admission_policy`.
- **Primary metrics**: per-dispatch p50/p95 µs against `B_INTERACT_US` / `B_SETTLE_US` and Criterion median ns/iter.
- **Noise band**: 5% on Criterion microbenchmarks and 500 µs on responsiveness-smoke dispatches (historical machine spread); no case meeting `B_INTERACT_US` or `B_SETTLE_US` on `main` may exceed it on the branch.
