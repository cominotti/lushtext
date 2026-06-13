## 1. Baseline Inventory And Boundaries

- [x] 1.1 Audit every app-owned metadata path covered by the proposal and record the initial inventory table for workspace, session, drafts, saved searches, search history, bookmarks, document notes, folder notes, local history, migration ledger, and Replace All undo journals.
- [x] 1.2 Decide and document the exact bounded scan limits for sidecar directories, local-history lineages, and replace-backup journal entries.
- [x] 1.3 Add or update module guidance so old-format structs and converter code are allowed only under `services::format_upgrade::legacy`.
- [x] 1.4 Identify the current startup calls that must move behind the preflight gate and define the new `continue_startup_data_flow` boundary before editing window startup.

## 2. Format Upgrade Service

- [x] 2.1 Create the GTK-free `services::format_upgrade` module family with inventory, plan, apply, backup, diagnostics, and legacy submodules.
- [x] 2.2 Define plain Rust value objects for metadata kind, item path, format classification, inventory item, plan action, grouped plan, backup record, and apply outcome.
- [x] 2.3 Implement read-only inventory scanning for known top-level metadata files and bounded app-owned metadata directories.
- [x] 2.4 Implement plan building so current and missing v1 metadata produce no action, future versions never produce Convert, and only converter-backed older versions produce upgrade actions.
- [x] 2.5 Add the current v1 baseline behavior as an explicit no-op path with tests proving no conversion write is needed.
- [x] 2.6 Add converter-chain scaffolding and fixture conventions for the first future v1-to-v2 migration without introducing an actual v2 format change.

## 3. Preservation And Apply Semantics

- [x] 3.1 Implement the format-upgrade backup directory and manifest shape with app-data-relative paths, metadata kind, original classification/version, LushText version, timestamp, and item results.
- [x] 3.2 Implement backup-before-write helpers through `services::filesystem` and existing durable write primitives.
- [x] 3.3 Implement `apply_plan` so scan/build_plan remain read-only and all writes happen only in the explicit apply command.
- [x] 3.4 Ensure dependent items such as session plus draft manifest can be grouped so partial upgrades do not create misleading restore state.
- [x] 3.5 Implement Start Fresh preservation so affected data is backed up or quarantined before latest default state is allowed to replace it.
- [x] 3.6 Add deterministic failure seams for backup failure, write failure, and grouped-item failure under test-only configuration.

## 4. Startup Gate And Dialog

- [x] 4.1 Refactor `LushtextWindow` startup so pending rename reconciliation, workspace load, workspace scope refresh, session/draft restore, and autosave start only after the new startup data-flow method is called.
- [x] 4.2 Add a startup preflight task using `spawn_blocking_then` and the format-upgrade service without blocking the GTK main thread.
- [x] 4.3 Continue normal startup automatically when the preflight plan has no blocking decision.
- [x] 4.4 Present the supported-older-data dialog with Convert, Start Fresh, and Quit actions, with Convert as the primary action.
- [x] 4.5 Present the future-version dialog with Quit and Start Fresh actions only, with no Convert or downgrade path.
- [x] 4.6 Run Convert and Start Fresh in background tasks, disable dialog actions while work is active, and continue normal startup only after success.
- [x] 4.7 Keep failed conversion states blocked and retryable instead of silently continuing with default or empty app state.
- [x] 4.8 Ensure CLI open activation still presents the requested files after the startup gate resolves and does not steal focus or duplicate restored tabs.

## 5. Preferences Data Page

- [x] 5.1 Add a `Data` page to the Preferences Blueprint template and generated UI with stable template IDs and accessibility labels.
- [x] 5.2 Wire the Preferences Data page to run manual scans through the same `services::format_upgrade` inventory and planning path.
- [x] 5.3 Render the current/no-data state with a quiet status and no required action.
- [x] 5.4 Render representative upgradeable, future-version, damaged, and failed-conversion states with concise grouped summaries.
- [x] 5.5 Add Convert or Retry controls only when the plan exposes a deterministic supported upgrade path.
- [x] 5.6 Preserve command reachability and item-region-only scrolling for many or awkward metadata items and constrained dialog geometry.
- [x] 5.7 Update action catalog and automation documentation if any new visible command is exported as an app or window action.

## 6. Deterministic Test Coverage

- [x] 6.1 Add service tests for inventory classification across missing, current, upgradeable fixture, future-version, unsupported-old, damaged, and unsafe-to-replace metadata.
- [x] 6.2 Add tests proving scan and build_plan do not write app data.
- [x] 6.3 Add tests proving future-version metadata never receives Convert and older versions without converters remain unsupported.
- [x] 6.4 Add tests for backup-before-write ordering using deterministic failure seams.
- [x] 6.5 Add integration tests for mixed app-data directories with workspace, session, draft manifest, sidecar, search, local-history, migration-ledger, and Replace All undo metadata.
- [x] 6.6 Add generated-input or property-style tests proving bounded malformed JSON classification does not panic and does not write app data.
- [x] 6.7 Add widget tests proving the startup gate appears before workspace/session/draft restore consumes affected metadata.
- [x] 6.8 Add widget tests for Convert success, Convert failure with retry, future-version no-Convert behavior, and Start Fresh preservation.
- [x] 6.9 Add widget tests for `Preferences > Data` empty/current, representative upgradeable, many awkward items, failed conversion, future-version, and constrained geometry states.
- [x] 6.10 Run targeted service/integration/widget tests during development and treat any `FLAKY:` output from widget tests as a blocker.

## 7. Documentation And Validation

- [x] 7.1 Update `docs/recovery-reliability.md` with the difference between format upgrades, recovery quarantine, Start Fresh preservation, and rename migration ledgers.
- [x] 7.2 Update persistent JSON fixture documentation or comments to describe the current v1 no-op baseline and future converter fixture expectations.
- [x] 7.3 Update root and nested module maps if new modules or Preferences files require documentation changes.
- [x] 7.4 Run `cargo fmt --all -- --check`.
- [x] 7.5 Run `./scripts/check-filesystem-boundary.sh`.
- [x] 7.6 Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [x] 7.7 Run targeted `cargo nextest` tests for format-upgrade, recovery metadata, persistent JSON format, session/draft, workspace, and affected integration modules.
- [x] 7.8 Run `make test-widget-headless` and confirm there is no `FLAKY:` output.
- [x] 7.9 Run `make check-blueprint` and `make lint-blueprint` for Preferences template changes.
- [x] 7.10 Confirm no exported actions, automation fields, readiness predicates, or helper flags were added, so `make check-automation-docs` and `make automation-client-self-test` are not required for this change.
- [x] 7.11 Skip `make crash-recovery-smoke` because the current implementation has only the v1 no-op baseline and converter scaffolding; run it after the first real non-no-op converter exists.
