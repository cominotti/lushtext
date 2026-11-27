# Proposal: Typed Service Error Boundaries

## Why
`.agents/rules/rust.md` currently contradicts itself: it requires domain-named cross-boundary error types (`EditorLoadError`, `EditorSaveError`, `WorkspaceWatchError`) while simultaneously stating *"Services return `anyhow::Result`"*. In practice, 22 files in `lushtext-services` and 12 files in `lushtext-core` still use `anyhow`, and 19 signatures return `Result<_, String>`, forcing UI callers either to treat all failures as opaque strings or to rely on brittle error-message inspection.

## Baseline
Measured on `main` (`3c442112`) across `crates/lushtext-{model,services,core}/src` (production files):
- `anyhow` references (`anyhow::` / `use anyhow`): **22** files in `lushtext-services`, **12** files in `lushtext-core` (0 in `lushtext-model`); `anyhow::Result` occurrences: **23**; `.context(`/`.with_context(`: **27**.
- `thiserror::Error` adoption: **4** files in `lushtext-services`, **1** file in `lushtext-core` (**10** total error enums).
- `Result<_, String>` occurrences: **19** (led by `conformance/write_shell.rs`: 9, `conformance/draft_service.rs`: 5, `format_upgrade/inventory.rs`: 2, plus 3 in UI: `preferences/data_page.rs`, `window/notes/policy.rs`, `window/startup_data.rs`).

## What Changes
- Replace `anyhow::Result` across all **22** files in `lushtext-services` (and production boundaries in `lushtext-ui`) with typed `thiserror::Error` enums named after their workflow/domain (`SessionServiceError`, `BookmarkServiceError`, `DocumentNoteError`, `FolderNoteError`, `LocalHistoryError`, `WorkspaceManagerError`, `FormatUpgradeError`, `RecentDocumentsError`, `SavedSearchesError`, `SearchHistoryError`, `SearchBackupError`, `JsonStoreError`, etc.) with structured variants (`Io`, `NotFound`, `PermissionDenied`, `CorruptFormat`, `BudgetExceeded`, `InvalidInput`, etc.).
- Replace all **19** `Result<_, String>` returns in production and conformance code with typed error enums.
- Remove `anyhow` from `lushtext-services/Cargo.toml` (and from `lushtext-ui/Cargo.toml` production dependencies, keeping `anyhow` only in test/bench/CLI harnesses if needed).
- Resolve the rule contradiction in `.agents/rules/rust.md`: unify on typed domain `thiserror` errors across all service and UI-service boundaries.
- Add mechanical gates: extend `scripts/check-hexagonal-boundaries.py` to forbid `anyhow` in `lushtext-model` and `lushtext-services` production dependencies, and add `scripts/check-error-contracts.py` (`--self-test`, wired into `make check-policy`) forbidding `Result<_, String>` and `anyhow::Result` on production service/UI boundaries.

## Non-goals
- Altering user-facing notification text or inline-alert behavior (callers match structured error variants to emit the exact same user messages and accessibility announcements).
- Refactoring `anyhow` in standalone dev/test binaries or `cargo-gtk-proof` CLI tooling.

## User-visible impact
Zero user-visible behavioral regression compared with `main`; error classification in UI adapters (`NotFound` vs `PermissionDenied` vs `CorruptFormat` vs `DurabilityUnconfirmed`) becomes exhaustive at compile time.

## Capabilities

### New Capabilities
- `typed-service-error-boundaries`: Require typed `thiserror` domain error enums across `lushtext-services` boundaries and forbid `anyhow::Result` and `Result<_, String>` on production service interfaces.

### Modified Capabilities
None.

## Impact
- **Code**: `crates/lushtext-services/Cargo.toml`, 22 `lushtext-services` modules, `crates/lushtext-ui/Cargo.toml`, 12 `lushtext-ui` callers, 3 UI `Result<_, String>` modules, and `formal/evaluation/stateright/tests/e1_findings.rs` (`draft_service::set_aside` caller).
- **Gates**: `scripts/check-error-contracts.py`, `scripts/check-hexagonal-boundaries.py`, `Makefile`, `.github/workflows/ci.yml`.
- **Docs**: `.agents/rules/rust.md`, `AGENTS.md`, `docs/next/idiomatic-rust-hexagonal-programme.md`.
