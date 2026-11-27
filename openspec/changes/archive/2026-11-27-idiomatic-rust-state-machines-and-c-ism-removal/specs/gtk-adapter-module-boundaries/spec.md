## ADDED Requirements

### Requirement: GObject imp modules remain thin widget containers
Each widget `imp.rs` SHALL stay focused on GObject subclassing, template children, property/signal definitions, and thin delegation (`<= 500` production lines), placing workflow-owned runtime state in its owning bounded coordination module and consolidating multi-slot `Box<dyn Fn...>` callback farms into typed GObject signals or one typed widget event callback.

#### Scenario: Workflow state moves out of oversized imp.rs without adding roles or manager traits
- **WHEN** an `imp.rs` exceeds 500 production lines due to embedded workflow state
- **THEN** workflow state structs move into their existing bounded coordination modules (`execution.rs`, `watch.rs`, `journal.rs`, or `*_execution.rs`) as plain Rust types while preserving `make check-workflow-boundaries` and `docs/workflow-readability-matrix.md`

#### Scenario: Callback farms consolidate into signals or a single typed event callback
- **WHEN** a widget notifies parents of multiple lifecycle events
- **THEN** GObject/primitive notifications use typed GObject signals checked by `make check-signal-connections`, and Rust-payload notifications (`DisposalOwned<T>`) route through at most one typed event callback enum per widget
