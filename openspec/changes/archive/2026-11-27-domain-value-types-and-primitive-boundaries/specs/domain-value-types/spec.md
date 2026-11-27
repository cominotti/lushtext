## ADDED Requirements

### Requirement: Typed generations and domain IDs share zero-cost representation
`lushtext-model::values` SHALL provide `#[repr(transparent)]`, `Copy`, `const fn`, Kani-compatible (`#[cfg_attr(kani, derive(kani::Arbitrary))]`) typed generation and ID wrappers via `define_generation!` and `define_id!`, verified by compile-time `size_of` assertions and used for workflow generations and `EditorId`.

#### Scenario: Swapping distinct generations fails at compile time
- **WHEN** a caller passes `PolicyGeneration` where `AccessGeneration` is expected (such as in `EditorResidency`)
- **THEN** compilation fails with a type mismatch rather than accepting an implicit `u64` swap

#### Scenario: Raw generation and editor_id fields are ratcheted at boundaries
- **WHEN** a new raw `u64`/`u32`/`usize` field named `*generation*` or `editor_id` is added to `seams.rs`, `policy.rs`, or a `*State`/`*Session` struct
- **THEN** `scripts/check-domain-value-types.py` (in `make check-policy`) fails naming the file and field

### Requirement: Domain units use distinct zero-cost value types across boundaries
`lushtext-model::values` SHALL define `Bytes(u64)`, `Sp(i32)`, `Micros(u64)`, `CharCount(u64)`, and `LineIndex(u32)` as `#[repr(transparent)]`, `Copy`, `const fn` types with saturating arithmetic and `const assert!(size_of::<T>() == size_of::<Inner>())`. Pure geometry and memory policies SHALL use `Sp` and `Bytes`/`CharCount` across module boundaries, leaving local loop indices as primitives.

#### Scenario: Whole-pixel geometry policies prevent unit confusion
- **WHEN** `ui/window/geometry/policy.rs`, `ui/window/focus_mode/policy.rs`, `ui/sidebar/width_preset.rs`, or `ui/window/tab_overflow/policy.rs` computes pixel widths or margins
- **THEN** widths and margins carry `Sp` rather than bare `i32`, preventing accidental mixing with raw divisors or Pango units

#### Scenario: Local loop indices stay ergonomic primitives
- **WHEN** a function iterates over a slice or buffer with a local loop index or temporary count
- **THEN** it uses `usize` without introducing newtype boilerplate

### Requirement: Multi-boolean parameter lists are replaced by typed enums or fact structs
Production functions SHALL NOT accept two or more positional `bool` parameters (`clippy::fn_params_excessive_bools = "deny"` with `max-fn-params-bools = 1` in `clippy.toml`). Multi-flag boundaries SHALL take 1-byte `Copy` two-variant enums or named `Copy` fact structs.

#### Scenario: Positional boolean transposition becomes unrepresentable
- **WHEN** a multi-flag policy function such as `draft_candidate_is_eligible` is called
- **THEN** each flag is a distinct `Copy` enum or named field in a fact struct so swapping two flags fails to compile

#### Scenario: New multi-bool production signature fails Clippy
- **WHEN** a production function is written with `>= 2` positional `bool` parameters
- **THEN** `make check` fails on `clippy::fn_params_excessive_bools`

### Requirement: Value-type gate self-test proves layout and field checks
`scripts/check-domain-value-types.py --self-test` SHALL prove failure on a raw `*generation*` field in a boundary file, a missing `size_of` assertion on a unit wrapper in `lushtext-model/src/values/`, and passing behavior on conforming wrappers.

#### Scenario: Missing layout assertion fails policy self-test
- **WHEN** a unit wrapper in `lushtext-model/src/values/` omits its `const _: () = assert!(size_of::<...>() == ...)` check
- **THEN** `scripts/check-domain-value-types.py` fails naming the type
