## ADDED Requirements

### Requirement: Production services return domain-named `thiserror` error enums
Every fallible `pub` or `pub(crate)` service function in `lushtext-services` SHALL return a typed `thiserror::Error` enum named for its workflow or domain (such as `SessionServiceError` or `WorkspaceManagerError`) with structured failure variants so UI adapters can match exhaustively without string matching or downcasting.

#### Scenario: Service failure variants drive UI handling without downcasting
- **WHEN** a service operation fails due to missing file, permission error, corrupt JSON, or budget overflow
- **THEN** the service returns a distinct enum variant carrying typed context (`#[source]` / path / limit)
- **AND** the UI caller matches on the enum directly without `anyhow::Error::downcast_ref` or error-string substring checks

### Requirement: `anyhow::Result` and `Result<_, String>` are forbidden on production boundaries
Production code in `lushtext-model`, `lushtext-services`, and `lushtext-ui` SHALL NOT declare `anyhow::Result` or `Result<_, String>` on module boundaries, and `lushtext-services` SHALL NOT depend on `anyhow` in production `[dependencies]`.

#### Scenario: Introducing `anyhow` or `Result<_, String>` fails policy
- **WHEN** a production module adds `anyhow` to `lushtext-services/Cargo.toml` or exports a `Result<_, String>` or `anyhow::Result` signature
- **THEN** `make check-policy` (`scripts/check-hexagonal-boundaries.py` and `scripts/check-error-contracts.py`) fails naming the file and line

#### Scenario: Error contract self-test proves both prohibitions
- **WHEN** `python3 scripts/check-error-contracts.py --self-test` runs
- **THEN** it proves failure on fixture files containing `pub fn foo() -> anyhow::Result<()>` and `pub(crate) fn bar() -> Result<(), String>`
