# lushtext

This crate is the thin binary entry point plus integration and widget tests.

## Boundaries

- Keep application logic in `crates/lushtext-core`. Do not migrate editor behavior, persistence rules, or GTK workflow logic into this crate for convenience.
- Keep `src/` focused on application startup and binary-specific glue.
- Prefer testing GTK-free behavior in `lushtext-core` unit tests when possible; reserve this crate for integration and real widget flows.

## Tests

- `tests/widget.rs` is the custom single-threaded GTK harness. Do not convert it to nextest/libtest patterns that would break the one-process-per-case model.
- Keep widget test files split by feature under `crates/lushtext/tests/widget/`.
- When test infrastructure changes, update `README.md` and the root `AGENTS.md` in the same change.
