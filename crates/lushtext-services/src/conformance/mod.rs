// SPDX-License-Identifier: GPL-3.0-or-later

//! Shell conformance against the proven cores
//! (`verify-shell-conformance-against-proven-cores`): drivers that run a real
//! imperative shell in lockstep with the Kani-checked core and the shared
//! environment model it is proved in, and compare their states after every
//! step. The proven code is the only oracle: there is no expected-state table
//! and no parallel state machine.
//!
//! Compiled only under `cfg(test)` and the `test-utils` feature. The drivers
//! name neither `proptest` nor `tempfile` (dev-dependencies): each takes the
//! operations to run and a root directory its caller owns and removes. The
//! deterministic enumerations live in the drivers' unit tests; the generated
//! properties live in `tests/properties/`.

pub mod draft_service;
pub mod write_shell;
