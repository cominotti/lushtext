# Proposal

## Why

GTK closures in `crates/lushtext-core/src/ui/` capture objects weakly by hand, and `glib::clone!` appears once in `crates/`. Measured on `origin/main` `761866ff`, among the 705 `.upgrade()` calls in `ui/` (task 0.1 re-measures after the typed-row change, which deletes many closures):

| Shape | Sites |
| --- | --- |
| `if let Some(x) = y.upgrade() {` | 301 |
| `let … else` | 176 |
| inside `if let` chains | 27 |
| split over lines | 96 |

78 `glib::Propagation` and 130 `ControlFlow` sites mark the closures that return a value, where an upgrade failure must answer the same value it answers today. `.agents/rules/rust.md` ("Modern Rust Idioms") prescribes the `if let … upgrade() &&` chain, so the manual form is today's rule. The maintainer decided on 2026-11-19 to adopt `glib::clone!` fully, without weighing churn.

## What Changes

- **A one-file probe first:** the 12 blocks of `ui/window/imp.rs:955-1040`. It proves:
  - `make check-fmt` and `make check-clippy` pass;
  - Clippy still reports a planted `.unwrap()` inside the macro body;
  - every text-parsing `check-*` audit still passes and still fails on its planted case.
- **The rewrite.** Every weak capture of a GObject in a closure uses `glib::clone!` with `#[weak]` or `#[weak(rename_to = …)]`. A closure that returns a value uses `#[upgrade_or …]` with the value its manual fallthrough returns today.
- **The rule changes with it.** `rust.md` prescribes `clone!` for weak captures. A policy audit holds the count of manual weak captures (`.downgrade()` calls in `ui/`) at or below a recorded ceiling that only decreases, down to the listed exceptions (design D2).

## Non-goals

- Strong captures, `ThreadGuard`, `SendWeakRef` and worker-thread code: `clone!` is for GTK-thread closures.
- Changing any closure's behaviour. An upgrade failure answers exactly what it answers today.
- Code outside `crates/lushtext-core/src/ui/` (the GTK Lush family keeps its own style).

## User-visible impact

None. Nothing is lost compared with `main`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rust-linting-policy`: GTK-thread weak captures use one form, held by a ratcheted audit.

## Impact

- **Code:** about 130 files under `ui/`, `scripts/` (the audit), `.agents/rules/rust.md`.
- **Proof:** every commit is under `ui/`, so each owes the three smokes.
- **Order:** the Start line in `tasks.md`, after `adopt-typed-row-state-and-dialog-futures` merges.
- **Shared with the other refactor-1 changes:** `ui/window/imp.rs` (with `close-audited-gate-gaps`), `scripts/check-signal-connections.py` (with `adopt-typed-row-state-and-dialog-futures`), and `check-policy` / `check-policy-ci` (`close-audited-gate-gaps` builds them). This change lands last.
