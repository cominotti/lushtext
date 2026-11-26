// SPDX-License-Identifier: GPL-3.0-or-later

//! Scope-owned admission-charge helper shared by palette ledgers.
//!
//! Bounded loading loops and traversal ledgers reserve construction or scratch
//! bytes against an admission budget, then must release exactly what they
//! charged on every exit path — item admitted, filtered out, budget rejected,
//! or error propagated. Hand-placing a `release` call on each exit path is the
//! vigilance anti-pattern this module retires: the caller never names the
//! release, so a new `continue`/`return` inside the body cannot leak a charge.
//!
//! [`with_charge`] is parameterized over an arbitrary ledger `state` plus its
//! `charge`/`release` operations, so both the note-source construction budget
//! (`NoteSourceAdmission`) and the file-index build ledger
//! (`FileIndexBuildLedger`) release through the same mechanism.

use std::ops::ControlFlow;

/// Outcome of one [`with_charge`] scope.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ChargeOutcome<T> {
    /// The body ran to completion; the charge was released by scope.
    Ran,
    /// The body exited early with a value; the charge was still released.
    Broke(T),
    /// The budget rejected the charge, so the body never ran.
    BudgetExhausted,
}

/// Run `body` under a scope-owned charge on `state`.
///
/// The charge is taken before `body` runs and released exactly once on every
/// return path. Callers cannot leak or double-release the charge because they
/// never see the release call; a new early exit inside `body` releases by scope
/// instead of by a manual call it could forget.
///
/// `charge` returns `false` when the budget rejects the reservation, in which
/// case `body` never runs and no release is performed.
pub(super) fn with_charge<S, T>(
    state: &mut S,
    charge: impl FnOnce(&mut S) -> bool,
    release: impl FnOnce(&mut S),
    body: impl FnOnce(&mut S) -> ControlFlow<T, ()>,
) -> ChargeOutcome<T> {
    if !charge(state) {
        return ChargeOutcome::BudgetExhausted;
    }
    let flow = body(state);
    release(state);
    match flow {
        ControlFlow::Continue(()) => ChargeOutcome::Ran,
        ControlFlow::Break(value) => ChargeOutcome::Broke(value),
    }
}

/// Fallible [`with_charge`]: `body` may propagate an error with `?`.
///
/// The release runs before any error is propagated, so a bailing `?` inside a
/// scoped section still returns the charge instead of leaking it. As with
/// [`with_charge`], the caller never names the release, so a new early exit
/// (including a new `?`) cannot leak the charge.
pub(super) fn try_with_charge<S, T, E>(
    state: &mut S,
    charge: impl FnOnce(&mut S) -> bool,
    release: impl FnOnce(&mut S),
    body: impl FnOnce(&mut S) -> Result<ControlFlow<T, ()>, E>,
) -> Result<ChargeOutcome<T>, E> {
    if !charge(state) {
        return Ok(ChargeOutcome::BudgetExhausted);
    }
    let flow = body(state);
    release(state);
    match flow? {
        ControlFlow::Continue(()) => Ok(ChargeOutcome::Ran),
        ControlFlow::Break(value) => Ok(ChargeOutcome::Broke(value)),
    }
}
