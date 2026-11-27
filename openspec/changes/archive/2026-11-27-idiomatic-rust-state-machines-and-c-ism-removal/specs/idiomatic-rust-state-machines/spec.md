## ADDED Requirements

### Requirement: Workflow lifecycle states use algebraic phase enums rather than flat flag bags
Workflow runtime state (`*State`, `*Session`, `*Progress`) SHALL model mutually exclusive lifecycle phases as sum-type enum variants rather than parallel `Cell<bool>` and `RefCell<Option<_>>` fields, transitioning via full-value replacement (`mem::take` / `RefCell::replace`) outside active borrows.

#### Scenario: Impossible partial state becomes unrepresentable
- **WHEN** a workflow such as `LoadState`, `TabOverflowState`, `SearchPreviewState`, `DraftState`, or `BufferReplacementSession` moves between phases
- **THEN** phase-specific fields live inside that phase's enum variant rather than as independent nullable/boolean fields
- **AND** manual field-by-field `reset_*_state` functions are replaced by resetting the whole enum to `Idle`

#### Scenario: Evidence surfaces remain side-effect-free and borrow-safe
- **WHEN** an `evidence.rs` accessor reads a consolidated phase enum
- **THEN** it copies scalars before releasing the borrow and continues to pass its side-effect-freedom and disposal-honesty proofs

### Requirement: C/C++-style sentinels, index loops, and unchecked casts are avoided
Workspace Clippy SHALL deny `struct_excessive_bools` (`max-struct-bools = 3`, reasoned `#[expect]` allowed only on `*Evidence`/`*Snapshot` DTOs, domain fact/feature masks, and Kani harnesses) and `cast_possible_wrap`. GTK `-1` sentinels SHALL convert to `Option` at adapter edges, and non-SIMD/non-cursor loops SHALL prefer iterators over `while idx < len`.

#### Scenario: GTK -1 sentinel converts to Option at the adapter boundary
- **WHEN** GtkSourceView or GTK returns `-1` for an indeterminate count or unset dimension
- **THEN** the adapter wrapper converts it to `Option` immediately so pure `policy.rs` modules never branch on `-1`

#### Scenario: Excessive boolean structs fail Clippy outside allowlisted categories
- **WHEN** a runtime state struct carries four or more raw `bool` fields without an evidence DTO, domain mask/facts, or Kani harness reason
- **THEN** `make check` fails on `clippy::struct_excessive_bools`

### Requirement: Idiomatic state and C-ism ceilings are gated with self-tests
`scripts/check-idiomatic-state.py` (with `--self-test`, run in `make check-policy`) SHALL enforce ratchets on parallel `Cell<bool>`/`RefCell<Option<_>>` state bags, `imp.rs` line counts (`<= 500` lines target), `Box<dyn Fn...>` slots per `imp.rs` (`<= 2` target), manual `reset_*_state` functions, and `-1` sentinels outside FFI wrappers.

#### Scenario: Gate self-test proves detection of every C-ism regression
- **WHEN** `python3 scripts/check-idiomatic-state.py --self-test` runs
- **THEN** it proves failure on an oversized `imp.rs` (`> 500` lines), a multi-callback `imp.rs` farm, a `>= 4` flag+option `*State` bag, a `reset_*_state` function, and a non-FFI `-1` sentinel
