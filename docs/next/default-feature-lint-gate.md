# Default-Feature Warning Gate

## Status: Deferred (candidate future proposal)

## Problem

The blocking lint gate is `cargo clippy --workspace --all-targets --all-features
-- -D warnings` (run by `make check`, pre-commit, and CI). With `--all-features`
the `test-utils` feature is enabled, so any symbol used *only* inside
`#[cfg(feature = "test-utils")]` or `#[cfg(test)]` code counts as used and never
triggers `unused_imports` (or other cfg-sensitive lints).

But the **default-feature** build — what the end-user smoke lanes
(`make accessibility-smoke`, `make visual-smoke`, …) and real releases compile —
sees those imports as unused and warns. So a green `make check` does not prove
default builds are warning-clean.

This was discovered while applying `close-consistency-and-decomposition-gaps`:
`BTreeSet` (`services/content_search/replace.rs`) and `Ordering`
(`services/local_history_service.rs`) both warned only under default features,
because in-scope changes left them used solely by test-utils-gated code. They
were fixed in that change by gating each `use` behind the same `#[cfg(...)]` as
its consumer, but nothing prevents the class from recurring.

## Proposed Behavior

Add a cheap default-feature warning gate so the class is caught before a smoke
run or release, without slowing the main `--all-features` gate. Options to weigh:

- A fast `cargo build -p lushtext-core --lib` (and the binary crate) with
  `RUSTFLAGS="-D warnings"` under **default** features, wired into
  `make check-policy` or a dedicated `make check-default-warnings` target.
- Or a `cargo clippy` pass with no `--all-features` (default feature set only),
  scoped to the workspace, as a second lint invocation.

Keep it bounded: the goal is only to surface cfg-gated unused imports / dead
code that the `--all-features` pass masks, not to duplicate the full lint matrix.

## Scope Notes

- Explicitly out of scope for `close-consistency-and-decomposition-gaps` (that
  change fixed the two concrete instances as pre-existing blockers; adding a
  systemic gate is new build-hygiene capability).
- If adopted, update `.agents/rules/build.md` so the default-feature gate is
  documented alongside the existing `--all-features` blocking command, and note
  that the two gates cover complementary feature sets.
