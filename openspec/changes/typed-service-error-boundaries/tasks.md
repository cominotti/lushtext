# Tasks: Typed Service Error Boundaries

Kani: none (no proven module or Kani harness inputs modified — verify via `scripts/kani-shards.py oracle` that `durable_write` / `draft_service` proven cores keep their existing typed protocol enums untouched).
Timing: none (no timed path touched).

## 1. Failing-first error contract gate & rule alignment

- [x] 1.1 Write `scripts/check-error-contracts.py` with `--self-test` (proving failure on `anyhow::Result` and `Result<_, String>` in production fixtures) and verify the real scan fails on the 22 `lushtext-services` `anyhow` files, 12 `lushtext-ui` `anyhow` files, and 19 `Result<_, String>` sites.
- [x] 1.2 Wire `check-error-contracts` into `Makefile` (`check-policy`) and `.github/workflows/ci.yml`, update `.agents/rules/rust.md` to resolve the `anyhow` contradiction in favor of domain-named `thiserror` enums, run `make check-agent-docs`, create a signed commit (`git commit -S`), and push the branch.

## 2. Migrate `lushtext-services` & conformance modules to typed `thiserror` enums

- [ ] 2.1 Replace `anyhow::Result` across persistence and storage services (`json_store`, `recovery_metadata`, `format_upgrade`, `session_service`, `workspace_manager`, `local_history_service`, `recent_documents`, `saved_searches`, `search_history`, `search_backup`, `migration_ledger`, `app_data_leftovers`) with domain-named `thiserror::Error` enums; add unit tests asserting structured error variants on corrupt/missing/permission failures, and verify `cargo test -p lushtext-services` passes.
- [ ] 2.2 Replace `anyhow::Result` across notes/editor/workspace services (`bookmark_service`, `bookmark_excerpt`, `document_note_service`, `folder_note_service`, `note_storage`, `editor_io`, `editorconfig`, `file_tree`, `markdown_render`, `workspace_watch`, `draft_service::set_aside`), replace the 16 `Result<_, String>` sites (including `conformance/write_shell.rs`, `conformance/draft_service.rs`, `format_upgrade/inventory.rs`), and update `formal/evaluation/stateright/tests/e1_findings.rs` for the typed `set_aside` error; verify `cargo test -p lushtext-services --all-features` and `stateright` tests pass.
- [ ] 2.3 Remove `anyhow` from `crates/lushtext-services/Cargo.toml` `[dependencies]` and enable the `anyhow` ban for `lushtext-services` in `scripts/check-hexagonal-boundaries.py`; create a signed commit (`git commit -S`) and push the branch.

## 3. Migrate `lushtext-ui` callers & full verification

- [ ] 3.1 Update all 12 `lushtext-ui` callers of `anyhow` and the 3 UI `Result<_, String>` sites (`preferences/data_page.rs`, `window/notes/policy.rs`, `window/startup_data.rs`) to match on typed service/domain errors; remove `anyhow` from `crates/lushtext-ui/Cargo.toml` `[dependencies]` (`cargo hakari generate` if workspace deps change).
- [ ] 3.2 Run `make check`, `make check-policy` (confirming `check-error-contracts` and `check-hexagonal-boundaries` pass at 0 violations), `make test-unit`, `make check-kani-shards`, and `openspec validate typed-service-error-boundaries --strict`.
- [ ] 3.3 Update `AGENTS.md` and `docs/next/idiomatic-rust-hexagonal-programme.md` with Change 3 completion metrics, create a signed commit (`git commit -S`), and push the branch.
