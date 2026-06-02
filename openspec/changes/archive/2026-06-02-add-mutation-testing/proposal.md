## Why

LushText already has broad unit, integration, and widget coverage, but normal test runs only prove that the current code behaves as expected. Mutation testing will actively inject small defects into high-risk logic so weak assertions, missing edge cases, and over-broad tests are discovered before regressions reach users.

## What Changes

- Add a repository-managed mutation-testing capability built around `cargo-mutants`.
- Introduce fast changed-code mutation checks for pull requests, using the existing non-widget `cargo nextest` lane as the primary execution surface.
- Introduce sharded full-scope mutation runs for core model, service, persistence, search, save-safety, encoding, Markdown-helper, and other deterministic logic.
- Keep GTK widget mutation testing separate and experimental until logic is intentionally extracted into deterministic seams.
- Add local developer commands and documentation for running, triaging, ratcheting, and excluding mutants.
- Publish `mutants.out` artifacts from CI so missed, timeout, and unviable mutants can be reviewed after failures.
- No breaking application behavior changes.

## Capabilities

### New Capabilities

- `mutation-testing`: Defines LushText's mutation-testing gates, scopes, artifacts, triage expectations, and CI/developer workflows.

### Modified Capabilities

- None.

## Impact

- Affected files likely include `.cargo/mutants.toml`, `Makefile`, `.github/workflows/*`, project testing documentation, and possibly small helper scripts for shared mutation commands.
- CI will gain a pull-request changed-code mutation lane and a scheduled or manual sharded full mutation lane.
- The implementation should not add runtime dependencies to the application crates; `cargo-mutants` and related installers are development/CI tooling only.
- The first robust scope should focus on deterministic non-widget code. Widget-harness and compositor-dependent behavior remain covered by the existing widget tests unless later evidence shows a stable mutation lane is worth adding.
