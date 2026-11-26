// SPDX-License-Identifier: GPL-3.0-or-later

//! Draft-service conformance over generated scripts
//! (`verify-shell-conformance-against-proven-cores` D2, D5, D7).
//!
//! Each generated case seeds a temporary data directory with a generated
//! previous-session layout (`INITIAL_LAYOUT_BITS` flags), runs an
//! enabled-indexed script of `DraftOp` steps against both the real
//! `draft_service` and the Kani-checked journal model (`SingleWindowScope`), and
//! compares the projected disk and authority state after every step.
//!
//! Domain restriction (`Oversized`): generated scripts always write small text
//! payloads (`<= 16` bytes), so `RestoreEnding::Oversized` is never exercised by
//! this property (it remains covered by unit tests in `draft_service`).

use lushtext_core::services::conformance::draft_service::{
    DraftFault, DraftOp, FaultDomain, INITIAL_LAYOUT_BITS, ReachCounts, check_draft_script,
    evaluate_model_reach,
};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{
    Config as ProptestConfig, RngAlgorithm, TestCaseError, TestRng, TestRunner,
};

use crate::support;

/// Maximum ops per generated script in the default 64-case property lane.
const DEFAULT_MAX_OPS: usize = 16;
/// Maximum ops per generated script in opt-in deep runs (`PROPTEST_DEEP_CASES >= 512`).
const DEEP_MAX_OPS: usize = 32;

fn draft_conformance_config() -> ProptestConfig {
    let base = support::property_config();
    ProptestConfig {
        max_shrink_iters: 256,
        max_shrink_time: 45_000,
        ..base
    }
}

fn configured_max_ops() -> usize {
    if support::property_config().cases > support::DEFAULT_PROPERTY_CASES {
        DEEP_MAX_OPS
    } else {
        DEFAULT_MAX_OPS
    }
}

fn draft_fault_strategy() -> impl Strategy<Value = DraftFault> {
    prop_oneof![
        9 => Just(DraftFault::None),
        1 => Just(DraftFault::BeforeEffect),
        1 => Just(DraftFault::AfterEffect),
        1 => (0usize..24).prop_map(DraftFault::At),
    ]
}

fn draft_op_strategy() -> impl Strategy<Value = DraftOp> {
    (
        any::<usize>(),
        any::<usize>(),
        draft_fault_strategy(),
        0u8..3,
    )
        .prop_map(|(action_index, id_index, fault, restore_choice)| DraftOp {
            action_index,
            id_index,
            fault,
            restore_choice,
        })
}

fn draft_script_strategy(
    max_ops: usize,
) -> impl Strategy<Value = ([bool; INITIAL_LAYOUT_BITS], Vec<DraftOp>)> {
    let min_long = 8.min(max_ops);
    (
        any::<[bool; INITIAL_LAYOUT_BITS]>(),
        prop_oneof![
            1 => prop::collection::vec(draft_op_strategy(), 1..=max_ops),
            3 => prop::collection::vec(draft_op_strategy(), min_long..=max_ops),
        ],
    )
}

proptest! {
    #![proptest_config(draft_conformance_config())]

    /// Every enabled-indexed draft script with `BeforeEffect`, `AfterEffect`,
    /// `At(k)` faults, and crashes keeps the real `draft_service` disk and
    /// authority aligned with the Kani-checked journal model.
    #[test]
    fn generated_draft_scripts_conform_to_journal_model(
        (initial_bits, ops) in draft_script_strategy(configured_max_ops())
    ) {
        let root = tempfile::tempdir().expect("tempdir");
        check_draft_script(root.path(), initial_bits, &ops, FaultDomain::Full)
            .map_err(TestCaseError::fail)?;
    }
}

/// Deterministic reach check over the journal model alone (Task 4.5, design D2):
/// verifies that the enabled-indexed generator reaches every journal stage
/// well above vacuous floors within the default 64-case budget.
#[test]
fn draft_conformance_strategy_reaches_every_journal_stage() {
    let strategy = draft_script_strategy(DEFAULT_MAX_OPS);
    let config = ProptestConfig {
        cases: support::DEFAULT_PROPERTY_CASES,
        ..draft_conformance_config()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    let mut totals = ReachCounts::default();

    for _ in 0..support::DEFAULT_PROPERTY_CASES {
        let tree = strategy
            .new_tree(&mut runner)
            .expect("generate draft script for reach check");
        let (initial_bits, ops) = tree.current();
        let counts = evaluate_model_reach(initial_bits, &ops);
        totals.add_assign(&counts);
    }

    // Recorded floors for the fixed deterministic ChaCha seed over 64 x <=16-op
    // scripts under the full fault domain (observed: startup=103, write_body=17,
    // commit=10, deletion_step=94, restore_apply=63, exec_cleanup=21).
    assert!(
        totals.startup >= 64,
        "startup reach too low: {}",
        totals.startup
    );
    assert!(
        totals.write_body >= 12,
        "write_body reach too low: {}",
        totals.write_body
    );
    assert!(
        totals.commit >= 6,
        "commit reach too low: {}",
        totals.commit
    );
    assert!(
        totals.deletion_step >= 50,
        "deletion_step reach too low: {}",
        totals.deletion_step
    );
    assert!(
        totals.restore_apply >= 35,
        "restore_apply reach too low: {}",
        totals.restore_apply
    );
    assert!(
        totals.exec_cleanup >= 10,
        "exec_cleanup reach too low: {}",
        totals.exec_cleanup
    );
}
