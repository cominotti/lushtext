// SPDX-License-Identifier: GPL-3.0-or-later

//! Role: test policy — this workflow's single `test-utils` override value.
//!
//! One value, one purpose: substitute the native print operation with a probe so
//! widget tests can prove which document the shell chose without opening a
//! native printer dialog. This is a **lifecycle probe**, not an inspection seam:
//! there is no production equivalent of "run the print dialog and tell me what
//! happened", so it is kept rather than retired. It is scoped to one call by
//! `with_print_runner_for_test`, so no override can leak between tests.

use std::cell::RefCell;

use super::evidence::PrintEvidence;
use super::policy::PrintOutcome;

type TestPrintRunner = Box<dyn Fn(&PrintEvidence) -> PrintOutcome>;

thread_local! {
    static TEST_PRINT_RUNNER: RefCell<Option<TestPrintRunner>> = RefCell::new(None);
}

/// Temporarily replace the native print operation with a test runner.
///
/// The runner receives the workflow's own evidence surface, so a test reads the
/// chosen document through the same value any other observer would.
pub fn with_print_runner_for_test<R>(
    runner: impl Fn(&PrintEvidence) -> PrintOutcome + 'static,
    f: impl FnOnce() -> R,
) -> R {
    TEST_PRINT_RUNNER.with(|cell| {
        let previous = cell.replace(Some(Box::new(runner)));
        let result = f();
        cell.replace(previous);
        result
    })
}

/// Run the installed probe, if a test installed one.
pub(super) fn installed_runner_outcome(evidence: &PrintEvidence) -> Option<PrintOutcome> {
    TEST_PRINT_RUNNER.with(|cell| cell.borrow().as_ref().map(|runner| runner(evidence)))
}
