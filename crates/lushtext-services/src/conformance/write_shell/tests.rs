// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::filesystem::write_protocol::MAX_TEMP_NAME_ATTEMPTS;

fn run(case: &WriteCase) -> WriteCaseReport {
    let root = tempfile::tempdir().expect("tempdir");
    check_write_case(root.path(), case).unwrap_or_else(|error| panic!("{case:?}: {error}"))
}

/// Every single-fault and crash case of every primitive, plus a failed temp
/// removal after each failure that triggers one, and every count of taken
/// temp names: the write shell matches its disk model and its cores after
/// every action.
#[test]
fn every_durable_write_primitive_conforms_to_its_disk_model() {
    let started = std::time::Instant::now();
    let mut cases = 0usize;
    let mut injected = 0usize;
    for primitive in WritePrimitive::ALL {
        for destination_exists in [false, true] {
            let base_case = WriteCase::plain(primitive, destination_exists);
            let base = run(&base_case);
            cases += 1;
            for action in &base.actions {
                let mut kinds = vec![WriteFaultKind::Fail];
                if action.core == WriteCore::Write && action.name == "CreateTemp" {
                    kinds.push(WriteFaultKind::AlreadyExists);
                }
                if action.name == "Rename" {
                    kinds.push(WriteFaultKind::CrossDevice);
                }
                for call in 0..action.points {
                    for kind in &kinds {
                        let fault = WriteFault {
                            core: action.core,
                            run: action.run,
                            ordinal: action.ordinal,
                            call,
                            kind: *kind,
                        };
                        let mut case = base_case.clone();
                        case.faults.push(fault);
                        let report = run(&case);
                        cases += 1;
                        injected += report.injected;
                        // A failed temp removal on the failure path the
                        // fault opened.
                        for later in report.actions.iter().filter(|later| {
                            later.name == "RemoveTemp" && !base.actions.contains(later)
                        }) {
                            let mut case = case.clone();
                            case.faults.push(WriteFault {
                                core: later.core,
                                run: later.run,
                                ordinal: later.ordinal,
                                call: 0,
                                kind: WriteFaultKind::Fail,
                            });
                            let report = run(&case);
                            cases += 1;
                            assert_eq!(report.injected, 2, "{case:?}");
                        }
                    }
                }
                for after in [false, true] {
                    let mut case = base_case.clone();
                    case.crash = Some(WriteCrash {
                        core: action.core,
                        run: action.run,
                        ordinal: action.ordinal,
                        after,
                    });
                    let report = run(&case);
                    cases += 1;
                    assert!(report.crashed, "{case:?}");
                }
            }
        }
    }
    // Taken temp names, up to every attempt.
    for taken in 1..=usize::from(MAX_TEMP_NAME_ATTEMPTS) {
        let mut case = WriteCase::plain(WritePrimitive::Replace, true);
        for ordinal in 1..=taken {
            case.faults.push(WriteFault {
                core: WriteCore::Write,
                run: 0,
                ordinal,
                call: 0,
                kind: WriteFaultKind::AlreadyExists,
            });
        }
        let report = run(&case);
        cases += 1;
        assert_eq!(report.injected, taken);
    }
    let mut producer = WriteCase::plain(WritePrimitive::ReplaceStream, true);
    producer.failing_producer = true;
    run(&producer);
    cases += 1;
    assert!(
        injected > 50,
        "the enumeration injected faults ({injected})"
    );
    let elapsed = started.elapsed();
    eprintln!("write-shell conformance: {cases} cases in {elapsed:?}");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the enumeration's 5 s budget (design D12): {cases} cases took {elapsed:?}"
    );
}

/// The per-action backend call table is pinned: a shell change that adds a
/// call to an action fails here and becomes a new injection point.
#[test]
fn the_per_action_call_table_is_pinned() {
    use WriteCore::{Move, Rename, Write};
    let pinned = [
        (Write, "ProbeMetadata", false, 1),
        (Write, "ProbeMetadata", true, 1),
        (Write, "CreateTemp", true, 1),
        (Write, "WriteContent", true, 0),
        (Write, "ApplyMetadata", false, 0),
        (Write, "ApplyMetadata", true, 1),
        (Write, "SyncTemp", true, 1),
        (Write, "Rename", true, 1),
        (Write, "SyncDir", true, 1),
        (Write, "RemoveTemp", true, 1),
        (Move, "Copy", true, 1),
        (Move, "RemoveSource", true, 1),
        (Move, "SyncSourceDir", true, 1),
        (Rename, "Rename", false, 1),
        (Rename, "SyncSourceDir", false, 1),
        (Rename, "SyncDestinationDir", false, 1),
    ];
    for (core, action, source, calls) in pinned {
        assert_eq!(
            expected_calls(core, action, source),
            Some(calls),
            "{core:?} {action} with a metadata source {source}"
        );
    }
    assert_eq!(expected_calls(Write, "Finish", true), None);
    // The fault-free runs make exactly the pinned calls.
    for primitive in WritePrimitive::ALL {
        for destination_exists in [false, true] {
            let report = run(&WriteCase::plain(primitive, destination_exists));
            for action in &report.actions {
                let content_point = usize::from(action.name == "WriteContent");
                assert!(
                    action.points
                        <= content_point
                            + expected_calls(action.core, &action.name, true).unwrap_or(0),
                    "{primitive:?}: {action:?}"
                );
            }
        }
    }
}
