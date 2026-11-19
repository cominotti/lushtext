# Design

## Context

See `proposal.md`. Locked glib-macros 0.22.9: the `clone!` macro generates the upgrade inside the closure or `async` block. The failure behaviour is `()` by default, or set by `#[upgrade_or]`, `#[upgrade_or_else]`, `#[upgrade_or_default]` or `#[upgrade_or_panic]`.

## Goals / Non-Goals

**Goals:** one weak-capture form, checked by the compiler and a ratchet. **Non-Goals:** `proposal.md`.

## Decisions

### D1. Probe before the sweep

`ui/window/imp.rs:955-1040` (12 blocks) is converted first, in its own commit, with three checks:
- Clippy diagnostics inside the macro: a planted `.unwrap()` must be reported, then removed;
- `make check-fmt`;
- every text-parsing audit that reads `ui/` (`check-signal-connections`, `check-settings-writes`, `check-font-zoom-registration`, `check-workflow-boundaries`, `check-filesystem-boundary`) on the converted file, each with its planted case.

If any check loses sight inside the macro, the sweep stops and the audit learns the macro form first.

### D2. Mapping rules

- **Plain closure.** `let w = x.downgrade(); move |…| { if let Some(x) = w.upgrade() { body } }` becomes `clone!(#[weak] x, move |…| body)`.
- **`let … else` with a value.** It becomes `#[upgrade_or(value)]` with the exact value. A `let … else { return; }` needs nothing.
- **Chains.** An `if let … && …` chain keeps its other conditions inside the body.
- **`Propagation` and `ControlFlow`.** Each site's fallthrough value is read and copied. A test covers one site of each type.
- **Untouched.** Closures that capture a `WeakRef` stored elsewhere (struct fields, registries) or upgrade conditionally in the middle of the body stay manual, and the ceiling counts them. They are listed in the handoff with the reason.

### D3. The ratchet

- **The audit.** A `scripts/check-weak-captures.py` (or a mode of `check-signal-connections.py`) counts `.downgrade()` calls in `ui/` (comments and strings stripped) and compares the count with a ceiling recorded in the script. A `.downgrade()` that stores a `WeakRef` in a field rather than capturing it counts too; those are D2's listed exceptions.
- **What fails.** It fails above the ceiling and when the count is below it, so the ceiling is lowered in the same change.
- **`--self-test`** plants an extra site and a stale ceiling.

## Risks / Trade-offs

- **A changed fallback value is a silent behaviour change.** → D2's explicit values, the per-type tests and review of every `Propagation`/`ControlFlow` site.
- **Macro bodies hide code from text-parsing audits.** → D1 proves each audit first.

## Verification scope

- **Kani:** none (no proven module touched).
- **Timing:** none (no timed path touched).
- **Not the same calls.** In glib-macros 0.22.9 the default upgrade-failure path emits `g_debug!("Failed to upgrade `x`")` (`clone.rs:277-297`); the manual form logs nothing. D1's probe checks that the widget harness's and the smoke lanes' log scans ignore debug-level messages. If one does not, the scan or the mapping (`#[upgrade_or_else]` with no log) is decided before the sweep.
- **Axioms:** none.
