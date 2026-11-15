## ADDED Requirements

### Requirement: The workspace stays clean under the next stable Clippy
Before the pin moves, the workspace SHALL stay free of diagnostics that the newest stable Clippy would make blocking under the workspace lint table. Fixes SHALL use only APIs available at the MSRV and on the pinned Kani toolchain, and MUST NOT raise the MSRV. A site with no such fix SHALL carry a narrow, reasoned `#[expect]`. The adopting change SHALL record the new diagnostics per lint and per crate.

#### Scenario: Clippy 1.100 reports nothing blocking
- **WHEN** `cargo +1.100.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` runs on the workspace
- **THEN** it exits successfully
- **AND** the blocking gate on the pinned toolchain (`make check`) stays clean

#### Scenario: map(f).unwrap_or_default() is written with map_or_default
- **WHEN** an `Option` or `Result` is mapped and then defaulted
- **THEN** the code uses `map_or_default(f)`, stable since Rust 1.98.0, instead of `map(f).unwrap_or_default()`

#### Scenario: Optional pedantic churn on proved code is not taken
- **WHEN** a newer Clippy suggests a pedantic, not-enabled rewrite inside a Kani-proved function
- **THEN** the rewrite is declined unless the change also reruns that function's Kani shard
