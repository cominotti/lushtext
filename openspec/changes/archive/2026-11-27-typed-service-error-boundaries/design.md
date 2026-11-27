# Design: Typed Service Error Boundaries

## Context
`.agents/rules/rust.md` currently holds two conflicting instructions: "Cross-boundary error types must name the workflow or domain they report for" (line 277) and "Services return `anyhow::Result`" (line 511). As a result, 22 service files and 12 UI files depend on `anyhow`, while 19 signatures return `Result<_, String>`. Replacing erased errors with domain-named `thiserror` enums makes error handling exhaustive, preserves `#[source]` chains for `tracing`, and aligns `lushtext-services` with `EditorLoadError` / `EditorSaveError`.

## Goals / Non-Goals
**Goals:**
- Define cohesive `thiserror::Error` enums per service domain in `lushtext-services`.
- Remove `anyhow` from `lushtext-services` (and `lushtext-ui` production `[dependencies]`).
- Eliminate all 19 `Result<_, String>` production/conformance signatures.
- Unify `.agents/rules/rust.md` and gate compliance with `scripts/check-error-contracts.py` (`--self-test`).

**Non-Goals:**
- Changing user-facing alert wording or failure recovery policies (data safety contracts stay 100% identical).
- Removing `anyhow` from CLI/test/benchmark harnesses outside production library code.

## Decisions

### D1. One domain `thiserror::Error` enum per service boundary (no giant global `ServiceError`)
- **Decision**: Each service family defines its own concise error enum (`SessionServiceError`, `BookmarkServiceError`, `DocumentNoteError`, `FolderNoteError`, `LocalHistoryError`, `WorkspaceManagerError`, `FormatUpgradeError`, `RecentDocumentsError`, `SavedSearchesError`, `SearchHistoryError`, `SearchBackupError`, `JsonStoreError`, etc.) deriving `thiserror::Error` and `Debug`, with `#[from]` only where lossy erasure doesn't occur (e.g., `Io(#[from] std::io::Error)` or explicit `Io { action, path, #[source] source }`).
- **Rationale**: A single global `LushTextError` forces every caller to handle variants that cannot happen in its workflow (e.g., Markdown errors in bookmark loading). Per-domain enums keep `match` arms exhaustive and local.
- **Alternatives rejected**: Keeping `anyhow::Result` internally and converting at the very end (still hides internal failure modes during unit tests and encourages `.context("string")` instead of structured handling); a monolithic workspace-wide error enum (causes excessive coupling).

### D2. Removing `Result<_, String>` in UI & conformance helpers
- **Decision**: Replace the 3 UI `Result<_, String>` sites (`preferences/data_page.rs`, `window/notes/policy.rs`, `window/startup_data.rs`) and 16 service/conformance `Result<_, String>` sites with dedicated typed enums (even small 2–4 variant enums), implementing `Display` via `thiserror`.
- **Rationale**: `Result<_, String>` is an untyped C-style error message disguised as a `Result`; callers cannot distinguish recoverable vs fatal failures without string comparisons.
- **Alternatives rejected**: Keeping `Result<_, String>` in `conformance/` or private UI helpers (invites copy-pasting into production boundaries).

### D3. Mechanical gate (`scripts/check-error-contracts.py`)
- **Decision**: Add `scripts/check-error-contracts.py` (`--self-test`, wired into `make check-policy`), scanning comment/literal-stripped production code in `lushtext-model`, `lushtext-services`, and `lushtext-ui` (excluding `#[cfg(test)]`, `conformance/` only if test-only — wait: `conformance/` is migrated too, so scan all `src/` outside `#[cfg(test)]`) for `anyhow::Result`, `use anyhow`, and `Result<..., String>` (with ceiling 0 once this change lands), alongside `scripts/check-hexagonal-boundaries.py` banning `anyhow` in `lushtext-services/Cargo.toml` `[dependencies]`.
- **Rationale**: Prevents `anyhow::Result` or `Result<_, String>` from creeping back into service boundaries after migration.
- **Alternatives rejected**: Clippy `disallowed_types` alone (`disallowed_types` is not easily scoped per-crate/per-visibility across test vs production targets without false positives in `dev-dependencies`).

## Budget Exceptions
None.

## GTK Axiom Ledger
None.

## Timing
Timing: none (no timed path touched). Error paths run only on I/O, parse, or validation failures; hot paths are unchanged.
