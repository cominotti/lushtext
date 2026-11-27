// SPDX-License-Identifier: GPL-3.0-or-later

//! Write-shell conformance over generated cases
//! (`verify-shell-conformance-against-proven-cores` D4).
//!
//! The deterministic enumeration in `services::conformance::write_shell`
//! covers every single fault and crash of every primitive with fixed modes
//! and payloads. This property adds what the enumeration does not vary:
//! permission bits, payload sizes and two injections in one run. Each case
//! runs the real primitive under the conformance tap and replays it through
//! the proven cores and their shared disk model, which are the oracle.
//!
//! Injection sites are chosen enabled-indexed (design D2): a fault-free run
//! of the same case lists its actions, and each generated index picks one of
//! them modulo their count, so every generated fault lands on an action the
//! run makes.

#![cfg(unix)]

use lushtext_services::instrumentation::conformance::write_shell::{
    WriteCase, WriteFault, WriteFaultKind, WritePrimitive, check_write_case,
};
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;

use crate::support;

/// The largest payload a case writes: several stream chunks, still cheap.
const MAX_PAYLOAD_BYTES: usize = 8192;

/// One generated injection: indices into the actions and calls a
/// fault-free run of the case makes, and the error kind.
#[derive(Clone, Copy, Debug)]
struct GeneratedFault {
    action: usize,
    call: usize,
    kind: u8,
}

fn fault_strategy() -> impl Strategy<Value = GeneratedFault> {
    (any::<usize>(), any::<usize>(), 0u8..3).prop_map(|(action, call, kind)| GeneratedFault {
        action,
        call,
        kind,
    })
}

/// The fault a generated one names in the fault-free run's actions.
fn resolve(
    generated: GeneratedFault,
    actions: &[lushtext_services::instrumentation::conformance::write_shell::ActionShape],
) -> Option<WriteFault> {
    let faultable: Vec<_> = actions.iter().filter(|action| action.points > 0).collect();
    if faultable.is_empty() {
        return None;
    }
    let action = faultable[generated.action % faultable.len()];
    let kind = match generated.kind {
        1 if action.name == "CreateTemp" => WriteFaultKind::AlreadyExists,
        2 if action.name == "Rename" => WriteFaultKind::CrossDevice,
        _ => WriteFaultKind::Fail,
    };
    Some(WriteFault {
        core: action.core,
        run: action.run,
        ordinal: action.ordinal,
        call: generated.call % action.points,
        kind,
    })
}

proptest! {
    #![proptest_config(support::property_config())]

    /// Every generated durable write, with up to two injected faults, steps
    /// in lockstep with its proven core and the shared disk model.
    #[test]
    fn generated_durable_writes_conform_to_their_disk_model(
        primitive in prop::sample::select(WritePrimitive::ALL.to_vec()),
        destination_exists in any::<bool>(),
        // Keep owner read and write so the fixtures can always read back.
        extra_bits in 0u32..0o177,
        old in proptest::collection::vec(any::<u8>(), 0..MAX_PAYLOAD_BYTES),
        new in proptest::collection::vec(any::<u8>(), 0..MAX_PAYLOAD_BYTES),
        failing_producer in any::<bool>(),
        faults in proptest::collection::vec(fault_strategy(), 0..=2),
    ) {
        let mut case = WriteCase::plain(primitive, destination_exists);
        case.mode = 0o600 | extra_bits;
        case.old = old;
        case.new = new;
        case.failing_producer = failing_producer && primitive == WritePrimitive::ReplaceStream;

        let base_root = tempfile::tempdir().expect("tempdir");
        let base = check_write_case(base_root.path(), &case)
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        for generated in faults {
            if let Some(fault) = resolve(generated, &base.actions)
                && !case.faults.contains(&fault)
            {
                case.faults.push(fault);
            }
        }
        let root = tempfile::tempdir().expect("tempdir");
        check_write_case(root.path(), &case)
            .map_err(|error| TestCaseError::fail(format!("{case:?}: {error}")))?;
    }
}
